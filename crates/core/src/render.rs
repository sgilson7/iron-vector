//! What the page draws: one 3×4 matrix, a colour and a glow per box, grouped
//! by mesh so each mesh is one instanced draw. Nothing here changes the world.
//!
//! On a ring planet the world is simulated flat, on the unrolled floor, and
//! drawn bent: `Warp` wraps every box round the inside of a cylinder whose
//! circumference is the map's width, so the floor curves up overhead.

use crate::campaign::Look;
use crate::combat::{CraftKind, EffectKind, Target, Team};
use crate::content::{q16, Palette, Rgb};
use crate::course::{GateKind, Role};
use crate::fx::{self, deg, int, ONE};
use crate::geom::{facing, v3, Affine, Mat4, V3};
use crate::map::Map;
use crate::mesh::Mesh;
use crate::model::Paint;
use crate::parts::WeaponKind;
use crate::rng::Rng;
use crate::world::World;
use serde::Serialize;

/// Values per instance: three columns with a translation component each, then colour and glow.
pub const INSTANCE_VALUES: usize = 16;
/// Values per draw in `draws`: buffer, mesh, byte offset, instance count, grid.
pub const DRAW_VALUES: usize = 5;
pub const BYTES_PER_VALUE: usize = 4;
pub const STATIC_BUFFER: u32 = 0;
pub const DYNAMIC_BUFFER: u32 = 1;

pub const FOV: i32 = deg(72);
pub const NEAR: i32 = ONE / 8;
pub const FAR: i32 = int(3200);
const GRID_STEP_M: i32 = 10;
const WINDOW_GLOW: i32 = ONE * 2 / 5;
const WINDOW_EVERY_M: i32 = 11;
const MAX_WINDOW_BANDS: i32 = 7;

/// Bends the flat world round the inside of a ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Warp {
    pub radius: i32,
    pub circumference: i32,
}

impl Warp {
    pub fn of(map: &Map) -> Option<Warp> {
        map.wrap.then(|| Warp {
            radius: map.ring_radius(),
            circumference: map.half_x * 2,
        })
    }

    /// The angle round the ring at a floor distance `x`.
    pub fn angle(&self, x: i32) -> i32 {
        (x as i64 * fx::TURN as i64 / self.circumference as i64) as i32
    }

    /// Where a flat point is on the ring: the floor is at radius `R` from the
    /// axis, and height climbs toward the axis.
    pub fn point(&self, p: V3) -> V3 {
        let a = self.angle(p.x);
        let r = self.radius - p.y;
        v3(fx::mul(r, fx::sin(a)), self.radius - fx::mul(r, fx::cos(a)), p.z)
    }

    /// A direction at a flat point, turned to the ring's local frame there.
    pub fn dir(&self, at: V3, d: V3) -> V3 {
        Affine::rot_z(self.angle(at.x)).apply_dir(d)
    }

    pub fn affine(&self, a: &Affine) -> Affine {
        let rot = Affine::rot_z(self.angle(a.t.x));
        let lin = rot.then(&Affine { t: V3::ZERO, ..*a });
        Affine {
            t: self.point(a.t),
            ..lin
        }
    }
}

/// A world point as drawn.
pub fn drawn(w: &World, p: V3) -> V3 {
    Warp::of(&w.map).map_or(p, |wp| wp.point(p))
}

pub struct Batch {
    meshes: [Vec<i32>; 5],
    warp: Option<Warp>,
}

impl Batch {
    pub fn new(warp: Option<Warp>) -> Batch {
        Batch {
            meshes: Default::default(),
            warp,
        }
    }

    /// A box in the world: bent onto the ring if there is one.
    pub fn push(&mut self, mesh: Mesh, a: &Affine, rgb: [i32; 3], glow: i32) {
        let a = self.warp.map_or(*a, |w| w.affine(a));
        self.push_raw(mesh, &a, rgb, glow);
    }

    /// A box placed directly, as for the cockpit and the stars.
    pub fn push_raw(&mut self, mesh: Mesh, a: &Affine, rgb: [i32; 3], glow: i32) {
        self.meshes[mesh as usize].extend_from_slice(&[
            a.c0.x, a.c0.y, a.c0.z, a.t.x, a.c1.x, a.c1.y, a.c1.z, a.t.y, a.c2.x, a.c2.y, a.c2.z, a.t.z,
            rgb[0], rgb[1], rgb[2], glow,
        ]);
    }

    /// The instance values, and one draw per mesh that has any.
    pub fn finish(self, buffer: u32) -> (Vec<i32>, Vec<u32>) {
        let mut values = Vec::new();
        let mut draws = Vec::new();
        for mesh in Mesh::ALL {
            let v = &self.meshes[mesh as usize];
            if v.is_empty() {
                continue;
            }
            let offset = (values.len() * BYTES_PER_VALUE) as u32;
            let count = (v.len() / INSTANCE_VALUES) as u32;
            let grid = (mesh == Mesh::Ground) as u32;
            draws.extend_from_slice(&[buffer, mesh as u32, offset, count, grid]);
            values.extend_from_slice(v);
        }
        (values, draws)
    }
}

pub fn boxed(at: V3, size: V3) -> Affine {
    Affine::translate(at).then(&Affine::scale(size))
}

pub fn shade(c: [i32; 3], pct: i32) -> [i32; 3] {
    c.map(|v| (v * pct / 100).min(ONE))
}

fn colour(c: Rgb) -> [i32; 3] {
    q16(c)
}

