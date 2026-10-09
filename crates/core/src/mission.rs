//! Missions, from data: what spawns and when, what must be protected, the
//! course a race runs, how the mission is won or lost, and the plus challenge
//! that asks for the same win under a constraint. Phases advance only here.

use crate::combat::{Craft, CraftKind, Team};
use crate::course::{self, Gate, GateKind, GateSpec, MachineSpec};
use crate::fx::{deg, int};
use crate::geom::{v3, V3};
use crate::mech::TICKS_PER_SECOND;
use crate::objects::{
    ArtillerySpec, EscortSpec, JamSpec, LavaSpec, ResupplySpec, SearchlightSpec, ShieldSpec,
};
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
    /// take a fortified position stage by stage
    Assault,
    /// stop a moving target before it gets away
    Intercept,
    /// find and scan, unseen if you can
    Recon,
    /// a simple job that turns into a fight
    Ambush,
    /// get out before the place comes down
    Escape,
    /// get to the top
    Climb,
    /// keep an ally alive to the end of its route
    Escort,
    /// get inside, break what matters, get out
    Sabotage,
}

impl Kind {
    pub fn objective(self) -> &'static str {
        match self {
            Kind::Destroy => "obj_destroy",
            Kind::Race => "obj_race",
            Kind::Defend => "obj_defend",
            Kind::Duel => "obj_duel",
            Kind::Survive => "obj_survive",
            Kind::Assault => "obj_assault",
            Kind::Intercept => "obj_intercept",
            Kind::Recon => "obj_recon",
            Kind::Ambush => "obj_ambush",
            Kind::Escape => "obj_escape",
            Kind::Climb => "obj_climb",
            Kind::Escort => "obj_escort",
            Kind::Sabotage => "obj_sabotage",
        }
    }

    /// Whether the mission is won by finishing its stages.
    pub fn staged(self) -> bool {
        matches!(
            self,
            Kind::Assault | Kind::Recon | Kind::Ambush | Kind::Escape | Kind::Climb | Kind::Sabotage
        )
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
    /// its AP, in place of its kind's
    #[serde(default)]
    pub ap: Option<i32>,
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
    /// pulse armour: a shield that turns this much damage, then is down a while
    #[serde(default)]
    pub pulse_ap: i32,
    #[serde(default = "pulse_down")]
    pub pulse_down_ms: i32,
    /// a giant: percent of standard size, of its parts' speed, of its weapons' damage
    #[serde(default = "hundred")]
    pub scale_pct: i32,
    #[serde(default = "hundred")]
    pub speed_pct: i32,
    #[serde(default = "hundred")]
    pub damage_pct: i32,
    /// turrets riding on its body, x, y, z centimetres at standard size
    #[serde(default)]
    pub turrets: Vec<[i32; 3]>,
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
    /// height of its underside above the ground (m): a tunnel's roof, a ledge
    #[serde(default)]
    pub base: i32,
}

/// One step of a staged mission: what to do, the wave that rises for it, and
/// where to get to, if anywhere.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    pub objective: String,
    pub wave: u32,
    /// x, z, altitude, radius (m)
    #[serde(default)]
    pub reach: Option<[i32; 4]>,
    /// whether its hostiles must be down too; a scan or a run need not
    #[serde(default = "yes")]
    pub fight: bool,
    /// seconds to stay at the place, scanning
    #[serde(default)]
    pub hold_s: i32,
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
    /// be seen by a searchlight no more than this many times
    MaxAlarms,
    /// bring the escort home with at least this share of its AP, in percent
    EscortApPct,
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

fn pulse_down() -> i32 {
    6000
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
    /// a race's gates, in order
    #[serde(default)]
    pub course: Vec<GateSpec>,
    /// a race's moving parts: lifts, sweepers, pistons and doors
    #[serde(default)]
    pub machines: Vec<MachineSpec>,
    #[serde(default)]
    pub time_limit_s: i32,
    #[serde(default = "yes")]
    pub weapons: bool,
    /// scales the AP and damage of every unit
    #[serde(default = "hundred")]
    pub power_pct: i32,
    /// steps taken in order; with stages the mission is won by finishing the last
    #[serde(default)]
    pub stages: Vec<Stage>,
    /// rectangles x0, z0, x1, z1 (m) cleared of the planet's buildings first
    #[serde(default)]
    pub clear: Vec<[i32; 4]>,
    #[serde(default)]
    pub fortresses: Vec<FortressSpec>,
    /// lights that raise the alarm, and the wave the alarm brings
    #[serde(default)]
    pub searchlights: Vec<SearchlightSpec>,
    #[serde(default)]
    pub alarm_wave: Option<u32>,
    #[serde(default)]
    pub shields: Vec<ShieldSpec>,
    #[serde(default)]
    pub artillery: Option<ArtillerySpec>,
    #[serde(default)]
    pub lava: Option<LavaSpec>,
    #[serde(default)]
    pub jam: Vec<JamSpec>,
    #[serde(default)]
    pub resupply: Vec<ResupplySpec>,
    #[serde(default)]
    pub escort: Option<EscortSpec>,
}

