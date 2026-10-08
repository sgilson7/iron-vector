//! The door the shim calls through: build a game from the data files, then
//! each display frame hand it the clock, the held keys and the mouse movement,
//! and read back what to draw.
//!
//! The game is in one mode at a time: the garage, the briefing, a sortie, the
//! test field, or the debrief. Only a sortie and the test field run the
//! simulation, at a fixed 60 ticks a second; `advance` runs as many ticks as
//! the elapsed time owes, and the drawing is placed between the last two
//! ticks. Mouse look is applied on every frame, before any tick, so the view
//! never waits for one.

use crate::content::{Missions, Palette};
use crate::fx::{self, deg, int, ONE};
use crate::garage::{Garage, GarageView};
use crate::geom::{facing, v3};
use crate::mech::{Controls, TICKS_PER_SECOND};
use crate::mesh::{self, Mesh};
use crate::mission::Debrief;
use crate::parts::{Catalog, Loadout};
use crate::pilot::Pilots;
use crate::render::{self, Camera, Hud};
use crate::world::{World, WEAPON_SLOTS};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

/// The actions a key or button can hold, and the bit each sets.
pub const ACTIONS: &[(&str, u32)] = &[
    ("forward", 1 << 0),
    ("back", 1 << 1),
    ("left", 1 << 2),
    ("right", 1 << 3),
    ("ascend", 1 << 4),
    ("quick_boost", 1 << 5),
    ("glide", 1 << 6),
    ("fire_r", 1 << 7),
    ("fire_l", 1 << 8),
    ("fire_s", 1 << 9),
    // set while the pointer is captured; without it the simulation holds still
    ("focus", 1 << 10),
    // set while the mouse is dragged over the view outside a sortie
    ("drag", 1 << 11),
];

fn bit(name: &str) -> u32 {
    ACTIONS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, b)| *b)
        .unwrap_or(0)
}

/// Angle units of turn per pixel of mouse movement (about 0.1°). The page
/// sends movement in hundredths of a pixel, so slow fractional motion is kept.
pub const LOOK_PER_PIXEL: i32 = 18;
const HUNDREDTHS: i32 = 100;
/// One tick, in microseconds × ticks per second, so the sum stays exact.
const TICK_UNITS: i64 = 1_000_000;
/// The most ticks one frame may run; a longer stall is dropped, not replayed.
pub const MAX_TICKS_PER_FRAME: u32 = 6;
/// A gap longer than this between frames is treated as a pause.
const MAX_FRAME_US: i64 = 250_000;
/// The largest drawing buffer, in pixels, before the resolution is scaled down.
const MAX_PIXELS: i64 = 2560 * 1440;
/// The garage camera turns this much a second on its own.
const ORBIT_PER_SECOND: i32 = deg(8);
const ORBIT_DISTANCE: i32 = int(15);
const ORBIT_PITCH: i32 = -deg(10);
const ORBIT_PIVOT_Y: i32 = int(4);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Garage,
    Briefing,
    Sortie,
    Test,
    Debrief,
}

/// What is saved between visits: the loadout and the paint scheme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Saved {
    loadout: BTreeMap<crate::parts::Slot, String>,
    paint: usize,
}

pub struct Game {
    cat: Catalog,
    pal: Palette,
    missions: Missions,
    pilots: Pilots,
    loadout: Loadout,
    paint: usize,
    mode: Mode,
    world: World,
    garage: Garage,
    debrief: Option<Debrief>,
    orbit: i32,
    scene_version: u32,
    names: [String; 3],
    prev_bits: u32,
    latched: u32,
    last_us: Option<i64>,
    acc: i64,
    alpha: i32,
    css: (i32, i32),
    pixels: (i32, i32),
    paused: bool,
    static_values: Vec<i32>,
    static_draws: Vec<u32>,
    values: Vec<i32>,
    draws: Vec<u32>,
    view: Vec<i32>,
    hud: Option<Hud>,
}

