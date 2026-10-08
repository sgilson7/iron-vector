//! iv_shim: moves values between the page and `iv_core::game`.
//!
//! Builder requirement: the shim decides nothing. Each function is one call
//! into the core and a conversion of its arguments or result. The core speaks
//! Q16.16 integers; WebGL wants `f32`, so values on the way out are divided by
//! the core's own `ONE`, and the page's float clock and mouse movement are
//! turned into the integers the core expects. `crates/core/tests/boundary.rs`
//! fails if a branch, a loop or the `?` operator appears here.

use iv_core::{fx, game};
use wasm_bindgen::prelude::*;

const ONE: f32 = fx::ONE as f32;

fn floats(v: Vec<i32>) -> Vec<f32> {
    v.into_iter().map(|x| x as f32 / ONE).collect()
}

#[wasm_bindgen]
pub struct Game {
    inner: game::Game,
}

#[wasm_bindgen]
impl Game {
    #[wasm_bindgen(constructor)]
    pub fn new(
        parts: &str,
        palette: &str,
        missions: &str,
        planets: &str,
        pilots: &str,
        units: &str,
        saved: &str,
    ) -> Result<Game, JsValue> {
        game::Game::new(parts, palette, missions, planets, pilots, units, saved)
            .map(|inner| Game { inner })
            .map_err(|e| JsValue::from_str(&e))
    }

    /// CSS size and device pixel ratio in; drawing-buffer size out.
    pub fn resize(&mut self, css_w: u32, css_h: u32, dpr: f64) -> Vec<i32> {
        self.inner
            .resize(css_w as i32, css_h as i32, (dpr * 100.0) as i32)
            .to_vec()
    }

    /// The page's clock in milliseconds, held action bits, mouse movement in CSS pixels.
    pub fn advance(&mut self, now_ms: f64, bits: u32, dx: f64, dy: f64) {
        self.inner.advance(
            (now_ms * 1000.0) as i64,
            bits,
            (dx * 100.0) as i32,
            (dy * 100.0) as i32,
        )
    }

    pub fn view(&self) -> Vec<f32> {
        floats(self.inner.view())
    }

    pub fn instances(&self) -> Vec<f32> {
        floats(self.inner.instances())
    }

    pub fn static_instances(&self) -> Vec<f32> {
        floats(self.inner.static_instances())
    }

    pub fn draws(&self) -> Vec<u32> {
        self.inner.draws()
    }

    pub fn hud(&self) -> String {
        self.inner.hud_json()
    }

    pub fn uniforms(&self) -> Vec<f32> {
        floats(self.inner.scene_uniforms())
    }

    pub fn clear_color(&self) -> Vec<f32> {
        floats(self.inner.clear_color())
    }

    pub fn scene_version(&self) -> u32 {
        self.inner.scene_version()
    }

    pub fn garage(&self) -> String {
        self.inner.garage_json()
    }

    pub fn garage_select(&mut self, slot: u32) {
        self.inner.garage_select(slot as usize)
    }

    /// The part under the pointer, or −1 for none.
    pub fn garage_hover(&mut self, part: i32) {
        self.inner.garage_hover(part)
    }

    pub fn garage_equip(&mut self, part: u32) {
        self.inner.garage_equip(part as usize)
    }

    pub fn garage_paint(&mut self, scheme: u32) {
        self.inner.garage_paint(scheme as usize)
    }

    /// The loadout, paint and progress, for the page to keep in local storage.
    pub fn saved(&self) -> String {
        self.inner.saved()
    }

    pub fn star_map(&mut self) {
        self.inner.star_map()
    }

    pub fn leave_star_map(&mut self) {
        self.inner.leave_star_map()
    }

    pub fn star_map_info(&self) -> String {
        self.inner.star_map_json()
    }

    pub fn hotspots(&self) -> String {
        self.inner.hotspots_json()
    }

    pub fn select_planet(&mut self, planet: u32) {
        self.inner.select_planet(planet as usize)
    }

    pub fn select_mission(&mut self, planet: u32, mission: u32) {
        self.inner.select_mission(planet as usize, mission as usize)
    }

    /// The map node under the pointer, or −1 for none.
    pub fn select_hover(&mut self, node: i32) {
        self.inner.select_hover(node)
    }

    pub fn launch(&mut self) {
        self.inner.launch()
    }

    pub fn retry(&mut self) {
        self.inner.retry()
    }

    pub fn test_field(&mut self) {
        self.inner.test_field()
    }

    pub fn to_garage(&mut self) {
        self.inner.to_garage()
    }
}

#[wasm_bindgen]
pub fn numbers() -> String {
    game::numbers()
}

#[wasm_bindgen]
pub fn mesh(id: u32) -> Vec<f32> {
    floats(game::mesh_vertices(id))
}
