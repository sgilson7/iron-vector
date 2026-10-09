//! The world: mechs, craft, structures, shots and effects, advanced one tick
//! at a time. `World::tick` is the only thing that changes it.

use crate::campaign::Planet;
use crate::combat::{
    blast_damage, segment_sphere, Craft, CraftKind, Effect, EffectKind, Host, Shot, Target, Team, Weapon,
    WeaponState,
};
use crate::content::Proving;
use crate::course::{self, Machine};
use crate::fx::{self, deg, int, ONE};
use crate::geom::{facing, v3, Affine, V3};
use crate::map::{Block, Map};
use crate::mech::{tuning_for, Body, Controls, Tuning, TICKS_PER_SECOND};
use crate::mission::{make_unit, FortressSpec, Gun, Mission, MissionSpec, Units};
use crate::model::Rig;
use crate::parts::{stats, Catalog, Loadout, Slot, Stats, WeaponKind};
use crate::pilot::{Pilot, Pilots, Senses};
use crate::rng::Rng;

/// The camera sits this far behind the pivot, and this far above the line of aim.
pub const CAMERA_BACK: i32 = int(15);
pub const CAMERA_LIFT: i32 = int(2);
/// The camera pivots this far above the chest, so the mech sits low in the
/// frame and the crosshair has a clear view over its shoulders.
pub const PIVOT_LIFT: i32 = int(3);
/// Shortest the camera is allowed to be pulled in by a wall.
const CAMERA_MIN_BACK: i32 = int(3);
/// How far the crosshair's ray looks for something to aim at.
const AIM_REACH: i32 = int(900);
/// The lock picks targets inside this cone and keeps them inside the wider one.
pub const LOCK_CONE: i32 = deg(15);
const LOCK_HOLD_CONE: i32 = deg(22);
/// A staggered frame cannot act for this long, and takes extra damage.
pub const STAGGER_TICKS: i32 = 90;
pub const STAGGER_DAMAGE_PCT: i32 = 150;
/// Impact drains away once a frame has gone this long without a hit.
const IMPACT_QUIET_TICKS: i32 = 60;
/// What is left of a fallen structure.
const RUBBLE: i32 = int(3);

/// The five paints of a mech, in `Paint` order, as Q16 colours.
pub type Paints = [[i32; 3]; 5];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mech {
    pub name: String,
    pub body: Body,
    pub tuning: Tuning,
    pub rig: Rig,
    pub stats: Stats,
    pub weapons: [Weapon; 3],
    pub wstate: [WeaponState; 3],
    pub ap: i32,
    pub team: Team,
    pub alive: bool,
    pub lock: Option<Target>,
    pub aim_point: V3,
    pub lock_range: i32,
    /// ticks of the white flash after a hit
    pub hit_flash: i32,
    pub paint: Paints,
    pub pilot: Option<Pilot>,
    /// what the pilot is doing, for the HUD
    pub intent: Option<String>,
    /// impact taken toward a stagger, and ticks since the last hit
    pub impact: i32,
    pub impact_quiet: i32,
    /// ticks of stagger left
    pub stagger: i32,
    /// a race's next checkpoint, and the tick the course was finished
    pub course_next: usize,
    pub finished_at: Option<u32>,
    /// burning-ground damage owed, in sixtieths of a point
    burn: i32,
    /// size against a standard frame, Q16; giants are drawn, hit and collide larger
    pub scale: i32,
    /// a melee strike under way: weapon slot, tick, and what it lunges at
    pub melee: Option<(usize, i32, Option<Target>)>,
}

/// A melee strike lunges this long, lands on this tick, and is done by the last.
pub const LUNGE_TICKS: i32 = 12;
const MELEE_DONE_TICKS: i32 = 26;

/// Weapon slots in the order the arrays use: right hand, left hand, shoulder.
pub const WEAPON_SLOTS: [Slot; 3] = [Slot::RightWeapon, Slot::LeftWeapon, Slot::ShoulderWeapon];

impl Mech {
    pub fn build(l: &Loadout, cat: &Catalog, pos: V3, yaw: i32, team: Team, paint: Paints) -> Mech {
        let rig = Rig::build(l, cat);
        let st = stats(l, cat);
        let tuning = tuning_for(&st, &rig);
        let weapons = WEAPON_SLOTS.map(|s| Weapon::from_part(l.part(s, cat)));
        Mech {
            name: String::new(),
            body: Body::new(pos, yaw, &tuning),
            tuning,
            rig,
            stats: st,
            weapons,
            wstate: weapons.map(|w| WeaponState {
                cooldown: 0,
                ammo: w.ammo,
            }),
            ap: st.ap,
            team,
            alive: true,
            lock: None,
            aim_point: pos,
            lock_range: int(st.lock_range_m),
            hit_flash: 0,
            paint,
            pilot: None,
            intent: None,
            impact: 0,
            impact_quiet: 0,
            stagger: 0,
            course_next: 0,
            finished_at: None,
            burn: 0,
            scale: ONE,
            melee: None,
        }
    }

    /// Makes this frame a giant: drawn, hit and colliding `scale` times
    /// larger, moving at `speed_pct` of its parts' speed, and hitting at
    /// `damage_pct` of its weapons' damage.
    pub fn grow(&mut self, scale_pct: i32, speed_pct: i32, damage_pct: i32) {
        self.scale = fx::ratio(scale_pct, 100);
        let t = &mut self.tuning;
        t.height = fx::mul(t.height, self.scale);
        t.half_width = fx::mul(t.half_width, self.scale);
        for v in [&mut t.walk, &mut t.glide, &mut t.quick_boost] {
            *v = *v * speed_pct / 100;
        }
        for w in &mut self.weapons {
            w.damage = w.damage * damage_pct / 100;
            w.impact = w.impact * damage_pct / 100;
        }
        self.lock_range = fx::mul(self.lock_range, self.scale);
    }

    /// The frame's feet, heading and size, as a transform.
    pub fn frame(&self, pos: V3, yaw: i32) -> Affine {
        self.root(pos, yaw)
            .then(&Affine::scale(v3(self.scale, self.scale, self.scale)))
    }

    /// Impact meter as a percent of stability.
    pub fn stagger_pct(&self) -> i32 {
        if self.stagger > 0 {
            return 100;
        }
        self.impact * 100 / self.stats.stability.max(1)
    }

    /// The feet and heading, as a transform.
    pub fn root(&self, pos: V3, yaw: i32) -> Affine {
        Affine::translate(pos).then(&Affine::rot_y(yaw))
    }

    /// The box a frame collides with.
    pub fn bounds(&self) -> (V3, V3) {
        let (p, t) = (self.body.pos, &self.tuning);
        (
            v3(p.x - t.half_width, p.y, p.z - t.half_width),
            v3(p.x + t.half_width, p.y + t.height, p.z + t.half_width),
        )
    }

    pub fn chest(&self) -> V3 {
        self.body.pos.add(self.rig.chest().scale(self.scale))
    }

    /// The two spheres a shot can hit: chest and hips.
    pub fn spheres(&self) -> [(V3, i32); 2] {
        let c = self.chest();
        let k = |n: i32| fx::mul(n, self.scale);
        [
            (c, k(int(3))),
            (v3(c.x, c.y - k(int(3)), c.z), k(int(2) + ONE / 2)),
        ]
    }
}

/// A building a mission asks the player to protect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Structure {
    /// its index among the map's buildings
    pub block: usize,
    pub ap: i32,
    pub max_ap: i32,
    pub alive: bool,
}

/// An arms fort: a hull that walks a path and carries whatever stands on
/// it, with weak points, a core and guns riding on it as craft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fortress {
    pub name: String,
    pub pos: V3,
    pub prev_pos: V3,
    pub path: Vec<V3>,
    pub next: usize,
    pub speed: i32,
    pub size_x: i32,
    /// its boxes in the map's movers: from this index, this many
    pub movers: (usize, usize),
    /// each box from the footprint, min and max, and its paint
    pub hull: Vec<(V3, V3, u8)>,
    pub weak: Vec<usize>,
    pub core: usize,
    pub alive: bool,
}

/// Paint names a hull box may use, in `Paint` order.
fn hull_paint(name: &str) -> u8 {
    match name {
        "secondary" => 1,
        "dark" => 2,
        "glow" => 3,
        _ => 0,
    }
}

