//! What a mission sets out besides units, frames and buildings: searchlights
//! that raise an alarm, shield domes held up by generators, artillery that
//! marks the ground before it lands, lava that rises, fields that jam a
//! lock, pads that resupply, and the ally a mission may ask you to escort.

use crate::fx::{self, int, ONE};
use crate::geom::{v3, V3};
use crate::mech::TICKS_PER_SECOND;
use serde::Deserialize;

fn ten() -> i32 {
    10
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchlightSpec {
    /// the lamp: x, z, height (m)
    pub at: [i32; 3],
    /// points on the ground its spot sweeps between, in a loop, x, z (m)
    pub path: Vec<[i32; 2]>,
    /// the spot's radius on the ground (m)
    pub r_m: i32,
    pub speed_ms: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Searchlight {
    pub lamp: V3,
    pub path: Vec<V3>,
    pub next: usize,
    pub spot: V3,
    pub prev_spot: V3,
    pub r: i32,
    pub speed: i32,
    /// whether it saw the player last tick
    pub saw: bool,
}

impl Searchlight {
    pub fn new(s: &SearchlightSpec) -> Searchlight {
        let path: Vec<V3> = s.path.iter().map(|[x, z]| v3(int(*x), 0, int(*z))).collect();
        let spot = path.first().copied().unwrap_or(v3(int(s.at[0]), 0, int(s.at[1])));
        Searchlight {
            lamp: v3(int(s.at[0]), int(s.at[2]), int(s.at[1])),
            path,
            next: 1,
            spot,
            prev_spot: spot,
            r: int(s.r_m),
            speed: fx::ratio(s.speed_ms, TICKS_PER_SECOND),
            saw: false,
        }
    }

    /// Sweeps the spot on along its path; once the alarm is up it hunts `chase`.
    pub fn step(&mut self, chase: Option<V3>) {
        self.prev_spot = self.spot;
        let goal = match chase {
            Some(p) => v3(p.x, 0, p.z),
            None if self.path.len() > 1 => self.path[self.next % self.path.len()],
            None => return,
        };
        let to = goal.sub(self.spot);
        let speed = if chase.is_some() {
            self.speed * 3 / 2
        } else {
            self.speed
        };
        if to.len() <= speed {
            self.spot = goal;
            if chase.is_none() {
                self.next = (self.next + 1) % self.path.len();
            }
        } else {
            self.spot = self.spot.add(to.norm().scale(speed));
        }
    }

    /// Whether `p` stands inside the cone of light from the lamp to the spot.
    pub fn lights(&self, p: V3) -> bool {
        let axis = self.spot.sub(self.lamp);
        let v = p.sub(self.lamp);
        let aa =
            axis.x as i64 * axis.x as i64 + axis.y as i64 * axis.y as i64 + axis.z as i64 * axis.z as i64;
        if aa == 0 {
            return false;
        }
        let va = v.x as i64 * axis.x as i64 + v.y as i64 * axis.y as i64 + v.z as i64 * axis.z as i64;
        // how far along the beam, as a fraction of the way to the spot
        let t = ((va << fx::FRAC_BITS) / aa) as i32;
        if t <= 0 || t > ONE * 6 / 5 {
            return false;
        }
        let off = v.sub(axis.scale(t)).len();
        off < fx::mul(self.r, t)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShieldSpec {
    /// the dome's centre, x, z, altitude (m)
    pub at: [i32; 3],
    pub r_m: i32,
    /// the generators that hold it up, x, z, altitude (m); below zero stands it on what is there
    pub generators: Vec<[i32; 3]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shield {
    pub centre: V3,
    pub r: i32,
    /// its generators, as craft
    pub gens: Vec<usize>,
    pub up: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtillerySpec {
    /// a shell is called this often, and lands this long after its mark shows
    pub every_ms: i32,
    pub warn_ms: i32,
    pub radius_m: i32,
    pub damage: i32,
    /// shells start falling at this stage of a staged mission, or at once
    #[serde(default)]
    pub from_stage: usize,
}

/// A shell on its way: where it will land, and ticks until it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Strike {
    pub at: V3,
    pub ticks: i32,
    pub warn: i32,
    pub r: i32,
    pub damage: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LavaSpec {
    pub from_m: i32,
    pub to_m: i32,
    pub over_s: i32,
    /// AP a second lost below its surface
    pub dps: i32,
}

impl LavaSpec {
    /// The lava's surface after `ticks`.
    pub fn level(&self, ticks: u32) -> i32 {
        let span = (self.over_s * TICKS_PER_SECOND).max(1) as i64;
        let t = (ticks as i64).min(span);
        int(self.from_m) + ((int(self.to_m - self.from_m) as i64 * t) / span) as i32
    }
}

/// A jamming field: no lock inside it, and EN drains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JamSpec {
    /// centre x, z and radius (m)
    pub at: [i32; 3],
    /// EN lost a second, in percent of capacity
    pub drain_pct: i32,
}

impl JamSpec {
    pub fn covers(&self, p: V3, delta: impl Fn(V3, V3) -> V3) -> bool {
        let c = v3(int(self.at[0]), p.y, int(self.at[1]));
        delta(p, c).len() < int(self.at[2])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResupplySpec {
    /// x, z, altitude (m): on the ground, or from a fortress's footprint
    pub at: [i32; 3],
    #[serde(default)]
    pub fortress: Option<usize>,
    #[serde(default = "ten")]
    pub r_m: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resupply {
    pub at: V3,
    pub fortress: Option<usize>,
    pub r: i32,
    pub used: bool,
}

/// A resupply gives back this share of a frame's AP, in percent.
pub const RESUPPLY_AP_PCT: i32 = 35;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EscortSpec {
    pub name: String,
    /// where it drives, in order, x, z (m); it starts at the first
    pub route: Vec<[i32; 2]>,
    pub speed_ms: i32,
    pub ap: i32,
    #[serde(default = "ten")]
    pub radius_m: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Escort {
    pub name: String,
    /// its craft
    pub craft: usize,
    pub route: Vec<V3>,
    pub next: usize,
    pub arrived: bool,
}

/// Where a pulse-armoured frame's shield stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pulse {
    pub ap: i32,
    pub max: i32,
    /// ticks the shield stays down once broken
    pub down_for: i32,
    pub down: i32,
}

impl Pulse {
    pub fn new(ap: i32, down_ms: i32) -> Pulse {
        Pulse {
            ap,
            max: ap,
            down_for: down_ms * TICKS_PER_SECOND / 1000,
            down: 0,
        }
    }

    /// Takes a hit; returns the damage that gets through.
    pub fn absorb(&mut self, dmg: i32) -> i32 {
        if self.down > 0 {
            return dmg;
        }
        self.ap -= dmg;
        if self.ap <= 0 {
            self.ap = 0;
            self.down = self.down_for;
        }
        0
    }

    /// One tick: a broken shield counts down, then comes back whole.
    pub fn tick(&mut self) {
        if self.down > 0 {
            self.down -= 1;
            if self.down == 0 {
                self.ap = self.max;
            }
        }
    }

    pub fn up(&self) -> bool {
        self.down == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn light() -> Searchlight {
        Searchlight::new(&SearchlightSpec {
            at: [0, 0, 40],
            path: vec![[0, -40], [40, -40]],
            r_m: 10,
            speed_ms: 10,
        })
    }

    #[test]
    fn a_searchlight_lights_its_cone_and_nothing_outside_it() {
        let l = light();
        assert!(l.lights(v3(0, int(3), -int(40))), "standing in the spot");
        assert!(l.lights(v3(0, int(20), -int(20))), "halfway up the beam");
        assert!(!l.lights(v3(int(15), int(3), -int(40))), "beside the spot");
        assert!(
            !l.lights(v3(int(8), int(20), -int(20))),
            "beside the beam, where it is narrow"
        );
        assert!(!l.lights(v3(0, int(50), int(20))), "behind the lamp");
    }

    #[test]
    fn a_searchlight_sweeps_its_path_and_hunts_once_the_alarm_is_up() {
        let mut l = light();
        let mut reached = false;
        for _ in 0..5 * 60 {
            l.step(None);
            reached |= l.spot == v3(int(40), 0, -int(40));
        }
        assert!(reached, "swept to the second point");
        assert!(l.spot.x < int(40), "and is on its way back");
        let mut hunter = light();
        let target = v3(-int(30), int(5), int(10));
        for _ in 0..10 * 60 {
            hunter.step(Some(target));
        }
        assert_eq!(hunter.spot, v3(target.x, 0, target.z));
    }

    #[test]
    fn lava_rises_from_its_start_to_its_top_and_stops() {
        let l = LavaSpec {
            from_m: 0,
            to_m: 60,
            over_s: 120,
            dps: 300,
        };
        assert_eq!(l.level(0), 0);
        assert_eq!(l.level(60 * TICKS_PER_SECOND as u32), int(30));
        assert_eq!(l.level(500 * TICKS_PER_SECOND as u32), int(60));
    }

    #[test]
    fn pulse_armour_turns_hits_until_it_breaks_then_comes_back() {
        let mut p = Pulse::new(1000, 1000);
        assert_eq!(p.absorb(600), 0);
        assert_eq!(p.absorb(600), 0, "the hit that breaks it is still turned");
        assert!(!p.up());
        assert_eq!(p.absorb(500), 500, "down: hits get through");
        for _ in 0..60 {
            p.tick();
        }
        assert!(p.up());
        assert_eq!(p.ap, 1000);
    }
}