/// The ground and the buildings. They never move, so the page uploads them once.
pub fn scene(w: &World, look: &Look) -> (Vec<i32>, Vec<u32>) {
    let warp = Warp::of(&w.map);
    let mut b = Batch::new(warp);
    let m = &w.map;
    let ground = colour(look.ground);
    let grid = colour(look.grid);
    let glow = look.ground_glow_pct * ONE / 100;
    match warp {
        None => b.push(
            Mesh::Ground,
            // four times the map, but never so large it overflows
            &boxed(
                V3::ZERO,
                v3(m.half_x.min(int(3_750)) * 8, ONE, m.half_z.min(int(3_750)) * 8),
            ),
            ground,
            glow,
        ),
        Some(_) => ring_floor(&mut b, m, ground, grid),
    }
    let window = colour(look.window);
    let roof = colour(look.roof);
    let structure_blocks: Vec<usize> = w.structures.iter().map(|s| s.block).collect();
    for (i, blk) in m.blocks.iter().enumerate() {
        if structure_blocks.contains(&i) {
            continue;
        }
        let size = blk.max.sub(blk.min);
        let centre = v3(
            (blk.min.x + blk.max.x) / 2,
            (blk.min.y + blk.max.y) / 2,
            (blk.min.z + blk.max.z) / 2,
        );
        let base = colour(look.buildings[blk.shade as usize % look.buildings.len()]);
        b.push(
            Mesh::Cube,
            &boxed(centre, size),
            shade(base, 88 + (blk.shade as i32 % 24)),
            0,
        );
        let cap = v3((size.x - int(2)).max(ONE), ONE, (size.z - int(2)).max(ONE));
        b.push(
            Mesh::Cube,
            &boxed(v3(centre.x, blk.max.y + ONE / 2, centre.z), cap),
            roof,
            0,
        );
        if look.windows {
            let bands = (size.y / int(WINDOW_EVERY_M) - 1).clamp(0, MAX_WINDOW_BANDS);
            for k in 0..bands {
                let y = blk.min.y + int(WINDOW_EVERY_M) * (k + 1) - int(3);
                let band = v3(size.x + ONE / 5, ONE + ONE / 4, size.z + ONE / 5);
                b.push(
                    Mesh::Cube,
                    &boxed(v3(centre.x, y, centre.z), band),
                    window,
                    WINDOW_GLOW,
                );
            }
        }
    }
    if look.stars && warp.is_none() {
        stars(&mut b, look);
    }
    b.finish(STATIC_BUFFER)
}

/// The inside of a ring: floor panels all the way round and along, a lit
/// seam every few panels, and a low wall at each open end.
fn ring_floor(b: &mut Batch, m: &Map, ground: [i32; 3], grid: [i32; 3]) {
    const AROUND: i32 = 48;
    let along = (m.half_z * 2 / int(150)).max(1);
    let w = m.half_x * 2 / AROUND;
    let len = m.half_z * 2 / along;
    for i in 0..AROUND {
        let x = -m.half_x + w * i + w / 2;
        for j in 0..along {
            let z = -m.half_z + len * j + len / 2;
            let c = if (i + j) % 2 == 0 {
                ground
            } else {
                shade(ground, 90)
            };
            b.push(
                Mesh::Cube,
                &boxed(v3(x, -ONE, z), v3(w + ONE, int(2), len + ONE)),
                c,
                0,
            );
        }
        if i % 6 == 0 {
            b.push(
                Mesh::Cube,
                &boxed(v3(x - w / 2, ONE / 8, 0), v3(ONE / 2, ONE / 4, m.half_z * 2)),
                shade(grid, 70),
                ONE / 3,
            );
        }
        for end in [-m.half_z, m.half_z] {
            b.push(
                Mesh::Cube,
                &boxed(v3(x, int(12), end), v3(w + ONE, int(24), int(4))),
                shade(ground, 60),
                0,
            );
            b.push(
                Mesh::Cube,
                &boxed(v3(x, int(24), end), v3(w + ONE, ONE, int(5))),
                grid,
                ONE,
            );
        }
    }
    let mut z = -m.half_z + int(300);
    while z < m.half_z {
        for i in 0..AROUND {
            let x = -m.half_x + w * i + w / 2;
            b.push(
                Mesh::Cube,
                &boxed(v3(x, ONE / 4, z), v3(w + ONE, ONE / 2, int(3))),
                grid,
                ONE / 2,
            );
        }
        z += int(300);
    }
}

/// A dome of stars far beyond the map: past the edge, inside the far plane.
fn stars(b: &mut Batch, look: &Look) {
    let mut r = Rng::new(77);
    let reach = int(2200).max(int(look.far_m) * 4 / 5);
    let grow = reach / int(2200);
    for _ in 0..420 {
        let yaw = r.range(0, fx::TURN - 1);
        let pitch = r.range(deg(4), deg(85));
        let p = facing(yaw, pitch).scale(reach);
        let s = ONE * r.range(3, 9) * grow.max(1);
        let tint = r.range(70, 100);
        let c = shade([ONE, ONE, ONE], tint);
        b.push_raw(Mesh::Octa, &boxed(p, v3(s, s, s)), c, ONE);
    }
}

