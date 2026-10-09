//! A race's obstacle course. Gates are rings to fly through the right way,
//! pads to land on, boost rings that throw a frame on, and switches that open
//! a door somewhere ahead. Machines are the parts that move: lifts that
//! carry, sweepers and pistons that knock a frame aside, and doors that stay
//! shut until their switch is touched.

use crate::fx::{self, deg, int, ONE};
use crate::geom::{facing, v3, V3};
use crate::mech::TICKS_PER_SECOND;
use serde::Deserialize;

/// A landing counts this close to the pad's top, in height.
const LAND_SLACK: i32 = int(2);
/// A boost ring throws a frame on at this speed (m/s), and a little upward.
const BOOST_MS: i32 = 70;
const BOOST_LIFT_MS: i32 = 6;
/// A hazard throws a frame this hard (m/s) and stuns it this long.
const KNOCK_MS: i32 = 26;
const KNOCK_LIFT_MS: i32 = 10;
pub const KNOCK_TICKS: i32 = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateKind {
    /// fly through it, the way it faces
    #[default]
    Ring,
    /// land on it
    Pad,
    /// fly through it and be thrown on
    Boost,
    /// land on it to open a door
    Switch,
}

impl GateKind {
    pub fn landing(self) -> bool {
        matches!(self, GateKind::Pad | GateKind::Switch)
    }

    /// The copy key the waypoint shows.
    pub fn label(self) -> &'static str {
        match self {
            GateKind::Ring => "wp_ring",
            GateKind::Pad => "wp_pad",
            GateKind::Boost => "wp_boost",
            GateKind::Switch => "wp_switch",
        }
    }
}