impl Game {
    pub fn new(
        parts_json: &str,
        palette_json: &str,
        missions_json: &str,
        pilots_json: &str,
        saved: &str,
    ) -> Result<Game, String> {
        let cat = Catalog::parse(parts_json).map_err(|e| format!("parts: {}", e.0))?;
        let pal = Palette::parse(palette_json)?;
        let missions = Missions::parse(missions_json)?;
        let pilots = Pilots::parse(pilots_json)?;
        if !pilots.0.contains_key(&missions.mission.enemy.pilot) {
            return Err(format!(
                "missions: no pilot called {}",
                missions.mission.enemy.pilot
            ));
        }
        let (loadout, paint) = match serde_json::from_str::<Saved>(saved) {
            Ok(s) => (
                Loadout::restore(&serde_json::to_string(&s.loadout).unwrap_or_default(), &cat),
                s.paint.min(pal.schemes.len() - 1),
            ),
            Err(_) => (cat.default_loadout(), 0),
        };
        let world = World::hangar(&cat, &loadout, pal.scheme(paint).paints());
        let mut g = Game {
            cat,
            pal,
            missions,
            pilots,
            loadout,
            paint,
            mode: Mode::Garage,
            world,
            garage: Garage::default(),
            debrief: None,
            orbit: fx::HALF + deg(30),
            scene_version: 0,
            names: Default::default(),
            prev_bits: 0,
            latched: 0,
            last_us: None,
            acc: 0,
            alpha: ONE,
            css: (1, 1),
            pixels: (1, 1),
            paused: true,
            static_values: Vec::new(),
            static_draws: Vec::new(),
            values: Vec::new(),
            draws: Vec::new(),
            view: Vec::new(),
            hud: None,
        };
        g.enter(Mode::Garage);
        Ok(g)
    }

    /// Switches mode, building the world that mode shows.
    fn enter(&mut self, mode: Mode) {
        let paint = self.pal.scheme(self.paint).paints();
        match mode {
            Mode::Garage | Mode::Briefing => {
                self.world = World::hangar(&self.cat, &self.loadout, paint);
            }
            Mode::Sortie => {
                let spec = &self.missions.mission;
                let pilot = &self.pilots.0[&spec.enemy.pilot];
                self.world = World::mission(
                    &self.cat,
                    &self.loadout,
                    paint,
                    spec,
                    pilot,
                    self.pal.enemy.paints(),
                );
            }
            Mode::Test => {
                self.world = World::proving(&self.cat, &self.loadout, &self.missions.proving, paint);
            }
            Mode::Debrief => {}
        }
        if mode != Mode::Debrief {
            self.debrief = None;
            let (values, draws) = render::scene(&self.world, &self.pal);
            self.static_values = values;
            self.static_draws = draws;
            self.scene_version += 1;
        }
        self.names = WEAPON_SLOTS.map(|s| self.loadout.part(s, &self.cat).name.clone());
        self.mode = mode;
        self.last_us = None;
        self.acc = 0;
        self.latched = 0;
        self.alpha = ONE;
        self.draw();
    }

    fn running(&self) -> bool {
        matches!(self.mode, Mode::Sortie | Mode::Test)
    }

    /// Sets the canvas size in CSS pixels and the device pixel ratio in
    /// percent; returns the drawing buffer's size.
    pub fn resize(&mut self, css_w: i32, css_h: i32, dpr_pct: i32) -> [i32; 2] {
        let (cw, ch) = (css_w.max(1), css_h.max(1));
        let mut w = cw as i64 * dpr_pct.clamp(50, 300) as i64 / 100;
        let mut h = ch as i64 * dpr_pct.clamp(50, 300) as i64 / 100;
        if w * h > MAX_PIXELS {
            // scale both sides by √(MAX / area), in thousandths
            let k = fx::isqrt((MAX_PIXELS * 1_000_000 / (w * h)) as u64) as i64;
            w = w * k / 1000;
            h = h * k / 1000;
        }
        self.css = (cw, ch);
        self.pixels = (w.max(1) as i32, h.max(1) as i32);
        self.draw();
        [self.pixels.0, self.pixels.1]
    }

