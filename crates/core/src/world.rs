//! The world: mechs, craft, shots and effects, advanced one tick at a time.
//! `World::tick` is the only thing that changes it.

use crate::combat::{
    blast_damage, segment_sphere, Craft, CraftKind, Effect, EffectKind, Shot, Target, Team, Weapon,
    WeaponState,
};
use crate::content::Proving;
use crate::fx::{self, deg, int, ONE};
use crate::geom::{facing, v3, Affine, V3};
use crate::map::Map;
use crate::mech::{tuning_for, Body, Controls, Tuning, TICKS_PER_SECOND};
use crate::mission::{heli_weapon, HeliGun, Mission, MissionSpec};
use crate::model::Rig;
use crate::parts::{stats, Catalog, Loadout, Slot, Stats, WeaponKind};
use crate::pilot::{Act, Pilot, PilotSpec, Senses};
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

/// The five paints of a mech, in `Paint` order, as Q16 colours.
pub type Paints = [[i32; 3]; 5];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mech {
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
    pub intent: Option<Act>,
    /// impact taken toward a stagger, and ticks since the last hit
    pub impact: i32,
    pub impact_quiet: i32,
    /// ticks of stagger left
    pub stagger: i32,
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
    pub shots: Vec<Shot>,
    pub effects: Vec<Effect>,
    pub rng: Rng,
    pub tick: u32,
    pub kills: u32,
    pub mission: Option<Mission>,
    pub heli: Option<HeliGun>,
}

impl World {
    pub fn proving(cat: &Catalog, l: &Loadout, p: &Proving, paint: Paints) -> World {
        let map = Map::generate(&p.map);
        let spawn = v3(int(p.spawn[0]), 0, int(p.spawn[1]));
        let player = Mech::build(l, cat, spawn, deg(p.spawn_yaw_deg), Team::Player, paint);
        let mut rng = Rng::new(p.drone_seed);
        let respawn = p.drone_respawn_ms * TICKS_PER_SECOND / 1000;
        let craft = (0..p.drones)
            .map(|i| {
                let a = fx::TURN * i / p.drones.max(1) + rng.range(0, deg(20));
                let r = int(rng.range(140, 460));
                let mut at = spawn.add(facing(a, 0).scale(r));
                at.y = int(rng.range(18, 70));
                // a drone never hovers inside a building
                at.y = at.y.max(map.floor_under(v3(at.x, map.ceiling, at.z)) + int(12));
                drone(at, i, respawn)
            })
            .collect();
        World {
            map,
            mechs: vec![player],
            craft,
            shots: Vec::new(),
            effects: Vec::new(),
            rng,
            tick: 0,
            kills: 0,
            mission: None,
            heli: None,
        }
    }

    /// The mission's city, the player at its start, and the helicopters and
    /// the enemy frame held in reserve until their phase.
    pub fn mission(
        cat: &Catalog,
        l: &Loadout,
        paint: Paints,
        spec: &MissionSpec,
        pilot: &PilotSpec,
        enemy_paint: Paints,
    ) -> World {
        let map = Map::generate(&spec.map);
        let start = v3(int(spec.start[0]), 0, int(spec.start[1]));
        let player = Mech::build(l, cat, start, deg(spec.start_yaw_deg), Team::Player, paint);
        let enemy_loadout = Loadout::restore(
            &serde_json::to_string(&spec.enemy.loadout).unwrap_or_default(),
            cat,
        );
        let [ex, ey, ez] = spec.enemy.spawn;
        let mut boss = Mech::build(
            &enemy_loadout,
            cat,
            v3(int(ex), int(ey), int(ez)),
            0,
            Team::Enemy,
            enemy_paint,
        );
        boss.stats.ap = boss.stats.ap * spec.enemy.ap_pct / 100;
        boss.ap = boss.stats.ap;
        boss.body.grounded = false;
        boss.pilot = Some(Pilot::new(pilot.clone()));
        World {
            map,
            mechs: vec![player],
            craft: Vec::new(),
            shots: Vec::new(),
            effects: Vec::new(),
            rng: Rng::new(spec.map.seed ^ 0x5eed),
            tick: 0,
            kills: 0,
            mission: Some(Mission::new(spec, boss)),
            heli: Some(spec.heli.clone()),
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
        });
        let wall = |x0: i32, z0: i32, x1: i32, z1: i32, h: i32, shade: u8| crate::map::Block {
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
        World {
            map,
            mechs: vec![player],
            craft: Vec::new(),
            shots: Vec::new(),
            effects: Vec::new(),
            rng: Rng::new(1),
            tick: 0,
            kills: 0,
            mission: None,
            heli: None,
        }
    }

    pub fn player(&self) -> &Mech {
        &self.mechs[0]
    }

    /// The camera eye and the aim direction of the player, right now.
    pub fn player_view(&self) -> (V3, V3) {
        let m = self.player();
        let pivot = m.chest().add(v3(0, PIVOT_LIFT, 0));
        let eye = camera_eye(&self.map, pivot, m.body.aim_yaw, m.body.aim_pitch);
        (eye, facing(m.body.aim_yaw, m.body.aim_pitch))
    }

