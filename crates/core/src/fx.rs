//! Q16.16 fixed point and integer angles.
//!
//! The core may not use floats (BUILDER-SPEC D2), so every length, speed and
//! matrix entry is an `i32` with 16 fractional bits: `ONE` is 1.0. Products go
//! through `i64` so they cannot overflow on the way. Angles are integers with
//! `TURN` units to a full circle, so wrapping is a mask, not a modulo of a float.

/// 1.0 in Q16.16.
pub const ONE: i32 = 1 << 16;
/// Fractional bits, reported to the page so the shim's divisor has one home.
pub const FRAC_BITS: u32 = 16;
/// Angle units in a full turn. A quarter turn is `TURN / 4`.
pub const TURN: i32 = 1 << 16;
pub const QUARTER: i32 = TURN / 4;
pub const HALF: i32 = TURN / 2;

/// 2π in Q16.16 (6.283185 × 65 536 = 411 774.8).
const TWO_PI: i64 = 411_775;

pub const fn int(n: i32) -> i32 {
    n * ONE
}

/// `num / den` as Q16.16, for constants written as fractions (`ratio(1, 60)`).
pub const fn ratio(num: i32, den: i32) -> i32 {
    ((num as i64 * ONE as i64) / den as i64) as i32
}

pub fn mul(a: i32, b: i32) -> i32 {
    ((a as i64 * b as i64) >> FRAC_BITS) as i32
}

/// `a / b` in Q16.16. A zero divisor gives zero rather than a panic: every
/// caller treats "no length" as "no direction".
pub fn div(a: i32, b: i32) -> i32 {
    if b == 0 {
        return 0;
    }
    (((a as i64) << FRAC_BITS) / b as i64) as i32
}

/// Degrees to angle units.
pub const fn deg(d: i32) -> i32 {
    ((d as i64 * TURN as i64) / 360) as i32
}

/// Wraps an angle into `[-HALF, HALF)`, for differences between headings.
pub fn wrap(a: i32) -> i32 {
    ((a + HALF) & (TURN - 1)) - HALF
}

/// Sine of an angle in `TURN` units, as Q16.16.
///
/// The angle is folded into the quarter wave around zero, converted to
/// radians, and summed as a Taylor series to the 11th power, which is exact to
/// the last Q16 bit over `[-π/2, π/2]`.
pub fn sin(angle: i32) -> i32 {
    let a = angle & (TURN - 1);
    let x = if a < QUARTER {
        a
    } else if a < 3 * QUARTER {
        HALF - a
    } else {
        a - TURN
    };
    let r = x as i64 * TWO_PI / TURN as i64;
    let r2 = (r * r) >> FRAC_BITS;
    let mut term = r;
    let mut sum = r;
    for k in 1..=5i64 {
        term = -((term * r2) >> FRAC_BITS) / ((2 * k) * (2 * k + 1));
        sum += term;
    }
    sum.clamp(-(ONE as i64), ONE as i64) as i32
}

pub fn cos(angle: i32) -> i32 {
    sin(angle + QUARTER)
}

/// The angle of the vector `(x, y)` from the +x axis, counter-clockwise, in
/// `TURN` units in `[0, TURN)`. Accurate to about 0.25°: the arctangent on
/// `[0, 1]` is `π/4·z + 0.273·z·(1 − z)`, a bounded polynomial with no table.
pub fn atan2(y: i32, x: i32) -> i32 {
    if x == 0 && y == 0 {
        return 0;
    }
    let (ax, ay) = (x.unsigned_abs() as i64, y.unsigned_abs() as i64);
    let octant = |z: i64| -> i64 {
        // radians in Q16: π/4 = 51 472, 0.273 = 17 891
        let rad = (51_472 * z + (17_891 * ((z * (ONE as i64 - z)) >> FRAC_BITS))) >> FRAC_BITS;
        rad * TURN as i64 / TWO_PI
    };
    let a = if ax >= ay {
        octant((ay << FRAC_BITS) / ax)
    } else {
        QUARTER as i64 - octant((ax << FRAC_BITS) / ay)
    } as i32;
    let full = match (x >= 0, y >= 0) {
        (true, true) => a,
        (false, true) => HALF - a,
        (false, false) => HALF + a,
        (true, false) => TURN - a,
    };
    full & (TURN - 1)
}

