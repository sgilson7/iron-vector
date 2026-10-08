//! Weapons, shots and what they hit.

use crate::fx::{self, deg, int, ONE};
use crate::geom::{facing, yaw_pitch_of, V3};
use crate::mech::TICKS_PER_SECOND;
use crate::parts::{Part, WeaponKind};
use crate::rng::Rng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Team {
    Player,
    Enemy,
}

/// Something a shot can hit or a lock can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Mech(usize),
    Craft(usize),
}

/// A weapon's numbers, per tick and in Q16, read from its part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Weapon {
    pub kind: WeaponKind,
    pub damage: i32,
    pub impact: i32,
    pub fire_ticks: i32,
    pub speed: i32,
    pub ammo: i32,
    pub range: i32,
    pub turn: i32,
    pub salvo: i32,
    pub spread: i32,
    pub blast: i32,
}

impl Weapon {
    pub fn from_part(p: &Part) -> Weapon {
        let s = p.stats;
        Weapon {
            kind: p.kind.unwrap_or(WeaponKind::Rifle),
            damage: s.damage,
            impact: s.impact,
            fire_ticks: (s.fire_ms * TICKS_PER_SECOND / 1000).max(1),
            speed: fx::ratio(s.speed_ms, TICKS_PER_SECOND),
            ammo: s.ammo,
            range: int(s.range_m),
            turn: deg(s.homing_dps) / TICKS_PER_SECOND,
            salvo: s.salvo.max(1),
            spread: deg(s.spread_deg),
            blast: int(s.blast_m),
        }
    }

