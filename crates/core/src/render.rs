//! What the page draws: one 3×4 matrix, a colour and a glow per box, grouped
//! by mesh so each mesh is one instanced draw. Nothing here changes the world.

use crate::combat::{CraftKind, EffectKind, Target};
use crate::content::{q16, Palette};
use crate::fx::{self, int, ONE};
use crate::geom::{v3, Affine, Mat4, V3};
use crate::mesh::Mesh;
use crate::model::Paint;
use crate::parts::WeaponKind;
use crate::world::World;
use serde::Serialize;

/// Values per instance: three columns with a translation component each, then colour and glow.
pub const INSTANCE_VALUES: usize = 16;
/// Values per draw in `draws`: buffer, mesh, byte offset, instance count, grid.
pub const DRAW_VALUES: usize = 5;
pub const BYTES_PER_VALUE: usize = 4;
pub const STATIC_BUFFER: u32 = 0;
pub const DYNAMIC_BUFFER: u32 = 1;

pub const FOV: i32 = fx::deg(72);
pub const NEAR: i32 = ONE / 4;
pub const FAR: i32 = int(2400);
const FOG_NEAR_M: i32 = 300;
const FOG_FAR_M: i32 = 1400;
const GRID_STEP_M: i32 = 10;
const WINDOW_GLOW: i32 = ONE * 2 / 5;
const WINDOW_EVERY_M: i32 = 11;
const MAX_WINDOW_BANDS: i32 = 7;

#[derive(Default)]
pub struct Batch {
    meshes: [Vec<i32>; 4],
}

