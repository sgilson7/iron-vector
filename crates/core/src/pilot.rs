//! Enemy pilots: behaviour trees after vagrancy's `Tree`.
//!
//! A pilot is shown what a player in its seat could see, and returns only
//! what a player could press: `Controls` and where to look. Its frame moves
//! by the same rule as the player's (`Body::step`), so it can do nothing the
//! player cannot.
//!
//! A tree is an ordered list of rules. Each rule is a list of conditions,
//! each with its own number, and the name of a move. When the pilot is free
//! it tries the rules top to bottom and starts the move of the first whose
//! conditions all hold, and it commits to that move until the move ends, as
//! a person commits to a manoeuvre; a rule marked `interrupt` may break in.
//! A move is beats of keys over ticks: forward, back, left, right, ascend,
//! quick boost, glide, walk and the three triggers, and which way to face.
//! The pilot sees its opponent as it was its reaction time ago, and feels
//! its own frame as it is now. Rules, moves and their labels live in
//! `data/pilots.json`.

use crate::fx::{self, deg, int, ONE};
use crate::geom::{yaw_pitch_of, V3};
use crate::mech::{Controls, TICKS_PER_SECOND};
use crate::rng::Rng;
use serde::Deserialize;
use std::collections::{BTreeMap, VecDeque};

/// One condition on what the pilot sees. Distances in metres, shares in percent.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cond {
    DistAbove(i32),
    DistBelow(i32),
    EnBelow(i32),
    EnAbove(i32),
    ApBelow(i32),
    OppApBelow(i32),
    CanSee,
    CannotSee,
    /// a missile meant for me is within this many metres
    MissileNear(i32),
    /// the opponent is higher than me by more than this
    OppAbove(i32),
    OppBelow(i32),
    /// I am lower than this above the ground
    AltBelow(i32),
    Airborne,
    Grounded,
    Staggered,
    OppStaggered,
    /// a seeded chance, drawn when the rule is tried
    Chance(u32),
    ReadyR,
    ReadyL,
    ReadyS,
    /// my next checkpoint is higher than me by more than this
    CourseAbove(i32),
    /// my next checkpoint is farther than this
    CourseFar(i32),
    /// I stand on burning ground
    FloorBurning,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    #[serde(default, rename = "if")]
    pub when: Vec<Cond>,
    #[serde(rename = "do")]
    pub act: String,
    #[serde(default)]
    pub interrupt: bool,
}

/// Which way a beat faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Face {
    /// at the opponent, led and with the pilot's error
    #[default]
    Target,
    /// at the next checkpoint
    Course,
    /// away from the opponent
    Away,
}

/// Keys held from tick `from` to tick `to` of a move, both included.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Beat {
    pub from: u32,
    pub to: u32,
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default)]
    pub face: Face,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveSpec {
    /// what the HUD shows while the move runs
    pub label: String,
    pub beats: Vec<Beat>,
}

impl MoveSpec {
    pub fn ticks(&self) -> u32 {
        self.beats.iter().map(|b| b.to + 1).max().unwrap_or(1)
    }
}