/// Where the camera stands for a pivot and an aim, pulled in front of any
/// building between it and the pivot.
pub fn camera_eye(map: &Map, pivot: V3, yaw: i32, pitch: i32) -> V3 {
    let back = facing(yaw, pitch).scale(CAMERA_BACK);
    let want = pivot.sub(back).add(v3(0, CAMERA_LIFT, 0));
    let mut eye = match map.segment_hit(pivot, want) {
        Some(t) => {
            let keep = fx::mul(CAMERA_BACK, t) - ONE;
            pivot.add(want.sub(pivot).norm().scale(keep.max(CAMERA_MIN_BACK)))
        }
        None => want,
    };
    eye.y = eye.y.max(ONE / 2);
    eye
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct World {
    pub map: Map,
    pub mechs: Vec<Mech>,
    pub craft: Vec<Craft>,
    pub structures: Vec<Structure>,
    pub shots: Vec<Shot>,
    pub effects: Vec<Effect>,
    pub rng: Rng,
    pub tick: u32,
    pub kills: u32,
    pub mission: Option<Mission>,
    pub fortresses: Vec<Fortress>,
    /// a race's moving parts
    pub machines: Vec<Machine>,
}

fn empty_world(map: Map, player: Mech, seed: u64) -> World {
    World {
        map,
        mechs: vec![player],
        craft: Vec::new(),
        structures: Vec::new(),
        shots: Vec::new(),
        effects: Vec::new(),
        rng: Rng::new(seed),
        tick: 0,
        kills: 0,
        mission: None,
        fortresses: Vec::new(),
        machines: Vec::new(),
    }
}

/// A box centred on `at` (m), its underside `base` above the ground.
fn standing(at: [i32; 2], size: [i32; 3], base: i32, shade: u8) -> Block {
    let [x, z] = at;
    let [w, d, h] = size;
    Block {
        min: v3(int(x) - int(w) / 2, int(base), int(z) - int(d) / 2),
        max: v3(int(x) + int(w) / 2, int(base + h), int(z) + int(d) / 2),
        shade,
    }
}

impl World {
    /// The test field: the first planet's city and practice drones that come back.
    pub fn proving(cat: &Catalog, l: &Loadout, p: &Proving, paint: Paints, units: &Units) -> World {
        let map = Map::generate(&p.map);
        let spawn = v3(int(p.spawn[0]), 0, int(p.spawn[1]));
        let player = Mech::build(l, cat, spawn, deg(p.spawn_yaw_deg), Team::Player, paint);
        let mut w = empty_world(map, player, p.drone_seed);
        let respawn = p.drone_respawn_ms * TICKS_PER_SECOND / 1000;
        for i in 0..p.drones {
            let a = fx::TURN * i / p.drones.max(1) + w.rng.range(0, deg(20));
            let r = int(w.rng.range(140, 460));
            let mut at = spawn.add(facing(a, 0).scale(r));
            at.y = int(w.rng.range(18, 70));
            // a drone never hovers inside a building
            at.y =
                at.y.max(w.map.floor_under(v3(at.x, w.map.ceiling, at.z)) + int(12));
            let mut d = make_unit("drone", &units.0["drone"], at, 0, None, 100, i);
            d.respawn = respawn;
            w.craft.push(d);
        }
        w
    }

    /// A mission on its planet: the planet's map with the mission's pads and
    /// structures set down, the player at the start, the first wave out and
    /// the rest held in reserve.
    #[allow(clippy::too_many_arguments)]
    pub fn mission(
        cat: &Catalog,
        l: &Loadout,
        paint: Paints,
        planet: &Planet,
        spec: &MissionSpec,
        pilots: &Pilots,
        units: &Units,
        enemy_paint: Paints,
        giant_paint: Paints,
    ) -> World {
        let mut map = Map::generate(&planet.map);
        map.climate = planet.climate;
        for [x0, z0, x1, z1] in &spec.clear {
            map.clear_rect(v3(int(*x0), 0, int(*z0)), v3(int(*x1), 0, int(*z1)));
        }
        for pad in &spec.pads {
            map.add_block(standing(pad.at, pad.size, pad.base, 2));
        }
        let mut structures = Vec::new();
        for s in &spec.protect {
            structures.push(Structure {
                block: map.blocks.len(),
                ap: s.ap,
                max_ap: s.ap,
                alive: true,
            });
            map.add_block(standing(s.at, s.size, s.base, 255));
        }
        let on_top = |m: &Map, x: i32, z: i32| m.floor_under(v3(int(x), m.ceiling, int(z)));
        let [sx, sz, syaw] = spec.start;
        let start = v3(int(sx), on_top(&map, sx, sz), int(sz));
        let player = Mech::build(l, cat, start, deg(syaw), Team::Player, paint);
        let mut w = empty_world(map, player, planet.map.seed ^ 0x5eed);
        w.structures = structures;
        for ms in &spec.machines {
            let mc = Machine::new(ms, w.map.movers.len());
            w.map.movers.push(Block {
                min: mc.home.sub(mc.half),
                max: mc.home.add(mc.half),
                shade: 2,
            });
            w.machines.push(mc);
        }
        let mut mission = Mission::new(spec);
        for (k, u) in spec.units.iter().enumerate() {
            let [x, z, alt] = u.at;
            // below zero: on top of whatever is there; otherwise on whatever
            // floor is under that height, as inside a tunnel
            let y = if alt < 0 {
                on_top(&w.map, x, z) + int(2)
            } else {
                int(alt).max(w.map.floor_under(v3(int(x), int(alt) + ONE, int(z))))
            };
            let goal = u.goal.map(|[gx, gz]| v3(int(gx), 0, int(gz)));
            let Some(us) = units.0.get(&u.unit) else { continue };
            let c = make_unit(
                &u.unit,
                us,
                v3(int(x), y, int(z)),
                u.wave,
                goal,
                spec.power_pct,
                k as i32,
            );
            if u.wave == 0 {
                w.craft.push(c);
            } else {
                mission.reserve_craft.push(c);
            }
        }
        for (k, ms) in spec.mechs.iter().enumerate() {
            let loadout = match &ms.loadout {
                Some(lo) => Loadout::restore(&serde_json::to_string(lo).unwrap_or_default(), cat),
                None => l.clone(),
            };
            let [x, z, yaw] = ms.at;
            let pos = v3(int(x), on_top(&w.map, x, z), int(z));
            let team = if ms.racer { Team::Player } else { Team::Enemy };
            let paint = if ms.scale_pct > 100 {
                giant_paint
            } else {
                enemy_paint
            };
            let mut m = Mech::build(&loadout, cat, pos, deg(yaw), team, paint);
            m.name = ms.name.clone();
            m.stats.ap = m.stats.ap * ms.ap_pct / 100;
            m.stats.stability = m.stats.stability * ms.ap_pct / 100;
            m.ap = m.stats.ap;
            m.pilot = pilots.pilot(&ms.pilot, k as u64 + 1);
            if ms.scale_pct != 100 || ms.speed_pct != 100 || ms.damage_pct != 100 {
                m.grow(ms.scale_pct, ms.speed_pct, ms.damage_pct);
            }
            let riders: Vec<Craft> = ms
                .turrets
                .iter()
                .enumerate()
                .map(|(j, [x, y, z])| {
                    let mut t = make_unit(
                        "turret",
                        &units.0["turret"],
                        pos,
                        ms.wave,
                        None,
                        spec.power_pct,
                        (k * 7 + j) as i32,
                    );
                    let cm = |n: i32| fx::ratio(n, 100);
                    t.mount = Some((Host::Mech(usize::MAX), v3(cm(*x), cm(*y), cm(*z))));
                    t
                })
                .collect();
            if ms.wave == 0 {
                w.add_mech(m, riders);
            } else {
                mission.reserve_mechs.push((ms.wave, m, riders));
            }
        }
        for (fi, f) in spec.fortresses.iter().enumerate() {
            w.add_fortress(fi, f, units, spec.power_pct);
        }
        w.mission = Some(mission);
        w
    }

    /// Sets a fortress down: its hull as moving blocks, its weak points, core
    /// and guns as craft riding on it.
    fn add_fortress(&mut self, fi: usize, f: &FortressSpec, units: &Units, power: i32) {
        let base = v3(int(f.at[0]), 0, int(f.at[1]));
        let start = self.map.movers.len();
        let mut hull = Vec::new();
        for h in &f.hull {
            let c = v3(int(h.at[0]), int(h.at[1]), int(h.at[2]));
            let half = v3(int(h.size[0]) / 2, int(h.size[1]) / 2, int(h.size[2]) / 2);
            let (lo, hi) = (c.sub(half), c.add(half));
            let paint = hull_paint(&h.paint);
            hull.push((lo, hi, paint));
            self.map.movers.push(Block {
                min: base.add(lo),
                max: base.add(hi),
                shade: paint,
            });
        }
        let ride = |kind: &str, at: &[i32; 3], k: usize| {
            let mut c = make_unit(kind, &units.0[kind], base, 0, None, power, (fi * 31 + k) as i32);
            c.mount = Some((Host::Fortress(fi), v3(int(at[0]), int(at[1]), int(at[2]))));
            c
        };
        let mut weak = Vec::new();
        for (k, at) in f.weak_points.iter().enumerate() {
            let mut c = ride("weak", at, k);
            c.ap = f.weak_ap;
            c.max_ap = f.weak_ap;
            c.radius = int(f.weak_radius_m);
            weak.push(self.craft.len());
            self.craft.push(c);
        }
        let mut core = ride("core", &f.core, 99);
        core.ap = f.core_ap;
        core.max_ap = f.core_ap;
        core.radius = int(f.core_radius_m);
        let core_at = self.craft.len();
        self.craft.push(core);
        for (k, at) in f.turrets.iter().enumerate() {
            self.craft.push(ride("turret", at, 200 + k));
        }
        for (k, at) in f.batteries.iter().enumerate() {
            self.craft.push(ride("battery", at, 300 + k));
        }
        let path = f.path.iter().map(|[x, z]| v3(int(*x), 0, int(*z))).collect();
        self.fortresses.push(Fortress {
            name: f.name.clone(),
            pos: base,
            prev_pos: base,
            path,
            next: 0,
            speed: fx::ratio(f.speed_ms, TICKS_PER_SECOND),
            size_x: f.size_x,
            movers: (start, f.hull.len()),
            hull,
            weak,
            core: core_at,
            alive: true,
        });
    }

    /// Walks every live fortress one step along its path, carrying whatever
    /// stands on its decks.
    fn update_fortresses(&mut self) {
        for fi in 0..self.fortresses.len() {
            let f = &mut self.fortresses[fi];
            f.prev_pos = f.pos;
            if !f.alive || f.path.is_empty() {
                continue;
            }
            let goal = f.path[f.next % f.path.len()];
            let to = v3(goal.x - f.pos.x, 0, goal.z - f.pos.z);
            if to.len() <= f.speed {
                f.next = (f.next + 1) % f.path.len();
            }
            let step = to.norm().scale(f.speed);
            f.pos = f.pos.add(step);
            let (start, n) = f.movers;
            self.carry(start, n, step);
        }
    }

    /// Moves the map's moving boxes `start..start + n` by `step`, and every
    /// frame standing on them with them.
    fn carry(&mut self, start: usize, n: usize, step: V3) {
        // what stands on a deck is found before the deck moves
        let riders: Vec<usize> = (0..self.mechs.len())
            .filter(|&i| {
                let m = &self.mechs[i];
                let p = m.body.pos;
                m.alive
                    && m.body.grounded
                    && self.map.movers[start..start + n].iter().any(|b| {
                        p.y == b.max.y && p.x >= b.min.x && p.x <= b.max.x && p.z >= b.min.z && p.z <= b.max.z
                    })
            })
            .collect();
        for b in &mut self.map.movers[start..start + n] {
            b.min = b.min.add(step);
            b.max = b.max.add(step);
        }
        for i in riders {
            self.mechs[i].body.pos = self.mechs[i].body.pos.add(step);
        }
    }

    /// Moves every machine of a race a step. A frame a hazard touches is
    /// thrown aside and stunned; one a lift or door closes on is lifted onto it.
    fn update_machines(&mut self) {
        for k in 0..self.machines.len() {
            let step = self.machines[k].step();
            let block = self.machines[k].block;
            if step != V3::ZERO {
                self.carry(block, 1, step);
            }
            let (role, centre, b) = (
                self.machines[k].role,
                self.machines[k].centre(),
                self.map.movers[block],
            );
            let touch = ONE / 4;
            for i in 0..self.mechs.len() {
                let m = &self.mechs[i];
                let (lo, hi) = m.bounds();
                let near = Block {
                    min: b.min.sub(v3(touch, touch, touch)),
                    max: b.max.add(v3(touch, touch, touch)),
                    shade: 0,
                };
                if !m.alive || !near.overlaps(lo, hi) {
                    continue;
                }
                let m = &mut self.mechs[i];
                if role.hazard() {
                    if m.stagger == 0 {
                        m.body.vel = course::knock(centre, m.body.pos, step);
                        m.body.grounded = false;
                        m.stagger = course::KNOCK_TICKS;
                        m.melee = None;
                    }
                    // out of the box the way it is thrown, or on top if it cannot be
                    let out = v3(m.body.vel.x, 0, m.body.vel.z).norm();
                    for _ in 0..80 {
                        let (lo, hi) = m.bounds();
                        if !b.overlaps(lo, hi) {
                            break;
                        }
                        m.body.pos = m.body.pos.add(out.scale(ONE / 2));
                    }
                } else if b.overlaps(lo, hi) {
                    m.body.pos.y = b.max.y;
                    m.body.vel.y = m.body.vel.y.max(0);
                    m.body.grounded = true;
                }
            }
        }
    }

    /// Whether a fortress's core is still sealed by a live weak point.
    pub fn core_sealed(&self, craft: usize) -> bool {
        match self.craft[craft].mount {
            Some((Host::Fortress(f), _)) => self.fortresses[f].weak.iter().any(|w| self.craft[*w].alive),
            _ => false,
        }
    }

    /// An empty bay with the player's frame standing in it, for the garage.
    pub fn hangar(cat: &Catalog, l: &Loadout, paint: Paints) -> World {
        let mut map = Map::generate(&crate::map::MapSpec {
            seed: 1,
            width_m: 240,
            length_m: 240,
            cell_m: 240,
            street_m: 0,
            empty_pct: 100,
            min_h_m: 1,
            max_h_m: 1,
            tall_pct: 0,
            tall_h_m: 1,
            ceiling_m: 60,
            clearings: Vec::new(),
            avenue_m: 0,
            style: Default::default(),
            wrap: false,
        });
        let wall = |x0: i32, z0: i32, x1: i32, z1: i32, h: i32, shade: u8| Block {
            min: v3(int(x0), 0, int(z0)),
            max: v3(int(x1), int(h), int(z1)),
            shade,
        };
        // the back wall, two gantries either side, and a low deck at the front
        map.add_block(wall(-30, 14, 30, 18, 24, 3));
        map.add_block(wall(-15, -6, -13, 10, 12, 1));
        map.add_block(wall(13, -6, 15, 10, 12, 1));
        map.add_block(wall(-15, 9, 15, 11, 13, 4));
        map.add_block(wall(-30, -40, -26, 14, 18, 0));
        map.add_block(wall(26, -40, 30, 14, 18, 0));
        let player = Mech::build(l, cat, V3::ZERO, 0, Team::Player, paint);
        empty_world(map, player, 1)
    }

    pub fn player(&self) -> &Mech {
        &self.mechs[0]
    }

    /// Brings a mech into the world with any turrets that ride on it.
    pub fn add_mech(&mut self, m: Mech, riders: Vec<Craft>) {
        let idx = self.mechs.len();
        self.mechs.push(m);
        for mut t in riders {
            t.mount = t.mount.map(|(_, off)| (Host::Mech(idx), off));
            self.craft.push(t);
        }
    }

    /// From `a` to `b`, the short way round on a ring.
    pub fn map_delta(&self, a: V3, b: V3) -> V3 {
        let d = b.sub(a);
        v3(self.map.wrap_x(d.x), d.y, d.z)
    }

    /// The camera eye and the aim direction of the player, right now.
    pub fn player_view(&self) -> (V3, V3) {
        let m = self.player();
        let pivot = m.chest().add(v3(0, PIVOT_LIFT, 0));
        let eye = camera_eye(&self.map, pivot, m.body.aim_yaw, m.body.aim_pitch);
        (eye, facing(m.body.aim_yaw, m.body.aim_pitch))
    }

    pub fn structure_centre(&self, i: usize) -> V3 {
        let b = &self.map.blocks[self.structures[i].block];
        v3(
            (b.min.x + b.max.x) / 2,
            (b.min.y + b.max.y) / 2,
            (b.min.z + b.max.z) / 2,
        )
    }

    /// The centre of a target, if it is still there.
    pub fn target_centre(&self, t: Target) -> Option<V3> {
        match t {
            Target::Mech(i) => self.mechs.get(i).filter(|m| m.alive).map(|m| m.chest()),
            Target::Craft(i) => self.craft.get(i).filter(|c| c.alive).map(|c| c.pos),
            Target::Structure(i) => self
                .structures
                .get(i)
                .filter(|s| s.alive)
                .map(|_| self.structure_centre(i)),
        }
    }

    fn target_vel(&self, t: Target) -> V3 {
        match t {
            Target::Mech(i) => self.mechs[i].body.vel,
            Target::Craft(i) => self.craft[i].vel,
            Target::Structure(_) => V3::ZERO,
        }
    }

    /// Everything the player could lock: live enemy mechs and live craft.
    pub fn hostiles(&self) -> Vec<Target> {
        let mechs = self
            .mechs
            .iter()
            .enumerate()
            .filter(|(_, m)| m.alive && m.team == Team::Enemy)
            .map(|(i, _)| Target::Mech(i));
        let craft = self
            .craft
            .iter()
            .enumerate()
            .filter(|(_, c)| c.alive)
            .map(|(i, _)| Target::Craft(i));
        mechs.chain(craft).collect()
    }

    fn weapons_free(&self) -> bool {
        self.mission.as_ref().is_none_or(|m| m.weapons)
    }

    pub fn tick(&mut self, player: Controls) {
        self.tick = self.tick.wrapping_add(1);
        let (eye, dir) = self.player_view();
        self.mechs[0].aim_point = self.aim_ray(eye, dir);
        self.update_lock(eye, dir);
        let burning = self.map.climate.floor_dps;
        for i in 0..self.mechs.len() {
            if !self.mechs[i].alive {
                continue;
            }
            let mut c = if i == 0 { player } else { self.pilot_controls(i) };
            let m = &mut self.mechs[i];
            if m.stagger > 0 {
                // a staggered frame drops whatever it was doing, a strike included
                c = Controls::default();
                m.stagger -= 1;
                m.melee = None;
            }
            self.melee_step(i);
            let m = &mut self.mechs[i];
            m.impact_quiet += 1;
            if m.impact_quiet > IMPACT_QUIET_TICKS {
                m.impact = (m.impact - m.stats.stability / 150 - 1).max(0);
            }
            m.body.step(&c, &m.tuning, &self.map);
            m.hit_flash = (m.hit_flash - 1).max(0);
            // the burning floor hurts a standard frame; a giant's feet are armoured for it
            if burning > 0 && m.body.grounded && m.body.pos.y == 0 && m.scale == ONE {
                m.burn += burning;
                let owed = m.burn / TICKS_PER_SECOND;
                m.burn %= TICKS_PER_SECOND;
                if owed > 0 {
                    self.hurt(Target::Mech(i), owed, 0);
                }
            }
            self.fire(i, &c);
        }
        self.update_fortresses();
        self.update_machines();
        self.update_craft();
        self.update_shots();
        self.update_effects();
        self.update_mission();
    }

    /// What mech `i` can see, as its pilot is shown it.
    pub fn senses(&self, i: usize) -> Senses {
        let me = &self.mechs[i];
        let target = self.player();
        let my_chest = me.chest();
        let missile = self
            .shots
            .iter()
            .filter(|s| {
                s.team != me.team && s.kind == WeaponKind::Missile && s.target == Some(Target::Mech(i))
            })
            .map(|s| self.map_delta(my_chest, s.pos).len())
            .min();
        let gate = self.mission.as_ref().and_then(|m| m.course.get(me.course_next));
        // a pad is aimed at as if standing on it, so "above" means its top is above my feet
        let to_course = gate.map(|g| {
            let lift = if g.kind.landing() {
                me.rig.chest().scale(me.scale)
            } else {
                V3::ZERO
            };
            self.map_delta(my_chest, self.gate_point(g).add(lift))
        });
        let course_landing = gate.is_some_and(|g| g.kind.landing());
        Senses {
            me: my_chest,
            my_vel: me.body.vel,
            alt: me.body.pos.y,
            en_pct: me.body.en_pct(&me.tuning),
            en_locked: me.body.en_locked,
            ap_pct: me.ap * 100 / me.stats.ap.max(1),
            glide: me.body.glide,
            staggered: me.stagger > 0,
            airborne: !me.body.grounded,
            floor_burning: self.map.climate.floor_dps > 0 && me.body.grounded && me.body.pos.y == 0,
            to_target: self.map_delta(my_chest, target.chest()),
            target_vel: target.body.vel,
            target_ap_pct: target.ap * 100 / target.stats.ap.max(1),
            target_staggered: target.stagger > 0,
            can_see: target.alive && self.map.clear_line(my_chest, target.chest()),
            missile,
            to_course,
            course_landing,
            weapons: me
                .weapons
                .map(|w| (w.speed, w.range, w.kind == WeaponKind::Missile)),
            ready: [0, 1, 2].map(|k| me.wstate[k].cooldown == 0 && me.wstate[k].ammo > 0),
            tick: self.tick,
        }
    }

    /// Lets the pilot of mech `i` decide, then turns its head and lock to match.
    fn pilot_controls(&mut self, i: usize) -> Controls {
        let s = self.senses(i);
        let range = self.mechs[i].lock_range;
        let Some(pilot) = self.mechs[i].pilot.as_mut() else {
            return Controls::default();
        };
        let d = pilot.think(&s);
        let m = &mut self.mechs[i];
        m.intent = Some(d.label);
        m.body.aim_yaw = d.aim_yaw & (fx::TURN - 1);
        m.body.aim_pitch = d.aim_pitch.clamp(crate::mech::PITCH_MIN, crate::mech::PITCH_MAX);
        // shots go where the pilot looks, error and all; only missiles home
        m.aim_point =
            s.me.add(facing(m.body.aim_yaw, m.body.aim_pitch).scale(s.to_target.len()));
        let hostile = m.team == Team::Enemy;
        m.lock = (hostile && s.can_see && s.to_target.len() <= range).then_some(Target::Mech(0));
        d.controls
    }

    fn aim_ray(&self, eye: V3, dir: V3) -> V3 {
        let far = eye.add(dir.scale(AIM_REACH));
        match self.map.segment_hit(eye, far) {
            Some(t) => eye.lerp(far, t),
            None => far,
        }
    }

    fn update_lock(&mut self, eye: V3, dir: V3) {
        let m = &self.mechs[0];
        let held = m.lock;
        let from = m.chest();
        let range = m.lock_range;
        let mut best: Option<(i32, Target)> = None;
        for t in self.hostiles() {
            let Some(c) = self.target_centre(t) else { continue };
            let to = self.map_delta(eye, c);
            if to.len() > range {
                continue;
            }
            let cos = dir.dot(to.norm());
            let cone = if held == Some(t) {
                LOCK_HOLD_CONE
            } else {
                LOCK_CONE
            };
            if cos < fx::cos(cone) || !self.map.clear_line(from, c) {
                continue;
            }
            // the held target wins a near tie, so the lock does not flicker
            let score = cos + if held == Some(t) { ONE / 64 } else { 0 };
            if best.is_none_or(|(s, _)| score > s) {
                best = Some((score, t));
            }
        }
        self.mechs[0].lock = best.map(|(_, t)| t);
    }

    fn fire(&mut self, i: usize, c: &Controls) {
        let free = self.weapons_free();
        for k in 0..3 {
            let st = &mut self.mechs[i].wstate[k];
            st.cooldown = (st.cooldown - 1).max(0);
            if !free || !c.fire[k] || st.cooldown > 0 || st.ammo <= 0 {
                continue;
            }
            let m = &self.mechs[i];
            let w = m.weapons[k];
            let muzzle = m
                .frame(m.body.pos, m.body.yaw)
                .apply(m.rig.muzzles(&m.body.pose())[k]);
            if w.kind == WeaponKind::Melee {
                // a strike starts here and plays out in `melee_step`
                if m.melee.is_none() {
                    let target = m.lock;
                    let st = &mut self.mechs[i].wstate[k];
                    st.cooldown = w.fire_ticks;
                    self.mechs[i].melee = Some((k, 0, target));
                }
                continue;
            }
            // a pilot's direct fire goes where it aims; the player's is led
            // onto a locked target, as the inspiring game's lock does
            let assisted = m.pilot.is_none() || w.kind == WeaponKind::Missile;
            let aim = match (
                m.lock.and_then(|t| self.target_centre(t)).filter(|_| assisted),
                w.kind,
            ) {
                (Some(c), WeaponKind::Missile) => c,
                (Some(c), _) => {
                    // lead the target by its velocity over the flight time
                    let flight = fx::div(c.sub(muzzle).len(), w.speed.max(1)) / ONE;
                    c.add(
                        self.target_vel(m.lock.unwrap_or(Target::Mech(0)))
                            .scale(int(flight)),
                    )
                }
                (None, _) => m.aim_point,
            };
            let shots = Shot::fire(&w, m.team, muzzle, aim, m.lock, &mut self.rng);
            // a missile salvo spends a missile each; any other weapon one round a pull
            let spent = if w.kind == WeaponKind::Missile {
                shots.len() as i32
            } else {
                1
            };
            let st = &mut self.mechs[i].wstate[k];
            st.cooldown = w.fire_ticks;
            st.ammo -= spent;
            if i == 0 {
                if let Some(m) = self.mission.as_mut() {
                    m.rounds += spent;
                }
            }
            self.shots.extend(shots);
            self.effects.push(Effect {
                kind: EffectKind::Flash,
                pos: muzzle,
                vel: V3::ZERO,
                age: 0,
                life: 3,
                size: ONE + ONE / 2,
                yaw: 0,
            });
        }
    }

    /// A melee strike: lunge at the target, homing, then strike everything in
    /// reach in front, once.
    fn melee_step(&mut self, i: usize) {
        let Some((k, t, target)) = self.mechs[i].melee else {
            return;
        };
        let w = self.mechs[i].weapons[k];
        let reach = fx::mul(w.blast, self.mechs[i].scale);
        let chest = self.mechs[i].chest();
        let goal = target
            .and_then(|tg| self.target_centre(tg))
            .filter(|g| self.map_delta(chest, *g).len() <= w.range);
        if t < LUNGE_TICKS {
            let dir = match goal {
                Some(g) => {
                    let d = self.map_delta(chest, g);
                    // close enough to strike: stop pushing, so the blow lands at reach
                    if d.len() < reach * 2 / 3 {
                        V3::ZERO
                    } else {
                        d.norm()
                    }
                }
                None => facing(self.mechs[i].body.aim_yaw, 0),
            };
            let m = &mut self.mechs[i];
            if dir != V3::ZERO {
                m.body.vel = dir.scale(w.speed);
                m.body.qb_ticks = 2;
                m.body.yaw = crate::geom::yaw_pitch_of(dir).0;
                m.body.aim_yaw = m.body.yaw;
                m.body.thrust = 2;
            }
        }
        if t == LUNGE_TICKS {
            let m = &self.mechs[i];
            let front = chest.add(facing(m.body.yaw, 0).scale(reach / 2));
            let (team, yaw) = (m.team, m.body.yaw);
            for tg in self.all_targets_of(team) {
                if matches!(tg, Target::Structure(_)) {
                    continue;
                }
                let Some(c) = self.target_centre(tg) else { continue };
                let size = match tg {
                    Target::Mech(j) => fx::mul(int(3), self.mechs[j].scale),
                    Target::Craft(j) => self.craft[j].radius,
                    Target::Structure(_) => 0,
                };
                if self.map_delta(front, c).len() <= reach + size {
                    self.hurt(tg, w.damage, w.impact);
                }
            }
            self.effects.push(Effect {
                kind: EffectKind::Slash,
                pos: front,
                vel: V3::ZERO,
                age: 0,
                life: 10,
                size: reach,
                yaw,
            });
        }
        self.mechs[i].melee = (t + 1 < MELEE_DONE_TICKS).then_some((k, t + 1, target));
    }

    /// One tick of every unit that is not a mech.
    fn update_craft(&mut self) {
        let player = self.player();
        let (target, target_vel, target_alive) = (player.chest(), player.body.vel, player.alive);
        let live_structures: Vec<V3> = (0..self.structures.len())
            .filter(|i| self.structures[*i].alive)
            .map(|i| self.structure_centre(i))
            .collect();
        let mut volleys: Vec<(V3, V3, Gun)> = Vec::new();
        let mut craft = std::mem::take(&mut self.craft);
        for c in &mut craft {
            c.prev_pos = c.pos;
            c.prev_yaw = c.yaw;
            if !c.alive {
                if c.respawn > 0 {
                    c.down_ticks -= 1;
                    if c.down_ticks <= 0 {
                        c.alive = true;
                        c.ap = c.max_ap;
                        c.pos = c.anchor;
                        c.prev_pos = c.anchor;
                    }
                }
                continue;
            }
            // a rider goes where its host goes, and falls with it
            if let Some((host, off)) = c.mount {
                let at = match host {
                    Host::Mech(j) => {
                        let h = &self.mechs[j];
                        h.alive.then(|| h.frame(h.body.pos, h.body.yaw).apply(off))
                    }
                    Host::Fortress(f) => {
                        let h = &self.fortresses[f];
                        h.alive.then(|| h.pos.add(off))
                    }
                };
                let Some(at) = at else {
                    c.alive = false;
                    continue;
                };
                c.vel = at.sub(c.pos);
                c.pos = at;
            }
            // what this unit is after: its goal, a structure, or the player
            let aim_at = match c.kind {
                CraftKind::Tank => c
                    .goal
                    .or_else(|| {
                        live_structures
                            .iter()
                            .min_by_key(|s| self.map_delta(c.pos, **s).len())
                            .copied()
                    })
                    .unwrap_or(target),
                _ => target,
            };
            match c.kind {
                CraftKind::Heli | CraftKind::Drone => {
                    // circle the anchor: gunships at about 20 m/s, drones slowly
                    let step = if c.kind == CraftKind::Heli {
                        (fx::TURN as i64 * 20 * ONE as i64 / (377 * c.orbit.max(ONE) as i64)) as i32
                    } else {
                        deg(1) / 3
                    };
                    c.phase = (c.phase + step) & (fx::TURN - 1);
                    let orbit = facing(c.phase, 0).scale(c.orbit);
                    let bob = fx::mul(fx::sin(c.phase * 2), int(3));
                    let next = c.anchor.add(orbit).add(v3(0, bob, 0));
                    c.vel = next.sub(c.pos);
                    c.pos = next;
                    c.yaw = c.phase + fx::QUARTER;
                }
                CraftKind::Tank => {
                    let to = self.map_delta(c.pos, aim_at);
                    let flat = v3(to.x, 0, to.z);
                    let stop = match (c.goal, c.gun) {
                        (None, Some(g)) => int(g.range_m) * 3 / 4,
                        _ => 0,
                    };
                    c.vel = V3::ZERO;
                    if flat.len() > stop {
                        let step = flat.norm().scale(c.speed);
                        // drive on, sliding along whatever is in the way
                        let r = c.radius;
                        let blocked = |p: V3| {
                            self.map
                                .box_blocked(v3(p.x - r, ONE, p.z - r), v3(p.x + r, int(3), p.z + r))
                        };
                        for next in [
                            c.pos.add(step),
                            c.pos.add(v3(step.x, 0, 0)),
                            c.pos.add(v3(0, 0, step.z)),
                        ] {
                            if !blocked(next) {
                                c.vel = next.sub(c.pos);
                                c.pos = v3(self.map.wrap_x(next.x), next.y, next.z);
                                break;
                            }
                        }
                    }
                    c.yaw = crate::geom::yaw_pitch_of(flat).0;
                }
                CraftKind::Turret | CraftKind::Battery | CraftKind::Weak | CraftKind::Core => {}
            }
            let Some(g) = c.gun else { continue };
            // fire at the structure in reach, or at the player in sight
            let at_structure = c.kind == CraftKind::Tank && c.goal.is_none() && aim_at != target;
            let (shoot_at, lead) = if at_structure {
                (aim_at, V3::ZERO)
            } else {
                (target, target_vel)
            };
            let to = self.map_delta(c.pos, shoot_at);
            let in_reach = (at_structure || target_alive) && to.len() < int(g.range_m);
            if c.kind != CraftKind::Tank && in_reach {
                c.yaw = crate::geom::yaw_pitch_of(to).0;
            }
            c.cooldown -= 1;
            let sight = at_structure || self.map.clear_line(c.pos, shoot_at);
            if in_reach && c.cooldown <= 0 && sight {
                if c.burst <= 0 {
                    c.burst = g.burst;
                }
                let muzzle = c.pos.add(facing(c.yaw, 0).scale(c.radius)).add(v3(0, ONE, 0));
                let flight = fx::div(to.len(), fx::ratio(g.speed_ms, TICKS_PER_SECOND)) / ONE;
                volleys.push((muzzle, muzzle.add(to).add(lead.scale(int(flight))), g));
                c.burst -= 1;
                c.cooldown = if c.burst > 0 {
                    g.burst_gap_ms * TICKS_PER_SECOND / 1000
                } else {
                    g.reload_ms * TICKS_PER_SECOND / 1000
                };
            }
        }
        self.craft = craft;
        for (muzzle, aim, g) in volleys {
            let shots = Shot::fire(
                &Weapon::from_gun(&g),
                Team::Enemy,
                muzzle,
                aim,
                None,
                &mut self.rng,
            );
            self.shots.extend(shots);
            self.effects.push(Effect {
                kind: EffectKind::Flash,
                pos: muzzle,
                vel: V3::ZERO,
                age: 0,
                life: 3,
                size: ONE,
                yaw: 0,
            });
        }
    }

    fn update_shots(&mut self) {
        let shots = std::mem::take(&mut self.shots);
        let mut kept = Vec::with_capacity(shots.len());
        for mut s in shots {
            let goal = s.target.and_then(|t| self.target_centre(t));
            s.steer(goal);
            s.prev_pos = s.pos;
            let next = s.pos.add(s.vel);
            let structure_of = |b: usize| self.structures.iter().position(|st| st.block == b && st.alive);
            let mut hit: Option<(i32, Option<Target>)> =
                self.map.segment_hit_block(s.pos, next).map(|(t, b)| {
                    // enemy fire breaks a protected building it strikes
                    let who = b
                        .and_then(structure_of)
                        .filter(|_| s.team == Team::Enemy)
                        .map(Target::Structure);
                    (t, who)
                });
            let mut consider = |t: i32, who: Target| {
                if hit.is_none_or(|(bt, _)| t < bt) {
                    hit = Some((t, Some(who)));
                }
            };
            for (i, m) in self.mechs.iter().enumerate() {
                if m.alive && m.team != s.team {
                    for (c, r) in m.spheres() {
                        if let Some(t) = segment_sphere(s.pos, next, c, r) {
                            consider(t, Target::Mech(i));
                        }
                    }
                }
            }
            if s.team == Team::Player {
                for (i, c) in self.craft.iter().enumerate() {
                    if c.alive {
                        if let Some(t) = segment_sphere(s.pos, next, c.pos, c.radius) {
                            consider(t, Target::Craft(i));
                        }
                    }
                }
            }
            s.age += 1;
            match hit {
                Some((t, who)) => {
                    s.pos = s.pos.lerp(next, t);
                    self.impact(&s, who);
                }
                None if s.age > s.life => {
                    s.pos = next;
                    if s.blast > 0 || s.kind == WeaponKind::Missile {
                        self.impact(&s, None);
                    }
                }
                None => {
                    let wrapped = self.map.wrap_x(next.x);
                    s.prev_pos.x += wrapped - next.x;
                    s.pos = v3(wrapped, next.y, next.z);
                    kept.push(s);
                }
            }
        }
        self.shots.extend(kept);
    }

    /// A shot has stopped at `s.pos`: hurt what it hit, then burst if it bursts.
    fn impact(&mut self, s: &Shot, who: Option<Target>) {
        if let Some(t) = who {
            self.hurt(t, s.damage_now(), s.impact);
        }
        if s.blast > 0 {
            for t in self.all_targets_of(s.team) {
                if Some(t) == who {
                    continue;
                }
                let Some(c) = self.target_centre(t) else { continue };
                let dist = self.map_delta(s.pos, c).len();
                let d = blast_damage(s.damage_now(), dist, s.blast);
                if d > 0 {
                    let i = blast_damage(s.impact, dist, s.blast);
                    self.hurt(t, d, i);
                }
            }
            self.effects.push(Effect {
                kind: EffectKind::Blast,
                pos: s.pos,
                vel: V3::ZERO,
                age: 0,
                life: 24,
                size: s.blast,
                yaw: 0,
            });
        } else {
            let (kind, life, size) = match s.kind {
                WeaponKind::Missile => (EffectKind::Blast, 14, int(5)),
                _ => (EffectKind::Spark, 5, ONE + ONE / 2),
            };
            self.effects.push(Effect {
                kind,
                pos: s.pos,
                vel: V3::ZERO,
                age: 0,
                life,
                size,
                yaw: 0,
            });
        }
    }

    /// Every target a shot from `team` can hurt.
    fn all_targets_of(&self, team: Team) -> Vec<Target> {
        let mechs = self
            .mechs
            .iter()
            .enumerate()
            .filter(|(_, m)| m.alive && m.team != team)
            .map(|(i, _)| Target::Mech(i));
        let others: Vec<Target> = if team == Team::Player {
            self.craft
                .iter()
                .enumerate()
                .filter(|(_, c)| c.alive)
                .map(|(i, _)| Target::Craft(i))
                .collect()
        } else {
            self.structures
                .iter()
                .enumerate()
                .filter(|(_, s)| s.alive)
                .map(|(i, _)| Target::Structure(i))
                .collect()
        };
        mechs.chain(others).collect()
    }

    /// Damage and impact to a target. A staggered mech takes extra damage, and
    /// impact past a mech's stability staggers it.
    pub fn hurt(&mut self, t: Target, dmg: i32, impact: i32) {
        let (pos, destroyed) = match t {
            Target::Mech(i) => {
                let m = &mut self.mechs[i];
                let dmg = if m.stagger > 0 {
                    dmg * STAGGER_DAMAGE_PCT / 100
                } else {
                    dmg
                };
                m.ap -= dmg;
                if impact > 0 {
                    m.hit_flash = 4;
                    m.impact_quiet = 0;
                }
                if m.stagger == 0 {
                    m.impact += impact;
                    if m.impact >= m.stats.stability {
                        m.impact = 0;
                        m.stagger = STAGGER_TICKS;
                    }
                }
                let gone = m.alive && m.ap <= 0;
                if gone {
                    m.ap = 0;
                    m.alive = false;
                }
                (m.chest(), gone)
            }
            Target::Craft(i) => {
                // a sealed core turns every hit
                let dmg = if self.craft[i].kind == CraftKind::Core && self.core_sealed(i) {
                    0
                } else {
                    dmg
                };
                let c = &mut self.craft[i];
                c.ap -= dmg;
                let gone = c.alive && c.ap <= 0;
                if gone {
                    c.ap = 0;
                    c.alive = false;
                    c.down_ticks = c.respawn;
                }
                (c.pos, gone)
            }
            Target::Structure(i) => {
                let centre = self.structure_centre(i);
                let s = &mut self.structures[i];
                s.ap -= dmg;
                let gone = s.alive && s.ap <= 0;
                if gone {
                    s.ap = 0;
                    s.alive = false;
                    // it falls to rubble, which still stands in the way
                    let b = &mut self.map.blocks[s.block];
                    b.max.y = b.max.y.min(RUBBLE);
                }
                (centre, gone)
            }
        };
        // a fallen core brings its fortress down
        if let (true, Target::Craft(i)) = (destroyed, t) {
            if let Some((Host::Fortress(f), _)) = self.craft[i]
                .mount
                .filter(|_| self.craft[i].kind == CraftKind::Core)
            {
                self.fortresses[f].alive = false;
                let (start, n) = self.fortresses[f].movers;
                for b in &self.map.movers[start..start + n] {
                    let c = v3((b.min.x + b.max.x) / 2, b.max.y, (b.min.z + b.max.z) / 2);
                    let size = (b.max.x - b.min.x).min(int(120));
                    self.effects.push(Effect {
                        kind: EffectKind::Blast,
                        pos: c,
                        vel: V3::ZERO,
                        age: 0,
                        life: 90,
                        size,
                        yaw: 0,
                    });
                }
            }
        }
        if destroyed {
            let hostile = match t {
                Target::Craft(_) => true,
                Target::Mech(i) => self.mechs[i].team == Team::Enemy,
                Target::Structure(_) => false,
            };
            if hostile {
                self.kills += 1;
            }
            self.effects.push(Effect {
                kind: EffectKind::Blast,
                pos,
                vel: V3::ZERO,
                age: 0,
                life: 36,
                size: int(14),
                yaw: 0,
            });
            for k in 0..6 {
                let dir = facing(
                    fx::TURN * k / 6 + self.rng.range(0, deg(40)),
                    deg(self.rng.range(20, 70)),
                );
                let vel = dir.scale(fx::ratio(self.rng.range(12, 30), TICKS_PER_SECOND));
                self.effects.push(Effect {
                    kind: EffectKind::Debris,
                    pos,
                    vel,
                    age: 0,
                    life: 70,
                    size: ONE,
                    yaw: 0,
                });
            }
        }
    }

    fn update_effects(&mut self) {
        let g = fx::ratio(30, TICKS_PER_SECOND * TICKS_PER_SECOND);
        for e in &mut self.effects {
            e.age += 1;
            if e.kind == EffectKind::Debris {
                e.vel.y -= g;
                e.pos = e.pos.add(e.vel);
                if e.pos.y < 0 {
                    e.pos.y = 0;
                    e.vel = V3::ZERO;
                }
            }
        }
        self.effects.retain(|e| e.age < e.life);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::combat::Shot;
    use crate::content::tests::{missions, palette};
    use crate::parts::tests::catalog;

    pub fn units() -> Units {
        Units::parse(include_str!("../../../data/units.json")).unwrap()
    }

    pub fn world() -> World {
        let c = catalog();
        World::proving(
            &c,
            &c.default_loadout(),
            &missions().proving,
            palette().scheme(0).paints(),
            &units(),
        )
    }

    /// The world of a campaign mission, with the default loadout.
    pub fn mission_world(id: &str) -> World {
        let c = catalog();
        let camp = crate::campaign::tests::campaign();
        let (p, i) = camp.find(id).unwrap();
        let pal = palette();
        World::mission(
            &c,
            &c.default_loadout(),
            pal.scheme(0).paints(),
            &camp.planets[p],
            &camp.planets[p].missions[i],
            &crate::pilot::tests::pilots(),
            &units(),
            pal.enemy.paints(),
            pal.giant.paints(),
        )
    }

    #[test]
    fn the_proving_ground_starts_with_its_drones_clear_of_buildings() {
        let w = world();
        assert_eq!(w.craft.len(), 12);
        for c in &w.craft {
            let r = c.radius;
            assert!(
                !w.map.box_blocked(c.pos.sub(v3(r, r, r)), c.pos.add(v3(r, r, r))),
                "drone at {:?}",
                c.pos
            );
        }
        assert!(
            !w.map
                .box_blocked(v3(-int(3), 0, -int(3)), v3(int(3), int(8), int(3))),
            "spawn is clear"
        );
    }

    #[test]
    fn the_same_inputs_give_the_same_world() {
        let mut a = mission_world("halden-4");
        let mut b = mission_world("halden-4");
        let script = |n: u32| Controls {
            move_z: ONE,
            move_x: if n % 90 < 45 { ONE } else { -ONE },
            ascend: n % 120 > 80,
            quick_boost: n.is_multiple_of(50),
            toggle_glide: n == 10,
            fire: [n.is_multiple_of(7), n.is_multiple_of(3), n == 200],
        };
        for n in 0..600 {
            a.mechs[0].body.look(deg(1) / 4, 0);
            b.mechs[0].body.look(deg(1) / 4, 0);
            a.tick(script(n));
            b.tick(script(n));
        }
        assert_eq!(a, b);
    }

    fn aim_at(w: &mut World, p: V3) {
        let (eye, _) = w.player_view();
        let (yaw, pitch) = crate::geom::yaw_pitch_of(p.sub(eye));
        let b = &mut w.mechs[0].body;
        b.aim_yaw = yaw;
        b.aim_pitch = pitch;
        b.yaw = yaw;
    }

    #[test]
    fn looking_at_a_drone_locks_it_and_rifle_fire_destroys_it() {
        let mut w = world();
        let at = w.mechs[0].body.pos.add(v3(0, int(30), -int(60)));
        w.craft[0].anchor = at;
        w.craft[0].pos = at;
        w.craft[0].orbit = 0;
        for _ in 0..3 {
            let p = w.craft[0].pos;
            aim_at(&mut w, p);
            w.tick(Controls::default());
        }
        assert_eq!(w.mechs[0].lock, Some(Target::Craft(0)));
        let rifle = Controls {
            fire: [false, true, false],
            ..Controls::default()
        };
        for _ in 0..240 {
            let p = w.craft[0].pos;
            aim_at(&mut w, p);
            w.tick(rifle);
            if !w.craft[0].alive {
                break;
            }
        }
        assert!(!w.craft[0].alive, "{} AP left", w.craft[0].ap);
        assert_eq!(w.kills, 1);
    }

    #[test]
    fn a_drone_behind_a_building_is_not_locked() {
        let mut w = world();
        let at = w.mechs[0].body.pos.add(v3(0, int(10), -int(80)));
        w.craft[0].anchor = at;
        w.craft[0].pos = at;
        w.craft[0].orbit = 0;
        w.map.add_block(Block {
            min: v3(-int(15), 0, -int(50)),
            max: v3(int(15), int(40), -int(40)),
            shade: 0,
        });
        for _ in 0..3 {
            let p = w.craft[0].pos;
            aim_at(&mut w, p);
            w.tick(Controls::default());
        }
        assert_eq!(w.mechs[0].lock, None);
    }

    #[test]
    fn impact_past_stability_staggers_and_a_staggered_frame_takes_half_again() {
        let mut w = world();
        let stab = w.mechs[0].stats.stability;
        w.hurt(Target::Mech(0), 100, stab - 1);
        assert_eq!(w.mechs[0].stagger, 0, "one short of stability does not stagger");
        let ap = w.mechs[0].ap;
        w.hurt(Target::Mech(0), 100, 1);
        assert_eq!(w.mechs[0].stagger, STAGGER_TICKS);
        assert_eq!(w.mechs[0].impact, 0, "the meter empties when it breaks");
        let ap2 = w.mechs[0].ap;
        assert_eq!(ap - ap2, 100);
        w.hurt(Target::Mech(0), 100, 0);
        assert_eq!(ap2 - w.mechs[0].ap, 150);
        let before = w.mechs[0].body.pos;
        w.tick(Controls {
            move_z: ONE,
            ..Controls::default()
        });
        assert_eq!(
            w.mechs[0].body.pos.z, before.z,
            "a staggered frame ignores its controls"
        );
    }

    #[test]
    fn impact_drains_away_after_a_quiet_second() {
        let mut w = world();
        w.hurt(Target::Mech(0), 0, 300);
        for _ in 0..IMPACT_QUIET_TICKS {
            w.tick(Controls::default());
        }
        assert_eq!(w.mechs[0].impact, 300);
        for _ in 0..600 {
            w.tick(Controls::default());
        }
        assert_eq!(w.mechs[0].impact, 0);
    }

    #[test]
    fn a_drone_comes_back_after_its_respawn_time() {
        let mut w = world();
        w.hurt(Target::Craft(3), 10_000, 0);
        assert!(!w.craft[3].alive);
        for _ in 0..360 {
            w.tick(Controls::default());
        }
        assert!(w.craft[3].alive);
        assert_eq!(w.craft[3].ap, w.craft[3].max_ap);
    }

    #[test]
    fn a_grenade_burst_hurts_what_stands_near_it() {
        let mut w = world();
        let at = v3(0, int(30), -int(80));
        w.craft[0].pos = at;
        let s = Shot {
            pos: at.add(v3(int(6), 0, 0)),
            prev_pos: at,
            vel: V3::ZERO,
            kind: WeaponKind::Grenade,
            team: Team::Player,
            damage: 900,
            impact: 0,
            age: 0,
            full_ticks: 100,
            life: 100,
            target: None,
            turn: 0,
            blast: int(18),
        };
        let before = w.craft[0].ap;
        w.impact(&s, None);
        // 6 m of 18: kept = 1 − (1/3)(2/3) = 7/9 → 700
        assert!(
            before - w.craft[0].ap >= 699 || !w.craft[0].alive,
            "{}",
            w.craft[0].ap
        );
    }

    #[test]
    fn the_camera_is_pulled_in_front_of_a_wall_behind_the_mech() {
        let mut w = world();
        let blk = Block {
            min: v3(-int(20), 0, int(6)),
            max: v3(int(20), int(60), int(30)),
            shade: 0,
        };
        w.map.add_block(blk);
        let (eye, _) = w.player_view();
        assert!(eye.z < blk.min.z, "eye at {:?}", eye);
    }

    fn with_blade(w: &mut World) {
        let c = catalog();
        let mut l = c.default_loadout();
        l.0.insert(Slot::RightWeapon, "bl-emberline".to_string());
        let paint = w.mechs[0].paint;
        let pos = w.mechs[0].body.pos;
        w.mechs[0] = Mech::build(&l, &c, pos, 0, Team::Player, paint);
    }

    #[test]
    fn a_blade_lunges_at_a_locked_target_and_cuts_it() {
        let mut w = world();
        with_blade(&mut w);
        let at = w.mechs[0].chest().add(v3(0, 0, -int(40)));
        w.craft[0].anchor = at;
        w.craft[0].pos = at;
        w.craft[0].orbit = 0;
        let ap = w.craft[0].ap;
        aim_at(&mut w, at);
        w.tick(Controls::default());
        assert_eq!(w.mechs[0].lock, Some(Target::Craft(0)));
        let start = w.mechs[0].body.pos;
        w.tick(Controls {
            fire: [true, false, false],
            ..Controls::default()
        });
        for _ in 0..LUNGE_TICKS + 2 {
            w.tick(Controls::default());
        }
        assert!(w.mechs[0].body.pos.dist(start) > int(15), "it lunged");
        // 1300 damage against a 700 AP drone
        assert!(!w.craft[0].alive || w.craft[0].ap < ap, "the cut landed");
        assert!(w.effects.iter().any(|e| e.kind == EffectKind::Slash) || !w.craft[0].alive);
        assert_eq!(w.mechs[0].wstate[0].ammo, 999, "a blade spends nothing");
    }

    #[test]
    fn a_stagger_breaks_off_a_strike() {
        let mut w = world();
        with_blade(&mut w);
        w.tick(Controls {
            fire: [true, false, false],
            ..Controls::default()
        });
        assert!(w.mechs[0].melee.is_some());
        w.mechs[0].stagger = STAGGER_TICKS;
        w.tick(Controls::default());
        assert!(w.mechs[0].melee.is_none());
    }

    #[test]
    fn a_giant_is_hit_collides_and_draws_at_its_size() {
        let mut w = world();
        let base = w.mechs[0].clone();
        w.mechs[0].grow(400, 50, 200);
        let g = &w.mechs[0];
        assert_eq!(g.spheres()[0].1, base.spheres()[0].1 * 4);
        assert_eq!(g.tuning.half_width, base.tuning.half_width * 4);
        assert_eq!(g.tuning.walk, base.tuning.walk / 2);
        assert_eq!(g.weapons[1].damage, base.weapons[1].damage * 2);
        assert!(g.chest().y > base.chest().y * 3);
    }

    #[test]
    fn turrets_ride_their_giant_and_fall_with_it() {
        let mut w = mission_world("tethys-2");
        let g = w.mechs.iter().position(|m| m.name == "COLOSSUS ALPHA").unwrap();
        let riders: Vec<usize> = (0..w.craft.len())
            .filter(|i| w.craft[*i].mount.is_some_and(|(h, _)| h == Host::Mech(g)))
            .collect();
        assert_eq!(riders.len(), 6);
        for _ in 0..120 {
            w.tick(Controls::default());
        }
        let host = &w.mechs[g];
        for &r in &riders {
            assert!(
                w.craft[r].pos.dist(host.body.pos) < fx::mul(int(10), host.scale),
                "turret {r} rides along"
            );
            assert!(w.craft[r].pos.y > int(60), "high on its body");
        }
        w.hurt(Target::Mech(g), 100_000_000, 0);
        w.tick(Controls::default());
        assert!(riders.iter().all(|r| !w.craft[*r].alive));
    }

    #[test]
    fn every_planet_has_a_giant_and_the_hidden_one_climbs_ten_thirty_hundred_three_hundred_thousand() {
        let camp = crate::campaign::tests::campaign();
        for p in camp.planets.iter().filter(|p| !p.hidden) {
            assert!(
                p.missions
                    .iter()
                    .any(|m| m.mechs.iter().any(|ms| ms.scale_pct > 100)),
                "{} has no giant",
                p.id
            );
        }
        let tethys = camp.planets.iter().find(|p| p.hidden).unwrap();
        // times a standard frame: frames by scale, fortresses by length over a 7 m frame
        let sizes: Vec<i32> = tethys
            .missions
            .iter()
            .map(|m| {
                let frames = m.mechs.iter().map(|ms| ms.scale_pct / 100).max().unwrap_or(0);
                let forts = m
                    .fortresses
                    .iter()
                    .map(|f| {
                        let long = f.hull.iter().map(|h| h.size[2]).max().unwrap_or(0);
                        assert!(
                            (long - f.size_x * 7).abs() <= f.size_x * 7 / 10,
                            "{} is {long} m long",
                            f.name
                        );
                        f.size_x
                    })
                    .max()
                    .unwrap_or(0);
                frames.max(forts)
            })
            .collect();
        assert_eq!(sizes, vec![10, 30, 100, 300, 1000]);
    }

    fn deck_of(w: &World) -> Block {
        let (start, n) = w.fortresses[0].movers;
        // the deck: the box with the largest footprint (its glowing edges are as long, but thin)
        *w.map.movers[start..start + n]
            .iter()
            .max_by_key(|b| (b.max.z - b.min.z) as i64 * (b.max.x - b.min.x) as i64)
            .unwrap()
    }

    #[test]
    fn a_frame_on_a_fortress_deck_is_carried_as_it_walks() {
        let mut w = mission_world("tethys-3");
        let deck = deck_of(&w);
        // beside the central tower, inside the row of guns
        w.mechs[0].body.pos = v3(int(60), deck.max.y + int(2), (deck.min.z + deck.max.z) / 2);
        w.mechs[0].body.grounded = false;
        for _ in 0..60 {
            w.tick(Controls::default());
        }
        assert!(
            w.mechs[0].body.grounded && w.mechs[0].body.pos.y == deck_of(&w).max.y,
            "it landed on the deck"
        );
        let (before, fort) = (w.mechs[0].body.pos, w.fortresses[0].pos);
        for _ in 0..120 {
            w.tick(Controls::default());
        }
        let moved = w.fortresses[0].pos.sub(fort);
        let carried = w.mechs[0].body.pos.sub(before);
        assert!(moved.len() > int(15), "the fortress walked {}", moved.len());
        assert!(
            (carried.z - moved.z).abs() < ONE && (carried.x - moved.x).abs() < ONE,
            "carried {carried:?} with {moved:?}"
        );
    }

    #[test]
    fn a_fortress_core_turns_hits_until_its_weak_points_fall_and_then_brings_it_down() {
        let mut w = mission_world("tethys-3");
        let core = w.fortresses[0].core;
        let ap = w.craft[core].ap;
        w.hurt(Target::Craft(core), 1_000_000, 0);
        assert_eq!(w.craft[core].ap, ap, "sealed");
        for i in w.fortresses[0].weak.clone() {
            w.hurt(Target::Craft(i), 1_000_000, 0);
        }
        assert!(!w.core_sealed(core));
        w.hurt(Target::Craft(core), 1_000_000, 0);
        assert!(!w.fortresses[0].alive);
        w.tick(Controls::default());
        assert!(w.craft.iter().all(|c| !c.alive), "every gun on it fell with it");
        assert_eq!(w.mission.as_ref().unwrap().success, Some(true));
    }

    #[test]
    fn a_fortress_hull_stops_shots() {
        let w = mission_world("tethys-3");
        let deck = deck_of(&w);
        let below = v3(0, deck.min.y - int(20), (deck.min.z + deck.max.z) / 2);
        let above = v3(0, deck.max.y + int(20), below.z);
        assert!(!w.map.clear_line(below, above));
    }

    #[test]
    fn a_plasma_orb_bursts_where_it_lands() {
        let c = catalog();
        let w = Weapon::from_part(c.get("pr-corona").unwrap());
        assert_eq!(w.kind, WeaponKind::Plasma);
        assert!(w.blast > 0);
        let mut world = world();
        let at = v3(0, int(30), -int(80));
        world.craft[0].pos = at;
        let mut rng = Rng::new(1);
        let mut s = Shot::fire(&w, Team::Player, at.add(v3(int(4), 0, 0)), at, None, &mut rng)[0];
        s.pos = at.add(v3(int(4), 0, 0));
        let ap = world.craft[0].ap;
        world.impact(&s, None);
        assert!(world.craft[0].ap < ap, "the burst reached a drone 4 m off");
    }

    #[test]
    fn tanks_drive_at_the_structures_and_damage_them() {
        let mut w = mission_world("halden-3");
        let ap: i32 = w.structures.iter().map(|s| s.ap).sum();
        // keep the player out of it: far off and high up
        w.mechs[0].body.pos = v3(int(600), int(300), int(1100));
        for _ in 0..60 * 60 {
            w.tick(Controls::default());
        }
        let now: i32 = w.structures.iter().map(|s| s.ap).sum();
        assert!(now < ap, "the tanks did no damage in a minute");
    }

    #[test]
    fn a_fallen_structure_drops_to_rubble() {
        let mut w = mission_world("halden-3");
        let b = w.structures[0].block;
        w.hurt(Target::Structure(0), 1_000_000, 0);
        assert!(!w.structures[0].alive);
        assert_eq!(w.map.blocks[b].max.y, RUBBLE);
        assert_eq!(w.structures_lost(), 1);
        assert_eq!(w.kills, 0, "a lost structure is not a kill");
    }

    #[test]
    fn a_turret_fires_at_a_player_in_reach() {
        let mut w = mission_world("sere-2");
        let t = w.craft.iter().position(|c| c.kind == CraftKind::Turret).unwrap();
        let at = w.craft[t].pos;
        // hover in the open beside it
        w.mechs[0].body.pos = v3(at.x + int(40), at.y + int(10), at.z);
        w.mechs[0].body.grounded = false;
        let ap = w.mechs[0].ap;
        for _ in 0..600 {
            w.tick(Controls {
                ascend: true,
                ..Controls::default()
            });
        }
        assert!(w.mechs[0].ap < ap, "ten seconds beside the turrets did no damage");
    }

    #[test]
    fn burning_ground_costs_ap_and_a_pad_does_not() {
        let mut w = mission_world("cinder-1");
        let ap = w.mechs[0].ap;
        for _ in 0..60 {
            w.tick(Controls::default());
        }
        assert_eq!(w.mechs[0].ap, ap, "the start is on a pad");
        // find bare ground and stand on it
        let mut x = int(300);
        while w
            .map
            .box_blocked(v3(x - int(3), 0, int(697)), v3(x + int(3), int(8), int(703)))
        {
            x += int(10);
        }
        w.mechs[0].body.pos = v3(x, 0, int(700));
        let ap = w.mechs[0].ap;
        for _ in 0..60 {
            w.tick(Controls::default());
        }
        // 240 AP a second
        let lost = ap - w.mechs[0].ap;
        assert!((230..=250).contains(&lost), "lost {lost}");
    }
}
#[cfg(test)]
mod race_balance {
    use super::tests::mission_world;
    use super::*;

    /// Flies the player's frame round a race with the plain racer tree, the
    /// rival's own kind of pilot, and returns both finishing times in seconds.
    fn race(id: &str) -> (Option<u32>, Option<u32>) {
        let mut w = mission_world(id);
        let mut bot = crate::pilot::tests::pilots().pilot("racer", 9).unwrap();
        let n = w.mission.as_ref().unwrap().course.len();
        let mut mine = None;
        for t in 0..200 * 60u32 {
            let d = bot.think(&w.senses(0));
            w.mechs[0].body.aim_yaw = d.aim_yaw & (fx::TURN - 1);
            w.mechs[0].body.aim_pitch = d.aim_pitch.clamp(crate::mech::PITCH_MIN, crate::mech::PITCH_MAX);
            w.tick(d.controls);
            if mine.is_none() && w.mechs[0].course_next == n {
                mine = Some(t / 60);
            }
            // keep both racing to the line, whoever finishes first
            let m = w.mission.as_mut().unwrap();
            m.ended_at = None;
            m.success = None;
            if mine.is_some() && w.mechs[1].finished_at.is_some() {
                break;
            }
        }
        (mine, w.mechs[1].finished_at.map(|t| t / 60))
    }

    #[test]
    fn the_starting_frame_flown_plainly_beats_every_rival_and_can_reach_each_plus_time() {
        // Sam, 2026-10-08: "the race missions are like really difficult"
        let camp = crate::campaign::tests::campaign();
        for m in camp
            .planets
            .iter()
            .flat_map(|p| &p.missions)
            .filter(|m| m.kind == crate::mission::Kind::Race)
        {
            let (mine, rival) = race(&m.id);
            let (mine, rival) = (mine.expect(&m.id), rival.expect(&m.id));
            assert!(
                mine < rival,
                "{}: the starting frame took {mine} s, the rival {rival} s",
                m.id
            );
            assert!(
                (mine as i32) < m.plus.value,
                "{}: the plus asks for under {} s; a plain run took {mine}",
                m.id,
                m.plus.value
            );
        }
    }
}
