//! The mission: cross the city to the waypoint, destroy the helicopters that
//! come up to meet you, then defeat the enemy frame. Phases advance only here.

use crate::combat::{Craft, CraftKind, Target, Weapon};
use crate::fx::{deg, int};
use crate::geom::{v3, V3};
use crate::map::MapSpec;
use crate::mech::TICKS_PER_SECOND;
use crate::parts::{Slot, WeaponKind};
use crate::world::{Mech, World};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeliSpec {
    pub x_m: i32,
    pub z_m: i32,
    pub alt_m: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeliGun {
    pub ap: i32,
    pub damage: i32,
    pub speed_ms: i32,
    pub range_m: i32,
    pub burst: i32,
    pub burst_gap_ms: i32,
    pub reload_ms: i32,
    pub spread_deg: i32,
    pub orbit_m: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnemySpec {
    pub name: String,
    pub loadout: BTreeMap<Slot, String>,
    pub pilot: String,
    pub spawn: [i32; 3],
    pub ap_pct: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionSpec {
    pub map: MapSpec,
    pub start: [i32; 2],
    pub start_yaw_deg: i32,
    pub waypoint: [i32; 2],
    pub waypoint_r_m: i32,
    pub helis: Vec<HeliSpec>,
    pub heli: HeliGun,
    pub enemy: EnemySpec,
    pub par_s: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Advance,
    Helis,
    Duel,
    Success,
    Failure,
}

impl Phase {
    /// The copy key of the objective shown in this phase.
    pub fn objective(self) -> &'static str {
        match self {
            Phase::Advance => "obj_advance",
            Phase::Helis => "obj_helis",
            Phase::Duel => "obj_duel",
            Phase::Success => "obj_success",
            Phase::Failure => "obj_failure",
        }
    }

    pub fn over(self) -> bool {
        matches!(self, Phase::Success | Phase::Failure)
    }
}

/// Seconds the world keeps running after the mission ends, before the debrief.
pub const AFTERMATH_TICKS: u32 = 3 * TICKS_PER_SECOND as u32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mission {
    pub phase: Phase,
    pub waypoint: V3,
    pub waypoint_r: i32,
    pub ticks: u32,
    pub ended_at: Option<u32>,
    pub par_s: i32,
    pub boss: Option<usize>,
    pub boss_name: String,
    reserve_helis: Vec<Craft>,
    reserve_boss: Option<Mech>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Debrief {
    pub success: bool,
    pub seconds: u32,
    pub ap_kept_pct: i32,
    pub kills: u32,
    pub rank: &'static str,
}

/// Rank from AP kept and time against par: each second under par is worth
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

pub fn heli(at: V3, g: &HeliGun, phase: i32) -> Craft {
    Craft {
        kind: CraftKind::Heli,
        pos: at,
        prev_pos: at,
        vel: V3::ZERO,
        yaw: 0,
        prev_yaw: 0,
        ap: g.ap,
        max_ap: g.ap,
        alive: true,
        anchor: at,
        phase,
        cooldown: (phase & 0x3f) + TICKS_PER_SECOND,
        burst: 0,
        respawn: 0,
        down_ticks: 0,
    }
}

/// The helicopters' gun, as a weapon the shot rule understands.
pub fn heli_weapon(g: &HeliGun) -> Weapon {
    Weapon {
        kind: WeaponKind::Rifle,
        damage: g.damage,
        impact: g.damage / 2,
        fire_ticks: (g.burst_gap_ms * TICKS_PER_SECOND / 1000).max(1),
        speed: crate::fx::ratio(g.speed_ms, TICKS_PER_SECOND),
        ammo: i32::MAX,
        range: int(g.range_m),
        turn: 0,
        salvo: 1,
        spread: deg(g.spread_deg),
        blast: 0,
    }
}

impl Mission {
    pub fn new(spec: &MissionSpec, boss: Mech) -> Mission {
        let reserve_helis = spec
            .helis
            .iter()
            .enumerate()
            .map(|(i, h)| {
                heli(
                    v3(int(h.x_m), int(h.alt_m), int(h.z_m)),
                    &spec.heli,
                    deg(90) * i as i32,
                )
            })
            .collect();
        Mission {
            phase: Phase::Advance,
            waypoint: v3(int(spec.waypoint[0]), 0, int(spec.waypoint[1])),
            waypoint_r: int(spec.waypoint_r_m),
            ticks: 0,
            ended_at: None,
            par_s: spec.par_s,
            boss: None,
            boss_name: spec.enemy.name.clone(),
            reserve_helis,
            reserve_boss: Some(boss),
        }
    }

    pub fn debrief(&self, w: &World) -> Debrief {
        let p = w.player();
        let seconds = self.ended_at.unwrap_or(self.ticks) / TICKS_PER_SECOND as u32;
        let ap_kept_pct = p.ap * 100 / p.stats.ap.max(1);
        let success = self.phase == Phase::Success;
        Debrief {
            success,
            seconds,
            ap_kept_pct,
            kills: w.kills,
            rank: rank(success, seconds as i32, ap_kept_pct, self.par_s),
        }
    }

    /// Whether the aftermath is over and the debrief should show.
    pub fn finished(&self) -> bool {
        self.ended_at.is_some_and(|t| self.ticks >= t + AFTERMATH_TICKS)
    }
}

impl World {
    /// Advances the mission's phase after a tick.
    pub fn update_mission(&mut self) {
        let Some(mut m) = self.mission.take() else { return };
        m.ticks += 1;
        let player = self.player();
        let flat = |a: V3, b: V3| v3(a.x - b.x, 0, a.z - b.z).len();
        let next = match m.phase {
            _ if m.phase.over() => m.phase,
            _ if !player.alive => Phase::Failure,
            Phase::Advance if flat(player.body.pos, m.waypoint) < m.waypoint_r => {
                self.craft.append(&mut m.reserve_helis);
                Phase::Helis
            }
            Phase::Helis if self.craft.iter().all(|c| !c.alive) => {
                if let Some(boss) = m.reserve_boss.take() {
                    m.boss = Some(self.mechs.len());
                    self.mechs.push(boss);
                }
                Phase::Duel
            }
            Phase::Duel
                if m.boss
                    .is_some_and(|b| self.target_centre(Target::Mech(b)).is_none()) =>
            {
                Phase::Success
            }
            p => p,
        };
        if next.over() && m.ended_at.is_none() {
            m.ended_at = Some(m.ticks);
        }
        m.phase = next;
        self.mission = Some(m);
    }

    /// Where the player is sent next, if the mission is still going.
    pub fn objective_point(&self) -> Option<V3> {
        let m = self.mission.as_ref()?;
        match m.phase {
            Phase::Advance => Some(m.waypoint),
            Phase::Duel => m.boss.and_then(|b| self.target_centre(Target::Mech(b))),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::Team;
    use crate::fx::ONE;

    use crate::content::tests::{missions, palette};
    use crate::mech::Controls;
    use crate::parts::tests::catalog;
    use crate::pilot::tests::spec as pilot_spec;

    pub fn mission_world() -> World {
        let c = catalog();
        let pal = palette();
        World::mission(
            &c,
            &c.default_loadout(),
            pal.scheme(0).paints(),
            &missions().mission,
            &pilot_spec(),
            pal.enemy.paints(),
        )
    }

    fn run(w: &mut World, ticks: u32) {
        for _ in 0..ticks {
            w.tick(Controls::default());
        }
    }

    #[test]
    fn the_mission_runs_from_waypoint_to_gunships_to_the_frame_to_success() {
        let mut w = mission_world();
        assert_eq!(w.mission.as_ref().unwrap().phase, Phase::Advance);
        assert!(
            w.craft.is_empty() && w.mechs.len() == 1,
            "nothing hostile before the waypoint"
        );
        let wp = w.mission.as_ref().unwrap().waypoint;
        w.mechs[0].body.pos = wp;
        run(&mut w, 1);
        assert_eq!(w.mission.as_ref().unwrap().phase, Phase::Helis);
        assert_eq!(w.craft.len(), 4);
        for i in 0..4 {
            w.hurt(Target::Craft(i), 100_000, 0);
        }
        run(&mut w, 1);
        assert_eq!(w.mission.as_ref().unwrap().phase, Phase::Duel);
        let boss = w.mission.as_ref().unwrap().boss.unwrap();
        assert_eq!(w.mechs[boss].team, Team::Enemy);
        w.hurt(Target::Mech(boss), 1_000_000, 0);
        run(&mut w, 1);
        let m = w.mission.as_ref().unwrap();
        assert_eq!(m.phase, Phase::Success);
        assert!(!m.finished(), "the aftermath plays before the debrief");
        run(&mut w, AFTERMATH_TICKS);
        let m = w.mission.as_ref().unwrap();
        assert!(m.finished());
        let d = m.debrief(&w);
        assert!(d.success);
        assert_eq!(d.kills, 5);
    }

    #[test]
    fn losing_all_ap_fails_the_mission() {
        let mut w = mission_world();
        w.hurt(Target::Mech(0), 1_000_000, 0);
        run(&mut w, 1);
        assert_eq!(w.mission.as_ref().unwrap().phase, Phase::Failure);
        assert_eq!(w.mission.as_ref().unwrap().debrief(&w).rank, "D");
    }

    #[test]
    fn gunships_shoot_at_a_player_who_stands_in_the_plaza() {
        let mut w = mission_world();
        let wp = w.mission.as_ref().unwrap().waypoint;
        w.mechs[0].body.pos = wp;
        let ap = w.mechs[0].ap;
        run(&mut w, 20 * TICKS_PER_SECOND as u32);
        assert!(w.mechs[0].ap < ap, "20 s under four gunships did no damage");
    }

    #[test]
    fn the_enemy_frame_closes_in_and_hurts_a_player_who_stands_still() {
        let mut w = mission_world();
        let wp = w.mission.as_ref().unwrap().waypoint;
        w.mechs[0].body.pos = wp;
        run(&mut w, 1);
        for i in 0..w.craft.len() {
            w.hurt(Target::Craft(i), 100_000, 0);
        }
        run(&mut w, 1);
        let boss = w.mission.as_ref().unwrap().boss.unwrap();
        let start = w.mechs[boss].body.pos.dist(w.mechs[0].body.pos);
        let ap = w.mechs[0].ap;
        run(&mut w, 30 * TICKS_PER_SECOND as u32);
        let end = w.mechs[boss].body.pos.dist(w.mechs[0].body.pos);
        assert!(end < start, "it closed from {} m to {} m", start / ONE, end / ONE);
        assert!(w.mechs[0].ap < ap, "30 s of the enemy frame did no damage");
        assert!(w.mechs[boss].intent.is_some());
    }

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

    #[test]
    fn a_heli_gun_fires_its_burst_gap() {
        let g = HeliGun {
            ap: 900,
            damage: 40,
            speed_ms: 300,
            range_m: 450,
            burst: 6,
            burst_gap_ms: 100,
            reload_ms: 3000,
            spread_deg: 2,
            orbit_m: 80,
        };
        let w = heli_weapon(&g);
        assert_eq!(w.fire_ticks, 6);
        assert_eq!(w.speed, ONE * 5);
    }
}