impl Batch {
    pub fn push(&mut self, mesh: Mesh, a: &Affine, rgb: [i32; 3], glow: i32) {
        self.meshes[mesh as usize].extend_from_slice(&[
            a.c0.x, a.c0.y, a.c0.z, a.t.x, a.c1.x, a.c1.y, a.c1.z, a.t.y, a.c2.x, a.c2.y, a.c2.z,
            a.t.z, rgb[0], rgb[1], rgb[2], glow,
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

fn boxed(at: V3, size: V3) -> Affine {
    Affine::translate(at).then(&Affine::scale(size))
}

fn shade(c: [i32; 3], pct: i32) -> [i32; 3] {
    c.map(|v| (v * pct / 100).min(ONE))
}

/// The ground and the buildings. They never move, so the page uploads them once.
pub fn scene(w: &World, pal: &Palette) -> (Vec<i32>, Vec<u32>) {
    let mut b = Batch::default();
    let m = &w.map;
    b.push(
        Mesh::Ground,
        &boxed(V3::ZERO, v3(m.half_x * 8, ONE, m.half_z * 8)),
        q16(pal.ground),
        0,
    );
    let window = q16(pal.window);
    let roof = q16(pal.roof);
    for blk in &m.blocks {
        let size = blk.max.sub(blk.min);
        let centre = v3(
            (blk.min.x + blk.max.x) / 2,
            (blk.min.y + blk.max.y) / 2,
            (blk.min.z + blk.max.z) / 2,
        );
        let base = q16(pal.buildings[blk.shade as usize % pal.buildings.len()]);
        b.push(
            Mesh::Cube,
            &boxed(centre, size),
            shade(base, 88 + (blk.shade as i32 % 24)),
            0,
        );
        let cap = v3(size.x - int(2), ONE, size.z - int(2));
        b.push(
            Mesh::Cube,
            &boxed(v3(centre.x, blk.max.y + ONE / 2, centre.z), cap),
            roof,
            0,
        );
        let bands = (size.y / int(WINDOW_EVERY_M) - 1).clamp(0, MAX_WINDOW_BANDS);
        for k in 0..bands {
            let y = int(WINDOW_EVERY_M) * (k + 1) - int(3);
            let band = v3(size.x + ONE / 5, ONE + ONE / 4, size.z + ONE / 5);
            b.push(
                Mesh::Cube,
                &boxed(v3(centre.x, y, centre.z), band),
                window,
                WINDOW_GLOW,
            );
        }
    }
    b.finish(crate::render::STATIC_BUFFER)
}

/// Everything that moves, placed between the last two ticks by `alpha`.
pub fn frame(w: &World, pal: &Palette, alpha: i32) -> (Vec<i32>, Vec<u32>) {
    let mut b = Batch::default();
    let shadow = q16(pal.shadow);
    let mut parts = Vec::new();
    for m in w.mechs.iter().filter(|m| m.alive) {
        let paints = pal.paint_set(m.team == crate::combat::Team::Enemy);
        let pos = m.body.prev_pos.lerp(m.body.pos, alpha);
        let yaw = fx::lerp_angle(m.body.prev_yaw, m.body.yaw, alpha);
        let root = m.root(pos, yaw);
        let pose = m.body.pose();
        parts.clear();
        m.rig.pose(&root, &pose, &mut parts);
        for (a, mesh, paint) in &parts {
            let glow = if *paint == Paint::Glow { ONE } else { 0 };
            let mut c = paints[*paint as usize];
            if m.hit_flash > 0 {
                c = c.map(|v| (v + ONE) / 2);
            }
            b.push(*mesh, a, c, glow);
        }
        if m.body.thrust > 0 {
            let (len, colour) = if m.body.thrust == 2 {
                (int(4), pal.flame_qb)
            } else {
                (int(2), pal.flame)
            };
            for f in m.rig.flames(&pose) {
                let at = root.then(&Affine::translate(f.add(v3(0, 0, len / 2))));
                let a = at.then(&Affine::scale(v3(ONE / 2, ONE / 2, len)));
                b.push(Mesh::Octa, &a, q16(colour), ONE);
            }
        }
        push_shadow(&mut b, w, pos, int(4), shadow);
    }
    let drone = q16(pal.drone);
    let eye = q16(pal.drone_eye);
    for c in w.craft.iter().filter(|c| c.alive) {
        let pos = c.prev_pos.lerp(c.pos, alpha);
        let yaw = fx::lerp_angle(c.prev_yaw, c.yaw, alpha);
        let root = Affine::translate(pos).then(&Affine::rot_y(yaw));
        match c.kind {
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
                    &root.then(&boxed(v3(0, 0, -int(2)), v3(ONE, ONE / 2, ONE / 2))),
                    eye,
                    ONE,
                );
            }
            CraftKind::Heli => {
                b.push(
                    Mesh::Cube,
                    &root.then(&Affine::scale(v3(int(4), int(3), int(8)))),
                    drone,
                    0,
                );
            }
        }
        push_shadow(&mut b, w, pos, int(4), shadow);
    }
    for s in &w.shots {
        let pos = s.prev_pos.lerp(s.pos, alpha);
        let dir = s.vel.norm();
        let colour = if s.team == crate::combat::Team::Player {
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
                    q16(pal.missile),
                    0,
                );
                b.push(
                    Mesh::Octa,
                    &a.then(&boxed(v3(0, 0, int(2)), v3(ONE, ONE, int(2)))),
                    q16(pal.flame),
                    ONE,
                );
            }
            WeaponKind::Grenade => {
                b.push(
                    Mesh::Octa,
                    &boxed(pos, v3(int(2), int(2), int(2))),
                    q16(pal.shell),
                    ONE,
                );
            }
            _ => {
                let len = s.vel.len().min(int(6));
                let a = Affine::translate(pos.sub(dir.scale(len / 2))).then(&Affine::looking(dir));
                b.push(
                    Mesh::Cube,
                    &a.then(&Affine::scale(v3(ONE / 4, ONE / 4, len))),
                    q16(colour),
                    ONE,
                );
            }
        }
    }
    let blast = q16(pal.blast);
    let dark = pal.paint_set(false)[Paint::Dark as usize];
    for e in &w.effects {
        let k = fx::ratio(e.age, e.life.max(1));
        let spin = Affine::rot_y(e.age * fx::deg(9)).then(&Affine::rot_x(e.age * fx::deg(5)));
        match e.kind {
            EffectKind::Blast => {
                let grow = ONE / 3 + fx::mul(k, ONE * 2 / 3);
                let fade = if k > ONE * 2 / 3 { (ONE - k) * 3 } else { ONE };
                let s = fx::mul(fx::mul(e.size, grow), fade.max(ONE / 8));
                b.push(
                    Mesh::Octa,
                    &Affine::translate(e.pos)
                        .then(&spin)
                        .then(&Affine::scale(v3(s, s, s))),
                    blast,
                    ONE,
                );
            }
            EffectKind::Flash | EffectKind::Spark => {
                let s = fx::mul(e.size, ONE - k / 2);
                b.push(
                    Mesh::Octa,
                    &Affine::translate(e.pos)
                        .then(&spin)
                        .then(&Affine::scale(v3(s, s, s))),
                    q16(pal.tracer),
                    ONE,
                );
            }
            EffectKind::Debris => {
                b.push(
                    Mesh::Cube,
                    &Affine::translate(e.pos)
                        .then(&spin)
                        .then(&Affine::scale(v3(ONE, ONE, ONE))),
                    dark,
                    0,
                );
            }
        }
    }
    b.finish(DYNAMIC_BUFFER)
}

/// A dark patch on the floor under something in the air, smaller the higher it is.
fn push_shadow(b: &mut Batch, w: &World, pos: V3, size: i32, colour: [i32; 3]) {
    let floor = w.map.floor_under(v3(pos.x, pos.y + ONE / 2, pos.z));
    let height = (pos.y - floor).max(0);
    let shrink = ONE - fx::div(height, int(120)).min(ONE * 3 / 5);
    let s = fx::mul(size, shrink);
    b.push(
        Mesh::Cube,
        &boxed(v3(pos.x, floor + ONE / 16, pos.z), v3(s, ONE / 16, s)),
        colour,
        0,
    );
}