/// Everything that moves, placed between the last two ticks by `alpha`.
/// `hide_player` leaves the player's frame out, for the view from inside it.
pub fn frame(w: &World, look: &Look, pal: &Palette, alpha: i32, hide_player: bool) -> Batch {
    let mut b = Batch::new(Warp::of(&w.map));
    let shadow = colour(pal.shadow);
    let mut parts = Vec::new();
    for (i, m) in w.mechs.iter().enumerate().filter(|(_, m)| m.alive) {
        if i == 0 && hide_player {
            continue;
        }
        let pos = m.body.prev_pos.lerp(m.body.pos, alpha);
        let yaw = fx::lerp_angle(m.body.prev_yaw, m.body.yaw, alpha);
        let root = m.frame(pos, yaw);
        let pose = m.body.pose();
        parts.clear();
        m.rig.pose(&root, &pose, &mut parts);
        for (a, mesh, paint) in &parts {
            let glow = if *paint == Paint::Glow { ONE } else { 0 };
            let mut c = m.paint[*paint as usize];
            if m.hit_flash > 0 || m.stagger > 0 && (w.tick / 4).is_multiple_of(2) {
                c = c.map(|v| (v + ONE) / 2);
            }
            b.push(*mesh, a, c, glow);
        }
        if m.body.thrust > 0 {
            let (len, col) = if m.body.thrust == 2 {
                (int(4), pal.flame_qb)
            } else {
                (int(2), pal.flame)
            };
            for f in m.rig.flames(&pose) {
                let at = root.then(&Affine::translate(f.add(v3(0, 0, len / 2))));
                b.push(
                    Mesh::Octa,
                    &at.then(&Affine::scale(v3(ONE / 2, ONE / 2, len))),
                    colour(col),
                    ONE,
                );
            }
        }
        push_shadow(&mut b, w, pos, fx::mul(int(4), m.scale), shadow);
    }
    // fortress hulls, walking between ticks; a fallen one goes dark
    let giant = pal.giant.paints();
    for f in &w.fortresses {
        let at = f.prev_pos.lerp(f.pos, alpha);
        for (lo, hi, paint) in &f.hull {
            let c = giant[*paint as usize];
            let (c, glow) = match (f.alive, *paint) {
                (false, _) => (shade(c, 30), 0),
                (true, 3) => (c, ONE),
                _ => (c, 0),
            };
            let centre = at.add(v3((lo.x + hi.x) / 2, (lo.y + hi.y) / 2, (lo.z + hi.z) / 2));
            b.push(Mesh::Cube, &boxed(centre, hi.sub(*lo)), c, glow);
        }
    }
    for (i, c) in w.craft.iter().enumerate().filter(|(_, c)| c.alive) {
        let pos = c.prev_pos.lerp(c.pos, alpha);
        let yaw = fx::lerp_angle(c.prev_yaw, c.yaw, alpha);
        let root = Affine::translate(pos).then(&Affine::rot_y(yaw));
        let sealed = c.kind == CraftKind::Core && w.core_sealed(i);
        craft_model(&mut b, pal, c.kind, &root, w.tick, alpha, c.radius, sealed);
        if matches!(c.kind, CraftKind::Drone | CraftKind::Heli | CraftKind::Tank) {
            push_shadow(&mut b, w, pos, int(4), shadow);
        }
    }
    let structure = colour(look.structure);
    for (i, s) in w.structures.iter().enumerate() {
        let blk = &w.map.blocks[s.block];
        let size = blk.max.sub(blk.min);
        let centre = w.structure_centre(i);
        let health = s.ap * 100 / s.max_ap.max(1);
        // a dark body that darkens as it is hurt; the glowing band says what it is
        let c = if s.alive {
            shade(structure, 25 + health / 5)
        } else {
            shade(structure, 12)
        };
        b.push(Mesh::Cube, &boxed(centre, size), c, 0);
        if s.alive {
            let lamp = boxed(
                v3(centre.x, blk.max.y + ONE, centre.z),
                v3(int(4), int(2), int(4)),
            );
            b.push(Mesh::Octa, &lamp, structure, ONE);
            let band = boxed(
                v3(centre.x, blk.max.y - int(2), centre.z),
                v3(size.x + ONE / 4, ONE, size.z + ONE / 4),
            );
            b.push(Mesh::Cube, &band, structure, ONE / 2);
        }
    }
    // a race's machines: hazards in warning colours, lifts and doors in the course's
    for mc in &w.machines {
        let at = mc.home.add(mc.prev_offset.lerp(mc.offset, alpha));
        let size = mc.half.scale(int(2));
        let (body, trim) = match mc.role {
            Role::Sweeper | Role::Piston => (colour(pal.hazard), colour(pal.hazard_trim)),
            Role::Lift => (shade(colour(pal.checkpoint), 30), colour(pal.checkpoint)),
            Role::Door => (shade(colour(pal.switch), 30), colour(pal.switch)),
        };
        b.push(
            Mesh::Cube,
            &boxed(at, size),
            body,
            if mc.role.hazard() { ONE / 3 } else { 0 },
        );
        // a glowing band round the top edge, so its shape reads at speed
        let band = boxed(
            v3(at.x, at.y + mc.half.y - ONE / 4, at.z),
            v3(size.x + ONE / 4, ONE / 2, size.z + ONE / 4),
        );
        b.push(Mesh::Cube, &band, trim, ONE);
    }
    if let Some(mi) = &w.mission {
        let next = w.player().course_next;
        for (k, g) in mi.course.iter().enumerate().skip(next).take(3) {
            let glow = if k == next { ONE } else { ONE / 4 };
            let at = w.gate_point(g);
            let c = match g.kind {
                GateKind::Ring | GateKind::Pad => colour(pal.checkpoint),
                GateKind::Boost => colour(pal.boost),
                GateKind::Switch => colour(pal.switch),
            };
            if g.kind.landing() {
                // a ring lying on the pad, and a beacon standing up from the next one
                checkpoint(&mut b, at.add(v3(0, ONE / 4, 0)), V3::UP, g.r, c, glow);
                if k == next {
                    let beam = boxed(at.add(v3(0, int(30), 0)), v3(ONE / 2, int(60), ONE / 2));
                    b.push(Mesh::Cube, &beam, c, ONE);
                }
            } else {
                checkpoint(&mut b, at, g.normal, g.r, c, glow);
                if g.kind == GateKind::Boost {
                    checkpoint(&mut b, at, g.normal, g.r * 2 / 3, c, glow);
                }
            }
        }
    }
    for s in &w.shots {
        let pos = s.prev_pos.lerp(s.pos, alpha);
        let dir = s.vel.norm();
        let col = if s.team == Team::Player {
            pal.tracer
        } else {
            pal.enemy_shot
        };
        match s.kind {
            WeaponKind::Missile => {
                let a = Affine::translate(pos).then(&Affine::looking(dir));
                b.push(
                    Mesh::Cube,
                    &a.then(&Affine::scale(v3(ONE / 2, ONE / 2, int(2)))),
                    colour(pal.missile),
                    0,
                );
                b.push(
                    Mesh::Octa,
                    &a.then(&boxed(v3(0, 0, int(2)), v3(ONE, ONE, int(2)))),
                    colour(pal.flame),
                    ONE,
                );
            }
            WeaponKind::Grenade => {
                b.push(
                    Mesh::Octa,
                    &boxed(pos, v3(int(2), int(2), int(2))),
                    colour(pal.shell),
                    ONE,
                );
            }
            WeaponKind::Plasma => {
                let r = ONE + s.blast / 6;
                b.push(Mesh::Sphere, &boxed(pos, v3(r, r, r)), colour(pal.plasma), ONE);
            }
            _ => {
                let len = s.vel.len().min(int(6));
                let a = Affine::translate(pos.sub(dir.scale(len / 2))).then(&Affine::looking(dir));
                let thick = if s.blast > 0 { ONE / 2 } else { ONE / 4 };
                b.push(
                    Mesh::Cube,
                    &a.then(&Affine::scale(v3(thick, thick, len))),
                    colour(col),
                    ONE,
                );
            }
        }
    }
    let blast = colour(pal.blast);
    let dark = pal.scheme(0).paints()[Paint::Dark as usize];
    for e in &w.effects {
        let k = fx::ratio(e.age, e.life.max(1));
        let spin = Affine::rot_y(e.age * deg(9)).then(&Affine::rot_x(e.age * deg(5)));
        let place = |s: i32| {
            Affine::translate(e.pos)
                .then(&spin)
                .then(&Affine::scale(v3(s, s, s)))
        };
        match e.kind {
            EffectKind::Blast => {
                let grow = ONE / 3 + fx::mul(k, ONE * 2 / 3);
                let fade = if k > ONE * 2 / 3 { (ONE - k) * 3 } else { ONE };
                b.push(
                    Mesh::Octa,
                    &place(fx::mul(fx::mul(e.size, grow), fade.max(ONE / 8))),
                    blast,
                    ONE,
                );
            }
            EffectKind::Flash | EffectKind::Spark => {
                b.push(
                    Mesh::Octa,
                    &place(fx::mul(e.size, ONE - k / 2)),
                    colour(pal.tracer),
                    ONE,
                );
            }
            EffectKind::Debris => b.push(Mesh::Cube, &place(ONE), dark, 0),
            EffectKind::Slash => {
                // an arc of light swept across the front, fading as it goes
                let sweep = fx::mul(deg(120), k) - deg(60);
                for s in 0..7 {
                    let a = e.yaw + sweep - deg(10) * s;
                    let p = e.pos.add(facing(a, 0).scale(e.size / 2));
                    let seg = Affine::translate(p)
                        .then(&Affine::rot_y(a))
                        .then(&Affine::scale(v3(e.size / 3, ONE / 3, ONE / 2)));
                    b.push(Mesh::Cube, &seg, colour(pal.slash), ONE - k / 2);
                }
            }
        }
    }
    b
}