    /// The centre of a target, if it is still there.
    pub fn target_centre(&self, t: Target) -> Option<V3> {
        match t {
            Target::Mech(i) => self.mechs.get(i).filter(|m| m.alive).map(|m| m.chest()),
            Target::Craft(i) => self.craft.get(i).filter(|c| c.alive).map(|c| c.pos),
        }
    }

    fn target_vel(&self, t: Target) -> V3 {
        match t {
            Target::Mech(i) => self.mechs[i].body.vel,
            Target::Craft(i) => self.craft[i].vel,
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

    pub fn tick(&mut self, player: Controls) {
        self.tick = self.tick.wrapping_add(1);
        let (eye, dir) = self.player_view();
        self.mechs[0].aim_point = self.aim_ray(eye, dir);
        self.update_lock(eye, dir);
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
            self.fire(i, &c);
        }
        self.update_craft();
        self.update_shots();
        self.update_effects();
        self.update_mission();
    }

    /// What enemy mech `i` can see, as its pilot is shown it.
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
            .map(|s| s.pos.dist(my_chest))
            .min();
        Senses {
            me: my_chest,
            my_vel: me.body.vel,
            en_pct: me.body.en_pct(&me.tuning),
            en_locked: me.body.en_locked,
            glide: me.body.glide,
            staggered: me.stagger > 0,
            target: target.chest(),
            target_vel: target.body.vel,
            can_see: target.alive && self.map.clear_line(my_chest, target.chest()),
            missile,
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
        m.intent = Some(d.act);
        m.body.aim_yaw = d.aim_yaw & (fx::TURN - 1);
        m.body.aim_pitch = d.aim_pitch.clamp(crate::mech::PITCH_MIN, crate::mech::PITCH_MAX);
        // shots go where the pilot looks, error and all; only missiles home
        m.aim_point =
            s.me.add(facing(m.body.aim_yaw, m.body.aim_pitch).scale(s.target.dist(s.me)));
        m.lock = (s.can_see && s.target.dist(s.me) <= range).then_some(Target::Mech(0));
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
            let Some(c) = self.target_centre(t) else {
                continue;
            };
            let to = c.sub(eye);
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
        for k in 0..3 {
            let st = &mut self.mechs[i].wstate[k];
            st.cooldown = (st.cooldown - 1).max(0);
            if !c.fire[k] || st.cooldown > 0 || st.ammo <= 0 {
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
                    c.add(self.target_vel(m.lock.unwrap()).scale(int(flight)))
                }
                (None, _) => m.aim_point,
            };
            let shots = Shot::fire(&w, m.team, muzzle, aim, m.lock, &mut self.rng);
            let st = &mut self.mechs[i].wstate[k];
            st.cooldown = w.fire_ticks;
            // a missile salvo spends a missile each; any other weapon one round a pull
            st.ammo -= if w.kind == WeaponKind::Missile {
                shots.len() as i32
            } else {
                1
            };
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

    fn update_craft(&mut self) {
        let player = self.player();
        let (target, target_vel, target_alive) = (player.chest(), player.body.vel, player.alive);
        let gun = self.heli.clone();
        let mut volleys: Vec<(V3, V3)> = Vec::new();
        for c in &mut self.craft {
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
            match (c.kind, &gun) {
                (CraftKind::Heli, Some(g)) => {
                    // circle the anchor at about 20 m/s, and face the player when in reach
                    let step = fx::TURN as i64 * 20 / (377 * g.orbit_m.max(1) as i64);
                    c.phase = (c.phase + step as i32) & (fx::TURN - 1);
                    let orbit = facing(c.phase, 0).scale(int(g.orbit_m));
                    let bob = fx::mul(fx::sin(c.phase * 2), int(4));
                    let next = c.anchor.add(orbit).add(v3(0, bob, 0));
                    c.vel = next.sub(c.pos);
                    c.pos = next;
                    let to = target.sub(c.pos);
                    let in_reach = target_alive && to.len() < int(g.range_m);
                    c.yaw = if in_reach {
                        crate::geom::yaw_pitch_of(to).0
                    } else {
                        c.phase + fx::QUARTER
                    };
                    c.cooldown -= 1;
                    if in_reach && c.cooldown <= 0 && self.map.clear_line(c.pos, target) {
                        if c.burst <= 0 {
                            c.burst = g.burst;
                        }
                        let muzzle = c.pos.add(facing(c.yaw, 0).scale(int(4))).sub(v3(0, int(2), 0));
                        let flight =
                            fx::div(target.dist(muzzle), fx::ratio(g.speed_ms, TICKS_PER_SECOND)) / ONE;
                        volleys.push((muzzle, target.add(target_vel.scale(int(flight)))));
                        c.burst -= 1;
                        c.cooldown = if c.burst > 0 {
                            g.burst_gap_ms * TICKS_PER_SECOND / 1000
                        } else {
                            g.reload_ms * TICKS_PER_SECOND / 1000
                        };
                    }
                }
                _ => {
                    // practice drones circle their anchor slowly and bob
                    c.phase = (c.phase + deg(1) / 3) & (fx::TURN - 1);
                    let orbit = facing(c.phase, 0).scale(int(14));
                    let bob = fx::mul(fx::sin(c.phase * 3), int(3));
                    let next = c.anchor.add(orbit).add(v3(0, bob, 0));
                    c.vel = next.sub(c.pos);
                    c.pos = next;
                    c.yaw = c.phase + fx::QUARTER;
                }
            }
        }
        if let Some(g) = &gun {
            let w = heli_weapon(g);
            for (muzzle, aim) in volleys {
                let shots = Shot::fire(&w, Team::Enemy, muzzle, aim, None, &mut self.rng);
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
    }

    fn update_shots(&mut self) {
        let shots = std::mem::take(&mut self.shots);
        let mut kept = Vec::with_capacity(shots.len());
        for mut s in shots {
            let goal = s.target.and_then(|t| self.target_centre(t));
            s.steer(goal);
            s.prev_pos = s.pos;
            let next = s.pos.add(s.vel);
            let mut hit: Option<(i32, Option<Target>)> = self.map.segment_hit(s.pos, next).map(|t| (t, None));
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
                        if let Some(t) = segment_sphere(s.pos, next, c.pos, c.radius()) {
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
                    s.pos = next;
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
                if let Some(c) = self.target_centre(t) {
                    let d = blast_damage(s.damage_now(), c.dist(s.pos), s.blast);
                    if d > 0 {
                        let i = blast_damage(s.impact, c.dist(s.pos), s.blast);
                        self.hurt(t, d, i);
                    }
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
        let craft: Vec<Target> = if team == Team::Player {
            self.craft
                .iter()
                .enumerate()
                .filter(|(_, c)| c.alive)
                .map(|(i, _)| Target::Craft(i))
                .collect()
        } else {
            Vec::new()
        };
        mechs.chain(craft).collect()
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
                m.hit_flash = 4;
                m.impact_quiet = 0;
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
        };
        if destroyed {
            let hostile = match t {
                Target::Craft(_) => true,
                Target::Mech(i) => self.mechs[i].team == Team::Enemy,
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

fn drone(at: V3, i: i32, respawn: i32) -> Craft {
    Craft {
        kind: CraftKind::Drone,
        pos: at,
        prev_pos: at,
        vel: V3::ZERO,
        yaw: 0,
        prev_yaw: 0,
        ap: 700,
        max_ap: 700,
        alive: true,
        anchor: at,
        phase: deg(37) * i,
        cooldown: 0,
        burst: 0,
        respawn,
        down_ticks: 0,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::content::tests::{missions, palette};
    use crate::parts::tests::catalog;

    pub fn world() -> World {
        let c = catalog();
        World::proving(
            &c,
            &c.default_loadout(),
            &missions().proving,
            palette().scheme(0).paints(),
        )
    }

    #[test]
    fn the_proving_ground_starts_with_its_drones_clear_of_buildings() {
        let w = world();
        assert_eq!(w.craft.len(), 12);
        for c in &w.craft {
            let r = c.radius();
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
        let mut a = world();
        let mut b = world();
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
        assert!(a.mechs[0].body.pos != V3::ZERO);
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
        // put drone 0 in open air straight ahead and hold it still
        let at = w.mechs[0].body.pos.add(v3(0, int(30), -int(60)));
        w.craft[0].anchor = at;
        w.craft[0].pos = at;
        for _ in 0..3 {
            let p = w.craft[0].pos;
            aim_at(&mut w, p);
            w.tick(Controls::default());
        }
        assert_eq!(w.mechs[0].lock, Some(Target::Craft(0)));
        let ap = w.craft[0].ap;
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
        assert!(
            !w.craft[0].alive,
            "the drone went from {ap} to {} AP",
            w.craft[0].ap
        );
        assert_eq!(w.kills, 1);
    }

    #[test]
    fn a_drone_behind_a_building_is_not_locked() {
        let mut w = world();
        let at = w.mechs[0].body.pos.add(v3(0, int(10), -int(80)));
        w.craft[0].anchor = at;
        w.craft[0].pos = at;
        w.map.add_block(crate::map::Block {
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
        // a staggered frame ignores its controls
        let before = w.mechs[0].body.pos;
        w.tick(Controls {
            move_z: ONE,
            ..Controls::default()
        });
        assert_eq!(w.mechs[0].body.pos.z, before.z);
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
        let blk = crate::map::Block {
            min: v3(-int(20), 0, int(6)),
            max: v3(int(20), int(60), int(30)),
            shade: 0,
        };
        w.map.add_block(blk);
        let (eye, _) = w.player_view();
        assert!(eye.z < blk.min.z, "eye at {:?}", eye);
    }
}
