//! How a mech moves: walking, glide boost, quick boost, ascent, gravity, EN,
//! and collision with the ground and the buildings. The player and every
//! enemy mech move by this one rule; only what fills `Controls` differs.

use crate::fx::{self, deg, int, ratio, ONE};
use crate::geom::{facing, right_of, v3, V3};
use crate::map::Map;
use crate::model::{Pose, Rig, STRIDE_MAX};
use crate::parts::Stats;

pub const TICKS_PER_SECOND: i32 = 60;

/// What a pilot asks for on one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Controls {
    /// strafe, −ONE (left) to ONE (right)
    pub move_x: i32,
    /// forward, −ONE (back) to ONE (forward)
    pub move_z: i32,
    pub ascend: bool,
    /// pressed this tick: start a quick boost
    pub quick_boost: bool,
    /// pressed this tick: switch glide boost on or off
    pub toggle_glide: bool,
    pub fire: [bool; 3],
}

// Tuning that does not come from parts. Rates are per second; `per_tick`
// turns them into Q16 per tick.
const GRAVITY_MS2: i32 = 42;
const MAX_FALL_MS: i32 = 70;
const GLIDE_FALL_MS: i32 = 9;
const RISE_MAX_MS: i32 = 36;
const GROUND_ACCEL_MS2: i32 = 190;
const AIR_ACCEL_MS2: i32 = 80;
const QB_DECEL_MS2: i32 = 420;
/// a quick boost holds its burst this long before it decays
pub const QB_TICKS: i32 = 9;
/// and cannot be repeated for this long
pub const QB_COOLDOWN_TICKS: i32 = 21;
const GLIDE_AIR_EN_PER_S: i32 = 160;
/// after running EN out, thrust is locked until EN refills to this percent
pub const EN_UNLOCK_PCT: i32 = 60;
/// and the supply delay is this many times longer
const EN_OUT_DELAY_FACTOR: i32 = 2;
/// EN is held in thousandths so a per-tick drain keeps its fraction
pub const EN_SCALE: i32 = 1000;
/// how fast the body turns to face the aim, angle units per tick
const BODY_TURN_PER_TICK: i32 = deg(12);
/// metres of walking per full walk cycle (two steps)
const STRIDE_LENGTH_M: i32 = 9;

pub const HALF_WIDTH: i32 = int(2);
pub const PITCH_MIN: i32 = -deg(70);
pub const PITCH_MAX: i32 = deg(75);

fn per_tick(rate: i32) -> i32 {
    ratio(rate, TICKS_PER_SECOND)
}

fn per_tick2(rate: i32) -> i32 {
    ratio(rate, TICKS_PER_SECOND * TICKS_PER_SECOND)
}

/// km/h to Q16 metres per tick: km/h ÷ 3.6 ÷ 60 = km/h ÷ 216.
pub fn kmh(k: i32) -> i32 {
    ratio(k, 216)
}

/// The per-tick numbers a loadout flies with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tuning {
    pub walk: i32,
    pub glide: i32,
    pub quick_boost: i32,
    pub jump: i32,
    pub ascend_accel: i32,
    pub ascend_en: i32,
    pub qb_en: i32,
    pub en_capacity: i32,
    pub en_recovery: i32,
    pub supply_delay: i32,
    pub max_ap: i32,
    pub height: i32,
    /// half the width of the frame's collision box
    pub half_width: i32,
}

impl Tuning {
    pub fn from_stats(s: &Stats, height: i32) -> Tuning {
        let mob = |k: i32| k * s.mobility_pct / 100;
        Tuning {
            walk: kmh(mob(s.walk_kmh)),
            glide: kmh(mob(s.glide_kmh)),
            quick_boost: kmh(mob(s.qb_kmh)),
            jump: per_tick(s.jump_ms),
            ascend_accel: per_tick2(mob(s.ascend_ms2)),
            ascend_en: s.ascend_en * EN_SCALE / TICKS_PER_SECOND,
            qb_en: s.qb_en * EN_SCALE,
            en_capacity: s.en_capacity * EN_SCALE,
            en_recovery: s.en_recovery * EN_SCALE / TICKS_PER_SECOND,
            supply_delay: s.supply_delay_ms * TICKS_PER_SECOND / 1000,
            max_ap: s.ap,
            height,
            half_width: HALF_WIDTH,
        }
    }
}

