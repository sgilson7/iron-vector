//! The door the shim calls through: build a game from the data files, then
//! each display frame hand it the clock, the held keys and the mouse movement,
//! and read back what to draw.
//!
//! The game is in one mode at a time: the garage, the star map in the
//! cockpit, a sortie, the test field, or the debrief. Only a sortie and the
//! test field run the simulation, at a fixed 60 ticks a second; `advance`
//! runs as many ticks as the elapsed time owes, and the drawing is placed
//! between the last two ticks. Mouse look is applied on every frame, before
//! any tick, so the view never waits for one.

use crate::campaign::{Campaign, Look, Progress};
use crate::content::{q16, Missions, Palette};
use crate::fx::{self, deg, int, ONE};
use crate::garage::Garage;
use crate::geom::{facing, v3, Affine, V3};
use crate::mech::{Controls, TICKS_PER_SECOND};
use crate::mesh::{self, Mesh};
use crate::mission::{Debrief, Units};
use crate::parts::{Catalog, Loadout, Slot};
use crate::pilot::Pilots;
use crate::render::{self, Camera, Hud};
use crate::starmap::{self, Power, Selection};
use crate::world::{World, WEAPON_SLOTS};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

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
/// The sweep into the cockpit: the camera's flight, then the lights, then the map.
pub const SWEEP_US: i64 = 1_400_000;
const LIGHTS_US: i64 = 500_000;
const GROW_US: i64 = 700_000;
pub const COCKPIT_US: i64 = SWEEP_US + GROW_US;
const COCKPIT_PITCH: i32 = -deg(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Garage,
    Select,
    Sortie,
    Test,
    Debrief,
}

/// What is saved between visits: the loadout, the paint scheme, the progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Saved {
    loadout: BTreeMap<Slot, String>,
    paint: usize,
    #[serde(default)]
    progress: Progress,
}

/// A save made sense of against this build: a part, paint or mission it
/// names that no longer exists, or a part not yet won, falls back.
fn restore(saved: Option<Saved>, cat: &Catalog, pal: &Palette, c: &Campaign) -> (Loadout, usize, Progress) {
    let (mut loadout, paint, progress) = match saved {
        Some(s) => (
            Loadout::restore(&serde_json::to_string(&s.loadout).unwrap_or_default(), cat),
            s.paint.min(pal.schemes.len() - 1),
            s.progress.clean(c),
        ),
        None => (cat.default_loadout(), 0, Progress::default()),
    };
    let unlocked = progress.unlocked(c, cat);
    for (slot, id) in cat.default_loadout().0 {
        if !unlocked.contains(&loadout.0[&slot]) {
            loadout.0.insert(slot, id);
        }
    }
    (loadout, paint, progress)
}

/// A place on screen the page should make clickable on the star map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hotspot {
    pub x: i32,
    pub y: i32,
    pub planet: usize,
    pub mission: Option<usize>,
    pub label: String,
}

pub struct Game {
    cat: Catalog,
    pal: Palette,
    missions: Missions,
    campaign: Campaign,
    pilots: Pilots,
    units: Units,
    loadout: Loadout,
    paint: usize,
    progress: Progress,
    unlocked: BTreeSet<String>,
    mode: Mode,
    world: World,
    look: Look,
    garage: Garage,
    select: Selection,
    /// microseconds into the sweep, and which way it runs: 1 in, −1 out
    cockpit_us: i64,
    cockpit_dir: i64,
    clock: i32,
    flying: Option<String>,
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
    hotspots: Vec<Hotspot>,
}

