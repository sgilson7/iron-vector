//! The data files the core reads: colours and scenarios. Each is parsed and
//! checked here, so a mistake in a data file is an error the page can show
//! rather than a wrong-looking scene.

use crate::fx::ONE;
use crate::map::MapSpec;
use serde::Deserialize;
use std::collections::BTreeMap;

pub type Rgb = [i32; 3];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Palette {
    #[serde(rename = "_note", default)]
    pub note: String,
    pub paints: BTreeMap<String, Rgb>,
    pub enemy_paints: BTreeMap<String, Rgb>,
    pub sky: Rgb,
    pub fog: Rgb,
    pub ground: Rgb,
    pub grid: Rgb,
    pub buildings: Vec<Rgb>,
    pub roof: Rgb,
    pub window: Rgb,
    pub shadow: Rgb,
    pub flame: Rgb,
    pub flame_qb: Rgb,
    pub tracer: Rgb,
    pub missile: Rgb,
    pub shell: Rgb,
    pub enemy_shot: Rgb,
    pub blast: Rgb,
    pub drone: Rgb,
    pub drone_eye: Rgb,
    pub light_dir: [i32; 3],
}

/// A colour in Q16, each channel 0 to ONE.
pub fn q16(c: Rgb) -> [i32; 3] {
    [c[0] * ONE / 255, c[1] * ONE / 255, c[2] * ONE / 255]
}

impl Palette {
    pub fn parse(json: &str) -> Result<Palette, String> {
        let p: Palette = serde_json::from_str(json).map_err(|e| format!("palette: {e}"))?;
        let all = p
            .paints
            .values()
            .chain(p.enemy_paints.values())
            .chain(p.buildings.iter());
        for c in all {
            if c.iter().any(|v| !(0..=255).contains(v)) {
                return Err(format!("palette: a colour channel is outside 0-255: {c:?}"));
            }
        }
        for name in crate::model::Paint::NAMES {
            if !p.paints.contains_key(name) || !p.enemy_paints.contains_key(name) {
                return Err(format!("palette: no paint called {name}"));
            }
        }
        if p.buildings.is_empty() {
            return Err("palette: no building colours".into());
        }
        Ok(p)
    }

    /// The five paints in `Paint` order, as Q16.
    pub fn paint_set(&self, enemy: bool) -> [[i32; 3]; 5] {
        let src = if enemy {
            &self.enemy_paints
        } else {
            &self.paints
        };
        crate::model::Paint::NAMES.map(|n| q16(src[n]))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proving {
    pub map: MapSpec,
    pub spawn: [i32; 2],
    pub spawn_yaw_deg: i32,
    pub drones: i32,
    pub drone_seed: u64,
    pub drone_respawn_ms: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Missions {
    pub proving: Proving,
}

impl Missions {
    pub fn parse(json: &str) -> Result<Missions, String> {
        serde_json::from_str(json).map_err(|e| format!("missions: {e}"))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn palette() -> Palette {
        Palette::parse(include_str!("../../../data/palette.json")).unwrap()
    }

    pub fn missions() -> Missions {
        Missions::parse(include_str!("../../../data/missions.json")).unwrap()
    }

    #[test]
    fn the_shipped_palette_and_missions_parse() {
        palette();
        missions();
    }

    #[test]
    fn white_is_one_in_q16() {
        assert_eq!(q16([255, 0, 255]), [ONE, 0, ONE]);
    }

    #[test]
    fn a_palette_missing_a_paint_is_refused() {
        let text =
            include_str!("../../../data/palette.json").replace("\"glow\": [255, 168, 60],", "");
        assert!(Palette::parse(&text).unwrap_err().contains("glow"));
    }
}