/// One mech's body: where it is, how it moves, and its EN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Body {
    pub pos: V3,
    pub prev_pos: V3,
    pub vel: V3,
    /// the way the torso faces
    pub yaw: i32,
    pub prev_yaw: i32,
    /// the way the pilot is looking
    pub aim_yaw: i32,
    pub aim_pitch: i32,
    pub grounded: bool,
    pub glide: bool,
    pub en: i32,
    pub en_locked: bool,
    pub supply_wait: i32,
    pub qb_ticks: i32,
    pub qb_cooldown: i32,
    /// thrust used on the last tick, for the flames: 0 none, 1 glide or ascend, 2 quick boost
    pub thrust: u8,
    pub walk_phase: i32,
    pub stride: i32,
    pub leg_tilt: i32,
    pub lean: i32,
}

impl Body {
    pub fn new(pos: V3, yaw: i32, t: &Tuning) -> Body {
        Body {
            pos,
            prev_pos: pos,
            vel: V3::ZERO,
            yaw,
            prev_yaw: yaw,
            aim_yaw: yaw,
            aim_pitch: 0,
            grounded: true,
            glide: false,
            en: t.en_capacity,
            en_locked: false,
            supply_wait: 0,
            qb_ticks: 0,
            qb_cooldown: 0,
            thrust: 0,
            walk_phase: 0,
            stride: 0,
            leg_tilt: 0,
            lean: 0,
        }
    }

    pub fn can_thrust(&self) -> bool {
        !self.en_locked
    }

    pub fn en_pct(&self, t: &Tuning) -> i32 {
        self.en * 100 / t.en_capacity.max(1)
    }

    pub fn pose(&self) -> Pose {
        Pose {
            walk_phase: self.walk_phase,
            stride: self.stride,
            leg_tilt: self.leg_tilt,
            aim_pitch: self.aim_pitch,
            lean: self.lean,
        }
    }

    pub fn speed(&self) -> i32 {
        self.vel.len()
    }

    /// The direction the pilot asked to move in, flat, at most unit length.
    fn wish(&self, c: &Controls) -> V3 {
        let f = facing(self.aim_yaw, 0);
        let r = right_of(self.aim_yaw);
        f.scale(c.move_z).add(r.scale(c.move_x)).clamp_len(ONE)
    }

    /// One tick of movement.
    pub fn step(&mut self, c: &Controls, t: &Tuning, map: &Map) {
        self.prev_pos = self.pos;
        self.prev_yaw = self.yaw;
        let mut used_en = false;
        self.thrust = 0;
        self.qb_cooldown = (self.qb_cooldown - 1).max(0);
        self.qb_ticks = (self.qb_ticks - 1).max(0);
        if c.toggle_glide {
            self.glide = !self.glide;
        }

        let wish = self.wish(c);
        let moving = wish != V3::ZERO;

        if c.quick_boost && self.qb_cooldown == 0 && self.can_thrust() && self.en > 0 {
            let dir = if moving {
                wish.norm()
            } else {
                facing(self.aim_yaw, 0)
            };
            let burst = dir.scale(t.quick_boost);
            self.vel = v3(burst.x, self.vel.y.max(0), burst.z);
            self.qb_ticks = QB_TICKS;
            self.qb_cooldown = QB_COOLDOWN_TICKS;
            self.en -= t.qb_en;
            self.glide = true;
            used_en = true;
        }

        let climate = map.climate;
        let gravity = per_tick2(GRAVITY_MS2) * climate.gravity_pct / 100;
        // Horizontal: steer toward the wished velocity.
        let top = if self.glide { t.glide } else { t.walk };
        let desired = wish.scale(top);
        let flat = v3(self.vel.x, 0, self.vel.z);
        let accel = if self.qb_ticks > 0 {
            0
        } else if flat.len() > top && !moving {
            per_tick2(QB_DECEL_MS2)
        } else if flat.len() > top {
            per_tick2(QB_DECEL_MS2) / 2
        } else if self.grounded {
            // grip: on ice a frame slides into and out of its moves
            per_tick2(GROUND_ACCEL_MS2) * climate.traction_pct / 100
        } else if self.glide {
            per_tick2(GROUND_ACCEL_MS2)
        } else {
            per_tick2(AIR_ACCEL_MS2)
        };
        let change = desired.sub(flat).clamp_len(accel);
        self.vel = v3(flat.x + change.x, self.vel.y, flat.z + change.z);

        // Vertical: ascend on thrust, otherwise fall; glide slows the fall.
        if c.ascend && self.can_thrust() && self.en > 0 {
            if self.grounded {
                self.vel.y = self.vel.y.max(t.jump);
            }
            self.vel.y = fx::approach(self.vel.y, per_tick(RISE_MAX_MS), t.ascend_accel);
            self.en -= t.ascend_en;
            used_en = true;
            self.thrust = 1;
        } else {
            let floor = if self.glide && !self.grounded {
                -per_tick(GLIDE_FALL_MS)
            } else {
                -per_tick(MAX_FALL_MS)
            };
            let fallen = self.vel.y - gravity;
            self.vel.y = if self.vel.y < floor {
                fx::approach(self.vel.y, floor, gravity)
            } else {
                fallen.max(floor)
            };
            if self.glide && !self.grounded {
                self.en -= GLIDE_AIR_EN_PER_S * EN_SCALE / TICKS_PER_SECOND;
                used_en = true;
            }
        }
        if self.glide && moving {
            self.thrust = self.thrust.max(1);
        }
        if self.qb_ticks > 0 {
            self.thrust = 2;
        }

        self.supply(used_en, t);
        self.collide_move(map, t);
        self.animate(t);
    }