impl Game {
    pub fn new(
        parts_json: &str,
        palette_json: &str,
        missions_json: &str,
        planets_json: &str,
        pilots_json: &str,
        units_json: &str,
        saved: &str,
    ) -> Result<Game, String> {
        let cat = Catalog::parse(parts_json).map_err(|e| format!("parts: {}", e.0))?;
        let pal = Palette::parse(palette_json)?;
        let missions = Missions::parse(missions_json)?;
        let campaign = Campaign::parse(planets_json, &cat)?;
        let pilots = Pilots::parse(pilots_json)?;
        let units = Units::parse(units_json)?;
        for m in campaign.planets.iter().flat_map(|p| &p.missions) {
            if let Some(ms) = m.mechs.iter().find(|ms| !pilots.trees.contains_key(&ms.pilot)) {
                return Err(format!("planets: {} names no pilot tree {}", m.id, ms.pilot));
            }
            if let Some(u) = m.units.iter().find(|u| !units.0.contains_key(&u.unit)) {
                return Err(format!("planets: {} names no unit {}", m.id, u.unit));
            }
        }
        let (loadout, paint, progress) =
            restore(serde_json::from_str::<Saved>(saved).ok(), &cat, &pal, &campaign);
        let unlocked = progress.unlocked(&campaign, &cat);
        let world = World::hangar(&cat, &loadout, pal.scheme(paint).paints());
        let look = pal.hangar.clone();
        let mut g = Game {
            cat,
            pal,
            missions,
            campaign,
            pilots,
            units,
            loadout,
            paint,
            progress,
            unlocked,
            mode: Mode::Garage,
            world,
            look,
            garage: Garage::default(),
            select: Selection::default(),
            cockpit_us: 0,
            cockpit_dir: 0,
            clock: 0,
            flying: None,
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
            hotspots: Vec::new(),
        };
        g.enter(Mode::Garage);
        Ok(g)
    }

