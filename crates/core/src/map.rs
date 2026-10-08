//! The ground and the buildings: a city grid made by a seeded generator, with
//! a bucket per city cell so a collision test looks at a handful of buildings
//! rather than all of them.

use crate::fx::{self, int, ONE};
use crate::geom::{v3, V3};
use crate::rng::Rng;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clearing {
    pub x_m: i32,
    pub z_m: i32,
    pub r_m: i32,
}

/// How to build a map. Metres throughout. x runs from −width/2 to width/2,
/// z from −length/2 to length/2.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapSpec {
    pub seed: u64,
    pub width_m: i32,
    pub length_m: i32,
    pub cell_m: i32,
    pub street_m: i32,
    pub empty_pct: i32,
    pub min_h_m: i32,
    pub max_h_m: i32,
    pub tall_pct: i32,
    pub tall_h_m: i32,
    pub ceiling_m: i32,
    #[serde(default)]
    pub clearings: Vec<Clearing>,
    /// width of a clear avenue down x = 0, the length of the map; 0 for none
    #[serde(default)]
    pub avenue_m: i32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub min: V3,
    pub max: V3,
    pub shade: u8,
}

impl Block {
    pub fn overlaps(&self, min: V3, max: V3) -> bool {
        min.x < self.max.x
            && max.x > self.min.x
            && min.y < self.max.y
            && max.y > self.min.y
            && min.z < self.max.z
            && max.z > self.min.z
    }

    pub fn contains(&self, p: V3) -> bool {
        p.x > self.min.x
            && p.x < self.max.x
            && p.y > self.min.y
            && p.y < self.max.y
            && p.z > self.min.z
            && p.z < self.max.z
    }