    /// One display frame: `now_us` is the page's clock in microseconds,
    /// `bits` the held actions, `dx`/`dy` the mouse movement since last frame
    /// in hundredths of a CSS pixel.
    pub fn advance(&mut self, now_us: i64, bits: u32, dx: i32, dy: i32) {
        let dt = match self.last_us {
            Some(last) => (now_us - last).clamp(0, MAX_FRAME_US),
            None => 0,
        };
        self.last_us = Some(now_us);
        if !self.running() {
            self.paused = false;
            let drag = if bits & bit("drag") != 0 {
                dx * LOOK_PER_PIXEL / HUNDREDTHS
            } else {
                0
            };
            let spin = (dt * ORBIT_PER_SECOND as i64 / 1_000_000) as i32;
            self.orbit = (self.orbit + spin - drag) & (fx::TURN - 1);
            self.draw();
            return;
        }
        let focused = bits & bit("focus") != 0;
        self.latched |= bits & !self.prev_bits;
        self.prev_bits = bits;
        self.paused = !focused;
        if !focused {
            self.last_us = None;
            self.latched = 0;
            self.draw();
            return;
        }
        self.world.mechs[0].body.look(
            -dx * LOOK_PER_PIXEL / HUNDREDTHS,
            -dy * LOOK_PER_PIXEL / HUNDREDTHS,
        );
        self.acc += dt * TICKS_PER_SECOND as i64;
        let mut ran = 0;
        while self.acc >= TICK_UNITS && ran < MAX_TICKS_PER_FRAME {
            let c = self.controls(bits | self.latched);
            self.latched = 0;
            self.world.tick(c);
            self.acc -= TICK_UNITS;
            ran += 1;
        }
        if ran == MAX_TICKS_PER_FRAME {
            self.acc = self.acc.min(TICK_UNITS - 1);
        }
        self.alpha = (self.acc * ONE as i64 / TICK_UNITS) as i32;
        if let Some(m) = self.world.mission.as_ref().filter(|m| m.finished()) {
            self.debrief = Some(m.debrief(&self.world));
            self.mode = Mode::Debrief;
            self.paused = false;
        }
        self.draw();
    }

    fn controls(&self, b: u32) -> Controls {
        let axis = |pos: &str, neg: &str| {
            (if b & bit(pos) != 0 { ONE } else { 0 }) - (if b & bit(neg) != 0 { ONE } else { 0 })
        };
        let latched = |name: &str| self.latched & bit(name) != 0;
        Controls {
            move_x: axis("right", "left"),
            move_z: axis("forward", "back"),
            ascend: b & bit("ascend") != 0,
            quick_boost: latched("quick_boost"),
            toggle_glide: latched("glide"),
            fire: [
                b & bit("fire_r") != 0,
                b & bit("fire_l") != 0,
                b & bit("fire_s") != 0,
            ],
        }
    }

    fn camera(&self) -> Camera {
        match self.mode {
            Mode::Garage | Mode::Briefing => {
                let pivot = v3(0, ORBIT_PIVOT_Y, 0);
                let eye = pivot.sub(facing(self.orbit, ORBIT_PITCH).scale(ORBIT_DISTANCE));
                Camera {
                    eye,
                    yaw: self.orbit,
                    pitch: ORBIT_PITCH,
                }
            }
            _ => render::player_camera(&self.world, self.alpha),
        }
    }

    fn draw(&mut self) {
        let aspect = fx::div(self.css.0, self.css.1).max(1);
        let cam = self.camera();
        let vp = render::view_projection(&cam, aspect);
        let mut view = vp.0.to_vec();
        view.extend([cam.eye.x, cam.eye.y, cam.eye.z]);
        self.view = view;
        let (values, draws) = render::frame(&self.world, &self.pal, self.alpha);
        self.values = values;
        self.draws = draws;
        let mut hud = render::hud(&self.world, &vp, self.css.0, self.css.1, &self.names, self.paused);
        hud.mode = self.mode;
        hud.debrief = self.debrief.clone();
        self.hud = Some(hud);
    }

    // ---- garage and mode commands ----

    pub fn garage_select(&mut self, slot: usize) {
        self.garage.select(slot);
    }

    pub fn garage_hover(&mut self, part: i32) {
        self.garage.hover = usize::try_from(part).ok();
    }

    pub fn garage_equip(&mut self, part: usize) {
        if self.garage.equip(part, &mut self.loadout, &self.cat) {
            self.enter(Mode::Garage);
        }
    }

    pub fn garage_paint(&mut self, scheme: usize) {
        self.paint = scheme.min(self.pal.schemes.len() - 1);
        self.enter(Mode::Garage);
    }

    pub fn garage_json(&self) -> String {
        let v: GarageView = self.garage.view(&self.loadout, self.paint, &self.cat, &self.pal);
        serde_json::to_string(&v).unwrap_or_default()
    }

    /// The loadout and paint as a string for the page to keep between visits.
    pub fn saved(&self) -> String {
        serde_json::to_string(&Saved {
            loadout: self.loadout.0.clone(),
            paint: self.paint,
        })
        .unwrap_or_default()
    }

    pub fn briefing(&mut self) {
        self.enter(Mode::Briefing);
    }

    pub fn launch(&mut self) {
        self.enter(Mode::Sortie);
    }

    pub fn test_field(&mut self) {
        self.enter(Mode::Test);
    }