/// One box of a fortress's hull: centre and size in metres from its footprint.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HullBox {
    pub at: [i32; 3],
    pub size: [i32; 3],
    #[serde(default)]
    pub paint: String,
}

/// An arms fort: a walking hull to land on, weak points on its legs and
/// engines, a core sealed until they fall, and guns along its decks.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FortressSpec {
    pub name: String,
    /// where its footprint starts, x, z (m)
    pub at: [i32; 2],
    /// the points it walks between, in a loop, x, z (m)
    #[serde(default)]
    pub path: Vec<[i32; 2]>,
    pub speed_ms: i32,
    /// how many standard frames long it is, for the record and the tests
    pub size_x: i32,
    pub hull: Vec<HullBox>,
    /// x, y, z (m) from its footprint
    pub weak_points: Vec<[i32; 3]>,
    pub weak_ap: i32,
    pub weak_radius_m: i32,
    pub core: [i32; 3],
    pub core_ap: i32,
    pub core_radius_m: i32,
    #[serde(default)]
    pub turrets: Vec<[i32; 3]>,
    #[serde(default)]
    pub batteries: Vec<[i32; 3]>,
    /// it runs its path once, and the mission fails if it gets to the end
    #[serde(default)]
    pub escapes: bool,
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
        for k in [
            "drone",
            "heli",
            "tank",
            "turret",
            "battery",
            "weak",
            "core",
            "mine",
            "generator",
        ] {
            if !u.contains_key(k) {
                return Err(format!("units: no {k}"));
            }
        }
        Ok(Units(u))
    }
}

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
        "battery" => CraftKind::Battery,
        "weak" => CraftKind::Weak,
        "core" => CraftKind::Core,
        "mine" => CraftKind::Mine,
        "generator" => CraftKind::Generator,
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
        mount: None,
        team: Team::Enemy,
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