    /// Switches mode, building the world that mode shows.
    fn enter(&mut self, mode: Mode) {
        let paint = self.pal.scheme(self.paint).paints();
        match mode {
            Mode::Garage | Mode::Select => {
                self.world = World::hangar(&self.cat, &self.loadout, paint);
                self.look = self.pal.hangar.clone();
            }
            Mode::Sortie => {
                let id = self.flying.clone().unwrap_or_default();
                let Some((p, i)) = self.campaign.find(&id) else {
                    return;
                };
                let planet = &self.campaign.planets[p];
                self.world = World::mission(
                    &self.cat,
                    &self.loadout,
                    paint,
                    planet,
                    &planet.missions[i],
                    &self.pilots,
                    &self.units,
                    self.pal.enemy.paints(),
                    self.pal.giant.paints(),
                );
                self.look = planet.look.clone();
            }
            Mode::Test => {
                self.world = World::proving(
                    &self.cat,
                    &self.loadout,
                    &self.missions.proving,
                    paint,
                    &self.units,
                );
                self.look = self.campaign.planets[0].look.clone();
            }
            Mode::Debrief => {}
        }
        if mode != Mode::Debrief {
            self.debrief = None;
            let (values, draws) = render::scene(&self.world, &self.look);
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
            self.clock = (self.clock + (dt * fx::TURN as i64 / 12_000_000) as i32) & (fx::TURN - 1);
            if self.mode == Mode::Garage {
                let drag = if bits & bit("drag") != 0 {
                    dx * LOOK_PER_PIXEL / HUNDREDTHS
                } else {
                    0
                };
                let spin = (dt * ORBIT_PER_SECOND as i64 / 1_000_000) as i32;
                self.orbit = (self.orbit + spin - drag) & (fx::TURN - 1);
            }
            if self.mode == Mode::Select {
                self.cockpit_us = (self.cockpit_us + dt * self.cockpit_dir).clamp(0, COCKPIT_US);
                if self.cockpit_dir < 0 && self.cockpit_us == 0 {
                    self.cockpit_dir = 0;
                    self.mode = Mode::Garage;
                }
            }
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
        if self.world.mission.as_ref().is_some_and(|m| m.finished()) {
            self.finish_mission();
        }
        self.draw();
    }

    /// Records the run, unlocks what it won, and shows the debrief.
    fn finish_mission(&mut self) {
        let Some(m) = self.world.mission.clone() else {
            return;
        };
        let revealed = self.progress.cleared.contains(&m.id);
        let success = m.success == Some(true);
        let plus_met = revealed && m.plus_met(&self.world);
        let won = self.progress.record(&self.campaign, &m.id, success, plus_met);
        self.unlocked = self.progress.unlocked(&self.campaign, &self.cat);
        let names = won
            .into_iter()
            .map(|(id, plus)| (self.cat.get(&id).map(|p| p.name.clone()).unwrap_or(id), plus))
            .collect();
        // the debrief reveals the plus challenge once the mission has been cleared
        let now_revealed = self.progress.cleared.contains(&m.id);
        self.debrief = Some(m.debrief(&self.world, now_revealed, names));
        self.mode = Mode::Debrief;
        self.paused = false;
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

    fn orbit_camera(&self) -> (V3, i32, i32) {
        let pivot = v3(0, ORBIT_PIVOT_Y, 0);
        (
            pivot.sub(facing(self.orbit, ORBIT_PITCH).scale(ORBIT_DISTANCE)),
            self.orbit,
            ORBIT_PITCH,
        )
    }

    fn cockpit_eye(&self) -> V3 {
        self.world.player().body.pos.add(self.world.player().rig.eye())
    }

    /// How far the sweep has come, each part 0 to ONE: the camera's flight,
    /// the lights, the map.
    fn sweep(&self) -> (i32, i32, i32) {
        let part =
            |from: i64, len: i64| (((self.cockpit_us - from) * ONE as i64) / len).clamp(0, ONE as i64) as i32;
        let t = part(0, SWEEP_US);
        // ease in and out: 3t² − 2t³
        let eased = fx::mul(fx::mul(t, t), 3 * ONE - 2 * t);
        (eased, part(SWEEP_US * 4 / 5, LIGHTS_US), part(SWEEP_US, GROW_US))
    }

    fn camera(&self) -> Camera {
        match self.mode {
            Mode::Garage => {
                let (eye, yaw, pitch) = self.orbit_camera();
                Camera::from_angles(eye, yaw, pitch)
            }
            Mode::Select => {
                let (eye0, yaw0, pitch0) = self.orbit_camera();
                let (t, _, _) = self.sweep();
                let eye = eye0.lerp(self.cockpit_eye(), t);
                Camera::from_angles(
                    eye,
                    fx::lerp_angle(yaw0, 0, t),
                    fx::lerp(pitch0, COCKPIT_PITCH, t),
                )
            }
            _ => render::player_camera(&self.world, self.alpha),
        }
    }

    fn draw(&mut self) {
        let aspect = fx::div(self.css.0, self.css.1).max(1);
        let cam = self.camera();
        let vp = render::view_projection_for(&cam, aspect, &self.look);
        let mut view = vp.0.to_vec();
        view.extend([cam.eye.x, cam.eye.y, cam.eye.z]);
        self.view = view;
        let (t, light, grow) = self.sweep();
        let inside = self.mode == Mode::Select && t > ONE * 9 / 10;
        let mut batch = render::frame(&self.world, &self.look, &self.pal, self.alpha, inside);
        self.hotspots.clear();
        if inside {
            let frame = Affine::translate(self.cockpit_eye()).then(&Affine::rot_x(COCKPIT_PITCH));
            let power = Power {
                light,
                grow,
                clock: self.clock,
            };
            let placed = starmap::push_cockpit(
                &mut batch,
                &frame,
                power,
                &self.campaign,
                &self.progress,
                &self.select,
                &self.pal,
            );
            if grow > ONE * 9 / 10 {
                let (nodes, _) = starmap::layout_for(&self.campaign, Some(&self.progress));
                for (n, p) in nodes.iter().zip(placed) {
                    if let Some((x, y)) = render::to_screen(&vp, p, self.css.0, self.css.1) {
                        let planet = &self.campaign.planets[n.planet];
                        let label = match n.mission {
                            None => planet.name.clone(),
                            Some(i) => planet.missions[i].name.clone(),
                        };
                        self.hotspots.push(Hotspot {
                            x,
                            y,
                            planet: n.planet,
                            mission: n.mission,
                            label,
                        });
                    }
                }
            }
        }
        let (values, draws) = batch.finish(render::DYNAMIC_BUFFER);
        self.values = values;
        self.draws = draws;
        let revealed = self
            .flying
            .as_ref()
            .is_some_and(|id| self.progress.cleared.contains(id))
            && self.mode == Mode::Sortie;
        let mut hud = render::hud(
            &self.world,
            &vp,
            &cam,
            self.css,
            &self.names,
            self.paused,
            revealed,
        );
        hud.mode = self.mode;
        hud.debrief = self.debrief.clone();
        self.hud = Some(hud);
    }

    // ---- garage ----

    pub fn garage_select(&mut self, slot: usize) {
        self.garage.select(slot);
        self.preview();
    }

    /// Hovering a part fits it to the preview frame at once, so it is seen before it is chosen.
    pub fn garage_hover(&mut self, part: i32) {
        self.garage.hover = usize::try_from(part).ok();
        self.preview();
    }

    fn preview(&mut self) {
        if self.mode != Mode::Garage {
            return;
        }
        let shown = self.garage.candidate(&self.loadout, &self.cat);
        let paint = self.pal.scheme(self.paint).paints();
        let mut w = World::hangar(&self.cat, &shown, paint);
        w.mechs[0].body.walk_phase = self.world.mechs[0].body.walk_phase;
        self.world = w;
        self.draw();
    }

    pub fn garage_equip(&mut self, part: usize) {
        if self
            .garage
            .equip(part, &mut self.loadout, &self.cat, &self.unlocked)
        {
            self.garage.hover = None;
            self.enter(Mode::Garage);
        }
    }

    pub fn garage_paint(&mut self, scheme: usize) {
        self.paint = scheme.min(self.pal.schemes.len() - 1);
        self.enter(Mode::Garage);
    }

    pub fn garage_json(&self) -> String {
        let v = self.garage.view(
            &self.loadout,
            self.paint,
            &self.cat,
            &self.pal,
            &self.unlocked,
            &self.campaign,
        );
        serde_json::to_string(&v).unwrap_or_default()
    }

    /// The loadout, paint and progress as a string for the page to keep.
    pub fn saved(&self) -> String {
        let s = Saved {
            loadout: self.loadout.0.clone(),
            paint: self.paint,
            progress: self.progress.clone(),
        };
        serde_json::to_string(&s).unwrap_or_default()
    }

    /// Replaces the game with a save the player brought, and goes to the
    /// garage. A save that cannot be read changes nothing and says so.
    pub fn load_save(&mut self, text: &str) -> Result<(), String> {
        let s = serde_json::from_str::<Saved>(text).map_err(|_| "save_unreadable".to_string())?;
        self.adopt(Some(s));
        Ok(())
    }

    /// Starts over: the starting frame, the first paint, nothing cleared.
    pub fn restart(&mut self) {
        self.adopt(None);
    }

    /// A save with every mission cleared and every plus met, in the current
    /// frame and paint: for trying anything without earning it.
    pub fn everything_save(&self) -> String {
        let all: BTreeSet<String> = self
            .campaign
            .planets
            .iter()
            .flat_map(|p| p.missions.iter().map(|m| m.id.clone()))
            .collect();
        let s = Saved {
            loadout: self.loadout.0.clone(),
            paint: self.paint,
            progress: Progress {
                cleared: all.clone(),
                plus: all,
            },
        };
        serde_json::to_string(&s).unwrap_or_default()
    }

    fn adopt(&mut self, s: Option<Saved>) {
        let (loadout, paint, progress) = restore(s, &self.cat, &self.pal, &self.campaign);
        self.loadout = loadout;
        self.paint = paint;
        self.progress = progress;
        self.unlocked = self.progress.unlocked(&self.campaign, &self.cat);
        self.flying = None;
        self.debrief = None;
        self.select = Selection::default();
        self.garage = Garage::default();
        self.enter(Mode::Garage);
    }

    // ---- the star map ----

    /// From the garage into the cockpit, or from the debrief straight to the map.
    pub fn star_map(&mut self) {
        let from_debrief = self.mode == Mode::Debrief;
        self.enter(Mode::Select);
        self.cockpit_dir = 1;
        self.cockpit_us = if from_debrief { COCKPIT_US } else { 0 };
    }

    /// Back out of the cockpit to the garage, the sweep in reverse.
    pub fn leave_star_map(&mut self) {
        if self.mode == Mode::Select {
            self.cockpit_dir = -1;
        }
    }

    pub fn select_planet(&mut self, planet: usize) {
        let p = planet.min(self.campaign.planets.len() - 1);
        if self.progress.planet_shown(&self.campaign, p) {
            self.select.planet = p;
            self.select.mission = None;
        }
    }

    pub fn select_mission(&mut self, planet: usize, mission: usize) {
        self.select_planet(planet);
        let n = self.campaign.planets[self.select.planet].missions.len();
        self.select.mission = (mission < n).then_some(mission);
    }

    pub fn select_hover(&mut self, node: i32) {
        self.select.hover = usize::try_from(node).ok();
    }

    /// Flies the chosen mission, if it is open.
    pub fn launch(&mut self) {
        let Some(i) = self.select.mission else { return };
        let id = self.campaign.planets[self.select.planet].missions[i].id.clone();
        if self.progress.can_fly(&self.campaign, &id) {
            self.flying = Some(id);
            self.enter(Mode::Sortie);
        }
    }

    /// Flies the last mission again.
    pub fn retry(&mut self) {
        if self.flying.is_some() {
            self.enter(Mode::Sortie);
        }
    }

    pub fn test_field(&mut self) {
        self.flying = None;
        self.enter(Mode::Test);
    }

    pub fn to_garage(&mut self) {
        self.enter(Mode::Garage);
    }

    /// Everything the star map's panel shows.
    pub fn star_map_json(&self) -> String {
        let c = &self.campaign;
        let part_name = |id: &str| self.cat.get(id).map(|p| p.name.clone()).unwrap_or_default();
        let planets: Vec<_> = c
            .planets
            .iter()
            .enumerate()
            .map(|(p, pl)| {
                let missions: Vec<_> = pl
                    .missions
                    .iter()
                    .enumerate()
                    .map(|(i, m)| {
                        let standing = self.progress.standing(c, &m.id);
                        let revealed = self.progress.cleared.contains(&m.id);
                        json!({
                            "name": m.name,
                            "scene": m.scene,
                            "kind": format!("kind_{}", serde_json::to_value(m.kind).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default()),
                            "standing": standing,
                            "reward": part_name(&m.reward),
                            "plus": revealed.then(|| json!({"rule": m.plus.rule, "value": m.plus.value, "reward": part_name(&m.plus.reward)})),
                            "requires": m.requires.iter().filter_map(|r| c.mission(r).map(|x| x.name.clone())).collect::<Vec<_>>(),
                                                        "level": c.level(p, i),
                            "plus_met": self.progress.plus.contains(&m.id),
                        })
                    })
                    .collect();
                json!({
                    "name": pl.name,
                    "blurb": pl.blurb,
                    "gimmick": pl.gimmick,
                    "opens_at": pl.opens_at,
                                        "open": self.progress.planet_open(c, p),
                    "cleared": pl.missions.iter().filter(|m| self.progress.cleared.contains(&m.id)).count(),
                    "plus": pl.missions.iter().filter(|m| self.progress.plus.contains(&m.id)).count(),
                    "missions": missions,
                })
            })
            .collect();
        json!({
            "planets": planets,
            "planet": self.select.planet,
            "mission": self.select.mission,
            "cleared": self.progress.cleared.len(),
            "plus": self.progress.plus.len(),
                        "total": c.total_missions(),
            // a hidden planet still to find: how many plus challenges it wants
            "hidden_need": c.planets.iter().enumerate()
                .find(|(p, pl)| pl.hidden && !self.progress.planet_open(c, *p))
                .map(|(_, pl)| pl.opens_at_plus),
            "ready": self.mode == Mode::Select && self.sweep().2 > ONE * 9 / 10,
        })
        .to_string()
    }

    pub fn hotspots_json(&self) -> String {
        serde_json::to_string(&self.hotspots).unwrap_or_default()
    }

    // ---- what the page reads ----

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

    /// The colour behind everything: the current look's fog.
    pub fn clear_color(&self) -> Vec<i32> {
        q16(self.look.fog).to_vec()
    }

    pub fn scene_uniforms(&self) -> Vec<i32> {
        render::scene_uniforms(&self.look)
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn progress(&self) -> &Progress {
        &self.progress
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

    pub fn game_with(saved: &str) -> Game {
        Game::new(
            include_str!("../../../data/parts.json"),
            include_str!("../../../data/palette.json"),
            include_str!("../../../data/missions.json"),
            include_str!("../../../data/planets.json"),
            include_str!("../../../data/pilots.json"),
            include_str!("../../../data/units.json"),
            saved,
        )
        .unwrap()
    }

    pub fn game() -> Game {
        game_with("")
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
        let speed = hud["speed"].as_i64().unwrap();
        assert!((90..=96).contains(&speed), "walking 95 km/h: {speed}");
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
        assert_eq!(n["meshes"].as_array().unwrap().len(), Mesh::ALL.len());
        assert_eq!(n["meshes"][0]["vertices"], 36);
    }

    #[test]
    fn hovering_a_part_shows_it_on_the_frame_and_leaving_puts_it_back() {
        let mut g = game();
        let legs = Slot::ALL.iter().position(|s| *s == Slot::Legs).unwrap();
        g.garage_select(legs);
        let before = g.world().player().rig.clone();
        g.garage_hover(1);
        assert_ne!(
            g.world().player().rig,
            before,
            "the hovered legs are on the frame"
        );
        g.garage_hover(-1);
        assert_eq!(g.world().player().rig, before);
    }

    #[test]
    fn a_locked_part_cannot_be_fitted_from_the_page() {
        let mut g = game();
        let legs = Slot::ALL.iter().position(|s| *s == Slot::Legs).unwrap();
        g.garage_select(legs);
        let l = g.loadout.clone();
        g.garage_equip(1);
        assert_eq!(g.loadout, l, "lg-wisp is not won yet");
    }

    #[test]
    fn the_sweep_flies_the_camera_into_the_cockpit_then_lights_it_and_grows_the_map() {
        let mut g = game();
        g.resize(1600, 900, 100);
        g.star_map();
        g.advance(0, 0, 0, 0);
        assert_eq!(g.sweep(), (0, 0, 0));
        assert!(g.hotspots.is_empty());
        let mut t = 0;
        while t < COCKPIT_US + 100_000 {
            t += FRAME_60;
            g.advance(t, 0, 0, 0);
        }
        let (fly, light, grow) = g.sweep();
        assert_eq!((fly, light, grow), (ONE, ONE, ONE));
        assert!(
            g.camera().eye.dist(g.cockpit_eye()) < ONE / 64,
            "the camera ends at the pilot's eye"
        );
        let nodes = starmap::layout_for(&g.campaign, Some(&g.progress)).0.len();
        assert_eq!(g.hotspots.len(), nodes, "every node is on screen");
        assert!(g
            .hotspots
            .iter()
            .all(|h| h.x > 0 && h.x < 1600 && h.y > 0 && h.y < 900));
        g.leave_star_map();
        while g.mode() == Mode::Select {
            t += FRAME_60;
            g.advance(t, 0, 0, 0);
        }
        assert_eq!(g.mode(), Mode::Garage);
    }

    #[test]
    fn a_locked_mission_cannot_be_launched_and_an_open_one_can() {
        let mut g = game();
        g.star_map();
        g.select_mission(0, 2);
        g.launch();
        assert_eq!(g.mode(), Mode::Select, "halden-3 needs halden-1");
        g.select_mission(0, 0);
        g.launch();
        assert_eq!(g.mode(), Mode::Sortie);
        assert_eq!(g.world().mission.as_ref().unwrap().id, "halden-1");
    }

    #[test]
    fn the_everything_save_opens_every_mission_and_part() {
        let mut g = game();
        let all = g.everything_save();
        g.load_save(&all).unwrap();
        assert_eq!(g.mode(), Mode::Garage);
        let total: usize = g.campaign.planets.iter().map(|p| p.missions.len()).sum();
        assert_eq!(g.progress().plus.len(), total);
        assert!(
            (0..g.campaign.planets.len()).all(|p| g.progress().planet_open(&g.campaign, p)),
            "the hidden planet too"
        );
        assert!(
            g.cat.parts.iter().all(|p| g.unlocked.contains(&p.id)),
            "every part is won"
        );
    }

    #[test]
    fn the_shipped_test_save_is_the_everything_save() {
        let file = include_str!("../../../data/saves/everything-unlocked.json");
        assert_eq!(
            file.trim(),
            game().everything_save(),
            "regenerate data/saves/everything-unlocked.json"
        );
    }

    #[test]
    fn a_save_round_trips_and_restart_wipes_it() {
        let mut g = game();
        g.load_save(&g.everything_save()).unwrap();
        let kept = g.saved();
        g.restart();
        assert!(g.progress().cleared.is_empty());
        assert_ne!(g.saved(), kept);
        g.load_save(&kept).unwrap();
        assert_eq!(g.saved(), kept);
    }

    #[test]
    fn an_unreadable_save_changes_nothing() {
        let mut g = game();
        let before = g.saved();
        assert_eq!(g.load_save("not a save"), Err("save_unreadable".to_string()));
        assert_eq!(g.saved(), before);
    }

    #[test]
    fn a_won_mission_unlocks_its_part_and_the_save_remembers() {
        let mut g = game();
        g.star_map();
        g.select_mission(0, 0);
        g.launch();
        for i in 0..g.world.craft.len() {
            g.world.hurt(crate::combat::Target::Craft(i), 1_000_000, 0);
        }
        let mut t = 0;
        g.advance(t, FOCUS, 0, 0);
        while g.mode() == Mode::Sortie {
            t += FRAME_60;
            g.advance(t, FOCUS, 0, 0);
            assert!(t < 10_000_000);
        }
        assert_eq!(g.mode(), Mode::Debrief);
        let d = g.debrief.clone().unwrap();
        assert!(d.success);
        assert_eq!(d.rewards, vec![("HD-07 KESTREL".to_string(), false)]);
        assert!(
            d.plus.is_some(),
            "the plus challenge is revealed by the first clear"
        );
        let again = game_with(&g.saved());
        assert!(again.progress.cleared.contains("halden-1"));
        assert!(again.unlocked.contains("hd-kestrel"));
    }

    #[test]
    fn a_saved_build_that_fits_a_part_no_longer_won_falls_back() {
        let saved = r#"{"loadout":{"head":"hd-kestrel"},"paint":1,"progress":{"cleared":[]}}"#;
        let g = game_with(saved);
        assert_eq!(g.loadout.0[&Slot::Head], "hd-warden");
        assert_eq!(g.paint, 1);
    }

    #[test]
    fn the_second_planet_opens_on_the_map_after_two_clears() {
        let saved = r#"{"loadout":{},"paint":0,"progress":{"cleared":["halden-1","halden-2"]}}"#;
        let g = game_with(saved);
        let v: serde_json::Value = serde_json::from_str(&g.star_map_json()).unwrap();
        assert_eq!(v["planets"][1]["open"], true);
        assert_eq!(v["planets"][2]["open"], false);
        assert_eq!(v["planets"][1]["missions"][0]["standing"], "open");
        assert_eq!(v["cleared"], 2);
    }
}