    pub fn to_garage(&mut self) {
        self.enter(Mode::Garage);
    }

    /// Bumped whenever the static scene changes, so the page re-uploads it.
    pub fn scene_version(&self) -> u32 {
        self.scene_version
    }

    /// The view-projection matrix (16, column-major) then the eye (3).
    pub fn view(&self) -> Vec<i32> {
        self.view.clone()
    }

    pub fn instances(&self) -> Vec<i32> {
        self.values.clone()
    }

    pub fn static_instances(&self) -> Vec<i32> {
        self.static_values.clone()
    }

    /// Every draw this frame, static ones first: `DRAW_VALUES` numbers each.
    pub fn draws(&self) -> Vec<u32> {
        let mut d = self.static_draws.clone();
        d.extend_from_slice(&self.draws);
        d
    }

    pub fn hud_json(&self) -> String {
        serde_json::to_string(&self.hud).unwrap_or_default()
    }

    /// The colour behind everything: the fog, so the far city melts into it.
    pub fn clear_color(&self) -> Vec<i32> {
        crate::content::q16(self.pal.fog).to_vec()
    }

    pub fn scene_uniforms(&self) -> Vec<i32> {
        render::scene_uniforms(&self.pal)
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }
}

/// Vertex data for one mesh, by its number.
pub fn mesh_vertices(id: u32) -> Vec<i32> {
    Mesh::ALL
        .get(id as usize)
        .map(|m| mesh::vertices(*m))
        .unwrap_or_default()
}

