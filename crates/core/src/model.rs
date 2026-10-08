//! Assembling a mech's boxes from its parts.
//!
//! A `Rig` is built once per loadout: every box's fixed transform within its
//! part, and which joint it hangs from. Each frame, `Rig::pose` places the
//! joints (hips swing, arms and the shoulder weapon follow the aim) and
//! composes the boxes, so a frame costs one matrix product per box.

use crate::fx::{self, deg};
use crate::geom::{v3, Affine, V3};
use crate::mesh::Mesh;
use crate::parts::{BoxSpec, Catalog, Loadout, Part, Slot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    Primary = 0,
    Secondary = 1,
    Dark = 2,
    Glow = 3,
    Accent = 4,
}

impl Paint {
    pub const NAMES: [&'static str; 5] = ["primary", "secondary", "dark", "glow", "accent"];

    fn from_name(s: &str) -> Paint {
        match s {
            "secondary" => Paint::Secondary,
            "dark" => Paint::Dark,
            "glow" => Paint::Glow,
            "accent" => Paint::Accent,
            _ => Paint::Primary,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Joint {
    Legs,
    LegL,
    LegR,
    Core,
    Head,
    ArmR,
    ArmL,
    HandR,
    HandL,
    Shoulder,
    Booster,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RigBox {
    joint: Joint,
    local: Affine,
    mesh: Mesh,
    paint: Paint,
}

/// Where the pose puts the joints, all relative to the mech's feet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pose {
    /// walk cycle angle, TURN per two steps
    pub walk_phase: i32,
    /// leg swing amplitude, angle units
    pub stride: i32,
    /// both legs swept back (negative) in the air or under boost
    pub leg_tilt: i32,
    pub aim_pitch: i32,
    /// the torso leaning into its motion
    pub lean: i32,
}

/// A mech's boxes in its own frame, plus the points the simulation needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rig {
    boxes: Vec<RigBox>,
    hip: V3,
    core_mount: V3,
    head: V3,
    arm: V3,
    hand: V3,
    back: V3,
    booster: V3,
    muzzle: [V3; 3],
    flames: Vec<V3>,
    /// height of the top of the head above the feet
    pub height: i32,
}

pub fn cm(n: i32) -> i32 {
    fx::ratio(n, 100)
}

fn cm3(a: [i32; 3]) -> V3 {
    v3(cm(a[0]), cm(a[1]), cm(a[2]))
}

fn mirror(p: V3) -> V3 {
    v3(-p.x, p.y, p.z)
}

fn box_local(b: &BoxSpec, mirrored: bool) -> Affine {
    let sign = if mirrored { -1 } else { 1 };
    let at = cm3([b.at[0] * sign, b.at[1], b.at[2]]);
    let [pitch, yaw, roll] = b.rot;
    Affine::translate(at)
        .then(&Affine::rot_y(deg(yaw * sign)))
        .then(&Affine::rot_x(deg(pitch)))
        .then(&Affine::rot_z(deg(roll * sign)))
        .then(&Affine::scale(cm3(b.size)))
}

fn mount(p: &Part, name: &str) -> V3 {
    p.mounts.get(name).map(|m| cm3(*m)).unwrap_or(V3::ZERO)
}

impl Rig {
    pub fn build(l: &Loadout, cat: &Catalog) -> Rig {
        let mut boxes = Vec::new();
        let mut add = |part: &Part, joint: Joint, mirrored: bool| {
            for b in &part.boxes {
                let joint = match (joint, b.anim.as_deref()) {
                    (Joint::Legs, Some("leg_l")) => Joint::LegL,
                    (Joint::Legs, Some("leg_r")) => Joint::LegR,
                    (j, _) => j,
                };
                boxes.push(RigBox {
                    joint,
                    local: box_local(b, mirrored),
                    mesh: Mesh::from_name(&b.mesh).unwrap_or(Mesh::Cube),
                    paint: Paint::from_name(&b.paint),
                });
            }
        };
        let legs = l.part(Slot::Legs, cat);
        let core = l.part(Slot::Core, cat);
        let head = l.part(Slot::Head, cat);
        let arms = l.part(Slot::Arms, cat);
        let booster = l.part(Slot::Booster, cat);
        let rw = l.part(Slot::RightWeapon, cat);
        let lw = l.part(Slot::LeftWeapon, cat);
        let sw = l.part(Slot::ShoulderWeapon, cat);
        add(legs, Joint::Legs, false);
        add(core, Joint::Core, false);
        add(head, Joint::Head, false);
        add(arms, Joint::ArmR, false);
        add(arms, Joint::ArmL, true);
        add(rw, Joint::HandR, false);
        add(lw, Joint::HandL, true);
        add(sw, Joint::Shoulder, false);
        add(booster, Joint::Booster, false);
        let core_mount = mount(legs, "core");
        let head_at = mount(core, "head");
        let head_top = head
            .boxes
            .iter()
            .map(|b| cm(b.at[1] + b.size[1] / 2))
            .max()
            .unwrap_or(0);
        let flames = ["flame_l", "flame_r"]
            .iter()
            .filter(|n| booster.mounts.contains_key(**n))
            .map(|n| mount(booster, n))
            .collect();
        Rig {
            boxes,
            hip: core_mount,
            core_mount,
            head: head_at,
            arm: mount(core, "arm"),
            hand: mount(arms, "hand"),
            back: mount(core, "back"),
            booster: mount(core, "booster"),
            muzzle: [
                mount(rw, "muzzle"),
                mirror(mount(lw, "muzzle")),
                mount(sw, "muzzle"),
            ],
            flames,
            height: core_mount.y + head_at.y + head_top,
        }
    }

    fn joints(&self, pose: &Pose) -> [Affine; 11] {
        let swing = fx::mul(fx::sin(pose.walk_phase), pose.stride);
        let leg = |a: i32| {
            Affine::translate(self.hip)
                .then(&Affine::rot_x(a + pose.leg_tilt))
                .then(&Affine::translate(self.hip.neg()))
        };
        let core = Affine::translate(self.core_mount).then(&Affine::rot_x(-pose.lean));
        let aim = Affine::rot_x(pose.aim_pitch + pose.lean);
        let arm_r = core.then(&Affine::translate(self.arm)).then(&aim);
        let arm_l = core.then(&Affine::translate(mirror(self.arm))).then(&aim);
        [
            Affine::IDENTITY,
            leg(swing),
            leg(-swing),
            core,
            core.then(&Affine::translate(self.head)),
            arm_r,
            arm_l,
            arm_r.then(&Affine::translate(self.hand)),
            arm_l.then(&Affine::translate(mirror(self.hand))),
            core.then(&Affine::translate(self.back)).then(&aim),
            core.then(&Affine::translate(self.booster)),
        ]
    }

    fn joint_index(j: Joint) -> usize {
        j as usize
    }

    /// Every box, placed by `root` (the mech's feet and heading) and the pose.
    pub fn pose(&self, root: &Affine, pose: &Pose, out: &mut Vec<(Affine, Mesh, Paint)>) {
        let joints = self.joints(pose);
        for b in &self.boxes {
            let j = root.then(&joints[Rig::joint_index(b.joint)]);
            out.push((j.then(&b.local), b.mesh, b.paint));
        }
    }

    /// Where each weapon's shots leave from, in the mech's frame: right,
    /// left, shoulder.
    pub fn muzzles(&self, pose: &Pose) -> [V3; 3] {
        let j = self.joints(pose);
        [
            j[Rig::joint_index(Joint::HandR)].apply(self.muzzle[0]),
            j[Rig::joint_index(Joint::HandL)].apply(self.muzzle[1]),
            j[Rig::joint_index(Joint::Shoulder)].apply(self.muzzle[2]),
        ]
    }

    /// The booster nozzles, in the mech's frame.
    pub fn flames(&self, pose: &Pose) -> Vec<V3> {
        let j = self.joints(pose)[Rig::joint_index(Joint::Booster)];
        self.flames.iter().map(|f| j.apply(*f)).collect()
    }

    pub fn box_count(&self) -> usize {
        self.boxes.len()
    }

    /// The centre of mass the camera and the lock aim at.
    pub fn chest(&self) -> V3 {
        v3(0, self.core_mount.y + cm(150), 0)
    }
}

/// How far a stride swings at full walking speed.
pub const STRIDE_MAX: i32 = deg(28);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parts::tests::catalog;

    fn rig() -> Rig {
        let c = catalog();
        Rig::build(&c.default_loadout(), &c)
    }

    #[test]
    fn the_default_mech_stands_on_its_feet_and_is_about_seven_metres_tall() {
        let r = rig();
        let mut out = Vec::new();
        r.pose(&Affine::IDENTITY, &Pose::default(), &mut out);
        // the lowest corner of any box is at the ground, within 5 cm
        let lowest = out
            .iter()
            .map(|(a, _, _)| a.t.y - (a.c0.y.abs() + a.c1.y.abs() + a.c2.y.abs()) / 2)
            .min()
            .unwrap();
        assert!(lowest.abs() < cm(5), "lowest point {lowest}");
        assert!(
            (fx::int(6)..fx::int(9)).contains(&r.height),
            "height {}",
            r.height
        );
    }

    #[test]
    fn the_left_arm_is_the_mirror_of_the_right() {
        let r = rig();
        let m = r.muzzles(&Pose::default());
        // the two hands hold different weapons, so only the side mirrors
        assert_eq!(m[0].x, -m[1].x);
    }

    #[test]
    fn aiming_up_raises_the_muzzles() {
        let r = rig();
        let level = r.muzzles(&Pose::default());
        let up = r.muzzles(&Pose {
            aim_pitch: deg(30),
            ..Pose::default()
        });
        for k in 0..3 {
            assert!(up[k].y > level[k].y, "muzzle {k}");
        }
    }

    #[test]
    fn a_stride_moves_the_feet_opposite_ways() {
        let r = rig();
        let pose = Pose {
            walk_phase: fx::QUARTER,
            stride: STRIDE_MAX,
            ..Pose::default()
        };
        let mut out = Vec::new();
        r.pose(&Affine::IDENTITY, &pose, &mut out);
        let feet: Vec<i32> = r
            .boxes
            .iter()
            .zip(&out)
            .filter(|(b, _)| matches!(b.joint, Joint::LegL | Joint::LegR) && b.local.t.y < cm(40))
            .map(|(_, (a, _, _))| a.t.z)
            .collect();
        assert_eq!(feet.len(), 2);
        assert!(feet[0].signum() * feet[1].signum() < 0, "feet at {feet:?}");
    }

    #[test]
    fn a_rig_builds_from_every_part_the_catalogue_offers() {
        let c = catalog();
        for slot in Slot::ALL {
            for p in c.for_slot(slot) {
                let mut l = c.default_loadout();
                l.0.insert(slot, p.id.clone());
                assert!(Rig::build(&l, &c).box_count() > 0);
            }
        }
    }
}
