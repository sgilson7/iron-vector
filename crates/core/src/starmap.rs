//! The cockpit and the star map hologram above its console.
//!
//! The map is a Hasse diagram: each planet heads a column, in the order the
//! planets open, and its missions hang beneath it by level, a mission's
//! level being the length of its longest chain of requirements. Lines join
//! each mission to what it requires and each planet to the next. Planets are
//! drawn as globes in their own colours, with bands, rings and moons, so each
//! is told apart at a glance.

use crate::campaign::{Campaign, Progress, Standing};
use crate::content::{q16, Palette};
use crate::fx::{self, deg, ratio, ONE};
use crate::geom::{v3, Affine, V3};
use crate::mesh::Mesh;
use crate::render::{boxed, shade, Batch};

/// A centimetre, in Q16 metres.
const fn cm(n: i32) -> i32 {
    ratio(n, 100)
}

/// A point on the map: a planet, or one of its missions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Node {
    pub planet: usize,
    pub mission: Option<usize>,
    /// in the hologram's own space, metres
    pub at: V3,
}

/// Lines between nodes, by index; `true` for a gate between planets.
pub type Edge = (usize, usize, bool);

const COLUMN: i32 = cm(26);
const TOP: i32 = cm(22);
const ROW: i32 = cm(11);
const SPREAD: i32 = cm(8);

/// Every node and line of the diagram.
pub fn layout(c: &Campaign) -> (Vec<Node>, Vec<Edge>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let n = c.planets.len() as i32;
    let mut last_planet: Option<usize> = None;
    for (p, planet) in c.planets.iter().enumerate() {
        let x = COLUMN * (2 * p as i32 - (n - 1)) / 2;
        let z = if p % 2 == 0 { 0 } else { -cm(6) };
        let head = nodes.len();
        nodes.push(Node {
            planet: p,
            mission: None,
            at: v3(x, TOP, z),
        });
        if let Some(prev) = last_planet {
            edges.push((prev, head, true));
        }
        last_planet = Some(head);
        let levels: Vec<usize> = (0..planet.missions.len()).map(|i| c.level(p, i)).collect();
        let first = nodes.len();
        for (i, &lv) in levels.iter().enumerate() {
            let same: Vec<usize> = (0..levels.len()).filter(|j| levels[*j] == lv).collect();
            let k = same.iter().position(|j| *j == i).unwrap_or(0) as i32;
            let count = same.len() as i32;
            let mx = x + SPREAD * (2 * k - (count - 1)) / 2;
            nodes.push(Node {
                planet: p,
                mission: Some(i),
                at: v3(mx, TOP - ROW * (lv as i32 + 1), z),
            });
        }
        for (i, m) in planet.missions.iter().enumerate() {
            if m.requires.is_empty() {
                edges.push((head, first + i, false));
            }
            for r in &m.requires {
                if let Some(j) = planet.missions.iter().position(|e| &e.id == r) {
                    edges.push((first + j, first + i, false));
                }
            }
        }
    }
    (nodes, edges)
}

/// What the star map has picked out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    pub planet: usize,
    pub mission: Option<usize>,
    pub hover: Option<usize>,
}

/// How far the cockpit has come on: the lights, and the map's growth, each 0 to ONE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Power {
    pub light: i32,
    pub grow: i32,
    /// a turning clock for the planets and moons, in angle units
    pub clock: i32,
}