    /// Where the segment from `a` to `b` first enters the box, as a fraction
    /// of the segment in Q16, or `None` if it misses (slab method).
    pub fn segment_hit(&self, a: V3, b: V3) -> Option<i32> {
        let mut t0: i64 = 0;
        let mut t1: i64 = ONE as i64;
        let axes = [
            (a.x, b.x, self.min.x, self.max.x),
            (a.y, b.y, self.min.y, self.max.y),
            (a.z, b.z, self.min.z, self.max.z),
        ];
        for (p, q, lo, hi) in axes {
            let d = (q - p) as i64;
            if d == 0 {
                if p < lo || p > hi {
                    return None;
                }
                continue;
            }
            let mut ta = ((lo - p) as i64 * ONE as i64) / d;
            let mut tb = ((hi - p) as i64 * ONE as i64) / d;
            if ta > tb {
                std::mem::swap(&mut ta, &mut tb);
            }
            t0 = t0.max(ta);
            t1 = t1.min(tb);
            if t0 > t1 {
                return None;
            }
        }
        Some(t0 as i32)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map {
    pub half_x: i32,
    pub half_z: i32,
    pub ceiling: i32,
    pub blocks: Vec<Block>,
    cell: i32,
    cells_x: i32,
    cells_z: i32,
    buckets: Vec<Vec<u16>>,
}

impl Map {
    pub fn generate(spec: &MapSpec) -> Map {
        let mut rng = Rng::new(spec.seed);
        let cells_x = (spec.width_m / spec.cell_m).max(1);
        let cells_z = (spec.length_m / spec.cell_m).max(1);
        let mut blocks = Vec::new();
        let mut buckets = vec![Vec::new(); (cells_x * cells_z) as usize];
        let x0 = -spec.width_m / 2;
        let z0 = -spec.length_m / 2;
        let usable = spec.cell_m - spec.street_m;
        for cz in 0..cells_z {
            for cx in 0..cells_x {
                let left = x0 + cx * spec.cell_m + spec.street_m / 2;
                let front = z0 + cz * spec.cell_m + spec.street_m / 2;
                let (mx, mz) = (left + usable / 2, front + usable / 2);
                let cleared = spec.clearings.iter().any(|c| {
                    let (dx, dz) = ((mx - c.x_m) as i64, (mz - c.z_m) as i64);
                    let reach = (c.r_m + spec.cell_m * 3 / 4) as i64;
                    dx * dx + dz * dz < reach * reach
                });
                let on_avenue =
                    spec.avenue_m > 0 && left < spec.avenue_m / 2 && left + usable > -spec.avenue_m / 2;
                if cleared || on_avenue || rng.chance(spec.empty_pct) {
                    continue;
                }
                // One building, or two side by side on a split lot.
                let lots = if rng.chance(30) { 2 } else { 1 };
                let lot_w = usable / lots;
                for k in 0..lots {
                    let w = rng.range(lot_w / 2, lot_w - 2);
                    let d = rng.range(usable / 2, usable - 2);
                    let ox = left + k * lot_w + rng.range(0, lot_w - w);
                    let oz = front + rng.range(0, usable - d);
                    let h = if rng.chance(spec.tall_pct) {
                        rng.range(spec.max_h_m, spec.tall_h_m)
                    } else {
                        rng.range(spec.min_h_m, spec.max_h_m)
                    };
                    let idx = blocks.len() as u16;
                    blocks.push(Block {
                        min: v3(int(ox), 0, int(oz)),
                        max: v3(int(ox + w), int(h), int(oz + d)),
                        shade: rng.range(0, 255) as u8,
                    });
                    buckets[(cz * cells_x + cx) as usize].push(idx);
                }
            }
        }
        Map {
            half_x: int(spec.width_m / 2),
            half_z: int(spec.length_m / 2),
            ceiling: int(spec.ceiling_m),
            blocks,
            cell: int(spec.cell_m),
            cells_x,
            cells_z,
            buckets,
        }
    }

    /// Places one more building, for a structure a mission sets by hand. It
    /// is filed under every cell it covers, so a query may meet it twice;
    /// every query here is a min, a max or an "any", which a repeat cannot change.
    pub fn add_block(&mut self, b: Block) {
        let (ax, az) = self.cell_of(b.min.x, b.min.z);
        let (bx, bz) = self.cell_of(b.max.x, b.max.z);
        let idx = self.blocks.len() as u16;
        self.blocks.push(b);
        for cz in az..=bz {
            for cx in ax..=bx {
                self.buckets[(cz * self.cells_x + cx) as usize].push(idx);
            }
        }
    }

    fn cell_of(&self, x: i32, z: i32) -> (i32, i32) {
        let cx = ((x + self.half_x) as i64 / self.cell as i64) as i32;
        let cz = ((z + self.half_z) as i64 / self.cell as i64) as i32;
        (cx.clamp(0, self.cells_x - 1), cz.clamp(0, self.cells_z - 1))
    }

    /// Calls `f` with every building whose city cell the box `min..max` touches.
    pub fn each_near(&self, min: V3, max: V3, mut f: impl FnMut(&Block)) {
        let (ax, az) = self.cell_of(min.x, min.z);
        let (bx, bz) = self.cell_of(max.x, max.z);
        for cz in az..=bz {
            for cx in ax..=bx {
                for &i in &self.buckets[(cz * self.cells_x + cx) as usize] {
                    f(&self.blocks[i as usize]);
                }
            }
        }
    }

    /// The first point a segment hits a building or the ground, as a fraction
    /// of the segment in Q16.
    pub fn segment_hit(&self, a: V3, b: V3) -> Option<i32> {
        let min = v3(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z));
        let max = v3(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z));
        let mut best: Option<i32> = None;
        if b.y < 0 && a.y >= 0 {
            best = Some(fx::div(a.y, a.y - b.y));
        }
        self.each_near(min, max, |blk| {
            if let Some(t) = blk.segment_hit(a, b) {
                best = Some(best.map_or(t, |bt| bt.min(t)));
            }
        });
        best
    }

    /// Whether nothing stands between two points.
    pub fn clear_line(&self, a: V3, b: V3) -> bool {
        self.segment_hit(a, b).is_none()
    }