/// Integer square root, rounded down.
pub fn isqrt(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// Moves `from` toward `to` by at most `step`.
pub fn approach(from: i32, to: i32, step: i32) -> i32 {
    if from < to {
        (from + step).min(to)
    } else {
        (from - step).max(to)
    }
}

/// Linear interpolation, `t` in `[0, ONE]`.
pub fn lerp(a: i32, b: i32, t: i32) -> i32 {
    a + mul(b - a, t)
}

/// Interpolates headings the short way round.
pub fn lerp_angle(a: i32, b: i32, t: i32) -> i32 {
    a + mul(wrap(b - a), t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: i32, b: i32, tol: i32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn sine_matches_hand_computed_values() {
        // sin 0 = 0, sin 30° = 0.5, sin 90° = 1, sin 180° = 0, sin 270° = −1
        assert_eq!(sin(0), 0);
        assert!(near(sin(deg(30)), ONE / 2, 2), "{}", sin(deg(30)));
        assert!(near(sin(QUARTER), ONE, 1));
        assert!(near(sin(HALF), 0, 1));
        assert!(near(sin(3 * QUARTER), -ONE, 1));
        // sin 45° = 0.707107 → 46 341
        assert!(near(sin(deg(45)), 46_341, 3), "{}", sin(deg(45)));
        // cos 60° = 0.5
        // deg() truncates, so 60° is a hair short and its cosine a hair long
        assert!(near(cos(deg(60)), ONE / 2, 8));
    }

    #[test]
    fn sine_squared_plus_cosine_squared_is_one_all_round() {
        let mut a = 0;
        while a < TURN {
            let s = sin(a) as i64;
            let c = cos(a) as i64;
            let one = (s * s + c * c) >> FRAC_BITS;
            assert!((one - ONE as i64).abs() < 8, "angle {a}: {one}");
            a += 256;
        }
    }

    #[test]
    fn negative_and_wrapped_angles_agree() {
        assert_eq!(sin(-deg(30)), -sin(deg(30)));
        assert_eq!(sin(deg(30) + TURN), sin(deg(30)));
    }

    #[test]
    fn atan2_finds_each_quadrant() {
        let tol = deg(1) / 2;
        assert!(near(atan2(0, ONE), 0, tol));
        assert!(near(atan2(ONE, ONE), deg(45), tol));
        assert!(near(atan2(ONE, 0), QUARTER, tol));
        assert!(near(atan2(ONE, -ONE), deg(135), tol));
        assert!(near(atan2(-ONE, -ONE), deg(225), tol));
        assert!(near(atan2(-ONE, ONE), deg(315), tol));
        // tan 30° = 0.57735: atan2(0.57735, 1) = 30°
        assert!(
            near(atan2(37_837, ONE), deg(30), tol),
            "{}",
            atan2(37_837, ONE)
        );
    }

    #[test]
    fn atan2_inverts_sine_and_cosine() {
        let mut a = 0;
        while a < TURN {
            let back = atan2(sin(a), cos(a));
            assert!(wrap(back - a).abs() <= deg(1) / 2, "{a} -> {back}");
            a += 997;
        }
    }

    #[test]
    fn isqrt_rounds_down() {
        assert_eq!(isqrt(0), 0);
        assert_eq!(isqrt(15), 3);
        assert_eq!(isqrt(16), 4);
        assert_eq!(isqrt(1 << 40), 1 << 20);
    }

    #[test]
    fn wrap_takes_the_short_way() {
        assert!((wrap(deg(350) - deg(10)) + deg(20)).abs() <= 1);
        assert!((lerp_angle(deg(350), deg(10), ONE / 2) - deg(360)).abs() <= 1);
    }

    #[test]
    fn mul_and_div_are_inverse() {
        assert_eq!(mul(int(3), ratio(1, 2)), ratio(3, 2));
        assert_eq!(div(int(3), int(2)), ratio(3, 2));
        assert_eq!(div(ONE, 0), 0);
    }
}