    fn supply(&mut self, used: bool, t: &Tuning) {
        if self.en <= 0 {
            self.en = 0;
            if !self.en_locked {
                self.en_locked = true;
                self.supply_wait = t.supply_delay * EN_OUT_DELAY_FACTOR;
            }
        } else if used {
            self.supply_wait = t.supply_delay;
        }
        if !used || self.en_locked {
            if self.supply_wait > 0 {
                self.supply_wait -= 1;
            } else {
                self.en = (self.en + t.en_recovery).min(t.en_capacity);
            }
        }
        if self.en_locked && self.en * 100 >= t.en_capacity * EN_UNLOCK_PCT {
            self.en_locked = false;
        }
    }

    fn bounds(&self, at: V3, t: &Tuning) -> (V3, V3) {
        (
            v3(at.x - t.half_width, at.y, at.z - t.half_width),
            v3(at.x + t.half_width, at.y + t.height, at.z + t.half_width),
        )
    }

    /// Moves one axis at a time and stops at whatever it meets.
    fn collide_move(&mut self, map: &Map, t: &Tuning) {
        let was_grounded = self.grounded;
        self.grounded = false;
        for axis in 0..3 {
            let mut next = self.pos;
            match axis {
                0 => next.x += self.vel.x,
                1 => next.z += self.vel.z,
                _ => next.y += self.vel.y,
            }
            let (min, max) = self.bounds(next, t);
            let mut stop: Option<i32> = None;
            map.each_near(min, max, |b| {
                if !b.overlaps(min, max) {
                    return;
                }
                let edge = match axis {
                    0 if self.vel.x > 0 => b.min.x - t.half_width - 1,
                    0 => b.max.x + t.half_width + 1,
                    1 if self.vel.z > 0 => b.min.z - t.half_width - 1,
                    1 => b.max.z + t.half_width + 1,
                    _ if self.vel.y > 0 => b.min.y - t.height - 1,
                    _ => b.max.y,
                };
                // keep the stop nearest the start of the move
                let better = match (stop, axis, self.vel) {
                    (None, ..) => true,
                    (Some(s), 0, v) => (v.x > 0 && edge < s) || (v.x <= 0 && edge > s),
                    (Some(s), 1, v) => (v.z > 0 && edge < s) || (v.z <= 0 && edge > s),
                    (Some(s), _, v) => (v.y > 0 && edge < s) || (v.y <= 0 && edge > s),
                };
                if better {
                    stop = Some(edge);
                }
            });
            match (axis, stop) {
                (0, Some(e)) => {
                    next.x = e;
                    self.vel.x = 0;
                }
                (1, Some(e)) => {
                    next.z = e;
                    self.vel.z = 0;
                }
                (_, Some(e)) => {
                    if self.vel.y <= 0 {
                        self.grounded = true;
                    }
                    next.y = e;
                    self.vel.y = 0;
                }
                _ => {}
            }
            self.pos = next;
        }
        if self.pos.y <= 0 {
            self.pos.y = 0;
            self.vel.y = self.vel.y.max(0);
            self.grounded = true;
        }
        // A standing mech presses on its floor every tick; a mech that was on
        // the ground and still has its floor under it stays grounded.
        if !self.grounded && was_grounded && self.vel.y <= 0 {
            let (min, max) = self.bounds(v3(self.pos.x, self.pos.y - ONE / 16, self.pos.z), t);
            self.grounded = map.box_blocked(min, max);
        }
        let mx = map.half_x - t.half_width;
        let mz = map.half_z - t.half_width;
        if map.wrap {
            let wrapped = map.wrap_x(self.pos.x);
            // move the last position by the same jump, so drawing does not streak
            self.prev_pos.x += wrapped - self.pos.x;
            self.pos.x = wrapped;
        } else if self.pos.x.abs() > mx {
            self.pos.x = self.pos.x.clamp(-mx, mx);
            self.vel.x = 0;
        }
        if self.pos.z.abs() > mz {
            self.pos.z = self.pos.z.clamp(-mz, mz);
            self.vel.z = 0;
        }
        if self.pos.y > map.ceiling {
            self.pos.y = map.ceiling;
            self.vel.y = self.vel.y.min(0);
        }
    }

