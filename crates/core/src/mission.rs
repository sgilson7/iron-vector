//! Missions, from data: what spawns and when, what must be protected, the
//! course a race runs, how the mission is won or lost, and the plus challenge
//! that asks for the same win under a constraint. Phases advance only here.

use crate::combat::{Craft, CraftKind, Team};
use crate::fx::{deg, int};
use crate::geom::{v3, V3};
use crate::mech::TICKS_PER_SECOND;
use crate::parts::Slot;
use crate::world::World;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// destroy every hostile, wave by wave
    Destroy,
    /// pass every checkpoint before the rival does
    Race,
    /// destroy every attacker before too many structures fall
    Defend,
    /// defeat the enemy frames
    Duel,
    /// still be standing when the clock runs out
    Survive,
}

impl Kind {
    pub fn objective(self) -> &'static str {
        match self {
            Kind::Destroy => "obj_destroy",
            Kind::Race => "obj_race",
            Kind::Defend => "obj_defend",
            Kind::Duel => "obj_duel",
            Kind::Survive => "obj_survive",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitSpawn {
    #[serde(rename = "type")]
    pub unit: String,
    /// x, z, altitude (m); an altitude below zero stands it on whatever is there
    pub at: [i32; 3],
    #[serde(default)]
    pub wave: u32,
    /// a place it drives for; reaching it fails the mission
    #[serde(default)]
    pub goal: Option<[i32; 2]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MechSpawn {
    pub name: String,
    pub pilot: String,
    /// x, z, heading in degrees
    pub at: [i32; 3],
    pub ap_pct: i32,
    /// no loadout: the frame copies the player's own
    #[serde(default)]
    pub loadout: Option<BTreeMap<Slot, String>>,
    #[serde(default)]
    pub wave: u32,
    /// a friendly rival in a race, not a hostile
    #[serde(default)]
    pub racer: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Building {
    /// centre x, z (m)
    pub at: [i32; 2],
    /// width, depth, height (m)
    pub size: [i32; 3],
    #[serde(default)]
    pub ap: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlusRule {
    /// fire no more than this many rounds (each missile counts)
    MaxRounds,
    /// win in under this many seconds
    UnderSeconds,
    /// finish with at least this share of AP, in percent
    MinApPct,
    /// lose no more than this many structures
    MaxLost,
    /// spend no more than this many seconds on burning ground
    MaxFloorSeconds,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Plus {
    pub rule: PlusRule,
    pub value: i32,
    pub reward: String,
}

fn yes() -> bool {
    true
}

fn hundred() -> i32 {
    100
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionSpec {
    pub id: String,
    pub name: String,
    pub scene: String,
    pub kind: Kind,
    /// x, z, heading in degrees
    pub start: [i32; 3],
    pub reward: String,
    pub plus: Plus,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub units: Vec<UnitSpawn>,
    #[serde(default)]
    pub mechs: Vec<MechSpawn>,
    #[serde(default)]
    pub protect: Vec<Building>,
    /// solid ground set down for this mission: a starting deck, a platform
    #[serde(default)]
    pub pads: Vec<Building>,
    #[serde(default)]
    pub may_lose: i32,
    /// checkpoints, x, z, altitude (m)
    #[serde(default)]
    pub course: Vec<[i32; 3]>,
    #[serde(default)]
    pub time_limit_s: i32,
    #[serde(default = "yes")]
    pub weapons: bool,
    /// scales the AP and damage of every unit
    #[serde(default = "hundred")]
    pub power_pct: i32,
}

/// One kind of unit, from `data/units.json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitSpec {
    pub ap: i32,
    pub radius_m: i32,
    pub speed_ms: i32,
    pub orbit_m: i32,
    #[serde(default)]
    pub gun: Option<Gun>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gun {
    pub damage: i32,
    pub speed_ms: i32,
    pub range_m: i32,
    pub burst: i32,
    pub burst_gap_ms: i32,
    pub reload_ms: i32,
    pub spread_deg: i32,
    pub blast_m: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Units(pub BTreeMap<String, UnitSpec>);

impl Units {
    pub fn parse(json: &str) -> Result<Units, String> {
        let mut v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("units: {e}"))?;
        if let Some(o) = v.as_object_mut() {
            o.remove("_note");
        }
        let u: BTreeMap<String, UnitSpec> = serde_json::from_value(v).map_err(|e| format!("units: {e}"))?;
        for k in ["drone", "heli", "tank", "turret"] {
            if !u.contains_key(k) {
                return Err(format!("units: no {k}"));
            }
        }
        Ok(Units(u))
    }
}

/// How far from a checkpoint's centre a frame passes through it.
pub const CHECKPOINT_RADIUS: i32 = int(24);
/// How near its goal a fleeing unit counts as escaped.
const ESCAPE_RADIUS: i32 = int(25);
/// Seconds the world keeps running after the mission ends, before the debrief.
pub const AFTERMATH_TICKS: u32 = 3 * TICKS_PER_SECOND as u32;

/// Builds a unit from its spec, scaled by the mission's power.
pub fn make_unit(
    kind: &str,
    spec: &UnitSpec,
    at: V3,
    wave: u32,
    goal: Option<V3>,
    power: i32,
    salt: i32,
) -> Craft {
    let kind = match kind {
        "heli" => CraftKind::Heli,
        "tank" => CraftKind::Tank,
        "turret" => CraftKind::Turret,
        _ => CraftKind::Drone,
    };
    let ap = spec.ap * power / 100;
    Craft {
        kind,
        pos: at,
        prev_pos: at,
        vel: V3::ZERO,
        yaw: 0,
        prev_yaw: 0,
        ap,
        max_ap: ap,
        alive: true,
        anchor: at,
        phase: deg(53) * salt,
        cooldown: (salt * 17) % 60 + TICKS_PER_SECOND,
        burst: 0,
        respawn: 0,
        down_ticks: 0,
        radius: int(spec.radius_m),
        speed: crate::fx::ratio(spec.speed_ms, TICKS_PER_SECOND),
        orbit: int(spec.orbit_m),
        gun: spec.gun.map(|g| Gun {
            damage: g.damage * power / 100,
            ..g
        }),
        goal,
        wave,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Debrief {
    pub mission: String,
    pub name: String,
    pub success: bool,
    pub seconds: u32,
    pub ap_kept_pct: i32,
    pub kills: u32,
    pub rank: &'static str,
    /// whether the run met the plus challenge (counted only once revealed)
    pub plus_met: bool,
    /// the challenge, once revealed: rule and value
    pub plus: Option<(PlusRule, i32)>,
    /// part names unlocked by this run, each with whether by the plus
    pub rewards: Vec<(String, bool)>,
    /// copy key of why a failed mission failed
    pub reason: Option<&'static str>,
}

/// Rank from AP kept and time against a par: each second under par is worth
/// half a point, capped at 30 either way.
pub fn rank(success: bool, seconds: i32, ap_kept_pct: i32, par_s: i32) -> &'static str {
    if !success {
        return "D";
    }
    let score = ap_kept_pct + ((par_s - seconds) / 2).clamp(-30, 30);
    match score {
        s if s >= 100 => "S",
        s if s >= 80 => "A",
        s if s >= 60 => "B",
        _ => "C",
    }
}

/// A mission under way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mission {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub wave: u32,
    pub last_wave: u32,
    pub ticks: u32,
    pub ended_at: Option<u32>,
    pub success: Option<bool>,
    pub reason: Option<&'static str>,
    pub time_limit: u32,
    pub may_lose: i32,
    pub plus: Plus,
    pub weapons: bool,
    /// rounds the player has fired, and ticks spent on burning ground
    pub rounds: i32,
    pub floor_ticks: i32,
    /// units and frames held for later waves
    pub reserve_craft: Vec<Craft>,
    pub reserve_mechs: Vec<(u32, crate::world::Mech)>,
    pub course: Vec<V3>,
}

impl Mission {
    pub fn new(spec: &MissionSpec) -> Mission {
        let last_wave = spec
            .units
            .iter()
            .map(|u| u.wave)
            .chain(spec.mechs.iter().map(|m| m.wave))
            .max()
            .unwrap_or(0);
        Mission {
            id: spec.id.clone(),
            name: spec.name.clone(),
            kind: spec.kind,
            wave: 0,
            last_wave,
            ticks: 0,
            ended_at: None,
            success: None,
            reason: None,
            time_limit: (spec.time_limit_s * TICKS_PER_SECOND) as u32,
            may_lose: spec.may_lose,
            plus: spec.plus.clone(),
            weapons: spec.weapons,
            rounds: 0,
            floor_ticks: 0,
            reserve_craft: Vec::new(),
            reserve_mechs: Vec::new(),
            course: spec
                .course
                .iter()
                .map(|c| v3(int(c[0]), int(c[2]), int(c[1])))
                .collect(),
        }
    }

    pub fn seconds(&self) -> u32 {
        self.ended_at.unwrap_or(self.ticks) / TICKS_PER_SECOND as u32
    }

    /// Seconds left on the clock, if there is one.
    pub fn time_left(&self) -> Option<u32> {
        (self.time_limit > 0).then(|| self.time_limit.saturating_sub(self.ticks) / TICKS_PER_SECOND as u32)
    }

    /// Whether the aftermath is over and the debrief should show.
    pub fn finished(&self) -> bool {
        self.ended_at.is_some_and(|t| self.ticks >= t + AFTERMATH_TICKS)
    }

    /// Whether a run met the plus challenge.
    pub fn plus_met(&self, w: &World) -> bool {
        let p = w.player();
        let v = self.plus.value;
        self.success == Some(true)
            && match self.plus.rule {
                PlusRule::MaxRounds => self.rounds <= v,
                PlusRule::UnderSeconds => (self.seconds() as i32) < v,
                PlusRule::MinApPct => p.ap * 100 >= p.stats.ap * v,
                PlusRule::MaxLost => w.structures_lost() <= v,
                PlusRule::MaxFloorSeconds => self.floor_ticks <= v * TICKS_PER_SECOND,
            }
    }

    /// How the run stands against the plus challenge right now.
    pub fn plus_progress(&self, w: &World) -> i32 {
        let p = w.player();
        match self.plus.rule {
            PlusRule::MaxRounds => self.rounds,
            PlusRule::UnderSeconds => self.seconds() as i32,
            PlusRule::MinApPct => p.ap * 100 / p.stats.ap.max(1),
            PlusRule::MaxLost => w.structures_lost(),
            PlusRule::MaxFloorSeconds => self.floor_ticks / TICKS_PER_SECOND,
        }
    }

    pub fn debrief(&self, w: &World, plus_revealed: bool, rewards: Vec<(String, bool)>) -> Debrief {
        let p = w.player();
        let ap_kept_pct = p.ap * 100 / p.stats.ap.max(1);
        let success = self.success == Some(true);
        // par is two thirds of the clock, or a minute and a half without one
        let par = if self.time_limit > 0 {
            (self.time_limit / TICKS_PER_SECOND as u32) as i32 * 2 / 3
        } else {
            90
        };
        Debrief {
            mission: self.id.clone(),
            name: self.name.clone(),
            success,
            seconds: self.seconds(),
            ap_kept_pct,
            kills: w.kills,
            rank: rank(success, self.seconds() as i32, ap_kept_pct, par),
            plus_met: plus_revealed && self.plus_met(w),
            plus: plus_revealed.then_some((self.plus.rule, self.plus.value)),
            rewards,
            reason: self.reason,
        }
    }
}

impl World {
    /// Hostiles still fighting: enemy frames and craft.
    pub fn hostiles_alive(&self) -> usize {
        self.craft.iter().filter(|c| c.alive).count()
            + self
                .mechs
                .iter()
                .filter(|m| m.alive && m.team == Team::Enemy)
                .count()
    }

    /// Advances the mission after a tick: spawns the next wave, counts
    /// toward the plus challenge, and decides success or failure.
    pub fn update_mission(&mut self) {
        let Some(mut m) = self.mission.take() else { return };
        m.ticks += 1;
        if m.ended_at.is_none() {
            let p = self.player();
            if p.alive && p.body.grounded && p.body.pos.y == 0 && self.map.climate.floor_dps > 0 {
                m.floor_ticks += 1;
            }
            // the next wave rises when this one is down
            if self.hostiles_alive() == 0 && m.wave < m.last_wave {
                m.wave += 1;
                let w = m.wave;
                let (now, later): (Vec<Craft>, Vec<Craft>) =
                    m.reserve_craft.drain(..).partition(|c| c.wave == w);
                m.reserve_craft = later;
                self.craft.extend(now);
                let (now, later): (Vec<_>, Vec<_>) = m.reserve_mechs.drain(..).partition(|(wv, _)| *wv == w);
                m.reserve_mechs = later;
                self.mechs.extend(now.into_iter().map(|(_, mech)| mech));
            }
            self.update_course(&m);
            if let Some((ok, why)) = self.judge(&m) {
                m.success = Some(ok);
                m.reason = why;
                m.ended_at = Some(m.ticks);
            }
        }
        self.mission = Some(m);
    }

    /// Moves every racer's next checkpoint on when it passes through one.
    fn update_course(&mut self, m: &Mission) {
        if m.course.is_empty() {
            return;
        }
        for i in 0..self.mechs.len() {
            let next = self.mechs[i].course_next;
            if !self.mechs[i].alive || next >= m.course.len() {
                continue;
            }
            if self.map_delta(self.mechs[i].chest(), m.course[next]).len() < CHECKPOINT_RADIUS {
                self.mechs[i].course_next += 1;
                if self.mechs[i].course_next == m.course.len() {
                    self.mechs[i].finished_at = Some(m.ticks);
                }
            }
        }
    }

    /// Success with no reason, or failure with the copy key of why.
    fn judge(&self, m: &Mission) -> Option<(bool, Option<&'static str>)> {
        let p = self.player();
        if !p.alive {
            return Some((false, Some("fail_destroyed")));
        }
        if m.kind == Kind::Defend && self.structures_lost() > m.may_lose {
            return Some((false, Some("fail_structures")));
        }
        let escaped = self.craft.iter().any(|c| {
            c.alive
                && c.goal
                    .is_some_and(|g| self.map_delta(c.pos, v3(g.x, c.pos.y, g.z)).len() < ESCAPE_RADIUS)
        });
        if escaped {
            return Some((false, Some("fail_escaped")));
        }
        if m.kind == Kind::Race {
            if p.finished_at.is_some() {
                return Some((true, None));
            }
            if self.mechs.iter().skip(1).any(|r| r.finished_at.is_some()) {
                return Some((false, Some("fail_beaten")));
            }
        }
        if m.time_limit > 0 && m.ticks >= m.time_limit {
            return Some(if m.kind == Kind::Survive {
                (true, None)
            } else {
                (false, Some("fail_time"))
            });
        }
        let cleared = self.hostiles_alive() == 0
            && m.wave >= m.last_wave
            && m.reserve_craft.is_empty()
            && m.reserve_mechs.is_empty();
        if cleared && matches!(m.kind, Kind::Destroy | Kind::Defend | Kind::Duel) {
            return Some((true, None));
        }
        None
    }

    /// Where the player is sent next, if anywhere.
    pub fn objective_point(&self) -> Option<V3> {
        let m = self.mission.as_ref()?;
        if m.ended_at.is_some() || m.kind != Kind::Race {
            return None;
        }
        m.course.get(self.player().course_next).copied()
    }

    /// The structures that have fallen.
    pub fn structures_lost(&self) -> i32 {
        self.structures.iter().filter(|s| !s.alive).count() as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_rewards_ap_kept_and_speed() {
        assert_eq!(rank(true, 200, 100, 300), "S");
        assert_eq!(rank(true, 300, 85, 300), "A");
        assert_eq!(
            rank(true, 360, 95, 300),
            "B",
            "a minute over par costs 30: 95 − 30 = 65"
        );
        assert_eq!(rank(true, 300, 10, 300), "C");
        assert_eq!(rank(false, 10, 100, 300), "D");
    }

    use crate::combat::Target;
    use crate::fx::{int, ONE};
    use crate::mech::Controls;
    use crate::world::tests::mission_world;

    fn run(w: &mut World, ticks: u32) {
        for _ in 0..ticks {
            w.tick(Controls::default());
        }
    }

    fn outcome(w: &World) -> (Option<bool>, Option<&'static str>) {
        let m = w.mission.as_ref().unwrap();
        (m.success, m.reason)
    }

    #[test]
    fn destroying_both_gunships_wins_and_counts_toward_the_plus() {
        let mut w = mission_world("halden-1");
        assert_eq!(w.craft.len(), 2);
        w.hurt(Target::Craft(0), 1_000_000, 0);
        run(&mut w, 1);
        assert_eq!(outcome(&w).0, None, "one gunship left");
        w.hurt(Target::Craft(1), 1_000_000, 0);
        run(&mut w, 1);
        assert_eq!(outcome(&w), (Some(true), None));
        let m = w.mission.as_ref().unwrap();
        assert!(m.plus_met(&w), "no rounds fired is under ten");
    }

    #[test]
    fn firing_more_than_ten_rounds_misses_the_plus() {
        let mut w = mission_world("halden-1");
        let rifle = Controls {
            fire: [false, true, false],
            ..Controls::default()
        };
        for _ in 0..11 * 8 {
            w.tick(rifle);
        }
        assert!(
            w.mission.as_ref().unwrap().rounds > 10,
            "{}",
            w.mission.as_ref().unwrap().rounds
        );
        for i in 0..w.craft.len() {
            w.hurt(Target::Craft(i), 1_000_000, 0);
        }
        run(&mut w, 1);
        let m = w.mission.as_ref().unwrap();
        assert_eq!(m.success, Some(true));
        assert!(!m.plus_met(&w));
    }

    #[test]
    fn a_race_is_won_by_passing_every_checkpoint_first() {
        let mut w = mission_world("halden-2");
        let course = w.mission.as_ref().unwrap().course.clone();
        for cp in &course {
            w.mechs[0].body.pos = cp.sub(w.mechs[0].rig.chest());
            w.mechs[0].body.grounded = false;
            run(&mut w, 1);
        }
        assert_eq!(outcome(&w), (Some(true), None));
    }

    #[test]
    fn the_rival_finishes_the_course_if_the_player_does_not_race() {
        let mut w = mission_world("halden-2");
        let n = w.mission.as_ref().unwrap().course.len();
        run(&mut w, 170 * 60);
        assert!(
            w.mechs[1].course_next >= n * 2 / 3,
            "the racer reached checkpoint {} of {n}",
            w.mechs[1].course_next
        );
        assert_eq!(outcome(&w), (Some(false), Some("fail_beaten")));
    }

    #[test]
    fn a_race_allows_no_shots() {
        let mut w = mission_world("halden-2");
        run(&mut w, 1);
        w.tick(Controls {
            fire: [true; 3],
            ..Controls::default()
        });
        assert!(w.shots.is_empty());
    }

    #[test]
    fn losing_more_structures_than_allowed_fails_a_defence() {
        let mut w = mission_world("halden-3");
        let may = w.mission.as_ref().unwrap().may_lose as usize;
        for i in 0..=may {
            w.hurt(Target::Structure(i), 1_000_000, 0);
        }
        run(&mut w, 1);
        assert_eq!(outcome(&w), (Some(false), Some("fail_structures")));
    }

    #[test]
    fn a_defence_is_won_when_every_wave_is_down() {
        let mut w = mission_world("halden-3");
        for _ in 0..4 {
            for i in 0..w.craft.len() {
                if w.craft[i].alive {
                    w.hurt(Target::Craft(i), 1_000_000, 0);
                }
            }
            run(&mut w, 1);
        }
        assert_eq!(w.mission.as_ref().unwrap().wave, 2, "three waves rose");
        assert_eq!(outcome(&w), (Some(true), None));
    }

    #[test]
    fn a_convoy_that_reaches_its_gate_fails_the_mission() {
        let mut w = mission_world("sere-1");
        let goal = w.craft[0].goal.unwrap();
        w.craft[0].pos = v3(goal.x, 0, goal.z + int(5));
        run(&mut w, 1);
        assert_eq!(outcome(&w), (Some(false), Some("fail_escaped")));
    }

    #[test]
    fn surviving_until_the_clock_runs_out_wins() {
        let mut w = mission_world("spindle-2");
        w.mechs[0].ap = i32::MAX / 4;
        let limit = w.mission.as_ref().unwrap().time_limit;
        run(&mut w, limit);
        assert_eq!(outcome(&w), (Some(true), None));
    }

    #[test]
    fn the_mirror_frame_fights_back() {
        let mut w = mission_world("halden-4");
        assert_eq!(w.mechs.len(), 2);
        assert_eq!(
            w.mechs[1].stats.ap, w.mechs[0].stats.ap,
            "as dangerous as you: the same frame"
        );
        let ap = w.mechs[0].ap;
        run(&mut w, 40 * 60);
        assert!(w.mechs[0].ap < ap, "forty seconds of the mirror did no damage");
        assert!(w.mechs[1].intent.is_some());
        w.hurt(Target::Mech(1), 1_000_000, 0);
        run(&mut w, 1);
        assert_eq!(outcome(&w).0, Some(true));
    }

    #[test]
    fn running_out_of_time_fails() {
        let mut w = mission_world("halden-1");
        let limit = w.mission.as_ref().unwrap().time_limit;
        run(&mut w, limit);
        assert_eq!(outcome(&w), (Some(false), Some("fail_time")));
        assert!(w.mission.as_ref().unwrap().time_left() == Some(0));
    }

    #[test]
    fn every_mission_builds_with_its_player_on_solid_footing() {
        let camp = crate::campaign::tests::campaign();
        for m in camp.planets.iter().flat_map(|p| &p.missions) {
            let mut w = mission_world(&m.id);
            run(&mut w, 30);
            let p = w.player();
            assert!(p.alive, "{}", m.id);
            let (lo, hi) = (
                p.body.pos.sub(v3(int(2), 0, int(2))),
                p.body.pos.add(v3(int(2), int(6), int(2))),
            );
            assert!(
                !w.map.box_blocked(v3(lo.x, lo.y + ONE, lo.z), hi),
                "{} starts inside a building",
                m.id
            );
            assert!(
                w.hostiles_alive() > 0 || m.kind == Kind::Race,
                "{} has nothing to fight",
                m.id
            );
        }
    }

    #[test]
    fn the_shipped_units_parse() {
        let u = Units::parse(include_str!("../../../data/units.json")).unwrap();
        assert!(u.0["tank"].gun.is_some());
        assert!(u.0["drone"].gun.is_none());
    }
}