/// The cockpit around a viewer whose eye and head turn are `frame`, and the
/// map above the console. Returns where each node of `layout` is drawn.
pub fn push_cockpit(
    b: &mut Batch,
    frame: &Affine,
    power: Power,
    c: &Campaign,
    progress: &Progress,
    sel: &Selection,
    pal: &Palette,
) -> Vec<V3> {
    let at = |x: i32, y: i32, z: i32| v3(cm(x), cm(y), cm(z));
    let part = |b: &mut Batch, p: V3, s: V3, col: [i32; 3], glow: i32| {
        b.push_raw(Mesh::Cube, &frame.then(&boxed(p, s)), col, glow)
    };
    let shell = q16(pal.cockpit);
    let lamp = shade(q16(pal.cockpit_light), 30 + power.light * 70 / ONE);
    // lamps glow at half strength: full glow washed the console out to white
    let lit = power.light / 2;
    // console, side walls, roof and the canopy frame
    part(b, at(0, -66, -95), at(190, 26, 62), shell, 0);
    part(b, at(0, -54, -126), at(190, 3, 3), lamp, lit);
    part(b, at(-66, -52, -84), at(30, 1, 16), lamp, lit / 2);
    part(b, at(66, -52, -84), at(30, 1, 16), lamp, lit / 2);
    part(b, at(-20, -52, -110), at(16, 2, 16), q16(pal.hologram), lit);
    for s in [-1, 1] {
        part(b, at(118 * s, -10, -40), at(14, 120, 150), shell, 0);
        part(b, at(110 * s, 22, -40), at(2, 3, 130), lamp, lit);
        part(b, at(104 * s, 6, -100), at(8, 132, 8), shade(shell, 120), 0);
    }
    part(b, at(0, 78, -40), at(250, 10, 150), shell, 0);
    part(b, at(0, 70, -102), at(210, 8, 8), shade(shell, 120), 0);
    part(b, at(0, 71, -96), at(180, 2, 3), lamp, lit);
    // the map: grown from the emitter, turning gently
    let sway = fx::mul(fx::sin(power.clock / 6), deg(12));
    let holo = frame
        .then(&Affine::translate(at(-14, -2, -105)))
        .then(&Affine::rot_y(sway))
        .then(&Affine::rot_x(-deg(8)))
        .then(&Affine::scale(v3(ONE, ONE, ONE).scale(power.grow.max(1) * 6 / 5)));
    let (nodes, edges) = layout(c);
    let placed: Vec<V3> = nodes.iter().map(|n| holo.apply(n.at)).collect();
    if power.grow <= ONE / 50 {
        return placed;
    }
    let beam = q16(pal.hologram);

    for &(a, z, gate) in &edges {
        let (pa, pz) = (nodes[a].at, nodes[z].at);
        let d = pz.sub(pa);
        let open = |n: &Node| match n.mission {
            None => progress.planet_open(c, n.planet),
            Some(i) => progress.can_fly(c, &c.planets[n.planet].missions[i].id),
        };
        let lit_edge = open(&nodes[z]);
        let col = if lit_edge { beam } else { q16(pal.locked) };
        let thick = if gate { cm(1) / 2 } else { cm(1) / 3 };
        let line = Affine::translate(pa.add(d.scale(ONE / 2)))
            .then(&Affine::looking(d.norm()))
            .then(&Affine::scale(v3(thick, thick, d.len())));
        b.push_raw(
            Mesh::Cube,
            &holo.then(&line),
            col,
            if lit_edge { ONE * 3 / 4 } else { ONE / 5 },
        );
    }
    for (k, n) in nodes.iter().enumerate() {
        let planet = &c.planets[n.planet];
        let selected_planet = sel.planet == n.planet;
        let hovered = sel.hover == Some(k);
        match n.mission {
            None => {
                let star = &planet.star;
                let open = progress.planet_open(c, n.planet);
                let tone = if open { 100 } else { 35 };
                let r = cm(5) * star.size / 100 * if selected_planet || hovered { 13 } else { 10 } / 10;
                let spin =
                    Affine::translate(n.at).then(&Affine::rot_y(power.clock * (n.planet as i32 + 2) / 3));
                let glow = if star.glow > 0 { ONE / 2 } else { ONE / 6 };
                b.push_raw(
                    Mesh::Sphere,
                    &holo.then(&spin).then(&Affine::scale(v3(2 * r, 2 * r, 2 * r))),
                    shade(q16(star.color), tone),
                    glow,
                );
                let band = Affine::scale(v3(2 * r + cm(1) / 3, r * 2 / 5, 2 * r + cm(1) / 3));
                b.push_raw(
                    Mesh::Sphere,
                    &holo.then(&spin).then(&band),
                    shade(q16(star.band), tone),
                    glow,
                );
                if star.rings {
                    for s in 0..24 {
                        let a = fx::TURN * s / 24;
                        let p = v3(
                            fx::mul(r * 17 / 10, fx::cos(a)),
                            0,
                            fx::mul(r * 17 / 10, fx::sin(a)),
                        );
                        let seg = Affine::translate(n.at)
                            .then(&Affine::rot_z(deg(20)))
                            .then(&Affine::translate(p))
                            .then(&Affine::rot_y(-a))
                            .then(&Affine::scale(v3(cm(1) / 2, cm(1) / 5, r / 3)));
                        b.push_raw(Mesh::Cube, &holo.then(&seg), shade(q16(star.band), tone), ONE / 2);
                    }
                }
                for m in 0..star.moons {
                    let a = power.clock * 2 + fx::TURN * m / star.moons.max(1);
                    let p = n.at.add(v3(
                        fx::mul(r * 2, fx::cos(a)),
                        cm(1) * m,
                        fx::mul(r * 2, fx::sin(a)),
                    ));
                    b.push_raw(
                        Mesh::Sphere,
                        &holo.then(&boxed(p, v3(r / 3, r / 3, r / 3))),
                        shade([ONE, ONE, ONE], tone * 7 / 10),
                        ONE / 6,
                    );
                }
                if selected_planet {
                    for s in 0..12 {
                        let a = fx::TURN * s / 12 + power.clock;
                        let p = n.at.add(v3(
                            fx::mul(r * 26 / 10, fx::cos(a)),
                            0,
                            fx::mul(r * 26 / 10, fx::sin(a)),
                        ));
                        b.push_raw(
                            Mesh::Cube,
                            &holo.then(&boxed(p, v3(cm(1) / 2, cm(1) / 2, cm(1) / 2))),
                            beam,
                            ONE,
                        );
                    }
                }
            }
            Some(i) => {
                let standing = progress.standing(c, &planet.missions[i].id);
                let col = match standing {
                    Standing::Locked => pal.locked,
                    Standing::Open => pal.open,
                    Standing::Cleared => pal.cleared,
                    Standing::Plus => pal.plus,
                };
                let chosen = selected_planet && sel.mission == Some(i);
                let s = if chosen || hovered { cm(4) } else { cm(5) / 2 };
                let turn = Affine::translate(n.at).then(&Affine::rot_y(power.clock * 3));
                let glow = if standing == Standing::Locked {
                    ONE / 6
                } else {
                    ONE
                };
                b.push_raw(
                    Mesh::Octa,
                    &holo.then(&turn).then(&Affine::scale(v3(s, s, s))),
                    q16(col),
                    glow,
                );
                if standing == Standing::Plus {
                    b.push_raw(
                        Mesh::Cube,
                        &holo
                            .then(&turn)
                            .then(&Affine::scale(v3(s * 3 / 2, cm(1) / 4, s * 3 / 2))),
                        q16(pal.plus),
                        ONE,
                    );
                }
            }
        }
    }
    placed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::campaign::tests::campaign;

    #[test]
    fn every_planet_and_mission_is_a_node_and_lower_levels_hang_lower() {
        let c = campaign();
        let (nodes, edges) = layout(&c);
        assert_eq!(nodes.len(), c.planets.len() + c.total_missions());
        for n in nodes.iter().filter(|n| n.mission.is_some()) {
            let head = nodes
                .iter()
                .find(|h| h.planet == n.planet && h.mission.is_none())
                .unwrap();
            assert!(n.at.y < head.at.y, "a mission hangs below its planet");
        }
        // every requirement is a line from the higher node to the lower
        for &(a, z, gate) in &edges {
            if !gate {
                assert!(nodes[a].at.y > nodes[z].at.y, "edge {a}→{z} runs downward");
            }
        }
        assert_eq!(
            edges.iter().filter(|e| e.2).count(),
            c.planets.len() - 1,
            "one gate between each pair of planets"
        );
    }

    #[test]
    fn planets_run_left_to_right_in_the_order_they_open() {
        let c = campaign();
        let (nodes, _) = layout(&c);
        let xs: Vec<i32> = nodes
            .iter()
            .filter(|n| n.mission.is_none())
            .map(|n| n.at.x)
            .collect();
        assert!(xs.windows(2).all(|w| w[0] < w[1]), "{xs:?}");
    }
}