    fn animate(&mut self, t: &Tuning) {
        let turn = fx::wrap(self.aim_yaw - self.yaw).clamp(-BODY_TURN_PER_TICK, BODY_TURN_PER_TICK);
        self.yaw += turn;
        let flat = v3(self.vel.x, 0, self.vel.z).len();
        let walking = self.grounded && !self.glide;
        let stride_goal = if walking {
            fx::mul(STRIDE_MAX, fx::div(flat, t.walk.max(1)).min(ONE))
        } else {
            0
        };
        self.stride = fx::approach(self.stride, stride_goal, deg(3));
        if walking {
            let per_m = fx::TURN / STRIDE_LENGTH_M;
            self.walk_phase = (self.walk_phase + fx::mul(flat, per_m)) & (fx::TURN - 1);
        }
        let tilt_goal = if !self.grounded {
            -deg(22)
        } else if self.glide && flat > t.walk {
            -deg(12)
        } else {
            0
        };
        self.leg_tilt = fx::approach(self.leg_tilt, tilt_goal, deg(2));
        let fwd = facing(self.yaw, 0).dot(v3(self.vel.x, 0, self.vel.z));
        let lean_goal = fx::mul(deg(14), fx::div(fwd, t.glide.max(1)).clamp(-ONE / 2, ONE));
        self.lean = fx::approach(self.lean, lean_goal, deg(1));
    }

    /// Turns the aim by a mouse movement. Called every frame, not every tick,
    /// so the view answers the mouse at the display's rate.
    pub fn look(&mut self, d_yaw: i32, d_pitch: i32) {
        self.aim_yaw = (self.aim_yaw + d_yaw) & (fx::TURN - 1);
        self.aim_pitch = (self.aim_pitch + d_pitch).clamp(PITCH_MIN, PITCH_MAX);
    }
}