/// A ring of glowing segments standing across the course at `at`.
fn checkpoint(b: &mut Batch, at: V3, dir: V3, r: i32, c: [i32; 3], glow: i32) {
    let basis = Affine::looking(if dir == V3::ZERO { v3(0, 0, -ONE) } else { dir });
    const SEGMENTS: i32 = 16;
    // each segment a little longer than its share of the rim, so they meet
    let seg_len = r * 2 / 5 + ONE;
    for k in 0..SEGMENTS {
        let a = fx::TURN * k / SEGMENTS;
        let p = v3(fx::mul(r, fx::cos(a)), fx::mul(r, fx::sin(a)), 0);
        let seg = Affine::translate(at)
            .then(&basis)
            .then(&Affine::translate(p))
            .then(&Affine::rot_z(a + fx::QUARTER))
            .then(&Affine::scale(v3(seg_len, ONE, ONE)));
        b.push(Mesh::Cube, &seg, c, glow);
    }
}

#[allow(clippy::too_many_arguments)]
fn craft_model(
    b: &mut Batch,
    pal: &Palette,
    kind: CraftKind,
    root: &Affine,
    tick: u32,
    alpha: i32,
    radius: i32,
    sealed: bool,
) {
    let part = |at: V3, size: V3| root.then(&boxed(at, size));
    let drone = colour(pal.drone);
    let eye = colour(pal.drone_eye);
    let dark = pal.scheme(0).paints()[Paint::Dark as usize];
    match kind {
        CraftKind::Drone => {
            b.push(
                Mesh::Octa,
                &root.then(&Affine::scale(v3(int(4), int(3), int(4)))),
                drone,
                0,
            );
            b.push(
                Mesh::Cube,
                &root.then(&Affine::scale(v3(int(6), ONE / 3, ONE))),
                drone,
                0,
            );
            b.push(
                Mesh::Cube,
                &part(v3(0, 0, -int(2)), v3(ONE, ONE / 2, ONE / 2)),
                eye,
                ONE,
            );
        }
        CraftKind::Heli => {
            b.push(Mesh::Cube, &part(V3::ZERO, v3(int(3), int(3), int(6))), drone, 0);
            let nose = root
                .then(&Affine::translate(v3(0, -ONE / 2, -int(4))))
                .then(&Affine::scale(v3(int(3), int(2), int(2))));
            b.push(Mesh::Wedge, &nose, drone, 0);
            b.push(
                Mesh::Cube,
                &part(v3(0, ONE / 2, int(6)), v3(ONE, ONE, int(7))),
                drone,
                0,
            );
            b.push(
                Mesh::Cube,
                &part(v3(0, int(2), int(9)), v3(ONE / 4, int(3), int(2))),
                drone,
                0,
            );
            let skid = v3(ONE / 4, ONE / 4, int(6));
            b.push(Mesh::Cube, &part(v3(-int(3) / 2, -int(2), 0), skid), dark, 0);
            b.push(Mesh::Cube, &part(v3(int(3) / 2, -int(2), 0), skid), dark, 0);
            b.push(
                Mesh::Cube,
                &part(v3(0, -ONE / 4, -int(7) / 2), v3(int(2), ONE / 2, ONE / 4)),
                eye,
                ONE,
            );
            let spin = (tick as i32).wrapping_mul(deg(31)) + fx::lerp(0, deg(31), alpha);
            for k in 0..2 {
                let blade = root
                    .then(&Affine::translate(v3(0, int(2), 0)))
                    .then(&Affine::rot_y(spin + k * fx::QUARTER))
                    .then(&Affine::scale(v3(int(13), ONE / 8, ONE / 2)));
                b.push(Mesh::Cube, &blade, dark, 0);
            }
        }
        CraftKind::Tank => {
            let tank = colour(pal.tank);
            b.push(
                Mesh::Cube,
                &part(v3(0, int(1), 0), v3(int(6), int(2), int(8))),
                tank,
                0,
            );
            b.push(
                Mesh::Cube,
                &part(v3(-int(3), ONE / 2, 0), v3(ONE + ONE / 2, int(1), int(9))),
                dark,
                0,
            );
            b.push(
                Mesh::Cube,
                &part(v3(int(3), ONE / 2, 0), v3(ONE + ONE / 2, int(1), int(9))),
                dark,
                0,
            );
            b.push(
                Mesh::Cube,
                &part(v3(0, int(3), ONE / 2), v3(int(4), int(2), int(4))),
                shade(tank, 85),
                0,
            );
            b.push(
                Mesh::Cube,
                &part(v3(0, int(3), -int(4)), v3(ONE / 2, ONE / 2, int(5))),
                dark,
                0,
            );
            b.push(
                Mesh::Cube,
                &part(v3(0, int(4), int(2)), v3(ONE, ONE / 4, ONE / 2)),
                eye,
                ONE,
            );
        }
        CraftKind::Weak => {
            // a pulsing beacon the size of its hit sphere
            let pulse = ONE * 3 / 4 + fx::mul(fx::sin((tick as i32).wrapping_mul(deg(6))), ONE / 4);
            let r = fx::mul(radius, pulse);
            b.push(
                Mesh::Octa,
                &root.then(&Affine::scale(v3(r, r, r))),
                colour(pal.enemy_shot),
                ONE,
            );
            b.push(
                Mesh::Cube,
                &root.then(&Affine::scale(v3(radius * 2, radius / 8, radius / 8))),
                dark,
                0,
            );
        }
        CraftKind::Core => {
            let (c, glow) = if sealed {
                (shade(colour(pal.plasma), 40), ONE / 5)
            } else {
                (colour(pal.plasma), ONE)
            };
            b.push(
                Mesh::Sphere,
                &root.then(&Affine::scale(v3(radius * 2, radius * 2, radius * 2))),
                c,
                glow,
            );
            if sealed {
                let shell = radius * 9 / 4;
                b.push(
                    Mesh::Octa,
                    &root.then(&Affine::scale(v3(shell, shell, shell))),
                    dark,
                    0,
                );
            }
        }
        CraftKind::Battery => {
            let turret = colour(pal.turret);
            b.push(
                Mesh::Cube,
                &part(v3(0, int(2), 0), v3(int(14), int(6), int(14))),
                dark,
                0,
            );
            b.push(
                Mesh::Cube,
                &part(v3(0, int(6), 0), v3(int(10), int(5), int(10))),
                turret,
                0,
            );
            b.push(
                Mesh::Cube,
                &part(v3(0, int(7), -int(12)), v3(int(2), int(2), int(18))),
                dark,
                0,
            );
            b.push(
                Mesh::Cube,
                &part(v3(0, int(9), -int(2)), v3(int(6), ONE, ONE)),
                eye,
                ONE,
            );
        }
        CraftKind::Turret => {
            let turret = colour(pal.turret);
            b.push(
                Mesh::Cube,
                &part(v3(0, -ONE, 0), v3(int(5), int(2), int(5))),
                dark,
                0,
            );
            b.push(
                Mesh::Cube,
                &part(v3(0, ONE / 2, 0), v3(int(3), int(2), int(3))),
                turret,
                0,
            );
            b.push(
                Mesh::Cube,
                &part(v3(-ONE / 2, ONE / 2, -int(3)), v3(ONE / 3, ONE / 3, int(4))),
                dark,
                0,
            );
            b.push(
                Mesh::Cube,
                &part(v3(ONE / 2, ONE / 2, -int(3)), v3(ONE / 3, ONE / 3, int(4))),
                dark,
                0,
            );
            b.push(
                Mesh::Cube,
                &part(v3(0, int(1), -int(3) / 2), v3(int(2), ONE / 3, ONE / 4)),
                eye,
                ONE,
            );
        }
    }
}