    /// Ticks a shot flies before its damage starts to fall off.
    pub fn full_ticks(&self) -> i32 {
        fx::div(self.range, self.speed.max(1)) / ONE
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WeaponState {
    pub cooldown: i32,
    pub ammo: i32,
}

/// Damage kept, in percent, by a shot flying past its weapon's range.
pub const FALLOFF_PCT: i32 = 55;
/// A shot outlives its full-damage range by this factor, in percent.
const LIFE_PCT: i32 = 160;
/// Missiles fly straight this long before they turn.
const MISSILE_ARM_TICKS: i32 = 8;
/// Shell drop, for grenades.
const SHELL_GRAVITY_MS2: i32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shot {
    pub pos: V3,
    pub prev_pos: V3,
    pub vel: V3,
    pub kind: WeaponKind,
    pub team: Team,
    pub damage: i32,
    pub impact: i32,
    pub age: i32,
    pub full_ticks: i32,
    pub life: i32,
    pub target: Option<Target>,
    pub turn: i32,
    pub blast: i32,
}

impl Shot {
    /// The shots one trigger pull fires from `muzzle` toward `aim`.
    pub fn fire(
        w: &Weapon,
        team: Team,
        muzzle: V3,
        aim: V3,
        lock: Option<Target>,
        rng: &mut Rng,
    ) -> Vec<Shot> {
        let (yaw, pitch) = yaw_pitch_of(aim.sub(muzzle));
        let full = w.full_ticks();
        (0..w.salvo)
            .map(|i| {
                let (dy, dp) = match w.kind {
                    // a fan that opens sideways and up, so a salvo reads as one
                    WeaponKind::Missile => {
                        let k = 2 * i - (w.salvo - 1);
                        (
                            w.spread * k / w.salvo.max(1),
                            w.spread / 2 + rng.range(0, w.spread / 4),
                        )
                    }
                    _ => (
                        rng.range(-w.spread, w.spread),
                        rng.range(-w.spread, w.spread),
                    ),
                };
                let dir = facing(yaw + dy, pitch + dp);
                Shot {
                    pos: muzzle,
                    prev_pos: muzzle,
                    vel: dir.scale(w.speed),
                    kind: w.kind,
                    team,
                    damage: w.damage,
                    impact: w.impact,
                    age: 0,
                    full_ticks: full,
                    life: full * LIFE_PCT / 100,
                    target: if w.kind == WeaponKind::Missile {
                        lock
                    } else {
                        None
                    },
                    turn: w.turn,
                    blast: w.blast,
                }
            })
            .collect()
    }

    /// Damage this shot does now, after falloff.
    pub fn damage_now(&self) -> i32 {
        if self.age > self.full_ticks {
            self.damage * FALLOFF_PCT / 100
        } else {
            self.damage
        }
    }

    /// Turns toward `goal` by at most the shot's turn rate, and drops a shell.
    pub fn steer(&mut self, goal: Option<V3>) {
        if let (Some(g), true) = (goal, self.age > MISSILE_ARM_TICKS) {
            let speed = self.vel.len();
            let (y0, p0) = yaw_pitch_of(self.vel);
            let (y1, p1) = yaw_pitch_of(g.sub(self.pos));
            let y = y0 + fx::wrap(y1 - y0).clamp(-self.turn, self.turn);
            let p = p0 + (p1 - p0).clamp(-self.turn, self.turn);
            self.vel = facing(y, p).scale(speed);
        }
        if self.kind == WeaponKind::Grenade {
            self.vel.y -= fx::ratio(SHELL_GRAVITY_MS2, TICKS_PER_SECOND * TICKS_PER_SECOND);
        }
    }
}

/// Where along the segment `a`→`b` it first comes within `r` of `c`, as a
/// fraction in Q16, or `None`.
pub fn segment_sphere(a: V3, b: V3, c: V3, r: i32) -> Option<i32> {
    let d = b.sub(a);
    let dd = d.x as i64 * d.x as i64 + d.y as i64 * d.y as i64 + d.z as i64 * d.z as i64;
    let ac = c.sub(a);
    let t = if dd == 0 {
        0
    } else {
        let proj = ac.x as i64 * d.x as i64 + ac.y as i64 * d.y as i64 + ac.z as i64 * d.z as i64;
        ((proj << fx::FRAC_BITS) / dd).clamp(0, ONE as i64) as i32
    };
    let closest = a.add(d.scale(t));
    let off = closest.sub(c);
    let dist2 =
        off.x as i64 * off.x as i64 + off.y as i64 * off.y as i64 + off.z as i64 * off.z as i64;
    if dist2 <= r as i64 * r as i64 {
        Some(t)
    } else {
        None
    }
}

/// Blast damage at a distance: full at the centre, falling to a third at the edge.
pub fn blast_damage(damage: i32, dist: i32, radius: i32) -> i32 {
    if dist >= radius || radius == 0 {
        return 0;
    }
    let kept = ONE - fx::mul(fx::div(dist, radius), ONE * 2 / 3);
    fx::mul(damage * ONE, kept) / ONE
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    Blast,
    Flash,
    Spark,
    Debris,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effect {
    pub kind: EffectKind,
    pub pos: V3,
    pub vel: V3,
    pub age: i32,
    pub life: i32,
    pub size: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CraftKind {
    Drone,
    Heli,
}

/// A flying target that is not a mech: practice drones and helicopters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Craft {
    pub kind: CraftKind,
    pub pos: V3,
    pub prev_pos: V3,
    pub vel: V3,
    pub yaw: i32,
    pub prev_yaw: i32,
    pub ap: i32,
    pub max_ap: i32,
    pub alive: bool,
    pub anchor: V3,
    pub phase: i32,
    pub cooldown: i32,
    /// shots left in the current burst
    pub burst: i32,
    /// ticks until a destroyed drone returns; 0 means it stays down
    pub respawn: i32,
    pub down_ticks: i32,
}

impl Craft {
    pub fn radius(&self) -> i32 {
        match self.kind {
            CraftKind::Drone => int(3),
            CraftKind::Heli => int(5),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::v3;
    use crate::parts::tests::catalog;

    #[test]
    fn a_segment_passing_through_a_sphere_hits_and_one_passing_wide_misses() {
        let a = v3(0, 0, 0);
        let b = v3(int(10), 0, 0);
        let t = segment_sphere(a, b, v3(int(5), int(1), 0), int(2)).unwrap();
        assert_eq!(t, ONE / 2);
        assert!(segment_sphere(a, b, v3(int(5), int(3), 0), int(2)).is_none());
        // a fast shot that would step over a target between ticks still hits it
        assert!(segment_sphere(a, v3(int(12), 0, 0), v3(int(6), 0, 0), int(1)).is_some());
    }

    #[test]
    fn blast_damage_is_full_at_the_centre_a_third_at_the_edge_and_none_beyond() {
        assert_eq!(blast_damage(900, 0, int(18)), 900);
        let edge = blast_damage(900, int(18) - 1, int(18));
        assert!((299..=301).contains(&edge), "{edge}");
        assert_eq!(blast_damage(900, int(19), int(18)), 0);
    }

    #[test]
    fn a_rifle_shot_loses_damage_past_its_range() {
        let c = catalog();
        let w = Weapon::from_part(c.get("rf-marrow").unwrap());
        let mut rng = Rng::new(1);
        let mut s = Shot::fire(
            &w,
            Team::Player,
            V3::ZERO,
            v3(0, 0, -int(100)),
            None,
            &mut rng,
        )[0];
        assert_eq!(s.damage_now(), w.damage);
        s.age = s.full_ticks + 1;
        assert_eq!(s.damage_now(), w.damage * FALLOFF_PCT / 100);
        // 260 m at 700 m/s is 22 ticks
        assert_eq!(w.full_ticks(), 22);
    }

    #[test]
    fn a_missile_salvo_fires_its_count_and_turns_toward_its_goal() {
        let c = catalog();
        let w = Weapon::from_part(c.get("ml-hornet").unwrap());
        let mut rng = Rng::new(1);
        let shots = Shot::fire(
            &w,
            Team::Player,
            V3::ZERO,
            v3(0, 0, -int(100)),
            Some(Target::Craft(0)),
            &mut rng,
        );
        assert_eq!(shots.len(), 4);
        let mut m = shots[0];
        m.age = MISSILE_ARM_TICKS + 1;
        let goal = v3(int(200), 0, 0);
        let before = goal.sub(m.pos).norm().dot(m.vel.norm());
        m.steer(Some(goal));
        let after = goal.sub(m.pos).norm().dot(m.vel.norm());
        assert!(after > before, "{before} → {after}");
        assert!((m.vel.len() - w.speed).abs() < 64, "steering keeps speed");
    }
}
