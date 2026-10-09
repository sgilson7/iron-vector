//! The data files the core reads: colours and scenarios. Each is parsed and
//! checked here, so a mistake in a data file is an error the page can show
//! rather than a wrong-looking scene.

use crate::fx::ONE;
use crate::map::MapSpec;
use serde::Deserialize;

pub type Rgb = [i32; 3];

/// A paint scheme: the five paints a mech is drawn in.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scheme {
    pub name: String,
    pub primary: Rgb,
    pub secondary: Rgb,
    pub dark: Rgb,
    pub glow: Rgb,
    pub accent: Rgb,
}

impl Scheme {
    /// The paints in `Paint` order, as Q16.
    pub fn paints(&self) -> [[i32; 3]; 5] {
        [self.primary, self.secondary, self.dark, self.glow, self.accent].map(q16)
    }

    fn channels(&self) -> [Rgb; 5] {
        [self.primary, self.secondary, self.dark, self.glow, self.accent]
    }
}

/// A colour as the page writes it in CSS.
pub fn hex(c: Rgb) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        c[0].clamp(0, 255),
        c[1].clamp(0, 255),
        c[2].clamp(0, 255)
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Palette {
    #[serde(rename = "_note", default)]
    pub note: String,
    pub schemes: Vec<Scheme>,
    pub enemy: Scheme,
    /// what giant frames are painted in, so they stand out against any sky
    pub giant: Scheme,
    /// the garage bay; each planet brings its own look
    pub hangar: crate::campaign::Look,
    pub shadow: Rgb,
    pub flame: Rgb,
    pub flame_qb: Rgb,
    pub tracer: Rgb,
    pub missile: Rgb,
    pub shell: Rgb,
    pub enemy_shot: Rgb,
    pub blast: Rgb,
    pub drone: Rgb,
    pub beacon: Rgb,
    pub drone_eye: Rgb,
    pub tank: Rgb,
    pub turret: Rgb,
    pub checkpoint: Rgb,
    pub boost: Rgb,
    pub switch: Rgb,
    pub hazard: Rgb,
    pub hazard_trim: Rgb,
    pub searchlight: Rgb,
    pub shield: Rgb,
    pub resupply: Rgb,
    pub lava: Rgb,
    pub slash: Rgb,
    pub plasma: Rgb,
    /// the cockpit and its star map
    pub cockpit: Rgb,
    pub cockpit_light: Rgb,
    pub hologram: Rgb,
    pub locked: Rgb,
    pub open: Rgb,
    pub cleared: Rgb,
    pub plus: Rgb,
}

/// A colour in Q16, each channel 0 to ONE.
pub fn q16(c: Rgb) -> [i32; 3] {
    [c[0] * ONE / 255, c[1] * ONE / 255, c[2] * ONE / 255]
}

impl Palette {
    pub fn parse(json: &str) -> Result<Palette, String> {
        let p: Palette = serde_json::from_str(json).map_err(|e| format!("palette: {e}"))?;
        let schemes = p.schemes.iter().chain([&p.enemy]).flat_map(|s| s.channels());
        for c in schemes.chain(p.hangar.buildings.iter().copied()) {
            if c.iter().any(|v| !(0..=255).contains(v)) {
                return Err(format!("palette: a colour channel is outside 0-255: {c:?}"));
            }
        }
        if p.schemes.is_empty() {
            return Err("palette: no paint schemes".into());
        }
        if p.hangar.buildings.is_empty() {
            return Err("palette: no building colours".into());
        }
        Ok(p)
    }

    /// A scheme by number; an out-of-range number gets the first.
    pub fn scheme(&self, i: usize) -> &Scheme {
        self.schemes.get(i).unwrap_or(&self.schemes[0])
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
    fn a_scheme_missing_a_paint_is_refused() {
        let mut v: serde_json::Value =
            serde_json::from_str(include_str!("../../../data/palette.json")).unwrap();
        v["schemes"][0].as_object_mut().unwrap().remove("glow");
        assert!(Palette::parse(&v.to_string()).unwrap_err().contains("glow"));
    }

    #[test]
    fn an_unknown_scheme_number_falls_back_to_the_first() {
        let p = palette();
        assert_eq!(p.scheme(999), &p.schemes[0]);
        assert_eq!(hex([255, 168, 60]), "#ffa83c");
    }
}