/// The keys a beat may name.
pub const KEYS: &[&str] = &[
    "fwd",
    "back",
    "left",
    "right",
    "ascend",
    "quick_boost",
    "glide",
    "walk",
    "fire_r",
    "fire_l",
    "fire_s",
];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeSpec {
    pub reaction_ms: i32,
    pub aim_error_deg: i32,
    pub salt: u64,
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pilots {
    #[serde(rename = "_note", default)]
    pub note: String,
    pub moves: BTreeMap<String, MoveSpec>,
    pub trees: BTreeMap<String, TreeSpec>,
}

impl Pilots {
    /// Reads and checks the pilots: every rule names a move, every beat a
    /// known key, and every tree ends with a rule that always holds.
    pub fn parse(json: &str) -> Result<Pilots, String> {
        let p: Pilots = serde_json::from_str(json).map_err(|e| format!("pilots: {e}"))?;
        for (name, m) in &p.moves {
            if m.beats.is_empty() {
                return Err(format!("pilots: move {name} has no beats"));
            }
            for b in &m.beats {
                if b.to < b.from {
                    return Err(format!(
                        "pilots: move {name} has a beat that ends before it starts"
                    ));
                }
                if let Some(k) = b.keys.iter().find(|k| !KEYS.contains(&k.as_str())) {
                    return Err(format!("pilots: move {name} names unknown key {k}"));
                }
            }
        }
        for (name, t) in &p.trees {
            if let Some(r) = t.rules.iter().find(|r| !p.moves.contains_key(&r.act)) {
                return Err(format!("pilots: tree {name} names unknown move {}", r.act));
            }
            if t.rules.last().map(|r| r.when.is_empty()) != Some(true) {
                return Err(format!(
                    "pilots: tree {name} must end with a rule that always holds"
                ));
            }
        }
        Ok(p)
    }

    pub fn pilot(&self, tree: &str, salt: u64) -> Option<Pilot> {
        let t = self.trees.get(tree)?;
        Some(Pilot::new(t.clone(), self.moves.clone(), salt))
    }
}

/// What the pilot can see this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Senses {
    pub me: V3,
    pub my_vel: V3,
    pub alt: i32,
    pub en_pct: i32,
    pub en_locked: bool,
    pub ap_pct: i32,
    pub glide: bool,
    pub staggered: bool,
    pub airborne: bool,
    pub floor_burning: bool,
    /// the opponent, as a vector from me (so a ring's wrap is already taken)
    pub to_target: V3,
    pub target_vel: V3,
    pub target_ap_pct: i32,
    pub target_staggered: bool,
    pub can_see: bool,
    /// distance to the nearest missile flying at me, if any
    pub missile: Option<i32>,
    /// my next checkpoint, as a vector from me
    pub to_course: Option<V3>,
    /// for each weapon slot: shot speed per tick, range, whether it homes
    pub weapons: [(i32, i32, bool); 3],
    pub ready: [bool; 3],
    pub tick: u32,
}

/// What the pilot decided: controls, where to look, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub controls: Controls,
    pub aim_yaw: i32,
    pub aim_pitch: i32,
    pub rule: usize,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pilot {
    pub tree: TreeSpec,
    moves: BTreeMap<String, MoveSpec>,
    seen: VecDeque<Senses>,
    rng: Rng,
    /// rule index, move name, tick within the move
    current: Option<(usize, String, u32)>,
}

fn ticks(ms: i32) -> usize {
    (ms * TICKS_PER_SECOND / 1000).max(0) as usize
}

impl Pilot {
    pub fn new(tree: TreeSpec, moves: BTreeMap<String, MoveSpec>, salt: u64) -> Pilot {
        let seed = tree.salt ^ salt;
        Pilot {
            tree,
            moves,
            seen: VecDeque::new(),
            rng: Rng::new(0xB7 ^ seed),
            current: None,
        }
    }

    /// Whether a condition holds: about myself as I am now, about my
    /// opponent as I saw it `reaction` ago.
    fn holds(&mut self, c: &Cond, now: &Senses, seen: &Senses) -> bool {
        let dist = seen.to_target.len();
        let m = int;
        match c {
            Cond::DistAbove(d) => dist > m(*d),
            Cond::DistBelow(d) => dist < m(*d),
            Cond::EnBelow(p) => now.en_locked || now.en_pct < *p,
            Cond::EnAbove(p) => !now.en_locked && now.en_pct > *p,
            Cond::ApBelow(p) => now.ap_pct < *p,
            Cond::OppApBelow(p) => seen.target_ap_pct < *p,
            Cond::CanSee => seen.can_see,
            Cond::CannotSee => !seen.can_see,
            Cond::MissileNear(d) => now.missile.is_some_and(|x| x < m(*d)),
            Cond::OppAbove(d) => seen.to_target.y > m(*d),
            Cond::OppBelow(d) => seen.to_target.y < -m(*d),
            Cond::AltBelow(d) => now.alt < m(*d),
            Cond::Airborne => now.airborne,
            Cond::Grounded => !now.airborne,
            Cond::Staggered => now.staggered,
            Cond::OppStaggered => seen.target_staggered,
            Cond::Chance(p) => self.rng.range(0, 99) < *p as i32,
            Cond::ReadyR => now.ready[0],
            Cond::ReadyL => now.ready[1],
            Cond::ReadyS => now.ready[2],
            Cond::CourseAbove(d) => now.to_course.is_some_and(|c| c.y > m(*d)),
            Cond::CourseFar(d) => now.to_course.is_some_and(|c| c.len() > m(*d)),
            Cond::FloorBurning => now.floor_burning,
        }
    }