fn ring_r() -> i32 {
    12
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateSpec {
    /// x, z, altitude (m): a ring's centre, or the top of a pad
    pub at: [i32; 3],
    #[serde(default)]
    pub kind: GateKind,
    /// a ring's radius, or how near the middle of a pad a landing counts
    #[serde(default = "ring_r")]
    pub r_m: i32,
    /// the heading a ring faces, in degrees; none faces it from the gate before
    #[serde(default)]
    pub yaw: Option<i32>,
    /// the machine it rides on, if it moves
    #[serde(default)]
    pub ride: Option<usize>,
    /// the door a switch opens
    #[serde(default)]
    pub opens: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// a platform that moves and carries what stands on it
    Lift,
    /// a bar that sweeps and knocks a frame aside
    Sweeper,
    /// a block that slams and knocks a frame aside
    Piston,
    /// a wall that moves out of the way once its switch is touched
    Door,
    /// a slab that falls once, at a set time or on the alarm, crushing what is under it
    Slab,
}

impl Role {
    pub fn hazard(self) -> bool {
        matches!(self, Role::Sweeper | Role::Piston | Role::Slab)
    }

    /// Whether it moves only once, when triggered.
    pub fn once(self) -> bool {
        matches!(self, Role::Door | Role::Slab)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MachineSpec {
    pub role: Role,
    /// centre x, z, and the height of its underside (m)
    pub at: [i32; 3],
    /// width, depth, height (m)
    pub size: [i32; 3],
    /// points it moves between, x, z, altitude from where it starts (m); it
    /// starts at the first, and a door stops at the last
    #[serde(default)]
    pub path: Vec<[i32; 3]>,
    #[serde(default)]
    pub speed_ms: i32,
    /// how long it waits at each point
    #[serde(default)]
    pub wait_ms: i32,
    /// a door or slab that moves this long after the mission starts
    #[serde(default)]
    pub after_ms: i32,
    /// a door or slab that moves when the alarm is raised
    #[serde(default)]
    pub on_alarm: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gate {
    pub at: V3,
    pub kind: GateKind,
    pub r: i32,
    /// the way through a ring
    pub normal: V3,
    pub ride: Option<usize>,
    pub opens: Option<usize>,
}

/// The gates of a course, each ring facing the way it is flown.
pub fn gates(spec: &[GateSpec], start: V3) -> Vec<Gate> {
    let mut before = start;
    spec.iter()
        .map(|g| {
            let at = v3(int(g.at[0]), int(g.at[2]), int(g.at[1]));
            let normal = match g.yaw {
                Some(y) => facing(deg(y), 0),
                None => {
                    let d = at.sub(before);
                    if d == V3::ZERO {
                        v3(0, 0, -ONE)
                    } else {
                        d.norm()
                    }
                }
            };
            before = at;
            Gate {
                at,
                kind: g.kind,
                r: int(g.r_m),
                normal,
                ride: g.ride,
                opens: g.opens,
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Machine {
    pub role: Role,
    /// its box in the map's movers
    pub block: usize,
    /// the centre of its box where it starts, and its half size
    pub home: V3,
    pub half: V3,
    /// how far it has moved from home, now and a tick ago
    pub offset: V3,
    pub prev_offset: V3,
    pub path: Vec<V3>,
    pub next: usize,
    pub speed: i32,
    pub wait: i32,
    pub waiting: i32,
    /// a door or slab: whether it has been set moving
    pub open: bool,
    /// ticks into the mission it sets itself moving, if it does
    pub after: Option<u32>,
    pub on_alarm: bool,
}

impl Machine {
    pub fn new(s: &MachineSpec, block: usize) -> Machine {
        let half = v3(int(s.size[0]) / 2, int(s.size[2]) / 2, int(s.size[1]) / 2);
        let home = v3(int(s.at[0]), int(s.at[2]) + half.y, int(s.at[1]));
        let path: Vec<V3> = s
            .path
            .iter()
            .map(|p| v3(int(p[0]), int(p[2]), int(p[1])))
            .collect();
        Machine {
            role: s.role,
            block,
            home,
            half,
            offset: V3::ZERO,
            prev_offset: V3::ZERO,
            path,
            next: 0,
            speed: fx::ratio(s.speed_ms, TICKS_PER_SECOND),
            wait: s.wait_ms * TICKS_PER_SECOND / 1000,
            waiting: 0,
            open: false,
            after: (s.after_ms > 0).then(|| (s.after_ms * TICKS_PER_SECOND / 1000) as u32),
            on_alarm: s.on_alarm,
        }
    }

    pub fn centre(&self) -> V3 {
        self.home.add(self.offset)
    }

    /// One tick of movement; returns how far it moved.
    pub fn step(&mut self) -> V3 {
        self.prev_offset = self.offset;
        if self.path.is_empty() || self.speed == 0 {
            return V3::ZERO;
        }
        if self.role.once() {
            // still until set moving; then to its last point, and it stays
            if !self.open {
                return V3::ZERO;
            }
            self.next = self.path.len() - 1;
        }
        if self.waiting > 0 {
            self.waiting -= 1;
            return V3::ZERO;
        }
        let goal = self.path[self.next];
        let to = goal.sub(self.offset);
        let step = if to.len() <= self.speed {
            if !self.role.once() {
                self.next = (self.next + 1) % self.path.len();
                self.waiting = self.wait;
            }
            to
        } else {
            to.norm().scale(self.speed)
        };
        self.offset = self.offset.add(step);
        step
    }
}

/// Whether a chest moving from `a` to `b` went through a ring the right way.
pub fn through_ring(g: &Gate, centre: V3, a: V3, b: V3) -> bool {
    let (da, db) = (a.sub(centre).dot(g.normal), b.sub(centre).dot(g.normal));
    if !(da < 0 && db >= 0) {
        return false;
    }
    let t = fx::div(-da, db - da);
    a.lerp(b, t).sub(centre).len() < g.r
}

/// Whether feet at `feet`, on the ground or not, have landed on a pad.
pub fn landed(g: &Gate, top: V3, feet: V3, grounded: bool) -> bool {
    let d = feet.sub(top);
    grounded && (d.y).abs() <= LAND_SLACK && v3(d.x, 0, d.z).len() < g.r
}

/// The velocity a boost ring gives.
pub fn boost(g: &Gate) -> V3 {
    let flat = v3(g.normal.x, 0, g.normal.z);
    let dir = if flat == V3::ZERO {
        v3(0, 0, -ONE)
    } else {
        flat.norm()
    };
    dir.scale(fx::ratio(BOOST_MS, TICKS_PER_SECOND))
        .add(v3(0, fx::ratio(BOOST_LIFT_MS, TICKS_PER_SECOND), 0))
}

/// The velocity a hazard throws a frame at `pos` with, away from its centre
/// and along the way it was moving.
pub fn knock(centre: V3, pos: V3, moving: V3) -> V3 {
    let away = v3(pos.x - centre.x, 0, pos.z - centre.z);
    let push = away.norm().add(v3(moving.x, 0, moving.z).norm().scale(ONE * 2));
    let dir = if push == V3::ZERO {
        v3(0, 0, ONE)
    } else {
        push.norm()
    };
    dir.scale(fx::ratio(KNOCK_MS, TICKS_PER_SECOND))
        .add(v3(0, fx::ratio(KNOCK_LIFT_MS, TICKS_PER_SECOND), 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring() -> Gate {
        gates(
            &[GateSpec {
                at: [0, -100, 20],
                kind: GateKind::Ring,
                r_m: 10,
                yaw: Some(0),
                ride: None,
                opens: None,
            }],
            V3::ZERO,
        )
        .remove(0)
    }

    #[test]
    fn a_ring_counts_only_when_flown_through_the_way_it_faces() {
        let g = ring();
        let c = g.at;
        let (front, back) = (c.add(v3(0, 0, int(1))), c.sub(v3(0, 0, int(1))));
        // heading 0 faces −z: flying from +z to −z goes through it
        assert!(through_ring(&g, c, front, back));
        assert!(!through_ring(&g, c, back, front), "the wrong way");
        let wide = v3(int(11), 0, 0);
        assert!(
            !through_ring(&g, c, front.add(wide), back.add(wide)),
            "round the outside"
        );
        let inside = v3(int(9), 0, 0);
        assert!(through_ring(&g, c, front.add(inside), back.add(inside)));
    }

    #[test]
    fn a_pad_counts_only_a_landing_on_its_top() {
        let mut g = ring();
        g.kind = GateKind::Pad;
        let top = g.at;
        assert!(landed(
            &g,
            top.add(v3(int(5), 0, 0)),
            top.add(v3(int(5), 0, 0)),
            true
        ));
        assert!(!landed(&g, top, top.add(v3(0, int(5), 0)), false), "flying over");
        assert!(
            !landed(&g, top, top.sub(v3(0, int(10), 0)), true),
            "standing below it"
        );
        assert!(!landed(&g, top, top.add(v3(int(13), 0, 0)), true), "off the edge");
    }

    #[test]
    fn a_door_waits_for_its_switch_and_a_piston_keeps_cycling() {
        let spec = MachineSpec {
            role: Role::Door,
            at: [0, 0, 0],
            size: [10, 2, 10],
            path: vec![[0, 0, 0], [0, 0, -11]],
            speed_ms: 30,
            wait_ms: 0,
            after_ms: 0,
            on_alarm: false,
        };
        let mut door = Machine::new(&spec, 0);
        for _ in 0..60 {
            door.step();
        }
        assert_eq!(door.offset, V3::ZERO, "shut");
        door.open = true;
        for _ in 0..60 {
            door.step();
        }
        assert_eq!(door.offset, v3(0, -int(11), 0), "sunk and staying");
        let mut piston = Machine::new(
            &MachineSpec {
                role: Role::Piston,
                path: vec![[0, 0, 0], [0, 0, -10]],
                ..spec
            },
            0,
        );
        let mut lowest = 0;
        let mut back_up = false;
        for _ in 0..120 {
            piston.step();
            lowest = lowest.min(piston.offset.y);
            back_up |= lowest < -int(9) && piston.offset.y == 0;
        }
        assert!(back_up, "it went down and came back");
    }

    #[test]
    fn a_boost_ring_throws_along_its_face_and_a_knock_throws_away() {
        let g = ring();
        assert!(boost(&g).z < 0 && boost(&g).y > 0);
        let v = knock(V3::ZERO, v3(int(3), 0, 0), V3::ZERO);
        assert!(v.x > 0 && v.y > 0, "{v:?}");
    }
}
