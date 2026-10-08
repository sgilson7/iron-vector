//! The four shapes everything is drawn from. Each is a triangle list of
//! (position, normal) pairs in Q16, centred on the origin and one unit across,
//! so a box's model matrix carries its size.

use crate::fx::{self, ONE};
use crate::geom::{v3, V3};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mesh {
    Cube = 0,
    Wedge = 1,
    Octa = 2,
    Ground = 3,
    /// a globe, for planets and moons on the star map
    Sphere = 4,
}

impl Mesh {
    pub const ALL: [Mesh; 5] = [Mesh::Cube, Mesh::Wedge, Mesh::Octa, Mesh::Ground, Mesh::Sphere];

    pub fn from_name(s: &str) -> Option<Mesh> {
        match s {
            "cube" => Some(Mesh::Cube),
            "wedge" => Some(Mesh::Wedge),
            "octa" => Some(Mesh::Octa),
            _ => None,
        }
    }
}

/// Values per vertex: position xyz, normal xyz.
pub const VERTEX_VALUES: usize = 6;

const H: i32 = ONE / 2;

fn push_tri(out: &mut Vec<i32>, a: V3, b: V3, c: V3) {
    let n = b.sub(a).cross(c.sub(a)).norm();
    for p in [a, b, c] {
        out.extend_from_slice(&[p.x, p.y, p.z, n.x, n.y, n.z]);
    }
}

fn push_quad(out: &mut Vec<i32>, a: V3, b: V3, c: V3, d: V3) {
    push_tri(out, a, b, c);
    push_tri(out, a, c, d);
}

/// Vertex data of a mesh, counter-clockwise from outside.
pub fn vertices(mesh: Mesh) -> Vec<i32> {
    let mut o = Vec::new();
    match mesh {
        Mesh::Cube => {
            let p = |x: i32, y: i32, z: i32| v3(x * H, y * H, z * H);
            push_quad(&mut o, p(1, -1, 1), p(1, -1, -1), p(1, 1, -1), p(1, 1, 1));
            push_quad(&mut o, p(-1, -1, -1), p(-1, -1, 1), p(-1, 1, 1), p(-1, 1, -1));
            push_quad(&mut o, p(-1, 1, 1), p(1, 1, 1), p(1, 1, -1), p(-1, 1, -1));
            push_quad(&mut o, p(-1, -1, -1), p(1, -1, -1), p(1, -1, 1), p(-1, -1, 1));
            push_quad(&mut o, p(-1, -1, 1), p(1, -1, 1), p(1, 1, 1), p(-1, 1, 1));
            push_quad(&mut o, p(1, -1, -1), p(-1, -1, -1), p(-1, 1, -1), p(1, 1, -1));
        }
        Mesh::Wedge => {
            // Full at the back (+z) and bottom; the top slopes down to the
            // front (−z) edge, like an armour plate or a nose.
            let p = |x: i32, y: i32, z: i32| v3(x * H, y * H, z * H);
            push_quad(&mut o, p(-1, -1, 1), p(1, -1, 1), p(1, 1, 1), p(-1, 1, 1));
            push_quad(&mut o, p(-1, -1, -1), p(1, -1, -1), p(1, -1, 1), p(-1, -1, 1));
            push_quad(&mut o, p(-1, 1, 1), p(1, 1, 1), p(1, -1, -1), p(-1, -1, -1));
            push_tri(&mut o, p(1, -1, 1), p(1, -1, -1), p(1, 1, 1));
            push_tri(&mut o, p(-1, -1, -1), p(-1, -1, 1), p(-1, 1, 1));
        }
        Mesh::Octa => {
            let px = v3(H, 0, 0);
            let nx = v3(-H, 0, 0);
            let py = v3(0, H, 0);
            let ny = v3(0, -H, 0);
            let pz = v3(0, 0, H);
            let nz = v3(0, 0, -H);
            for (a, b) in [(px, pz), (pz, nx), (nx, nz), (nz, px)] {
                push_tri(&mut o, a, py, b);
                push_tri(&mut o, b, ny, a);
            }
        }
        Mesh::Ground => {
            let p = |x: i32, z: i32| v3(x * H, 0, z * H);
            push_quad(&mut o, p(-1, 1), p(1, 1), p(1, -1), p(-1, -1));
        }
        Mesh::Sphere => {
            // latitude and longitude bands; each vertex's normal is itself
            const LAT: i32 = 10;
            const LON: i32 = 16;
            let at = |i: i32, j: i32| {
                let lat = fx::HALF * i / LAT - fx::QUARTER;
                let lon = fx::TURN * j / LON;
                let r = fx::cos(lat);
                v3(
                    fx::mul(fx::mul(r, fx::cos(lon)), H),
                    fx::mul(fx::sin(lat), H),
                    fx::mul(fx::mul(r, -fx::sin(lon)), H),
                )
            };
            let mut tri = |a: V3, b: V3, c: V3| {
                for p in [a, b, c] {
                    let n = p.norm();
                    o.extend_from_slice(&[p.x, p.y, p.z, n.x, n.y, n.z]);
                }
            };
            for i in 0..LAT {
                for j in 0..LON {
                    let (a, b, c, d) = (at(i, j), at(i, j + 1), at(i + 1, j + 1), at(i + 1, j));
                    if i > 0 {
                        tri(a, b, c);
                    }
                    if i < LAT - 1 {
                        tri(a, c, d);
                    }
                }
            }
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tris(m: Mesh) -> Vec<[V3; 4]> {
        vertices(m)
            .chunks(3 * VERTEX_VALUES)
            .map(|t| {
                let v = |i: usize| v3(t[i], t[i + 1], t[i + 2]);
                [v(0), v(6), v(12), v(3)]
            })
            .collect()
    }

    #[test]
    fn every_face_of_a_closed_mesh_points_away_from_its_centre() {
        // The wedge's solid centre sits back and down, a third of the way.
        let wedge_centre = v3(0, -H / 3, H / 3);
        for (m, centre) in [
            (Mesh::Cube, V3::ZERO),
            (Mesh::Wedge, wedge_centre),
            (Mesh::Octa, V3::ZERO),
            (Mesh::Sphere, V3::ZERO),
        ] {
            for [a, b, c, n] in tris(m) {
                let centroid = v3(
                    (a.x + b.x + c.x) / 3,
                    (a.y + b.y + c.y) / 3,
                    (a.z + b.z + c.z) / 3,
                );
                assert!(
                    centroid.sub(centre).dot(n) > 0,
                    "{m:?}: face at {centroid:?} has normal {n:?}"
                );
            }
        }
    }

    #[test]
    fn the_cube_has_twelve_triangles_and_the_ground_faces_up() {
        assert_eq!(tris(Mesh::Cube).len(), 12);
        assert_eq!(tris(Mesh::Octa).len(), 8);
        for [_, _, _, n] in tris(Mesh::Ground) {
            assert_eq!(n, V3::UP);
        }
    }
}