    /// The first rule whose conditions all hold; only `interrupt` rules when
    /// a move is under way.
    fn pick(&mut self, now: &Senses, seen: &Senses, interrupts_only: bool) -> Option<usize> {
        for i in 0..self.tree.rules.len() {
            let r = self.tree.rules[i].clone();
            if interrupts_only && !r.interrupt {
                continue;
            }
            let mut all = true;
            for c in &r.when {
                if !self.holds(c, now, seen) {
                    all = false;
                    break;
                }
            }
            if all {
                return Some(i);
            }
        }
        None
    }

    pub fn think(&mut self, now: &Senses) -> Decision {
        self.seen.push_back(*now);
        while self.seen.len() > ticks(self.tree.reaction_ms) + 1 {
            self.seen.pop_front();
        }
        let seen = *self.seen.front().unwrap_or(now);
        // an interrupt may break into a running move; otherwise a free pilot picks
        if let Some((running, _, _)) = self.current.clone() {
            // only a rule of higher priority than the running one breaks in
            if let Some(i) = self.pick(now, &seen, true) {
                if i < running {
                    self.current = Some((i, self.tree.rules[i].act.clone(), 0));
                }
            }
        }
        if self.current.is_none() {
            let i = self.pick(now, &seen, false).unwrap_or(self.tree.rules.len() - 1);
            self.current = Some((i, self.tree.rules[i].act.clone(), 0));
        }
        let (rule, name, t) = self.current.clone().unwrap_or((0, String::new(), 0));
        let mut c = Controls::default();
        let mut face = Face::Target;
        let mut label = String::new();
        match self.moves.get(&name).cloned() {
            Some(mv) => {
                label = mv.label.clone();
                for b in mv.beats.iter().filter(|b| b.from <= t && t <= b.to) {
                    face = b.face;
                    let held = |k: &str| b.keys.iter().any(|x| x == k);
                    c.move_z += (held("fwd") as i32 - held("back") as i32) * ONE;
                    c.move_x += (held("right") as i32 - held("left") as i32) * ONE;
                    c.ascend |= held("ascend");
                    // a quick boost is a press: only on a beat's first tick
                    c.quick_boost |= held("quick_boost") && t == b.from;
                    if (held("glide") && !now.glide) || (held("walk") && now.glide) {
                        c.toggle_glide = true;
                    }
                    for (k, key) in ["fire_r", "fire_l", "fire_s"].iter().enumerate() {
                        c.fire[k] |= held(key);
                    }
                }
                let done = t + 1 >= mv.ticks();
                self.current = if done {
                    None
                } else {
                    Some((rule, name.clone(), t + 1))
                };
            }
            None => self.current = None,
        }
        c.move_z = c.move_z.clamp(-ONE, ONE);
        c.move_x = c.move_x.clamp(-ONE, ONE);
        // a trigger is only pulled at something seen and in reach
        let dist = seen.to_target.len();
        for k in 0..3 {
            let (_, range, _) = now.weapons[k];
            c.fire[k] &= seen.can_see && now.ready[k] && dist <= range + range / 5;
        }
        let (aim_yaw, aim_pitch) = self.aim(face, now, &seen);
        Decision {
            controls: c,
            aim_yaw,
            aim_pitch,
            rule,
            label,
        }
    }

    fn aim(&self, face: Face, now: &Senses, seen: &Senses) -> (i32, i32) {
        match (face, now.to_course) {
            (Face::Course, Some(to)) => yaw_pitch_of(to),
            (Face::Away, _) => {
                let (y, _) = yaw_pitch_of(seen.to_target);
                (y + fx::HALF, 0)
            }
            _ => {
                // led by the direct-fire weapon's flight, from where it was
                // seen, with a small wandering error so it can miss
                let (speed, _, _) = now.weapons[1];
                let flight = fx::div(seen.to_target.len(), speed.max(1)) / ONE;
                let lead = seen.to_target.add(seen.target_vel.scale(int(flight)));
                let (yaw, pitch) = yaw_pitch_of(lead);
                let err = deg(self.tree.aim_error_deg);
                let wobble = |k: u32| fx::mul(fx::sin((now.tick.wrapping_mul(k) & 0xffff) as i32), err);
                (yaw + wobble(311), pitch + wobble(197))
            }
        }
    }

