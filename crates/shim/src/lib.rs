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
        saved_loadout: &str,
    ) -> Result<Game, JsValue> {
        game::Game::new(parts, palette, missions, saved_loadout)
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
}

#[wasm_bindgen]
pub fn numbers() -> String {
    game::numbers()
}

#[wasm_bindgen]
pub fn mesh(id: u32) -> Vec<f32> {
    floats(game::mesh_vertices(id))
}