/// A dark patch on the floor under something in the air, smaller the higher it is.
fn push_shadow(b: &mut Batch, w: &World, pos: V3, size: i32, c: [i32; 3]) {
    let floor = w.map.floor_under(v3(pos.x, pos.y + ONE / 2, pos.z));
    let height = (pos.y - floor).max(0);
    let shrink = ONE - fx::div(height, int(120)).min(ONE * 3 / 5);
    let s = fx::mul(size, shrink);
    b.push(
        Mesh::Cube,
        &boxed(v3(pos.x, floor + ONE / 16, pos.z), v3(s, ONE / 16, s)),
        c,
        0,
    );
}

/// Where the view is from, which way it looks, and which way is up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Camera {
    pub eye: V3,
    pub fwd: V3,
    pub up: V3,
}

impl Camera {
    pub fn from_angles(eye: V3, yaw: i32, pitch: i32) -> Camera {
        Camera {
            eye,
            fwd: facing(yaw, pitch),
            up: facing(yaw, pitch + fx::QUARTER),
        }
    }

    /// The camera as drawn on a ring: moved onto it and turned with it.
    pub fn warped(self, w: &World) -> Camera {
        match Warp::of(&w.map) {
            None => self,
            Some(wp) => Camera {
                eye: wp.point(self.eye),
                fwd: wp.dir(self.eye, self.fwd),
                up: wp.dir(self.eye, self.up),
            },
        }
    }
}