    /// The move under way, for a trace.
    pub fn current_move(&self) -> Option<(&str, u32)> {
        self.current.as_ref().map(|(_, n, t)| (n.as_str(), *t))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::geom::v3;

    pub fn pilots() -> Pilots {
        Pilots::parse(include_str!("../../../data/pilots.json")).unwrap()
    }

    pub fn senses() -> Senses {
        Senses {
            me: V3::ZERO,
            my_vel: V3::ZERO,
            alt: 0,
            en_pct: 100,
            en_locked: false,
            ap_pct: 100,
            glide: true,
            staggered: false,
            airborne: false,
            floor_burning: false,
            to_target: v3(0, 0, -int(150)),
            target_vel: V3::ZERO,
            target_ap_pct: 100,
            target_staggered: false,
            can_see: true,
            missile: None,
            to_course: None,
            weapons: [
                (int(3), int(360), true),
                (int(11), int(260), false),
                (int(4), int(400), false),
            ],
            ready: [true; 3],
            tick: 1,
        }
    }

    fn holds(c: Cond, s: Senses) -> bool {
        let mut p = pilots().pilot("duelist", 1).unwrap();
        p.holds(&c, &s, &s)
    }

    #[test]
    fn each_condition_holds_in_the_state_built_for_it_and_not_otherwise() {
        let s = senses();
        let cases: Vec<(Cond, Senses, Senses)> = vec![
            (
                Cond::DistAbove(100),
                s,
                Senses {
                    to_target: v3(0, 0, -int(50)),
                    ..s
                },
            ),
            (
                Cond::DistBelow(200),
                s,
                Senses {
                    to_target: v3(0, 0, -int(250)),
                    ..s
                },
            ),
            (Cond::EnBelow(30), Senses { en_pct: 20, ..s }, s),
            (Cond::EnAbove(50), s, Senses { en_locked: true, ..s }),
            (Cond::ApBelow(50), Senses { ap_pct: 40, ..s }, s),
            (
                Cond::OppApBelow(50),
                Senses {
                    target_ap_pct: 40,
                    ..s
                },
                s,
            ),
            (Cond::CanSee, s, Senses { can_see: false, ..s }),
            (Cond::CannotSee, Senses { can_see: false, ..s }, s),
            (
                Cond::MissileNear(60),
                Senses {
                    missile: Some(int(30)),
                    ..s
                },
                s,
            ),
            (
                Cond::OppAbove(20),
                Senses {
                    to_target: v3(0, int(40), -int(100)),
                    ..s
                },
                s,
            ),
            (
                Cond::OppBelow(20),
                Senses {
                    to_target: v3(0, -int(40), -int(100)),
                    ..s
                },
                s,
            ),
            (Cond::AltBelow(30), s, Senses { alt: int(50), ..s }),
            (Cond::Airborne, Senses { airborne: true, ..s }, s),
            (Cond::Grounded, s, Senses { airborne: true, ..s }),
            (Cond::Staggered, Senses { staggered: true, ..s }, s),
            (
                Cond::OppStaggered,
                Senses {
                    target_staggered: true,
                    ..s
                },
                s,
            ),
            (
                Cond::ReadyR,
                s,
                Senses {
                    ready: [false, true, true],
                    ..s
                },
            ),
            (
                Cond::ReadyL,
                s,
                Senses {
                    ready: [true, false, true],
                    ..s
                },
            ),
            (
                Cond::ReadyS,
                s,
                Senses {
                    ready: [true, true, false],
                    ..s
                },
            ),
            (
                Cond::CourseAbove(10),
                Senses {
                    to_course: Some(v3(0, int(30), -int(50))),
                    ..s
                },
                s,
            ),
            (
                Cond::CourseFar(100),
                Senses {
                    to_course: Some(v3(0, 0, -int(150))),
                    ..s
                },
                s,
            ),
            (
                Cond::FloorBurning,
                Senses {
                    floor_burning: true,
                    ..s
                },
                s,
            ),
        ];
        for (c, yes, no) in cases {
            assert!(holds(c.clone(), yes), "{c:?} should hold");
            assert!(!holds(c.clone(), no), "{c:?} should not hold");
        }
        assert!(holds(Cond::Chance(100), s) && !holds(Cond::Chance(0), s));
    }

    #[test]
    fn a_pilot_commits_to_its_move_until_it_ends() {
        let mut p = pilots().pilot("duelist", 1).unwrap();
        let far = Senses {
            to_target: v3(0, 0, -int(400)),
            ..senses()
        };
        p.think(&far);
        let name = p.current_move().unwrap().0.to_string();
        // the opponent is suddenly close: a free pilot would back off, a
        // committed one finishes its move first
        let near = Senses {
            to_target: v3(0, 0, -int(30)),
            ..senses()
        };
        let mut n = 1;
        while p.current_move().is_some_and(|(m, _)| m == name) {
            p.think(&near);
            n += 1;
            assert!(n < 400);
        }
        assert_eq!(
            n as u32,
            pilots().moves[&name].ticks(),
            "{name} ran its full length"
        );
    }

    #[test]
    fn an_interrupt_rule_breaks_into_a_move() {
        let mut p = pilots().pilot("duelist", 1).unwrap();
        p.think(&Senses {
            to_target: v3(0, 0, -int(400)),
            ..senses()
        });
        let d = p.think(&Senses {
            missile: Some(int(20)),
            to_target: v3(0, 0, -int(400)),
            ..senses()
        });
        assert!(
            d.controls.quick_boost,
            "a missile close in breaks into the move with a dodge"
        );
    }

    #[test]
    fn a_pilot_sees_its_opponent_late_by_its_reaction_time() {
        let mut p = pilots().pilot("duelist", 1).unwrap();
        let react = ticks(p.tree.reaction_ms);
        assert!(react > 0);
        let far = Senses {
            to_target: v3(0, 0, -int(400)),
            ..senses()
        };
        for _ in 0..react + 2 {
            p.think(&far);
        }
        p.think(&Senses {
            to_target: v3(0, 0, -int(20)),
            ..senses()
        });
        assert_eq!(
            p.seen.front().unwrap().to_target,
            far.to_target,
            "the close opponent is not yet seen"
        );
    }

    #[test]
    fn a_staggered_pilot_holds_still() {
        let mut p = pilots().pilot("duelist", 1).unwrap();
        let d = p.think(&Senses {
            staggered: true,
            ..senses()
        });
        assert_eq!((d.controls.move_x, d.controls.move_z), (0, 0));
        assert_eq!(d.controls.fire, [false; 3]);
    }

    #[test]
    fn it_never_fires_at_what_it_cannot_see() {
        let mut p = pilots().pilot("duelist", 1).unwrap();
        for t in 0..400 {
            let d = p.think(&Senses {
                tick: t,
                can_see: false,
                ..senses()
            });
            assert_eq!(d.controls.fire, [false; 3]);
        }
    }

    #[test]
    fn it_aims_at_its_target_within_its_error() {
        let mut p = pilots().pilot("duelist", 1).unwrap();
        let err = deg(p.tree.aim_error_deg) + deg(1) / 2;
        let d = p.think(&senses());
        assert!(fx::wrap(d.aim_yaw).abs() <= err, "{}", d.aim_yaw);
        assert!(d.aim_pitch.abs() <= err);
    }

    #[test]
    fn a_racer_faces_its_next_checkpoint_and_drives_at_it() {
        let mut p = pilots().pilot("racer", 1).unwrap();
        let to = v3(int(100), 0, 0);
        let d = p.think(&Senses {
            to_course: Some(to),
            ..senses()
        });
        assert!(fx::wrap(d.aim_yaw - yaw_pitch_of(to).0).abs() < deg(1));
        assert_eq!(d.controls.move_z, ONE);
    }

    #[test]
    fn every_tree_the_campaign_names_exists() {
        let p = pilots();
        let c = crate::campaign::tests::campaign();
        for m in c
            .planets
            .iter()
            .flat_map(|pl| &pl.missions)
            .flat_map(|m| &m.mechs)
        {
            assert!(p.trees.contains_key(&m.pilot), "no tree {}", m.pilot);
        }
    }

    #[test]
    fn a_file_whose_tree_names_an_unknown_move_or_key_is_refused() {
        let text = include_str!("../../../data/pilots.json");
        assert!(
            Pilots::parse(&text.replacen("\"do\": \"close_in\"", "\"do\": \"no_such_move\"", 1)).is_err()
        );
        assert!(Pilots::parse(&text.replacen("\"fwd\"", "\"sideways\"", 1)).is_err());
    }
}
