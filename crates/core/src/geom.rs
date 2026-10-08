//! Vectors, affine transforms and the projection, all in Q16.16.
//!
//! Conventions: y is up, the world is right-handed, and a heading (`yaw`) of
//! zero faces −z, as an OpenGL camera does. Positive yaw turns left (counter-
//! clockwise seen from above); positive pitch looks up.

use crate::fx::{self, ONE};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct V3 {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

pub const fn v3(x: i32, y: i32, z: i32) -> V3 {
    V3 { x, y, z }
}

// Named methods rather than operator traits: every call site reads as the
// fixed-point operation it is, and none needs a trait import.
#[allow(clippy::should_implement_trait, clippy::len_without_is_empty)]
impl V3 {
    pub const ZERO: V3 = v3(0, 0, 0);
    pub const UP: V3 = v3(0, ONE, 0);

    pub fn add(self, o: V3) -> V3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
    pub fn sub(self, o: V3) -> V3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
    pub fn scale(self, s: i32) -> V3 {
        v3(fx::mul(self.x, s), fx::mul(self.y, s), fx::mul(self.z, s))
    }
    pub fn neg(self) -> V3 {
        v3(-self.x, -self.y, -self.z)
    }
    pub fn dot(self, o: V3) -> i32 {
        ((self.x as i64 * o.x as i64 + self.y as i64 * o.y as i64 + self.z as i64 * o.z as i64)
            >> fx::FRAC_BITS) as i32
    }
    pub fn cross(self, o: V3) -> V3 {
        v3(
            fx::mul(self.y, o.z) - fx::mul(self.z, o.y),
            fx::mul(self.z, o.x) - fx::mul(self.x, o.z),
            fx::mul(self.x, o.y) - fx::mul(self.y, o.x),
        )
    }
    pub fn len(self) -> i32 {
        let sq = self.x as i64 * self.x as i64
            + self.y as i64 * self.y as i64
            + self.z as i64 * self.z as i64;
        fx::isqrt(sq as u64) as i32
    }
    pub fn len_xz(self) -> i32 {
        v3(self.x, 0, self.z).len()
    }
    /// The unit vector, or zero for a zero vector.
    pub fn norm(self) -> V3 {
        let l = self.len();
        if l == 0 {
            return V3::ZERO;
        }
        v3(fx::div(self.x, l), fx::div(self.y, l), fx::div(self.z, l))
    }
    /// The vector shortened to at most `max` long.
    pub fn clamp_len(self, max: i32) -> V3 {
        let l = self.len();
        if l <= max || l == 0 {
            return self;
        }
        self.scale(fx::div(max, l))
    }
    pub fn lerp(self, o: V3, t: i32) -> V3 {
        v3(
            fx::lerp(self.x, o.x, t),
            fx::lerp(self.y, o.y, t),
            fx::lerp(self.z, o.z, t),
        )
    }
    pub fn dist(self, o: V3) -> i32 {
        self.sub(o).len()
    }
}

/// The unit vector a heading and pitch face.
pub fn facing(yaw: i32, pitch: i32) -> V3 {
    let cp = fx::cos(pitch);
    v3(
        -fx::mul(fx::sin(yaw), cp),
        fx::sin(pitch),
        -fx::mul(fx::cos(yaw), cp),
    )
}

/// The flat unit vector to the right of a heading.
pub fn right_of(yaw: i32) -> V3 {
    v3(fx::cos(yaw), 0, -fx::sin(yaw))
}

/// The heading and pitch that face along `d`.
pub fn yaw_pitch_of(d: V3) -> (i32, i32) {
    let yaw = fx::atan2(-d.x, -d.z);
    let pitch = fx::wrap(fx::atan2(d.y, d.len_xz()));
    (yaw, pitch)
}

/// A 3×3 linear part (columns) and a translation: what one drawn box needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Affine {
    pub c0: V3,
    pub c1: V3,
    pub c2: V3,
    pub t: V3,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        c0: v3(ONE, 0, 0),
        c1: v3(0, ONE, 0),
        c2: v3(0, 0, ONE),
        t: V3::ZERO,
    };

    pub fn translate(t: V3) -> Affine {
        Affine {
            t,
            ..Affine::IDENTITY
        }
    }
    pub fn scale(s: V3) -> Affine {
        Affine {
            c0: v3(s.x, 0, 0),
            c1: v3(0, s.y, 0),
            c2: v3(0, 0, s.z),
            t: V3::ZERO,
        }
    }
    pub fn rot_y(a: i32) -> Affine {
        let (s, c) = (fx::sin(a), fx::cos(a));
        Affine {
            c0: v3(c, 0, -s),
            c1: v3(0, ONE, 0),
            c2: v3(s, 0, c),
            t: V3::ZERO,
        }
    }
    pub fn rot_x(a: i32) -> Affine {
        let (s, c) = (fx::sin(a), fx::cos(a));
        Affine {
            c0: v3(ONE, 0, 0),
            c1: v3(0, c, s),
            c2: v3(0, -s, c),
            t: V3::ZERO,
        }
    }
    pub fn rot_z(a: i32) -> Affine {
        let (s, c) = (fx::sin(a), fx::cos(a));
        Affine {
            c0: v3(c, s, 0),
            c1: v3(-s, c, 0),
            c2: v3(0, 0, ONE),
            t: V3::ZERO,
        }
    }
    /// A basis whose −z axis points along `dir` (a unit vector), for tracers
    /// and missiles that face where they fly.
    pub fn looking(dir: V3) -> Affine {
        let back = dir.neg();
        let helper = if back.y.abs() > ONE - ONE / 16 {
            v3(ONE, 0, 0)
        } else {
            V3::UP
        };
        let x = helper.cross(back).norm();
        let y = back.cross(x);
        Affine {
            c0: x,
            c1: y,
            c2: back,
            t: V3::ZERO,
        }
    }
    pub fn apply(&self, p: V3) -> V3 {
        self.apply_dir(p).add(self.t)
    }
    pub fn apply_dir(&self, d: V3) -> V3 {
        self.c0
            .scale(d.x)
            .add(self.c1.scale(d.y))
            .add(self.c2.scale(d.z))
    }
    /// `self * o`: apply `o` first, then `self`.
    pub fn then(&self, o: &Affine) -> Affine {
        Affine {
            c0: self.apply_dir(o.c0),
            c1: self.apply_dir(o.c1),
            c2: self.apply_dir(o.c2),
            t: self.apply(o.t),
        }
    }
}