/// The chase camera behind the player's frame, between the last two ticks.
pub fn player_camera(w: &World, alpha: i32) -> Camera {
    let m = w.player();
    let pos = m.body.prev_pos.lerp(m.body.pos, alpha);
    let pivot = pos.add(m.rig.chest()).add(v3(0, crate::world::PIVOT_LIFT, 0));
    let eye = crate::world::camera_eye(&w.map, pivot, m.body.aim_yaw, m.body.aim_pitch);
    Camera::from_angles(eye, m.body.aim_yaw, m.body.aim_pitch).warped(w)
}

pub fn view_projection(cam: &Camera, aspect: i32) -> Mat4 {
    view_projection_to(cam, aspect, NEAR, FAR)
}

/// As `view_projection`, with the look's own near and far planes.
pub fn view_projection_for(cam: &Camera, aspect: i32, look: &Look) -> Mat4 {
    view_projection_to(cam, aspect, fx::ratio(look.near_cm, 100), int(look.far_m))
}

fn view_projection_to(cam: &Camera, aspect: i32, near: i32, far: i32) -> Mat4 {
    Mat4::perspective(FOV, aspect, near, far).mul(&Mat4::look(cam.eye, cam.fwd, cam.up))
}

/// Scene-wide shader inputs, named in the order `UNIFORM_LAYOUT` lists them.
pub fn scene_uniforms(look: &Look) -> Vec<i32> {
    let l = look.light_dir;
    let l = v3(l[0] * ONE / 100, l[1] * ONE / 100, l[2] * ONE / 100).norm();
    let mut u = vec![l.x, l.y, l.z];
    u.extend(q16(look.sky));
    u.extend(q16(look.fog));
    u.extend([int(look.fog_near_m), int(look.fog_far_m)]);
    u.extend(q16(look.grid));
    u.push(int(GRID_STEP_M));
    u
}

pub const UNIFORM_LAYOUT: &[(&str, usize)] = &[
    ("uLightDir", 3),
    ("uSky", 3),
    ("uFog", 3),
    ("uFogRange", 2),
    ("uGridColor", 3),
    ("uGridStep", 1),
];