    /// The height of the floor under a point: a roof below it, or the ground.
    pub fn floor_under(&self, p: V3) -> i32 {
        let mut floor = 0;
        self.each_near(p, p, |blk| {
            if p.x >= blk.min.x
                && p.x <= blk.max.x
                && p.z >= blk.min.z
                && p.z <= blk.max.z
                && blk.max.y <= p.y + ONE
            {
                floor = floor.max(blk.max.y);
            }
        });
        floor
    }

    /// Whether a box overlaps any building.
    pub fn box_blocked(&self, min: V3, max: V3) -> bool {
        let mut hit = false;
        self.each_near(min, max, |blk| hit |= blk.overlaps(min, max));
        hit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn spec() -> MapSpec {
        MapSpec {
            seed: 3,
            width_m: 1000,
            length_m: 1000,
            cell_m: 80,
            street_m: 22,
            empty_pct: 20,
            min_h_m: 12,
            max_h_m: 60,
            tall_pct: 10,
            tall_h_m: 120,
            ceiling_m: 400,
            clearings: vec![Clearing {
                x_m: 0,
                z_m: 0,
                r_m: 60,
            }],
            avenue_m: 0,
        }
    }

    #[test]
    fn an_avenue_runs_the_length_of_the_map_with_no_building_on_it() {
        let m = Map::generate(&MapSpec {
            avenue_m: 60,
            ..spec()
        });
        let half = int(30);
        assert!(!m.box_blocked(v3(-half, 0, -m.half_z), v3(half, int(300), m.half_z)));
        assert!(m.blocks.len() > 40);
    }
    #[test]
    fn the_same_seed_builds_the_same_city() {
        assert_eq!(Map::generate(&spec()), Map::generate(&spec()));
        let other = MapSpec { seed: 4, ..spec() };
        assert_ne!(Map::generate(&spec()).blocks, Map::generate(&other).blocks);
    }

    #[test]
    fn a_clearing_has_no_building_in_it() {
        let m = Map::generate(&spec());
        assert!(m.blocks.len() > 50, "{} buildings", m.blocks.len());
        let r = int(60);
        assert!(!m.box_blocked(v3(-r, 0, -r), v3(r, int(200), r)));
    }

    #[test]
    fn buildings_stay_inside_the_map_and_in_their_own_cell() {
        let m = Map::generate(&spec());
        for (i, b) in m.blocks.iter().enumerate() {
            assert!(b.min.x >= -m.half_x && b.max.x <= m.half_x);
            assert!(b.min.z >= -m.half_z && b.max.z <= m.half_z);
            let mut found = 0;
            m.each_near(b.min, b.max, |n| found += (n == b) as i32);
            assert_eq!(found, 1, "building {i} is found once by a query over itself");
        }
    }

    #[test]
    fn a_segment_through_a_building_hits_its_near_face() {
        let blk = Block {
            min: v3(int(10), 0, int(-5)),
            max: v3(int(20), int(30), int(5)),
            shade: 0,
        };
        let t = blk
            .segment_hit(v3(0, int(10), 0), v3(int(40), int(10), 0))
            .unwrap();
        assert!((t - ONE / 4).abs() < 4, "{t}");
        assert!(blk
            .segment_hit(v3(0, int(40), 0), v3(int(40), int(40), 0))
            .is_none());
    }

    #[test]
    fn the_ground_stops_a_falling_segment() {
        let m = Map::generate(&spec());
        let t = m.segment_hit(v3(0, int(10), 0), v3(0, int(-10), 0)).unwrap();
        assert_eq!(t, ONE / 2);
    }

    #[test]
    fn the_floor_under_a_roof_is_the_roof() {
        let m = Map::generate(&spec());
        let b = m.blocks[0];
        let mid = v3((b.min.x + b.max.x) / 2, b.max.y + int(5), (b.min.z + b.max.z) / 2);
        assert_eq!(m.floor_under(mid), b.max.y);
        assert_eq!(m.floor_under(v3(0, int(5), 0)), 0);
    }
}