/// A 4×4 matrix, column-major, as WebGL reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mat4(pub [i32; 16]);

impl Mat4 {
    pub fn mul(&self, o: &Mat4) -> Mat4 {
        let mut m = [0i32; 16];
        for col in 0..4 {
            for row in 0..4 {
                let mut s: i64 = 0;
                for k in 0..4 {
                    s += self.0[k * 4 + row] as i64 * o.0[col * 4 + k] as i64;
                }
                m[col * 4 + row] = (s >> fx::FRAC_BITS) as i32;
            }
        }
        Mat4(m)
    }

    pub fn from_affine(a: &Affine) -> Mat4 {
        Mat4([
            a.c0.x, a.c0.y, a.c0.z, 0, a.c1.x, a.c1.y, a.c1.z, 0, a.c2.x, a.c2.y, a.c2.z, 0, a.t.x,
            a.t.y, a.t.z, ONE,
        ])
    }

    /// Perspective projection. `fov` is the vertical field of view in angle
    /// units; `aspect` is width over height in Q16; `near` and `far` are in Q16.
    pub fn perspective(fov: i32, aspect: i32, near: i32, far: i32) -> Mat4 {
        let f = fx::div(fx::cos(fov / 2), fx::sin(fov / 2));
        let mut m = [0i32; 16];
        m[0] = fx::div(f, aspect);
        m[5] = f;
        m[10] = fx::div(far + near, near - far);
        m[11] = -ONE;
        m[14] = fx::div(2 * fx::mul(far, near), near - far);
        Mat4(m)
    }