/// A point on screen, in CSS pixels, or `None` if it is behind the camera.
pub fn to_screen(vp: &Mat4, p: V3, css_w: i32, css_h: i32) -> Option<(i32, i32)> {
    let [x, y, _, w] = vp.project(p);
    if w <= ONE / 8 {
        return None;
    }
    let nx = fx::div(x, w);
    let ny = fx::div(y, w);
    let sx = ((nx + ONE) as i64 * css_w as i64 / (2 * ONE as i64)) as i32;
    let sy = ((ONE - ny) as i64 * css_h as i64 / (2 * ONE as i64)) as i32;
    Some((sx, sy))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WeaponHud {
    pub part: String,
    pub ammo: i32,
    pub ready_pct: i32,
    pub empty: bool,
    /// a melee weapon: no rounds to count
    pub infinite: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Mark {
    pub x: i32,
    pub y: i32,
    pub locked: bool,
    pub ap_pct: i32,
    pub dist: i32,
    /// a fortress core that cannot be hurt yet
    pub sealed: bool,
}

/// A marker for something the player should head for: on screen where it
/// is, or pinned to the edge of the screen in its direction.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Pointer {
    pub x: i32,
    pub y: i32,
    pub dist: i32,
    pub edge: bool,
    /// copy key of what it points at
    pub label: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FrameHud {
    pub name: String,
    pub ap_pct: i32,
    pub stagger_pct: i32,
    pub staggered: bool,
    /// what its pilot is doing
    pub intent: Option<String>,
    pub racer: bool,
    /// a fortress: weak points standing, of how many
    pub weak: Option<[usize; 2]>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MissionHud {
    pub name: String,
    /// copy key of the objective
    pub objective: &'static str,
    pub hostiles: usize,
    /// structures standing, all of them, and how many may fall
    pub structures: Option<[i32; 3]>,
    /// checkpoint reached, of how many, and the player's place
    pub race: Option<[usize; 3]>,
    pub time_left: Option<u32>,
    pub seconds: u32,
    /// the plus challenge, once revealed: rule, target, and the run so far
    pub plus: Option<(crate::mission::PlusRule, i32, i32)>,
    pub over: bool,
    pub success: Option<bool>,
    /// a staged mission's current objective, and its step of how many
    pub stage: Option<(String, usize, usize)>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Hud {
    pub ap: i32,
    pub ap_pct: i32,
    pub en_pct: i32,
    pub en_locked: bool,
    pub en_low: bool,
    pub speed: i32,
    pub alt: i32,
    pub glide: bool,
    pub qb_ready: bool,
    /// left hand, right hand, shoulder: the order the screen shows them
    pub weapons: Vec<WeaponHud>,
    pub marks: Vec<Mark>,
    pub kills: u32,
    pub paused: bool,
    pub mode: crate::game::Mode,
    pub stagger_pct: i32,
    pub staggered: bool,
    pub burning: bool,
    pub mission: Option<MissionHud>,
    pub waypoint: Option<Pointer>,
    pub frames: Vec<FrameHud>,
    pub debrief: Option<crate::mission::Debrief>,
}

/// Where a pointer goes for a drawn point: on screen, or on the edge in its
/// direction from the middle of the view.
pub fn pointer(vp: &Mat4, cam: &Camera, p: V3, css_w: i32, css_h: i32, label: &'static str) -> Pointer {
    let rel = p.sub(cam.eye);
    let dist = rel.len() / ONE;
    let margin = 48;
    if let Some((x, y)) = to_screen(vp, p, css_w, css_h) {
        if x >= margin && y >= margin && x <= css_w - margin && y <= css_h - margin {
            return Pointer {
                x,
                y,
                dist,
                edge: false,
                label,
            };
        }
    }
    let side = cam.fwd.cross(cam.up).norm();
    let (dx, dy) = (rel.dot(side), rel.dot(cam.up));
    // straight behind: the bottom edge
    let (dx, dy) = if dx.abs() < ONE / 8 && dy.abs() < ONE / 8 {
        (0, -ONE)
    } else {
        (dx, dy)
    };
    let len = v3(dx, dy, 0).len().max(1);
    let (half_w, half_h) = (css_w / 2 - margin, css_h / 2 - margin);
    let x = css_w / 2 + (dx as i64 * half_w as i64 / len as i64) as i32;
    let y = css_h / 2 - (dy as i64 * half_h as i64 / len as i64) as i32;
    Pointer {
        x,
        y,
        dist,
        edge: true,
        label,
    }
}

/// What the HUD shows this frame.
pub fn hud(
    w: &World,
    vp: &Mat4,
    cam: &Camera,
    css: (i32, i32),
    names: &[String; 3],
    paused: bool,
    plus_revealed: bool,
) -> Hud {
    let (css_w, css_h) = css;
    let m = w.player();
    let t = &m.tuning;
    let speed_ms_x10 = m.body.vel.len() as i64 * 60 * 10 / ONE as i64;
    let weapons = [1usize, 0, 2]
        .iter()
        .map(|&k| {
            let (wpn, st) = (&m.weapons[k], &m.wstate[k]);
            WeaponHud {
                part: names[k].clone(),
                ammo: st.ammo,
                ready_pct: 100 - st.cooldown * 100 / wpn.fire_ticks.max(1),
                empty: st.ammo <= 0,
                infinite: wpn.kind == WeaponKind::Melee,
            }
        })
        .collect();
    let marks = w
        .hostiles()
        .into_iter()
        .filter_map(|tg| {
            let c = w.target_centre(tg)?;
            let (x, y) = to_screen(vp, drawn(w, c), css_w, css_h)?;
            let (ap, max) = match tg {
                Target::Mech(i) => (w.mechs[i].ap, w.mechs[i].stats.ap),
                Target::Craft(i) => (w.craft[i].ap, w.craft[i].max_ap),
                Target::Structure(_) => (0, 1),
            };
                        Some(Mark {
                x,
                y,
                locked: m.lock == Some(tg),
                ap_pct: ap * 100 / max.max(1),
                dist: w.map_delta(m.chest(), c).len() / ONE,
                sealed: matches!(tg, Target::Craft(i) if w.craft[i].kind == CraftKind::Core && w.core_sealed(i)),
            })
        })
        .filter(|mk| mk.x >= 0 && mk.y >= 0 && mk.x <= css_w && mk.y <= css_h)
        .collect();
    // where to go: the next checkpoint, or else the nearest hostile
    let nearest = w
        .hostiles()
        .into_iter()
        .filter_map(|tg| w.target_centre(tg))
        .min_by_key(|c| w.map_delta(m.chest(), *c).len());
    let waypoint = match (w.objective_point(), nearest) {
        (Some(p), _) => {
            let mi = w.mission.as_ref();
            let label = match mi.and_then(|mi| mi.course.get(m.course_next)) {
                _ if mi.is_some_and(|mi| mi.staged()) => "wp_objective",
                Some(g) => g.kind.label(),
                None => "wp_checkpoint",
            };
            Some(pointer(vp, cam, drawn(w, p), css_w, css_h, label))
        }
        (None, Some(p)) if w.mission.is_some() => {
            Some(pointer(vp, cam, drawn(w, p), css_w, css_h, "wp_target"))
        }
        _ => None,
    };
    let place = 1 + w
        .mechs
        .iter()
        .skip(1)
        .filter(|r| r.alive && (r.course_next > m.course_next))
        .count();
    let mission = w.mission.as_ref().map(|mi| MissionHud {
        name: mi.name.clone(),
        objective: mi.kind.objective(),
        hostiles: w.hostiles_alive(),
        structures: (!w.structures.is_empty()).then(|| {
            let total = w.structures.len() as i32;
            [total - w.structures_lost(), total, mi.may_lose]
        }),
        race: (!mi.course.is_empty()).then_some([m.course_next, mi.course.len(), place]),
        time_left: mi.time_left(),
        seconds: mi.seconds(),
        plus: plus_revealed.then(|| (mi.plus.rule, mi.plus.value, mi.plus_progress(w))),
        over: mi.ended_at.is_some(),
        success: mi.success,
        stage: mi
            .stages
            .get(mi.stage)
            .map(|(text, _, _)| (text.clone(), mi.stage + 1, mi.stages.len())),
    });
    let frames = w
        .mechs
        .iter()
        .skip(1)
        .map(|e| FrameHud {
            name: e.name.clone(),
            ap_pct: e.ap * 100 / e.stats.ap.max(1),
            stagger_pct: e.stagger_pct(),
            staggered: e.stagger > 0,
            intent: e.intent.clone().filter(|_| e.alive),
            racer: e.team == Team::Player,
            weak: None,
        })
        .chain(w.fortresses.iter().map(|f| {
            let core = &w.craft[f.core];
            FrameHud {
                name: f.name.clone(),
                ap_pct: core.ap * 100 / core.max_ap.max(1),
                stagger_pct: 0,
                staggered: false,
                intent: None,
                racer: false,
                weak: Some([f.weak.iter().filter(|i| w.craft[**i].alive).count(), f.weak.len()]),
            }
        }))
        .collect();
    Hud {
        ap: m.ap,
        ap_pct: m.ap * 100 / m.stats.ap.max(1),
        en_pct: m.body.en_pct(t),
        en_locked: m.body.en_locked,
        en_low: m.body.en_pct(t) < 25,
        // m/s × 3.6 = km/h
        speed: (speed_ms_x10 * 36 / 100) as i32,
        alt: m.body.pos.y / ONE,
        glide: m.body.glide,
        qb_ready: m.body.qb_cooldown == 0 && !m.body.en_locked,
        weapons,
        marks,
        kills: w.kills,
        paused,
        mode: crate::game::Mode::Garage,
        stagger_pct: m.stagger_pct(),
        staggered: m.stagger > 0,
        burning: w.map.climate.floor_dps > 0 && m.body.grounded && m.body.pos.y == 0,
        mission,
        waypoint,
        frames,
        debrief: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::tests::palette;
    use crate::world::tests::{mission_world, world};

    fn look() -> Look {
        palette().hangar
    }

    #[test]
    fn instances_come_grouped_by_mesh_with_byte_offsets_that_follow_each_other() {
        let w = mission_world("halden-3");
        let (values, draws) = frame(&w, &look(), &palette(), 0, false).finish(DYNAMIC_BUFFER);
        assert_eq!(values.len() % INSTANCE_VALUES, 0);
        let mut expect_offset = 0;
        for d in draws.chunks(DRAW_VALUES) {
            assert_eq!(d[0], DYNAMIC_BUFFER);
            assert_eq!(d[2] as usize, expect_offset);
            expect_offset += d[3] as usize * INSTANCE_VALUES * BYTES_PER_VALUE;
        }
        assert_eq!(expect_offset, values.len() * BYTES_PER_VALUE);
    }

    #[test]
    fn the_scene_has_the_ground_flagged_for_the_grid_and_every_building() {
        let w = world();
        let (values, draws) = scene(&w, &look());
        let ground: Vec<&[u32]> = draws
            .chunks(DRAW_VALUES)
            .filter(|d| d[1] == Mesh::Ground as u32)
            .collect();
        assert_eq!(ground.len(), 1);
        assert_eq!(ground[0][4], 1);
        assert!(values.len() / INSTANCE_VALUES > w.map.blocks.len() * 2);
    }

    #[test]
    fn the_point_the_camera_looks_at_is_the_centre_of_the_screen() {
        let w = world();
        let cam = player_camera(&w, ONE);
        let vp = view_projection(&cam, fx::ratio(16, 9));
        let (x, y) = to_screen(&vp, cam.eye.add(cam.fwd.scale(int(100))), 1600, 900).unwrap();
        assert!((x - 800).abs() <= 1 && (y - 450).abs() <= 1, "{x},{y}");
        assert!(to_screen(&vp, cam.eye.sub(cam.fwd.scale(int(100))), 1600, 900).is_none());
    }

    #[test]
    fn a_point_behind_the_camera_is_pinned_to_the_bottom_edge_and_one_to_the_left_on_the_left() {
        let w = world();
        let cam = player_camera(&w, ONE);
        let vp = view_projection(&cam, fx::ratio(16, 9));
        let p = pointer(&vp, &cam, cam.eye.sub(cam.fwd.scale(int(300))), 1600, 900, "x");
        assert!(p.edge && (p.x - 800).abs() < 4 && p.y > 800, "{p:?}");
        let left = cam.eye.sub(cam.fwd.cross(cam.up).scale(int(300)));
        let p = pointer(&vp, &cam, left, 1600, 900, "x");
        assert!(p.edge && p.x < 100, "{p:?}");
    }

    #[test]
    fn on_a_ring_the_floor_across_the_ring_is_overhead() {
        let w = mission_world("spindle-1");
        let wp = Warp::of(&w.map).unwrap();
        // the floor directly beneath and the floor half way round
        let here = wp.point(V3::ZERO);
        let opposite = wp.point(v3(w.map.half_x, 0, 0));
        assert_eq!(here, V3::ZERO);
        assert!(
            (opposite.y - 2 * wp.radius).abs() < ONE,
            "{:?} vs radius {}",
            opposite,
            wp.radius
        );
        // a quarter of the way round, the floor is a wall at the side
        let quarter = wp.point(v3(w.map.half_x / 2, 0, 0));
        assert!(
            (quarter.x - wp.radius).abs() < ONE && (quarter.y - wp.radius).abs() < ONE,
            "{quarter:?}"
        );
        // up, at a quarter turn, points back toward the axis
        let up = wp.dir(v3(w.map.half_x / 2, 0, 0), V3::UP);
        assert!(up.x < -ONE + ONE / 64, "{up:?}");
    }

    #[test]
    fn the_hud_reads_full_ap_and_en_at_the_start_and_names_the_objective() {
        let w = mission_world("halden-1");
        let cam = player_camera(&w, ONE);
        let vp = view_projection(&cam, ONE);
        let names = ["a".to_string(), "b".to_string(), "c".to_string()];
        let h = hud(&w, &vp, &cam, (800, 600), &names, false, false);
        assert_eq!(h.ap_pct, 100);
        assert_eq!(h.en_pct, 100);
        assert_eq!(h.weapons[0].part, "b", "the left hand is shown first");
        let mi = h.mission.unwrap();
        assert_eq!(mi.objective, "obj_destroy");
        assert_eq!(mi.hostiles, 2);
        assert!(
            mi.plus.is_none(),
            "the plus challenge is hidden before the first clear"
        );
        assert_eq!(h.waypoint.unwrap().label, "wp_target");
    }

    #[test]
    fn the_uniforms_match_their_layout() {
        let n: usize = UNIFORM_LAYOUT.iter().map(|(_, k)| k).sum();
        assert_eq!(scene_uniforms(&look()).len(), n);
    }
}
