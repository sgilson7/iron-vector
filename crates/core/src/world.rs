//! The world: mechs, craft, structures, shots and effects, advanced one tick
//! at a time. `World::tick` is the only thing that changes it.

use crate::campaign::Planet;
use crate::combat::{
    blast_damage, segment_sphere, Craft, CraftKind, Effect, EffectKind, Shot, Target, Team, Weapon,
    WeaponState,
};
use crate::content::Proving;
use crate::fx::{self, deg, int, ONE};
use crate::geom::{facing, v3, Affine, V3};
use crate::map::{Block, Map};
use crate::mech::{tuning_for, Body, Controls, Tuning, TICKS_PER_SECOND};
use crate::mission::{make_unit, Gun, Mission, MissionSpec, Units};
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
}

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
        }
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

    pub fn chest(&self) -> V3 {
        self.body.pos.add(self.rig.chest())
    }

    /// The two spheres a shot can hit: chest and hips.
    pub fn spheres(&self) -> [(V3, i32); 2] {
        let c = self.chest();
        [(c, int(3)), (v3(c.x, c.y - int(3), c.z), int(2) + ONE / 2)]
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
    }
}

/// A box standing on the ground, centred on `at` (m).
fn standing(at: [i32; 2], size: [i32; 3], shade: u8) -> Block {
    let [x, z] = at;
    let [w, d, h] = size;
    Block {
        min: v3(int(x) - int(w) / 2, 0, int(z) - int(d) / 2),
        max: v3(int(x) + int(w) / 2, int(h), int(z) + int(d) / 2),
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
    ) -> World {
        let mut map = Map::generate(&planet.map);
        map.climate = planet.climate;
        for pad in &spec.pads {
            map.add_block(standing(pad.at, pad.size, 2));
        }
        let mut structures = Vec::new();
        for s in &spec.protect {
            structures.push(Structure {
                block: map.blocks.len(),
                ap: s.ap,
                max_ap: s.ap,
                alive: true,
            });
            map.add_block(standing(s.at, s.size, 255));
        }
        let on_top = |m: &Map, x: i32, z: i32| m.floor_under(v3(int(x), m.ceiling, int(z)));
        let [sx, sz, syaw] = spec.start;
        let start = v3(int(sx), on_top(&map, sx, sz), int(sz));
        let player = Mech::build(l, cat, start, deg(syaw), Team::Player, paint);
        let mut w = empty_world(map, player, planet.map.seed ^ 0x5eed);
        w.structures = structures;
        let mut mission = Mission::new(spec);
        for (k, u) in spec.units.iter().enumerate() {
            let [x, z, alt] = u.at;
            let ground = on_top(&w.map, x, z);
            let y = if alt < 0 {
                ground + int(2)
            } else {
                int(alt).max(ground)
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
            let mut m = Mech::build(&loadout, cat, pos, deg(yaw), team, enemy_paint);
            m.name = ms.name.clone();
            m.stats.ap = m.stats.ap * ms.ap_pct / 100;
            m.ap = m.stats.ap;
            m.pilot = pilots.pilot(&ms.pilot, k as u64 + 1);
            if ms.wave == 0 {
                w.mechs.push(m);
            } else {
                mission.reserve_mechs.push((ms.wave, m));
            }
        }
        w.mission = Some(mission);
        w
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
                // a staggered frame drops whatever it was doing
                c = Controls::default();
                m.stagger -= 1;
            }
            m.impact_quiet += 1;
            if m.impact_quiet > IMPACT_QUIET_TICKS {
                m.impact = (m.impact - m.stats.stability / 150 - 1).max(0);
            }
            m.body.step(&c, &m.tuning, &self.map);
            m.hit_flash = (m.hit_flash - 1).max(0);
            if burning > 0 && m.body.grounded && m.body.pos.y == 0 {
                m.burn += burning;
                let owed = m.burn / TICKS_PER_SECOND;
                m.burn %= TICKS_PER_SECOND;
                if owed > 0 {
                    self.hurt(Target::Mech(i), owed, 0);
                }
            }
            self.fire(i, &c);
        }
        self.update_craft();
        self.update_shots();
        self.update_effects();
        self.update_mission();
    }

    /// What mech `i` can see, as its pilot is shown it.
    fn senses(&self, i: usize) -> Senses {
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
        let to_course = self
            .mission
            .as_ref()
            .and_then(|m| m.course.get(me.course_next))
            .map(|c| self.map_delta(my_chest, *c));
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
                .root(m.body.pos, m.body.yaw)
                .apply(m.rig.muzzles(&m.body.pose())[k]);
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
            });
        }
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
                CraftKind::Turret => {}
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