/// Every number the page needs, from where it is decided.
pub fn numbers() -> String {
    let vertex_bytes = mesh::VERTEX_VALUES * render::BYTES_PER_VALUE;
    let instance_bytes = render::INSTANCE_VALUES * render::BYTES_PER_VALUE;
    let vec4 = 4 * render::BYTES_PER_VALUE;
    json!({
        "frac_bits": fx::FRAC_BITS,
        "ticks_per_second": TICKS_PER_SECOND,
        "actions": ACTIONS.iter().map(|(n, b)| (n.to_string(), json!(b))).collect::<serde_json::Map<_, _>>(),
        "meshes": Mesh::ALL.iter().map(|m| json!({
            "id": *m as u32,
            "vertices": mesh::vertices(*m).len() / mesh::VERTEX_VALUES,
        })).collect::<Vec<_>>(),
        "vertex": {
            "stride": vertex_bytes,
            "attributes": [["aPos", 3, 0], ["aNormal", 3, 3 * render::BYTES_PER_VALUE]],
        },
        "instance": {
            "stride": instance_bytes,
            "attributes": [["iA", 4, 0], ["iB", 4, vec4], ["iC", 4, 2 * vec4], ["iD", 4, 3 * vec4]],
        },
        "draw_values": render::DRAW_VALUES,
        "buffers": { "static": render::STATIC_BUFFER, "dynamic": render::DYNAMIC_BUFFER },
        "uniforms": render::UNIFORM_LAYOUT.iter().map(|(n, k)| json!([n, k])).collect::<Vec<_>>(),
        "view_values": 16,
    })
    .to_string()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn game() -> Game {
        Game::new(
            include_str!("../../../data/parts.json"),
            include_str!("../../../data/palette.json"),
            include_str!("../../../data/missions.json"),
            include_str!("../../../data/pilots.json"),
            "",
        )
        .unwrap()
    }

    fn test_game() -> Game {
        let mut g = game();
        g.test_field();
        g
    }

    const FOCUS: u32 = 1 << 10;
    const FRAME_60: i64 = 16_667;

    #[test]
    fn a_new_game_opens_in_the_garage_and_runs_no_ticks() {
        let mut g = game();
        assert_eq!(g.mode(), Mode::Garage);
        for k in 0..60 {
            g.advance(k * FRAME_60, FOCUS | bit("forward"), 0, 0);
        }
        assert_eq!(g.world().tick, 0);
    }

    #[test]
    fn one_second_of_frames_runs_sixty_ticks_at_any_frame_rate() {
        for fps in [30i64, 60, 144, 240] {
            let mut g = test_game();
            let step = 1_000_000 / fps;
            for k in 0..=fps {
                g.advance(k * step, FOCUS, 0, 0);
            }
            let t = g.world().tick as i64;
            assert!((59..=60).contains(&t), "{fps} fps ran {t} ticks");
        }
    }

    #[test]
    fn a_long_stall_runs_at_most_the_cap_and_alpha_stays_in_range() {
        let mut g = test_game();
        g.advance(0, FOCUS, 0, 0);
        g.advance(2_000_000, FOCUS, 0, 0);
        assert!(g.world().tick <= MAX_TICKS_PER_FRAME);
        assert!((0..=ONE).contains(&g.alpha));
    }

    #[test]
    fn mouse_look_turns_the_aim_on_a_frame_that_runs_no_tick() {
        let mut g = test_game();
        g.advance(0, FOCUS, 0, 0);
        let before = g.world().player().body.aim_yaw;
        g.advance(1_000, FOCUS, 1_000, 0);
        assert_eq!(g.world().tick, 0, "a millisecond is not a tick");
        assert_eq!(
            fx::wrap(g.world().player().body.aim_yaw - before),
            -10 * LOOK_PER_PIXEL
        );
    }

    #[test]
    fn without_focus_nothing_moves_and_the_hud_says_paused() {
        let mut g = test_game();
        g.advance(0, bit("forward"), 0, 0);
        g.advance(500_000, bit("forward"), 50, 0);
        assert_eq!(g.world().tick, 0);
        assert!(g.hud_json().contains("\"paused\":true"));
    }

    #[test]
    fn a_tap_shorter_than_a_tick_still_quick_boosts() {
        let mut g = test_game();
        let qb = bit("quick_boost");
        g.advance(0, FOCUS, 0, 0);
        // pressed and released inside one tick
        g.advance(4_000, FOCUS | qb, 0, 0);
        g.advance(8_000, FOCUS, 0, 0);
        g.advance(FRAME_60 + 1_000, FOCUS, 0, 0);
        assert!(
            g.world().player().body.qb_ticks > 0,
            "the press was latched until the tick"
        );
    }

    #[test]
    fn holding_forward_shows_speed_on_the_hud() {
        let mut g = test_game();
        for k in 0..60 {
            g.advance(k * FRAME_60, FOCUS | bit("forward"), 0, 0);
        }
        let hud: serde_json::Value = serde_json::from_str(&g.hud_json()).unwrap();
        // walking 95 km/h
        let speed = hud["speed"].as_i64().unwrap();
        assert!((90..=96).contains(&speed), "{speed}");
    }

    #[test]
    fn a_large_screen_is_drawn_at_a_capped_resolution() {
        let mut g = game();
        assert_eq!(g.resize(1280, 720, 100), [1280, 720]);
        let [w, h] = g.resize(2560, 1600, 200);
        assert!((w as i64) * (h as i64) <= MAX_PIXELS);
        assert!((w * 1000 / h - 1600).abs() < 5, "aspect kept: {w}×{h}");
    }

    #[test]
    fn numbers_name_every_action_and_mesh() {
        let n: serde_json::Value = serde_json::from_str(&numbers()).unwrap();
        assert_eq!(n["actions"]["focus"], FOCUS);
        assert_eq!(n["meshes"].as_array().unwrap().len(), 4);
        assert_eq!(n["meshes"][0]["vertices"], 36);
    }

    #[test]
    fn fitting_a_part_and_paint_survives_a_save_and_a_new_game() {
        let mut g = game();
        g.garage_select(
            crate::parts::Slot::ALL
                .iter()
                .position(|s| *s == crate::parts::Slot::Legs)
                .unwrap(),
        );
        g.garage_equip(1);
        g.garage_paint(2);
        let saved = g.saved();
        let again = Game::new(
            include_str!("../../../data/parts.json"),
            include_str!("../../../data/palette.json"),
            include_str!("../../../data/missions.json"),
            include_str!("../../../data/pilots.json"),
            &saved,
        )
        .unwrap();
        assert_eq!(again.loadout, g.loadout);
        assert_eq!(again.paint, 2);
        assert_ne!(again.loadout, again.cat.default_loadout());
    }

    #[test]
    fn each_mode_change_bumps_the_scene_and_the_hud_names_the_mode() {
        let mut g = game();
        let v = g.scene_version();
        g.launch();
        assert!(g.scene_version() > v);
        assert!(g.hud_json().contains("\"mode\":\"sortie\""));
        g.to_garage();
        assert!(g.hud_json().contains("\"mode\":\"garage\""));
    }

    #[test]
    fn dragging_in_the_garage_turns_the_view() {
        let mut g = game();
        g.advance(0, 0, 0, 0);
        let a = g.orbit;
        g.advance(1, bit("drag"), 5_000, 0);
        assert_ne!(g.orbit, a);
    }
}