/// The height a rig's tuning needs, rounded to cover the head.
pub fn tuning_for(stats: &Stats, rig: &Rig) -> Tuning {
    Tuning::from_stats(stats, rig.height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Map, MapSpec};
    use crate::parts::{stats, tests::catalog};

    fn open_map() -> Map {
        Map::generate(&MapSpec {
            seed: 1,
            width_m: 2000,
            length_m: 2000,
            cell_m: 100,
            street_m: 20,
            empty_pct: 100,
            min_h_m: 10,
            max_h_m: 10,
            tall_pct: 0,
            tall_h_m: 10,
            ceiling_m: 500,
            clearings: vec![],
            avenue_m: 0,
            style: Default::default(),
            wrap: false,
        })
    }

    fn tuning() -> Tuning {
        let c = catalog();
        let l = c.default_loadout();
        let rig = Rig::build(&l, &c);
        tuning_for(&stats(&l, &c), &rig)
    }

    fn run(b: &mut Body, c: Controls, t: &Tuning, m: &Map, ticks: i32) {
        for _ in 0..ticks {
            b.step(&c, t, m);
        }
    }

    const FWD: Controls = Controls {
        move_x: 0,
        move_z: ONE,
        ascend: false,
        quick_boost: false,
        toggle_glide: false,
        fire: [false; 3],
    };

    #[test]
    fn walking_reaches_the_legs_walking_speed_and_no_more() {
        let (t, m) = (tuning(), open_map());
        let mut b = Body::new(V3::ZERO, 0, &t);
        run(&mut b, FWD, &t, &m, 60);
        // by hand: 95 km/h = 26.389 m/s = 0.43981 m/tick = 28 823 in Q16
        let expect = 28_823;
        assert!((b.vel.len() - expect).abs() < 64, "{} vs {expect}", b.vel.len());
        assert!(b.pos.z < -int(20), "walked forward (−z), at {:?}", b.pos);
        assert!(b.grounded);
    }

    #[test]
    fn glide_boost_is_faster_than_walking() {
        let (t, m) = (tuning(), open_map());
        let mut b = Body::new(V3::ZERO, 0, &t);
        b.step(
            &Controls {
                toggle_glide: true,
                ..FWD
            },
            &t,
            &m,
        );
        run(&mut b, FWD, &t, &m, 90);
        assert!(b.glide);
        assert!(b.vel.len() > t.walk * 2, "{}", b.vel.len());
        assert!((b.vel.len() - t.glide).abs() < 64);
    }

    #[test]
    fn a_quick_boost_bursts_costs_its_en_and_has_a_cooldown() {
        let (t, m) = (tuning(), open_map());
        let mut b = Body::new(V3::ZERO, 0, &t);
        let before = b.en;
        b.step(
            &Controls {
                quick_boost: true,
                ..FWD
            },
            &t,
            &m,
        );
        assert!(b.vel.len() >= t.quick_boost - 64);
        assert_eq!(before - b.en, t.qb_en);
        let en = b.en;
        b.step(
            &Controls {
                quick_boost: true,
                ..FWD
            },
            &t,
            &m,
        );
        assert_eq!(b.en, en, "a second quick boost inside the cooldown does nothing");
    }

    #[test]
    fn ascending_rises_until_en_runs_out_then_locks_thrust() {
        let (t, m) = (tuning(), open_map());
        let mut b = Body::new(V3::ZERO, 0, &t);
        let up = Controls {
            ascend: true,
            ..Controls::default()
        };
        run(&mut b, up, &t, &m, 30);
        assert!(b.pos.y > int(5), "rose to {}", b.pos.y);
        // capacity 2600 at 520/s lasts 5 s
        run(&mut b, up, &t, &m, 5 * 60);
        assert!(b.en_locked, "EN ran out");
        let h = b.pos.y;
        // still rising at 36 m/s when EN locks; gravity turns it in under a second
        run(&mut b, up, &t, &m, 120);
        assert!(
            b.pos.y < h,
            "with EN locked, ascend does nothing and the mech falls"
        );
    }

    #[test]
    fn locked_en_unlocks_at_sixty_percent_after_a_doubled_delay() {
        let (t, m) = (tuning(), open_map());
        let mut b = Body::new(V3::ZERO, 0, &t);
        b.en = 1;
        b.step(
            &Controls {
                ascend: true,
                ..Controls::default()
            },
            &t,
            &m,
        );
        assert!(b.en_locked);
        let mut ticks = 0;
        while b.en_locked {
            b.step(&Controls::default(), &t, &m);
            ticks += 1;
            assert!(ticks < 1000);
        }
        // delay 0.9 s doubled = 108 ticks, then 60% of 2600 at 1400/s = 67 ticks
        assert!((170..=180).contains(&ticks), "{ticks}");
        assert!(b.en_pct(&t) >= 60);
    }

    #[test]
    fn a_falling_mech_lands_on_the_ground() {
        let (t, m) = (tuning(), open_map());
        let mut b = Body::new(v3(0, int(50), 0), 0, &t);
        b.grounded = false;
        run(&mut b, Controls::default(), &t, &m, 240);
        assert_eq!(b.pos.y, 0);
        assert!(b.grounded);
    }

    #[test]
    fn low_gravity_lets_a_jump_rise_higher() {
        let t = tuning();
        let mut low = open_map();
        low.climate.gravity_pct = 35;
        let peak = |m: &Map| {
            let mut b = Body::new(V3::ZERO, 0, &t);
            b.step(
                &Controls {
                    ascend: true,
                    ..Controls::default()
                },
                &t,
                m,
            );
            let mut top = 0;
            for _ in 0..300 {
                b.step(&Controls::default(), &t, m);
                top = top.max(b.pos.y);
            }
            top
        };
        let (earth, moon) = (peak(&open_map()), peak(&low));
        // a jump's height goes as 1/g: about 2.9 times higher at 35%
        assert!(moon > earth * 5 / 2, "{earth} vs {moon}");
    }

    #[test]
    fn on_ice_a_frame_keeps_sliding_after_it_lets_go() {
        let t = tuning();
        let mut ice = open_map();
        ice.climate.traction_pct = 20;
        let slide = |m: &Map| {
            let mut b = Body::new(V3::ZERO, 0, &t);
            run(&mut b, FWD, &t, m, 60);
            let z = b.pos.z;
            run(&mut b, Controls::default(), &t, m, 60);
            z - b.pos.z
        };
        assert!(
            slide(&ice) > slide(&open_map()) * 3,
            "{} vs {}",
            slide(&ice),
            slide(&open_map())
        );
    }

    #[test]
    fn on_a_ring_walking_off_one_edge_comes_back_on_the_other() {
        let t = tuning();
        let mut m = open_map();
        m.wrap = true;
        let mut b = Body::new(v3(m.half_x - int(1), 0, 0), crate::fx::deg(-90), &t);
        b.aim_yaw = crate::fx::deg(-90);
        run(&mut b, FWD, &t, &m, 30);
        assert!(b.pos.x < 0 && b.pos.x > -m.half_x, "wrapped to {}", b.pos.x);
    }

    #[test]
    fn glide_slows_a_fall() {
        let (t, m) = (tuning(), open_map());
        let mut free = Body::new(v3(0, int(200), 0), 0, &t);
        let mut glide = free.clone();
        glide.glide = true;
        run(&mut free, Controls::default(), &t, &m, 120);
        run(&mut glide, Controls::default(), &t, &m, 120);
        assert!(glide.pos.y > free.pos.y + int(20));
    }

    #[test]
    fn a_building_stops_the_mech_and_its_roof_holds_it() {
        let t = tuning();
        let mut m = open_map();
        let blk = crate::map::Block {
            min: v3(-int(10), 0, -int(40)),
            max: v3(int(10), int(30), -int(20)),
            shade: 0,
        };
        m.add_block(blk);
        let mut b = Body::new(V3::ZERO, 0, &t);
        run(&mut b, FWD, &t, &m, 180);
        assert!(
            b.pos.z >= blk.max.z + HALF_WIDTH,
            "stopped at the wall: {:?}",
            b.pos
        );
        let mut roof = Body::new(v3(0, int(40), -int(30)), 0, &t);
        roof.grounded = false;
        run(&mut roof, Controls::default(), &t, &m, 120);
        assert_eq!(roof.pos.y, blk.max.y);
        assert!(roof.grounded);
    }

    #[test]
    fn looking_turns_the_aim_at_once_and_the_body_follows_over_ticks() {
        let (t, m) = (tuning(), open_map());
        let mut b = Body::new(V3::ZERO, 0, &t);
        b.look(deg(90), deg(100));
        assert_eq!(b.aim_yaw, deg(90));
        assert_eq!(b.aim_pitch, PITCH_MAX, "pitch is clamped");
        assert_eq!(b.yaw, 0);
        run(&mut b, Controls::default(), &t, &m, 10);
        assert_eq!(b.yaw, deg(90));
    }
}