/// The view-projection matrix for the player's camera this frame.
pub fn view_projection(w: &World, alpha: i32, aspect: i32) -> (Mat4, V3) {
    let m = w.player();
    let pos = m.body.prev_pos.lerp(m.body.pos, alpha);
    let pivot = pos
        .add(m.rig.chest())
        .add(v3(0, crate::world::PIVOT_LIFT, 0));
    let eye = crate::world::camera_eye(&w.map, pivot, m.body.aim_yaw, m.body.aim_pitch);
    let vp = Mat4::perspective(FOV, aspect, NEAR, FAR).mul(&Mat4::view(
        eye,
        m.body.aim_yaw,
        m.body.aim_pitch,
    ));
    (vp, eye)
}

/// Scene-wide shader inputs, named in the order `uniform_layout` lists them.
pub fn scene_uniforms(pal: &Palette) -> Vec<i32> {
    let l = v3(
        pal.light_dir[0] * ONE / 100,
        pal.light_dir[1] * ONE / 100,
        pal.light_dir[2] * ONE / 100,
    )
    .norm();
    let mut u = vec![l.x, l.y, l.z];
    u.extend(q16(pal.sky));
    u.extend(q16(pal.fog));
    u.extend([int(FOG_NEAR_M), int(FOG_FAR_M)]);
    u.extend(q16(pal.grid));
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
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Mark {
    pub x: i32,
    pub y: i32,
    pub locked: bool,
    pub ap_pct: i32,
    pub dist: i32,
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
}

/// What the HUD shows this frame.
pub fn hud(w: &World, vp: &Mat4, css_w: i32, css_h: i32, names: &[String; 3], paused: bool) -> Hud {
    let m = w.player();
    let t = &m.tuning;
    let speed_ms_x10 = m.body.vel.len() as i64 * 60 * 10 / ONE as i64;
    let weapons = [1usize, 0, 2]
        .iter()
        .map(|&k| {
            let wpn = &m.weapons[k];
            let st = &m.wstate[k];
            WeaponHud {
                part: names[k].clone(),
                ammo: st.ammo,
                ready_pct: 100 - st.cooldown * 100 / wpn.fire_ticks.max(1),
                empty: st.ammo <= 0,
            }
        })
        .collect();
    let marks = w
        .hostiles()
        .into_iter()
        .filter_map(|tg| {
            let c = w.target_centre(tg)?;
            let (x, y) = to_screen(vp, c, css_w, css_h)?;
            let (ap, max) = match tg {
                Target::Mech(i) => (w.mechs[i].ap, w.mechs[i].stats.ap),
                Target::Craft(i) => (w.craft[i].ap, w.craft[i].max_ap),
            };
            Some(Mark {
                x,
                y,
                locked: m.lock == Some(tg),
                ap_pct: ap * 100 / max.max(1),
                dist: c.dist(m.chest()) / ONE,
            })
        })
        .filter(|mk| mk.x >= 0 && mk.y >= 0 && mk.x <= css_w && mk.y <= css_h)
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::tests::palette;
    use crate::world::tests::world;

    #[test]
    fn instances_come_grouped_by_mesh_with_byte_offsets_that_follow_each_other() {
        let w = world();
        let (values, draws) = frame(&w, &palette(), 0);
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
        let (values, draws) = scene(&w, &palette());
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
        let (vp, eye) = view_projection(&w, ONE, fx::ratio(16, 9));
        let m = w.player();
        let ahead = eye.add(crate::geom::facing(m.body.aim_yaw, m.body.aim_pitch).scale(int(100)));
        let (x, y) = to_screen(&vp, ahead, 1600, 900).unwrap();
        assert!((x - 800).abs() <= 1 && (y - 450).abs() <= 1, "{x},{y}");
        let behind = eye.sub(crate::geom::facing(m.body.aim_yaw, 0).scale(int(100)));
        assert!(to_screen(&vp, behind, 1600, 900).is_none());
    }

    #[test]
    fn the_hud_reads_full_ap_and_en_at_the_start() {
        let w = world();
        let (vp, _) = view_projection(&w, ONE, ONE);
        let names = ["a".to_string(), "b".to_string(), "c".to_string()];
        let h = hud(&w, &vp, 800, 600, &names, false);
        assert_eq!(h.ap_pct, 100);
        assert_eq!(h.en_pct, 100);
        assert_eq!(h.speed, 0);
        assert_eq!(h.weapons[0].part, "b", "the left hand is shown first");
    }

    #[test]
    fn the_uniforms_match_their_layout() {
        let n: usize = UNIFORM_LAYOUT.iter().map(|(_, k)| k).sum();
        assert_eq!(scene_uniforms(&palette()).len(), n);
    }
}