/// A stage as flown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageNow {
    pub objective: String,
    pub wave: u32,
    /// where to reach, and how near
    pub reach: Option<(V3, i32)>,
    pub fight: bool,
    /// ticks to stay there
    pub hold: u32,
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
    pub reserve_mechs: Vec<(u32, crate::world::Mech, Vec<Craft>)>,
    pub course: Vec<Gate>,
    pub stages: Vec<StageNow>,
    pub stage: usize,
    /// ticks spent at the current stage's place
    pub held: u32,
    /// whether a searchlight has raised the alarm, and how many times one has seen the player
    pub alarm: bool,
    pub alarms: i32,
    pub alarm_wave: Option<u32>,
    pub artillery: Option<ArtillerySpec>,
    pub lava: Option<LavaSpec>,
    pub jam: Vec<JamSpec>,
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
            course: course::gates(&spec.course, v3(int(spec.start[0]), int(5), int(spec.start[1]))),
            stages: spec
                .stages
                .iter()
                .map(|s| StageNow {
                    objective: s.objective.clone(),
                    wave: s.wave,
                    reach: s.reach.map(|[x, z, a, r]| (v3(int(x), int(a), int(z)), int(r))),
                    fight: s.fight,
                    hold: (s.hold_s * TICKS_PER_SECOND) as u32,
                })
                .collect(),
            stage: 0,
            held: 0,
            alarm: false,
            alarms: 0,
            alarm_wave: spec.alarm_wave,
            artillery: spec.artillery.clone(),
            lava: spec.lava,
            jam: spec.jam.clone(),
        }
    }

    pub fn staged(&self) -> bool {
        !self.stages.is_empty()
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
                PlusRule::MaxAlarms => self.alarms <= v,
                PlusRule::EscortApPct => w.escort_ap_pct() >= v,
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
            PlusRule::MaxAlarms => self.alarms,
            PlusRule::EscortApPct => w.escort_ap_pct(),
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
    /// Hostiles still fighting: enemy frames and craft. Mines lie in wait and
    /// need not be cleared.
    pub fn hostiles_alive(&self) -> usize {
        self.craft
            .iter()
            .filter(|c| c.alive && c.team == Team::Enemy && c.kind != CraftKind::Mine)
            .count()
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
            if p.alive && self.burn_rate(0) > 0 {
                m.floor_ticks += 1;
            }
            if m.staged() {
                self.update_stage(&mut m);
            } else if self.hostiles_alive() == 0 && m.wave < m.last_wave {
                // the next wave rises when this one is down
                m.wave += 1;
                let w = m.wave;
                self.release_wave(&mut m, w);
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

    pub fn release_wave(&mut self, m: &mut Mission, w: u32) {
        let (now, later): (Vec<Craft>, Vec<Craft>) = m.reserve_craft.drain(..).partition(|c| c.wave == w);
        m.reserve_craft = later;
        self.craft.extend(now);
        let (now, later): (Vec<_>, Vec<_>) = m.reserve_mechs.drain(..).partition(|(wv, _, _)| *wv == w);
        m.reserve_mechs = later;
        for (_, mech, riders) in now {
            self.add_mech(mech, riders);
        }
    }

    /// A stage is done when its hostiles are down (unless it is a run or a
    /// scan) and the player has got to its place, if it names one, and stayed
    /// there long enough; then the next stage's wave rises.
    fn update_stage(&mut self, m: &mut Mission) {
        let Some(st) = m.stages.get(m.stage).cloned() else {
            return;
        };
        let there = st
            .reach
            .is_none_or(|(p, r)| self.map_delta(self.player().chest(), p).len() < r);
        m.held = if there && st.reach.is_some() {
            m.held + 1
        } else {
            0
        };
        let stayed = m.held >= st.hold;
        if (!st.fight || self.hostiles_alive() == 0) && there && stayed {
            m.stage += 1;
            m.held = 0;
            if let Some(next) = m.stages.get(m.stage).cloned() {
                m.wave = next.wave;
                self.release_wave(m, next.wave);
            }
        }
    }

    /// Moves every racer's next gate on when it flies through a ring or lands
    /// on a pad; a boost ring throws it on, and a switch opens its door.
    fn update_course(&mut self, m: &Mission) {
        if m.course.is_empty() {
            return;
        }
        for i in 0..self.mechs.len() {
            let next = self.mechs[i].course_next;
            if !self.mechs[i].alive || next >= m.course.len() {
                continue;
            }
            let g = &m.course[next];
            let at = self.gate_point(g);
            let me = &self.mechs[i];
            let passed = if g.kind.landing() {
                course::landed(g, at, at.add(self.map_delta(at, me.body.pos)), me.body.grounded)
            } else {
                // the chest's path this tick, measured from the gate so a ring's seam does not matter
                let chest = me.rig.chest().scale(me.scale);
                let a = at.add(self.map_delta(at, me.body.prev_pos.add(chest)));
                let b = at.add(self.map_delta(at, me.chest()));
                course::through_ring(g, at, a, b)
            };
            if !passed {
                continue;
            }
            let me = &mut self.mechs[i];
            me.course_next += 1;
            if me.course_next == m.course.len() {
                me.finished_at = Some(m.ticks);
            }
            if g.kind == GateKind::Boost {
                me.body.vel = course::boost(g);
                me.body.glide = true;
            }
            if let Some(d) = g.opens.and_then(|d| self.machines.get_mut(d)) {
                d.open = true;
            }
        }
    }

    /// Where a gate is now: a gate riding a machine moves with it.
    pub fn gate_point(&self, g: &Gate) -> V3 {
        match g.ride.and_then(|k| self.machines.get(k)) {
            Some(mc) => g.at.add(mc.offset),
            None => g.at,
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
        let got_away = self.fortresses.iter().any(|f| f.alive && f.escapes && f.arrived);
        if escaped || got_away {
            return Some((false, Some("fail_escaped")));
        }
        if let Some(e) = &self.escort {
            if !self.craft[e.craft].alive {
                return Some((false, Some("fail_escort")));
            }
            if e.arrived {
                return Some((true, None));
            }
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
        if m.staged() {
            return (m.stage >= m.stages.len()).then_some((true, None));
        }
        if cleared
            && matches!(
                m.kind,
                Kind::Destroy | Kind::Defend | Kind::Duel | Kind::Intercept
            )
        {
            return Some((true, None));
        }
        None
    }

    /// Where the player is sent next, if anywhere.
    pub fn objective_point(&self) -> Option<V3> {
        let m = self.mission.as_ref()?;
        if m.ended_at.is_some() {
            return None;
        }
        if let Some((p, _)) = m.stages.get(m.stage).and_then(|st| st.reach) {
            return Some(p);
        }
        if let Some(e) = &self.escort {
            return Some(self.craft[e.craft].pos);
        }
        if m.kind != Kind::Race {
            return None;
        }
        m.course
            .get(self.player().course_next)
            .map(|g| self.gate_point(g))
    }

    /// The escort's AP, in percent; 0 with no escort.
    pub fn escort_ap_pct(&self) -> i32 {
        self.escort.as_ref().map_or(0, |e| {
            let c = &self.craft[e.craft];
            c.ap * 100 / c.max_ap.max(1)
        })
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
        let mut passed = 0;
        for g in &course {
            let at = w.gate_point(g);
            if g.kind.landing() {
                w.mechs[0].body.pos = at;
                w.mechs[0].body.prev_pos = at;
                w.mechs[0].body.vel = V3::ZERO;
                run(&mut w, 1);
            } else {
                let chest = w.mechs[0].rig.chest();
                w.mechs[0].body.pos = at.sub(chest).sub(g.normal.scale(ONE / 2));
                w.mechs[0].body.vel = g.normal.scale(ONE);
                w.mechs[0].body.grounded = false;
                run(&mut w, 1);
            }
            passed += 1;
            assert_eq!(w.mechs[0].course_next, passed, "{g:?}");
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

    fn race_at(w: &mut World, kind: GateKind) -> usize {
        let k = w
            .mission
            .as_ref()
            .unwrap()
            .course
            .iter()
            .position(|g| g.kind == kind)
            .unwrap();
        w.mechs[0].course_next = k;
        k
    }

    fn stand(w: &mut World, at: V3) {
        w.mechs[0].body.pos = at;
        w.mechs[0].body.prev_pos = at;
        w.mechs[0].body.vel = V3::ZERO;
        w.mechs[0].body.grounded = true;
    }

    #[test]
    fn landing_on_a_switch_opens_its_door_and_the_door_clears_the_way() {
        let mut w = mission_world("halden-2");
        let k = race_at(&mut w, GateKind::Switch);
        let g = w.mission.as_ref().unwrap().course[k].clone();
        let door = g.opens.unwrap();
        run(&mut w, 120);
        assert_eq!(w.machines[door].offset, V3::ZERO, "shut until the switch");
        let at = w.gate_point(&g);
        stand(&mut w, at);
        run(&mut w, 1);
        assert_eq!(w.mechs[0].course_next, k + 1);
        assert!(w.machines[door].open);
        run(&mut w, 120);
        let top = w.map.movers[w.machines[door].block].max.y;
        assert!(top <= 0, "sunk into the ground, its top at {top}");
    }

    #[test]
    fn a_pad_is_not_passed_by_flying_over_it() {
        let mut w = mission_world("halden-2");
        let k = race_at(&mut w, GateKind::Pad);
        let g = w.mission.as_ref().unwrap().course[k].clone();
        let over = w.gate_point(&g).add(v3(0, int(6), 0));
        w.mechs[0].body.pos = over;
        w.mechs[0].body.grounded = false;
        run(&mut w, 1);
        assert_eq!(w.mechs[0].course_next, k, "still to land");
    }

    #[test]
    fn a_sweeper_throws_a_frame_aside_and_stuns_it() {
        let mut w = mission_world("halden-2");
        let sweeper = w
            .machines
            .iter()
            .position(|m| m.role == course::Role::Sweeper)
            .unwrap();
        let mut hit = false;
        let at = w.machines[sweeper].centre().add(v3(int(20), 0, 0));
        for _ in 0..6 * 60 {
            stand(&mut w, v3(at.x, 0, at.z));
            run(&mut w, 1);
            if w.mechs[0].stagger > 0 {
                hit = true;
                break;
            }
        }
        assert!(hit, "the sweeper's run crosses where the frame stands");
        assert!(w.mechs[0].body.vel.len_xz() > int(1) / 4, "thrown");
        let (lo, hi) = w.mechs[0].bounds();
        assert!(
            !w.map.movers[w.machines[sweeper].block].overlaps(lo, hi),
            "and out of the bar"
        );
        assert_eq!(w.mechs[0].ap, w.mechs[0].stats.ap, "a knock costs time, not AP");
    }

    #[test]
    fn a_lift_carries_whoever_stands_on_it() {
        let mut w = mission_world("halden-2");
        let lift = w
            .machines
            .iter()
            .position(|m| m.role == course::Role::Lift)
            .unwrap();
        let top = w.map.movers[w.machines[lift].block].max;
        let centre = w.machines[lift].centre();
        stand(&mut w, v3(centre.x, top.y, centre.z));
        let start = w.mechs[0].body.pos.y;
        let mut highest = start;
        for _ in 0..8 * 60 {
            run(&mut w, 1);
            highest = highest.max(w.mechs[0].body.pos.y);
        }
        assert!(highest >= start + int(25), "rode up from {start} to {highest}");
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
            kill_all(&mut w);
        }
        assert_eq!(w.mission.as_ref().unwrap().wave, 2, "three waves rose");
        assert_eq!(outcome(&w), (Some(true), None));
    }

    #[test]
    fn a_train_that_reaches_the_tunnel_fails_the_intercept() {
        let mut w = mission_world("sere-1");
        let end = *w.fortresses[0].path.last().unwrap();
        run(&mut w, 60);
        assert_eq!(outcome(&w).0, None, "still running");
        assert!(w.fortresses[0].pos.z < int(760), "it runs south");
        w.fortresses[0].pos = end.add(v3(0, 0, ONE / 8));
        run(&mut w, 2);
        assert!(w.fortresses[0].arrived, "arrived at {:?}", w.fortresses[0].pos);
        assert_eq!(outcome(&w), (Some(false), Some("fail_escaped")));
    }

    #[test]
    fn breaking_the_trains_cars_opens_its_engine_and_breaking_that_wins() {
        let mut w = mission_world("sere-1");
        let core = w.fortresses[0].core;
        w.hurt(Target::Craft(core), 1_000_000, 0);
        assert!(w.craft[core].alive, "sealed while a car stands");
        kill_all(&mut w);
        assert!(!w.craft[core].alive);
        assert_eq!(outcome(&w), (Some(true), None));
    }

    #[test]
    fn a_searchlight_that_finds_you_raises_the_alarm_and_wakes_the_nest() {
        let mut w = mission_world("sere-2");
        run(&mut w, 30);
        assert_eq!(w.hostiles_alive(), 0, "the nest sleeps");
        assert!(!w.mission.as_ref().unwrap().alarm);
        let spot = w.searchlights[0].spot;
        put(&mut w, spot.x >> 16, 0, spot.z >> 16);
        let m = w.mission.as_ref().unwrap();
        assert!(m.alarm, "seen standing in the light");
        assert_eq!(m.alarms, 1);
        assert!(
            w.hostiles_alive() >= 8,
            "the turrets, the gunships and the strider are out"
        );
    }

    #[test]
    fn a_scan_needs_the_player_to_stay_and_needs_no_fight() {
        let mut w = mission_world("sere-2");
        w.searchlights.clear();
        let (at, _) = w.mission.as_ref().unwrap().stages[0].reach.unwrap();
        let p = at.sub(w.mechs[0].rig.chest());
        w.mechs[0].body.pos = p;
        run(&mut w, 60);
        assert_eq!(stage(&w), 0, "a second is not enough");
        w.mechs[0].body.pos = p.add(v3(int(40), 0, 0));
        run(&mut w, 1);
        w.mechs[0].body.pos = p;
        run(&mut w, 2 * 60);
        assert_eq!(stage(&w), 0, "stepping away starts it over");
        run(&mut w, 70);
        assert_eq!(stage(&w), 1, "three seconds in one go");
    }

    #[test]
    fn the_scorpion_waits_until_the_routine_job_is_done() {
        let mut w = mission_world("sere-4");
        assert!(
            w.mechs.iter().all(|m| m.team == crate::combat::Team::Player),
            "no frame yet"
        );
        kill_all(&mut w);
        let frames: Vec<&str> = w.mechs.iter().skip(1).map(|m| m.name.as_str()).collect();
        assert_eq!(frames, ["SCORPION", "STINGER"], "the ambush");
        assert_eq!(outcome(&w).0, None);
        kill_all(&mut w);
        assert_eq!(outcome(&w), (Some(true), None));
    }

    #[test]
    fn surviving_until_the_clock_runs_out_wins() {
        // no shipped mission is a survival now; the rule stays, tried on the first map
        let mut w = mission_world("halden-1");
        w.mission.as_mut().unwrap().kind = Kind::Survive;
        kill_all(&mut w);
        assert_eq!(outcome(&w).0, None, "a survival is not won by clearing the sky");
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
                w.hostiles_alive() > 0 || matches!(m.kind, Kind::Race | Kind::Recon | Kind::Sabotage),
                "{} has nothing to fight",
                m.id
            );
        }
    }

    fn kill_all(w: &mut World) {
        for i in 0..w.craft.len() {
            if w.craft[i].alive {
                w.hurt(Target::Craft(i), 1_000_000, 0);
            }
        }
        for i in 1..w.mechs.len() {
            if w.mechs[i].alive && w.mechs[i].team == crate::combat::Team::Enemy {
                w.hurt(Target::Mech(i), 10_000_000, 0);
            }
        }
        run(w, 1);
    }

    fn stage(w: &World) -> usize {
        w.mission.as_ref().unwrap().stage
    }

    fn put(w: &mut World, x: i32, y: i32, z: i32) {
        w.mechs[0].body.pos = v3(int(x), int(y), int(z));
        w.mechs[0].body.prev_pos = w.mechs[0].body.pos;
        w.mechs[0].body.grounded = false;
        run(w, 2);
    }

    #[test]
    fn the_wall_is_taken_stage_by_stage_and_its_guardian_waits_on_top() {
        let mut w = mission_world("halden-5");
        assert_eq!(stage(&w), 0);
        kill_all(&mut w);
        assert_eq!(stage(&w), 1, "the approach is clear");
        assert!(
            w.craft.iter().filter(|c| c.alive).all(|c| c.pos.y < int(30)),
            "the next guns are inside the tunnel"
        );
        kill_all(&mut w);
        assert_eq!(
            stage(&w),
            1,
            "clearing the tunnel is not enough: the player must come through"
        );
        put(&mut w, 0, 0, -960);
        assert_eq!(stage(&w), 2, "through the gate into the inner yard");
        kill_all(&mut w);
        put(&mut w, 30, 181, -1070);
        assert_eq!(stage(&w), 3, "on top of the wall");
        let guardian = w
            .mechs
            .iter()
            .find(|m| m.name == "GATEKEEPER")
            .expect("the guardian has come out");
        assert!(
            guardian.body.pos.y >= int(180),
            "it stands on the wall top, at {}",
            guardian.body.pos.y
        );
        kill_all(&mut w);
        assert_eq!(outcome(&w), (Some(true), None));
    }

    #[test]
    fn the_wall_lets_a_frame_through_its_gate_and_nowhere_else() {
        let w = mission_world("halden-5");
        let walk = |x: i32| {
            let mut w = w.clone();
            w.mechs[0].body.pos = v3(int(x), 0, -int(800));
            w.mechs[0].body.aim_yaw = 0;
            w.mission.as_mut().unwrap().time_limit = 0;
            for _ in 0..20 * 60 {
                w.tick(Controls {
                    move_z: ONE,
                    ..Controls::default()
                });
            }
            w.mechs[0].body.pos.z
        };
        assert!(walk(0) < -int(915), "through the tunnel");
        assert!(walk(200) > -int(850), "stopped at the face");
    }

    #[test]
    fn the_wall_opens_only_when_the_rest_of_halden_is_clear() {
        let c = crate::campaign::tests::campaign();
        let mut p = crate::campaign::Progress::default();
        for id in ["halden-1", "halden-2", "halden-3"] {
            p.cleared.insert(id.to_string());
        }
        assert_eq!(p.standing(&c, "halden-5"), crate::campaign::Standing::Locked);
        p.cleared.insert("halden-4".to_string());
        assert_eq!(p.standing(&c, "halden-5"), crate::campaign::Standing::Open);
    }

    #[test]
    fn some_fights_put_several_enemy_frames_against_you_at_once() {
        for (id, frames) in [("sere-4", 2), ("spindle-4", 3), ("rime-4", 2)] {
            let w = mission_world(id);
            let held = &w.mission.as_ref().unwrap().reserve_mechs;
            let n = w
                .mechs
                .iter()
                .filter(|m| m.team == crate::combat::Team::Enemy)
                .count()
                + held
                    .iter()
                    .filter(|(_, m, _)| m.team == crate::combat::Team::Enemy)
                    .count();
            assert_eq!(n, frames, "{id}");
        }
    }

    /// One rifle round from `from` at the hub's coolant plant; returns the AP it took.
    fn shoot_core(w: &mut World, from: V3) -> i32 {
        let core = w.craft.iter().position(|c| c.kind == CraftKind::Core).unwrap();
        let before = w.craft[core].ap;
        let rifle =
            crate::combat::Weapon::from_part(crate::parts::tests::catalog().get("rf-marrow").unwrap());
        let shots =
            crate::combat::Shot::fire(&rifle, Team::Player, from, w.craft[core].pos, None, &mut w.rng);
        w.shots.extend(shots);
        run(w, 30);
        before - w.craft[core].ap
    }

    #[test]
    fn the_dome_turns_fire_until_its_generators_fall() {
        let mut w = mission_world("spindle-2");
        // only the plant and its generators: the shot test should not be about gunships
        for c in &mut w.craft {
            if !matches!(c.kind, CraftKind::Core | CraftKind::Generator) {
                c.alive = false;
            }
        }
        w.mechs.truncate(1);
        let outside = w.shields[0].centre.add(v3(int(90), int(20), 0));
        assert_eq!(shoot_core(&mut w, outside), 0, "the dome stops it");
        let gens = w.shields[0].gens.clone();
        for g in gens {
            w.hurt(Target::Craft(g), 1_000_000, 0);
        }
        run(&mut w, 1);
        assert!(!w.shields[0].up);
        assert!(
            shoot_core(&mut w, outside) > 0,
            "with the generators down it gets through"
        );
    }

    #[test]
    fn the_ring_comes_down_on_its_schedule_and_a_fallen_slab_stays() {
        let mut w = mission_world("spindle-1");
        let k = 0;
        let top = w.map.movers[w.machines[k].block].max.y;
        run(&mut w, 2 * 60);
        assert_eq!(
            w.map.movers[w.machines[k].block].max.y, top,
            "still up after two seconds"
        );
        run(&mut w, 4 * 60);
        let down = w.map.movers[w.machines[k].block];
        assert!(down.min.y <= ONE, "fallen to the floor, at {}", down.min.y);
        run(&mut w, 60);
        assert_eq!(w.map.movers[w.machines[k].block], down, "and it stays");
    }

    #[test]
    fn the_escape_is_a_run_not_a_fight() {
        let mut w = mission_world("spindle-1");
        assert!(w.hostiles_alive() > 0);
        let reaches: Vec<V3> = w
            .mission
            .as_ref()
            .unwrap()
            .stages
            .iter()
            .map(|s| s.reach.unwrap().0)
            .collect();
        for p in reaches {
            put(&mut w, p.x >> 16, 0, p.z >> 16);
        }
        assert_eq!(outcome(&w), (Some(true), None), "every gunship is still up");
    }

    #[test]
    fn artillery_marks_the_ground_then_lands_on_it() {
        let mut w = mission_world("spindle-4");
        // the frames stay in the fight but sit it out, far off
        for m in w.mechs.iter_mut().skip(1) {
            m.pilot = None;
            m.body.pos = v3(0, 0, int(1500));
        }
        let a = w.mission.as_ref().unwrap().artillery.clone().unwrap();
        run(&mut w, (a.every_ms * TICKS_PER_SECOND / 1000) as u32);
        assert_eq!(w.strikes.len(), 1, "a mark on the ground");
        let mark = w.strikes[0];
        assert!(
            w.map_delta(mark.at, w.mechs[0].body.pos).len_xz() < int(2),
            "under a frame standing still"
        );
        let ap = w.mechs[0].ap;
        run(&mut w, mark.ticks as u32 - 1);
        assert_eq!(w.mechs[0].ap, ap, "nothing until it lands");
        run(&mut w, 1);
        assert!(w.mechs[0].ap < ap, "then it does");
        assert!(w.strikes.is_empty());
    }

    #[test]
    fn an_alarm_in_the_foundry_seals_the_door_you_came_in_by() {
        let mut w = mission_world("cinder-3");
        let door = w.machines[0].block;
        let open_bottom = w.map.movers[door].min.y;
        assert!(open_bottom >= int(30), "open: up in the roof");
        let spot = w.searchlights[0].spot;
        put(&mut w, spot.x >> 16, 3, spot.z >> 16);
        assert!(w.mission.as_ref().unwrap().alarm);
        run(&mut w, 3 * 60);
        assert!(
            w.map.movers[door].min.y <= int(3),
            "shut, at {}",
            w.map.movers[door].min.y
        );
    }

    #[test]
    fn a_mine_bursts_when_a_frame_comes_near_and_not_before() {
        let mut w = mission_world("cinder-3");
        w.searchlights.clear();
        let k = w.craft.iter().position(|c| c.kind == CraftKind::Mine).unwrap();
        let at = w.craft[k].pos;
        put(&mut w, (at.x >> 16) + 20, (at.y >> 16) + 1, at.z >> 16);
        assert!(w.craft[k].alive, "twenty metres off");
        let ap = w.mechs[0].ap;
        put(&mut w, (at.x >> 16) + 2, (at.y >> 16) + 1, at.z >> 16);
        assert!(!w.craft[k].alive, "it burst");
        assert!(w.mechs[0].ap < ap - 500, "and hurt: {} of {ap}", w.mechs[0].ap);
    }

    #[test]
    fn the_aces_pulse_armour_turns_hits_until_it_breaks_then_comes_back() {
        let mut w = mission_world("cinder-4");
        let ace = w.mechs.iter().position(|m| m.name.starts_with("ACE")).unwrap();
        let ap = w.mechs[ace].ap;
        let shield = w.mechs[ace].pulse.unwrap().max;
        w.hurt(Target::Mech(ace), shield / 2, 400);
        assert_eq!(w.mechs[ace].ap, ap, "turned");
        assert_eq!(w.mechs[ace].impact, 0, "impact too");
        w.hurt(Target::Mech(ace), shield, 0);
        assert!(!w.mechs[ace].pulse.unwrap().up(), "broken");
        w.hurt(Target::Mech(ace), 500, 0);
        assert_eq!(w.mechs[ace].ap, ap - 500, "the window");
        run(&mut w, 7 * 60 + 1);
        assert!(w.mechs[ace].pulse.unwrap().up(), "whole again");
    }

    #[test]
    fn the_crawler_drives_its_route_and_its_arrival_wins() {
        let mut w = mission_world("rime-3");
        let e = w.escort.clone().unwrap();
        let start = w.craft[e.craft].pos;
        // keep the raiders out of it
        for c in w.craft.iter_mut().filter(|c| c.team == Team::Enemy) {
            c.alive = false;
        }
        w.mission.as_mut().unwrap().reserve_craft.clear();
        run(&mut w, 10 * 60);
        assert!(
            w.map_delta(start, w.craft[e.craft].pos).len_xz() > int(60),
            "it moves along"
        );
        run(&mut w, 200 * 60);
        assert!(w.escort.as_ref().unwrap().arrived);
        assert_eq!(outcome(&w), (Some(true), None));
    }

    #[test]
    fn raiders_go_for_the_crawler_and_losing_it_fails() {
        let mut w = mission_world("rime-3");
        let k = w.escort.as_ref().unwrap().craft;
        // the player far away and high up, out of every fight
        w.mechs[0].body.pos = v3(-int(700), int(400), int(900));
        let ap = w.craft[k].ap;
        for _ in 0..60 * 60 {
            w.mechs[0].body.pos = v3(-int(700), int(400), int(900));
            w.tick(Controls::default());
            if !w.craft[k].alive {
                break;
            }
        }
        assert!(w.craft[k].ap < ap, "the raiders shot at the crawler");
        w.hurt(Target::Craft(k), 1_000_000, 0);
        run(&mut w, 1);
        assert_eq!(outcome(&w), (Some(false), Some("fail_escort")));
    }

    #[test]
    fn in_the_storm_front_nothing_locks_and_en_drains() {
        let mut w = mission_world("rime-4");
        let twin = w.mechs.iter().position(|m| m.team == Team::Enemy).unwrap();
        // stand still facing a twin, inside the field
        let (me, them) = (w.mechs[0].chest(), w.mechs[twin].chest());
        let (yaw, pitch) = crate::geom::yaw_pitch_of(them.sub(me));
        w.mechs[0].body.aim_yaw = yaw;
        w.mechs[0].body.aim_pitch = pitch;
        let en = w.mechs[0].body.en;
        run(&mut w, 30);
        assert!(w.jammed());
        assert_eq!(w.mechs[0].lock, None);
        assert!(w.mechs[0].body.en < en, "drained");
        w.mission.as_mut().unwrap().jam.clear();
        let (me, them) = (w.mechs[0].chest(), w.mechs[twin].chest());
        let (yaw, pitch) = crate::geom::yaw_pitch_of(them.sub(me));
        w.mechs[0].body.aim_yaw = yaw;
        w.mechs[0].body.aim_pitch = pitch;
        run(&mut w, 1);
        assert!(w.mechs[0].lock.is_some(), "out of the field it locks");
    }

    #[test]
    fn the_needle_is_climbed_then_its_guns_silenced_then_the_colossus_comes() {
        let mut w = mission_world("rime-1");
        let mine_heights: Vec<i32> = w
            .craft
            .iter()
            .filter(|c| c.kind == CraftKind::Mine)
            .map(|c| c.pos.y >> 16)
            .collect();
        assert!(
            mine_heights.iter().all(|h| *h >= 70),
            "mines lie on the ledges: {mine_heights:?}"
        );
        assert_eq!(stage(&w), 0);
        put(&mut w, 0, 266, 0);
        assert_eq!(stage(&w), 1, "on the summit");
        assert!(w.mechs.iter().all(|m| m.team == Team::Player), "no colossus yet");
        kill_all(&mut w);
        let colossus = w
            .mechs
            .iter()
            .find(|m| m.name == "FROST COLOSSUS")
            .expect("it comes");
        assert!(
            colossus.body.pos.y >= int(260),
            "on top: {}",
            colossus.body.pos.y >> 16
        );
        kill_all(&mut w);
        assert_eq!(outcome(&w), (Some(true), None));
    }

    #[test]
    fn the_shipped_units_parse() {
        let u = Units::parse(include_str!("../../../data/units.json")).unwrap();
        assert!(u.0["tank"].gun.is_some());
        assert!(u.0["drone"].gun.is_none());
    }
}
