//! Enemy pilots. A pilot is shown what a player in its seat could see, and
//! returns only what a player could press: `Controls` and where to look. It
//! moves by the same rule as the player's mech (`Body::step`), so it can do
//! nothing the player cannot. This follows vagrancy's `pilot` crate.
//!
//! The behaviour is a priority list from `data/pilots.json`: each time the
//! pilot thinks, it tries the rules top to bottom and does the first whose
//! condition holds. The rule it chose is reported, so the HUD can say what
//! the enemy is doing and a test can check each branch in the state built
//! for it.

use crate::fx::{self, deg, int, ONE};
use crate::geom::{facing, yaw_pitch_of, V3};
use crate::mech::{Controls, TICKS_PER_SECOND};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cond {
    Staggered,
    MissileNear,
    EnLow,
    Far,
    TooClose,
    BelowTarget,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Act {
    Hold,
    Dodge,
    Recover,
    Approach,
    Retreat,
    Climb,
    Strafe,
}

impl Act {
    /// The copy key the HUD shows for this action.
    pub fn key(self) -> &'static str {
        match self {
            Act::Hold => "ai_hold",
            Act::Dodge => "ai_dodge",
            Act::Recover => "ai_recover",
            Act::Approach => "ai_approach",
            Act::Retreat => "ai_retreat",
            Act::Climb => "ai_climb",
            Act::Strafe => "ai_strafe",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub when: Cond,
    #[serde(rename = "do")]
    pub act: Act,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PilotSpec {
    pub think_ms: i32,
    pub aim_error_deg: i32,
    pub keep_min_m: i32,
    pub keep_max_m: i32,
    pub strafe_flip_ms: i32,
    pub burst_ms: i32,
    pub pause_ms: i32,
    pub missile_every_ms: i32,
    pub shoulder_every_ms: i32,
    pub dodge_range_m: i32,
    pub climb_gap_m: i32,
    pub en_low_pct: i32,
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Pilots(pub BTreeMap<String, PilotSpec>);

impl Pilots {
    pub fn parse(json: &str) -> Result<Pilots, String> {
        let mut v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("pilots: {e}"))?;
        if let Some(o) = v.as_object_mut() {
            o.remove("_note");
        }
        let p: Pilots = serde_json::from_value(v).map_err(|e| format!("pilots: {e}"))?;
        for (name, spec) in &p.0 {
            if spec.rules.last().map(|r| r.when) != Some(Cond::Always) {
                return Err(format!("pilots: {name} must end with an \"always\" rule"));
            }
        }
        Ok(p)
    }
}

/// What the pilot can see this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Senses {
    pub me: V3,
    pub my_vel: V3,
    pub en_pct: i32,
    pub en_locked: bool,
    pub glide: bool,
    pub staggered: bool,
    pub target: V3,
    pub target_vel: V3,
    pub can_see: bool,
    /// distance to the nearest missile flying at me, if any
    pub missile: Option<i32>,
    /// for each weapon slot: its speed per tick, range, and whether it homes
    pub weapons: [(i32, i32, bool); 3],
    pub ready: [bool; 3],
    pub tick: u32,
}

/// What the pilot decided: controls, where to look, and the rule it used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    pub controls: Controls,
    pub aim_yaw: i32,
    pub aim_pitch: i32,
    pub rule: usize,
    pub act: Act,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pilot {
    pub spec: PilotSpec,
    pub rule: usize,
    think_wait: i32,
    strafe: i32,
    strafe_wait: i32,
    burst: i32,
    missile_wait: i32,
    shoulder_wait: i32,
}

fn ticks(ms: i32) -> i32 {
    ms * TICKS_PER_SECOND / 1000
}

impl Pilot {
    pub fn new(spec: PilotSpec) -> Pilot {
        let last = spec.rules.len().saturating_sub(1);
        Pilot {
            missile_wait: ticks(spec.missile_every_ms) / 2,
            shoulder_wait: ticks(spec.shoulder_every_ms) / 2,
            spec,
            rule: last,
            think_wait: 0,
            strafe: ONE,
            strafe_wait: 0,
            burst: 0,
        }
    }

    fn holds(&self, c: Cond, s: &Senses) -> bool {
        let dist = s.target.sub(s.me).len();
        match c {
            Cond::Staggered => s.staggered,
            Cond::MissileNear => s.missile.is_some_and(|d| d < int(self.spec.dodge_range_m)),
            Cond::EnLow => s.en_locked || s.en_pct < self.spec.en_low_pct,
            Cond::Far => dist > int(self.spec.keep_max_m) || !s.can_see,
            Cond::TooClose => dist < int(self.spec.keep_min_m),
            Cond::BelowTarget => s.target.y - s.me.y > int(self.spec.climb_gap_m),
            Cond::Always => true,
        }
    }

    /// The first rule whose condition holds.
    pub fn choose(&self, s: &Senses) -> usize {
        self.spec
            .rules
            .iter()
            .position(|r| self.holds(r.when, s))
            .unwrap_or(self.spec.rules.len() - 1)
    }

    pub fn think(&mut self, s: &Senses) -> Decision {
        self.think_wait -= 1;
        if self.think_wait <= 0 || self.holds(Cond::Staggered, s) || self.holds(Cond::MissileNear, s) {
            self.rule = self.choose(s);
            self.think_wait = ticks(self.spec.think_ms);
        }
        self.strafe_wait -= 1;
        if self.strafe_wait <= 0 {
            self.strafe = -self.strafe;
            self.strafe_wait = ticks(self.spec.strafe_flip_ms) + (s.tick % 37) as i32;
        }
        let act = self.spec.rules[self.rule].act;
        let mut c = Controls::default();
        let want_glide = |on: bool| on != s.glide;
        match act {
            Act::Hold => {}
            Act::Dodge => {
                c.move_x = self.strafe;
                c.quick_boost = true;
                self.strafe = -self.strafe;
                self.strafe_wait = ticks(self.spec.strafe_flip_ms);
                // a dodge is one burst, then it thinks again
                self.think_wait = 0;
            }
            Act::Recover => {
                c.move_x = self.strafe / 2;
                c.toggle_glide = want_glide(false);
            }
            Act::Approach => {
                c.move_z = ONE;
                c.move_x = self.strafe / 3;
                c.toggle_glide = want_glide(true);
                c.ascend = s.target.y - s.me.y > int(self.spec.climb_gap_m) && s.en_pct > 45;
            }
            Act::Retreat => {
                c.move_z = -ONE;
                c.move_x = self.strafe;
                c.toggle_glide = want_glide(true);
            }
            Act::Climb => {
                c.move_x = self.strafe;
                c.ascend = s.en_pct > 30;
                c.toggle_glide = want_glide(true);
            }
            Act::Strafe => {
                c.move_x = self.strafe;
                let dist = s.target.sub(s.me).len();
                let mid = int((self.spec.keep_min_m + self.spec.keep_max_m) / 2);
                c.move_z = if dist > mid { ONE / 3 } else { -ONE / 3 };
                c.toggle_glide = want_glide(true);
                c.quick_boost = s.tick.is_multiple_of(173) && s.en_pct > 60;
            }
        }
        if act != Act::Hold {
            self.fire(s, &mut c);
        }
        // aim at the target's chest, led for the first weapon, with a small
        // wandering error so it can miss
        let (speed, _, _) = s.weapons[0];
        let dist = s.target.sub(s.me).len();
        let flight = fx::div(dist, speed.max(1)) / ONE;
        let lead = s.target.add(s.target_vel.scale(int(flight)));
        let (yaw, pitch) = yaw_pitch_of(lead.sub(s.me));
        let err = deg(self.spec.aim_error_deg);
        let wobble = |k: u32| fx::mul(fx::sin((s.tick.wrapping_mul(k) & 0xffff) as i32), err);
        Decision {
            controls: c,
            aim_yaw: yaw + wobble(311),
            aim_pitch: pitch + wobble(197),
            rule: self.rule,
            act,
        }
    }

    fn fire(&mut self, s: &Senses, c: &mut Controls) {
        self.burst += 1;
        let cycle = ticks(self.spec.burst_ms) + ticks(self.spec.pause_ms);
        if self.burst >= cycle {
            self.burst = 0;
        }
        let in_burst = self.burst < ticks(self.spec.burst_ms);
        self.missile_wait -= 1;
        self.shoulder_wait -= 1;
        if !s.can_see {
            return;
        }
        let dist = s.target.sub(s.me).len();
        for k in 0..3 {
            let (_, range, homes) = s.weapons[k];
            if !s.ready[k] || dist > range + range / 5 {
                continue;
            }
            let go = match (k, homes) {
                (2, _) => self.shoulder_wait <= 0,
                (_, true) => self.missile_wait <= 0,
                _ => in_burst,
            };
            if go {
                c.fire[k] = true;
                match (k, homes) {
                    (2, _) => self.shoulder_wait = ticks(self.spec.shoulder_every_ms),
                    (_, true) => self.missile_wait = ticks(self.spec.missile_every_ms),
                    _ => {}
                }
            }
        }
    }
}

/// The direction a pilot faces when it has nothing to look at.
pub fn idle_aim(yaw: i32) -> V3 {
    facing(yaw, 0)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::geom::v3;

    pub fn spec() -> PilotSpec {
        Pilots::parse(include_str!("../../../data/pilots.json"))
            .unwrap()
            .0["frame"]
            .clone()
    }

    fn senses() -> Senses {
        Senses {
            me: V3::ZERO,
            my_vel: V3::ZERO,
            en_pct: 100,
            en_locked: false,
            glide: true,
            staggered: false,
            target: v3(0, 0, -int(150)),
            target_vel: V3::ZERO,
            can_see: true,
            missile: None,
            weapons: [
                (int(11), int(260), false),
                (int(3), int(360), true),
                (int(4), int(400), false),
            ],
            ready: [true; 3],
            tick: 1,
        }
    }

    fn act_in(s: Senses) -> Act {
        let mut p = Pilot::new(spec());
        p.think(&s).act
    }

    #[test]
    fn each_rule_is_chosen_in_the_state_built_for_it() {
        assert_eq!(act_in(senses()), Act::Strafe, "in the band, seen, level: strafe");
        assert_eq!(
            act_in(Senses {
                staggered: true,
                ..senses()
            }),
            Act::Hold
        );
        assert_eq!(
            act_in(Senses {
                missile: Some(int(40)),
                ..senses()
            }),
            Act::Dodge
        );
        assert_eq!(
            act_in(Senses {
                en_pct: 10,
                ..senses()
            }),
            Act::Recover
        );
        assert_eq!(
            act_in(Senses {
                en_locked: true,
                ..senses()
            }),
            Act::Recover
        );
        assert_eq!(
            act_in(Senses {
                target: v3(0, 0, -int(400)),
                ..senses()
            }),
            Act::Approach
        );
        assert_eq!(
            act_in(Senses {
                can_see: false,
                ..senses()
            }),
            Act::Approach,
            "lost sight: close in"
        );
        assert_eq!(
            act_in(Senses {
                target: v3(0, 0, -int(40)),
                ..senses()
            }),
            Act::Retreat
        );
        assert_eq!(
            act_in(Senses {
                target: v3(0, int(60), -int(150)),
                ..senses()
            }),
            Act::Climb
        );
    }

    #[test]
    fn earlier_rules_win_when_two_hold() {
        // staggered and a missile near: staggered is first
        let s = Senses {
            staggered: true,
            missile: Some(int(10)),
            en_pct: 5,
            ..senses()
        };
        assert_eq!(act_in(s), Act::Hold);
        // a missile near and EN low: the dodge is tried first and needs no check of EN
        let s = Senses {
            missile: Some(int(10)),
            en_pct: 5,
            ..senses()
        };
        assert_eq!(act_in(s), Act::Dodge);
    }

    #[test]
    fn a_held_pilot_presses_nothing() {
        let mut p = Pilot::new(spec());
        let d = p.think(&Senses {
            staggered: true,
            ..senses()
        });
        assert_eq!(d.controls, Controls::default());
    }

    #[test]
    fn a_dodge_is_a_sideways_quick_boost() {
        let mut p = Pilot::new(spec());
        let d = p.think(&Senses {
            missile: Some(int(30)),
            ..senses()
        });
        assert!(d.controls.quick_boost);
        assert_ne!(d.controls.move_x, 0);
        assert_eq!(d.controls.move_z, 0);
    }

    #[test]
    fn it_fires_its_rifle_in_bursts_and_never_without_sight() {
        let mut p = Pilot::new(spec());
        let shots: Vec<bool> = (0..240)
            .map(|t| p.think(&Senses { tick: t, ..senses() }).controls.fire[0])
            .collect();
        let on = shots.iter().filter(|f| **f).count() as i32;
        // the trigger is held for burst_ms of every burst_ms + pause_ms
        let s = spec();
        let expect = 240 * s.burst_ms / (s.burst_ms + s.pause_ms);
        assert!(
            (on - expect).abs() <= 240 / 8,
            "{on} of 240 ticks, expected about {expect}"
        );
        let mut blind = Pilot::new(spec());
        assert!((0..240).all(|t| !blind
            .think(&Senses {
                tick: t,
                can_see: false,
                ..senses()
            })
            .controls
            .fire
            .contains(&true)));
    }

    #[test]
    fn it_aims_at_its_target_within_its_error() {
        let mut p = Pilot::new(spec());
        let d = p.think(&senses());
        // target straight ahead (−z): yaw 0, pitch 0, give or take the spec's error
        let err = deg(spec().aim_error_deg) + deg(1) / 2;
        assert!(fx::wrap(d.aim_yaw).abs() <= err, "{}", d.aim_yaw);
        assert!(d.aim_pitch.abs() <= err);
    }

    #[test]
    fn a_pilot_file_without_a_final_always_rule_is_refused() {
        let text = include_str!("../../../data/pilots.json")
            .replace(",\n      { \"when\": \"always\", \"do\": \"strafe\" }", "");
        assert!(Pilots::parse(&text).is_err());
    }
}