    /// The view matrix of a camera at `eye` with a heading and a pitch.
    pub fn view(eye: V3, yaw: i32, pitch: i32) -> Mat4 {
        let inv = Affine::rot_x(-pitch)
            .then(&Affine::rot_y(-yaw))
            .then(&Affine::translate(eye.neg()));
        Mat4::from_affine(&inv)
    }

    /// Clip-space coordinates `(x, y, z, w)` of a world point.
    pub fn project(&self, p: V3) -> [i32; 4] {
        let mut out = [0i32; 4];
        for (row, o) in out.iter_mut().enumerate() {
            let s = self.0[row] as i64 * p.x as i64
                + self.0[4 + row] as i64 * p.y as i64
                + self.0[8 + row] as i64 * p.z as i64
                + ((self.0[12 + row] as i64) << fx::FRAC_BITS);
            *o = (s >> fx::FRAC_BITS) as i32;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::{deg, int, QUARTER};

    fn near(a: V3, b: V3, tol: i32) -> bool {
        (a.x - b.x).abs() <= tol && (a.y - b.y).abs() <= tol && (a.z - b.z).abs() <= tol
    }

    #[test]
    fn heading_zero_faces_minus_z_and_right_is_plus_x() {
        assert!(near(facing(0, 0), v3(0, 0, -ONE), 2));
        assert!(near(right_of(0), v3(ONE, 0, 0), 2));
        // a quarter turn left faces −x
        assert!(near(facing(QUARTER, 0), v3(-ONE, 0, 0), 2));
        // right is forward × up
        let f = facing(deg(37), 0);
        assert!(near(f.cross(V3::UP), right_of(deg(37)), 4));
    }

    #[test]
    fn rotation_about_y_turns_forward_the_way_facing_does() {
        let r = Affine::rot_y(deg(70));
        assert!(near(r.apply_dir(v3(0, 0, -ONE)), facing(deg(70), 0), 3));
        let p = Affine::rot_x(deg(20));
        assert!(near(p.apply_dir(v3(0, 0, -ONE)), facing(0, deg(20)), 3));
    }

    #[test]
    fn yaw_pitch_of_inverts_facing() {
        for (y, p) in [(deg(10), deg(5)), (deg(200), -deg(30)), (deg(300), deg(60))] {
            let (y2, p2) = yaw_pitch_of(facing(y, p));
            assert!(fx::wrap(y2 - y).abs() <= deg(1), "{y} {y2}");
            assert!((p2 - p).abs() <= deg(1), "{p} {p2}");
        }
    }

    #[test]
    fn a_point_ahead_of_the_camera_lands_in_the_clip_volume_and_one_behind_does_not() {
        let eye = v3(int(10), int(5), int(20));
        let yaw = deg(30);
        let vp = Mat4::perspective(deg(70), ratio_16_9(), ONE / 4, int(1000))
            .mul(&Mat4::view(eye, yaw, 0));
        let ahead = eye.add(facing(yaw, 0).scale(int(50)));
        let [x, y, z, w] = vp.project(ahead);
        assert!(
            w > 0 && x.abs() < w / 64 && y.abs() < w / 64 && z.abs() < w,
            "{x} {y} {z} {w}"
        );
        let behind = eye.sub(facing(yaw, 0).scale(int(50)));
        assert!(vp.project(behind)[3] < 0);
    }

    #[test]
    fn looking_points_minus_z_along_the_direction() {
        let d = v3(ONE / 2, ONE / 2, -46_341).norm();
        let b = Affine::looking(d);
        assert!(near(b.apply_dir(v3(0, 0, -ONE)), d, 8));
        // straight up still gives a basis
        let up = Affine::looking(V3::UP);
        assert!(near(up.apply_dir(v3(0, 0, -ONE)), V3::UP, 8));
    }

    fn ratio_16_9() -> i32 {
        fx::ratio(16, 9)
    }
}
