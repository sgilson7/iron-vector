//! The door the shim calls through: build a game from the data files, then
//! each display frame hand it the clock, the held keys and the mouse movement,
//! and read back what to draw.
//!
//! The simulation runs at a fixed 60 ticks a second. The page's frame rate is
//! whatever the display gives; `advance` runs as many ticks as the elapsed
//! time owes, and the drawing is placed between the last two ticks. Mouse look
//! is applied on every frame, before any tick, so the view never waits for one.

use crate::content::{Missions, Palette};
use crate::fx::{self, ONE};
use crate::mech::{Controls, TICKS_PER_SECOND};
use crate::mesh::{self, Mesh};
use crate::parts::{Catalog, Loadout};
use crate::render::{self, Hud};
use crate::world::{World, WEAPON_SLOTS};
use serde_json::json;

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

pub struct Game {
    pal: Palette,
    world: World,
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
        saved_loadout: &str,
    ) -> Result<Game, String> {
        let cat = Catalog::parse(parts_json).map_err(|e| format!("parts: {}", e.0))?;
        let pal = Palette::parse(palette_json)?;
        let missions = Missions::parse(missions_json)?;
        let loadout = Loadout::restore(saved_loadout, &cat);
        let world = World::proving(&cat, &loadout, &missions.proving);
        let names = WEAPON_SLOTS.map(|s| loadout.part(s, &cat).name.clone());
        let (static_values, static_draws) = render::scene(&world, &pal);
        let mut g = Game {
            pal,
            world,
            names,
            prev_bits: 0,
            latched: 0,
            last_us: None,
            acc: 0,
            alpha: ONE,
            css: (1, 1),
            pixels: (1, 1),
            paused: true,
            static_values,
            static_draws,
            values: Vec::new(),
            draws: Vec::new(),
            view: Vec::new(),
            hud: None,
        };
        g.draw();
        Ok(g)
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
        let dt = match self.last_us {
            Some(last) => (now_us - last).clamp(0, MAX_FRAME_US),
            None => 0,
        };
        self.last_us = Some(now_us);
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

    fn draw(&mut self) {
        let aspect = fx::div(self.css.0, self.css.1).max(1);
        let (vp, eye) = render::view_projection(&self.world, self.alpha, aspect);
        let mut view = vp.0.to_vec();
        view.extend([eye.x, eye.y, eye.z]);
        self.view = view;
        let (values, draws) = render::frame(&self.world, &self.pal, self.alpha);
        self.values = values;
        self.draws = draws;
        self.hud = Some(render::hud(
            &self.world,
            &vp,
            self.css.0,
            self.css.1,
            &self.names,
            self.paused,
        ));
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
mod tests {
    use super::*;

    fn game() -> Game {
        Game::new(
            include_str!("../../../data/parts.json"),
            include_str!("../../../data/palette.json"),
            include_str!("../../../data/missions.json"),
            "",
        )
        .unwrap()
    }

    const FOCUS: u32 = 1 << 10;
    const FRAME_60: i64 = 16_667;

    #[test]
    fn one_second_of_frames_runs_sixty_ticks_at_any_frame_rate() {
        for fps in [30i64, 60, 144, 240] {
            let mut g = game();
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
        let mut g = game();
        g.advance(0, FOCUS, 0, 0);
        g.advance(2_000_000, FOCUS, 0, 0);
        assert!(g.world().tick <= MAX_TICKS_PER_FRAME);
        assert!((0..=ONE).contains(&g.alpha));
    }

    #[test]
    fn mouse_look_turns_the_aim_on_a_frame_that_runs_no_tick() {
        let mut g = game();
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
        let mut g = game();
        g.advance(0, bit("forward"), 0, 0);
        g.advance(500_000, bit("forward"), 50, 0);
        assert_eq!(g.world().tick, 0);
        assert!(g.hud_json().contains("\"paused\":true"));
    }

    #[test]
    fn a_tap_shorter_than_a_tick_still_quick_boosts() {
        let mut g = game();
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
        let mut g = game();
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
}
