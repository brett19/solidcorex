// Written for solidcorex. Apache-2.0.
//! The planar generators: blocks, prisms, sheets, a holed slab, a block with a cavity, a
//! stack of blocks and a checkerboard of them. Every face is a plane bounded by straight edges over `[0, 1]`.

use std::collections::BTreeMap;
use std::f64::consts::TAU;

use scx_brep::Draft;
use scx_brep::{RegionId, ShellId, ShellRecord, VertexId};
use scx_num::fmath;
use scx_num::frame::Sense;

use super::mesh::{Builder, Ed};
use super::{Dice, GenError, Place, Result};

/// The straight edges added so far, by their ends in id order.
#[derive(Default)]
struct Lines(BTreeMap<(VertexId, VertexId), Ed>);

impl Lines {
    /// The edge from `a` to `b`, added the first time, and the sense of a use from `a`.
    fn get(&mut self, b: &mut Builder, a: VertexId, c: VertexId) -> Result<(Ed, Sense)> {
        let key = (a.min(c), a.max(c));
        let sense = if a < c {
            Sense::Forward
        } else {
            Sense::Reversed
        };
        if let Some(e) = self.0.get(&key) {
            return Ok((e.clone(), sense));
        }
        let e = b.line(key.0, key.1)?;
        self.0.insert(key, e.clone());
        Ok((e, sense))
    }
}

/// Adds the planar face bounded by `rings`.
fn polygon(
    b: &mut Builder,
    lines: &mut Lines,
    sides: [ShellId; 2],
    rings: &[Vec<VertexId>],
) -> Result<()> {
    b.polygon(sides, rings, |b, p, q| lines.get(b, p, q))?;
    Ok(())
}

/// `n + 1` breakpoints from 0 to `length`, the inner ones moved up to 0.3 of a cell.
fn breaks(dice: &mut Dice, n: usize, length: f64) -> Vec<f64> {
    (0..=n)
        .map(|i| {
            let j = if i == 0 || i == n {
                0.0
            } else {
                dice.jitter(0.3)
            };
            length * (i as f64 + j) / n as f64
        })
        .collect()
}

/// A box over the lattice `axes` (breakpoints along x, y, z, from `at`), each face cell a
/// planar quad; fronts outward, or inward when `inward`.
fn lattice_box(
    b: &mut Builder,
    lines: &mut Lines,
    place: &Place,
    at: [f64; 3],
    axes: &[Vec<f64>; 3],
    sides: [ShellId; 2],
    inward: bool,
) -> Result<()> {
    let n = [0, 1, 2].map(|a| axes.get(a).map_or(0, |v| v.len().saturating_sub(1)));
    let mut vertices: BTreeMap<[usize; 3], VertexId> = BTreeMap::new();
    let mut vertex = |b: &mut Builder, idx: [usize; 3]| -> Result<VertexId> {
        if let Some(&v) = vertices.get(&idx) {
            return Ok(v);
        }
        let mut l = [0.0; 3];
        for (a, slot) in l.iter_mut().enumerate() {
            let (Some(axis), Some(&i)) = (axes.get(a), idx.get(a)) else {
                return Err(GenError::Degenerate("lattice"));
            };
            *slot = at.get(a).copied().unwrap_or(0.0)
                + axis
                    .get(i)
                    .copied()
                    .ok_or(GenError::Degenerate("lattice"))?;
        }
        let v = b.vertex(place.point(l))?;
        vertices.insert(idx, v);
        Ok(v)
    };
    for axis in 0..3 {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        let (nu, nv) = (
            n.get(u).copied().unwrap_or(0),
            n.get(v).copied().unwrap_or(0),
        );
        for side in [0, n.get(axis).copied().unwrap_or(0)] {
            for p in 0..nu {
                for q in 0..nv {
                    let mut ring = Vec::with_capacity(4);
                    // Counter-clockwise about +axis in (u, v).
                    for (du, dv) in [(0, 0), (1, 0), (1, 1), (0, 1)] {
                        let mut idx = [0; 3];
                        for (a, value) in [(axis, side), (u, p + du), (v, q + dv)] {
                            if let Some(slot) = idx.get_mut(a) {
                                *slot = value;
                            }
                        }
                        ring.push(vertex(b, idx)?);
                    }
                    if (side == 0) != inward {
                        ring.reverse();
                    }
                    polygon(b, lines, sides, &[ring])?;
                }
            }
        }
    }
    Ok(())
}

/// The block: `n x n` cells a face; seed 0 the unit cube at the origin.
pub(crate) fn block(dice: &mut Dice, n: usize) -> Result<Draft> {
    let place = Place {
        frame: dice.frame(),
    };
    let dims = [0; 3].map(|_| dice.range(0.5, 3.0, 1.0));
    let axes = dims.map(|l| breaks(dice, n, l));
    let mut b = Builder::new();
    let (outer, inner) = b.solid_shells()?;
    lattice_box(
        &mut b,
        &mut Lines::default(),
        &place,
        [0.0; 3],
        &axes,
        [outer, inner],
        false,
    )?;
    Ok(b.d)
}

/// The prism over a star-shaped polygon of `n + 3` sides; seed 0 the regular one of
/// circumradius 1 and height 1.
pub(crate) fn prism(dice: &mut Dice, n: usize) -> Result<Draft> {
    let place = Place {
        frame: dice.frame(),
    };
    let m = n + 3;
    let h = dice.range(0.5, 3.0, 1.0);
    let r = dice.range(0.5, 3.0, 1.0);
    let step = TAU / m as f64;
    let polygon_at = |dice: &mut Dice| -> Vec<[f64; 2]> {
        (0..m)
            .map(|j| {
                let phi = step * (j as f64 + dice.jitter(0.3));
                let rho = r * (1.0 + dice.jitter(0.3));
                [rho * fmath::cos(phi), rho * fmath::sin(phi)]
            })
            .collect()
    };
    let base = polygon_at(dice);
    let mut b = Builder::new();
    let (outer, inner) = b.solid_shells()?;
    let mut lines = Lines::default();
    let ring_at = |b: &mut Builder, z: f64| -> Result<Vec<VertexId>> {
        base.iter()
            .map(|&[x, y]| b.vertex(place.point([x, y, z])))
            .collect()
    };
    let bottom = ring_at(&mut b, 0.0)?;
    let top = ring_at(&mut b, h)?;
    let sides = [outer, inner];
    walls(&mut b, &mut lines, sides, &bottom, &top)?;
    let mut down = bottom;
    down.reverse();
    polygon(&mut b, &mut lines, sides, &[down])?;
    polygon(&mut b, &mut lines, sides, &[top])?;
    Ok(b.d)
}

/// The quads between two rings, `bottom[j], bottom[j + 1], top[j + 1], top[j]`: fronts
/// away from the axis when the rings run counter-clockwise about it.
fn walls(
    b: &mut Builder,
    lines: &mut Lines,
    sides: [ShellId; 2],
    bottom: &[VertexId],
    top: &[VertexId],
) -> Result<()> {
    let m = bottom.len();
    for j in 0..m {
        let k = (j + 1) % m;
        let ring = [bottom.get(j), bottom.get(k), top.get(k), top.get(j)]
            .into_iter()
            .map(|v| v.copied().ok_or(GenError::Degenerate("ring")))
            .collect::<Result<Vec<_>>>()?;
        polygon(b, lines, sides, &[ring])?;
    }
    Ok(())
}

/// The planar sheet of `n x n` quads (or `2 n^2` triangles), both sides the outside; seed
/// 0 the unit square, cells uniform; otherwise inner vertices moved up to a quarter cell
/// in the plane and each cell's diagonal drawn.
pub(crate) fn sheet(dice: &mut Dice, n: usize, triangles: bool) -> Result<Draft> {
    let place = Place {
        frame: dice.frame(),
    };
    let (w, h) = (dice.range(0.5, 3.0, 1.0), dice.range(0.5, 3.0, 1.0));
    let mut b = Builder::new();
    let outside = b.d.add_shell(ShellRecord::new(RegionId::OUTSIDE))?;
    let mut grid = Vec::with_capacity((n + 1) * (n + 1));
    for j in 0..=n {
        for i in 0..=n {
            let inner = i > 0 && i < n && j > 0 && j < n;
            let (ji, jj) = if inner {
                (dice.jitter(0.25), dice.jitter(0.25))
            } else {
                (0.0, 0.0)
            };
            let x = w * (i as f64 + ji) / n as f64;
            let y = h * (j as f64 + jj) / n as f64;
            grid.push(b.vertex(place.point([x, y, 0.0]))?);
        }
    }
    let at = |i: usize, j: usize| {
        grid.get(j * (n + 1) + i)
            .copied()
            .ok_or(GenError::Degenerate("grid"))
    };
    let mut lines = Lines::default();
    let sides = [outside, outside];
    for j in 0..n {
        for i in 0..n {
            let [c00, c10, c11, c01] = [at(i, j)?, at(i + 1, j)?, at(i + 1, j + 1)?, at(i, j + 1)?];
            if !triangles {
                polygon(&mut b, &mut lines, sides, &[vec![c00, c10, c11, c01]])?;
            } else if dice.range(0.0, 1.0, 0.0) < 0.5 {
                polygon(&mut b, &mut lines, sides, &[vec![c00, c10, c11]])?;
                polygon(&mut b, &mut lines, sides, &[vec![c00, c11, c01]])?;
            } else {
                polygon(&mut b, &mut lines, sides, &[vec![c00, c10, c01]])?;
                polygon(&mut b, &mut lines, sides, &[vec![c10, c11, c01]])?;
            }
        }
    }
    Ok(b.d)
}

/// The slab `[-w, w]^2 x [0, h]` with a through hole over a star-shaped polygon of `n + 3`
/// sides: top and bottom faces of two loops, the hole's walls facing its axis. Seed 0:
/// `w = 2`, `h = 1`, the hole regular of circumradius 1.
pub(crate) fn holed(dice: &mut Dice, n: usize) -> Result<Draft> {
    let place = Place {
        frame: dice.frame(),
    };
    let m = n + 3;
    let w = dice.range(2.0, 3.0, 2.0);
    let h = dice.range(0.5, 2.0, 1.0);
    let step = TAU / m as f64;
    let hole: Vec<[f64; 2]> = (0..m)
        .map(|j| {
            let phi = step * (j as f64 + dice.jitter(0.3));
            let rho = 1.0 + dice.jitter(0.3);
            [rho * fmath::cos(phi), rho * fmath::sin(phi)]
        })
        .collect();
    let square = [[-w, -w], [w, -w], [w, w], [-w, w]];
    let mut b = Builder::new();
    let (outer, inner) = b.solid_shells()?;
    let sides = [outer, inner];
    let mut lines = Lines::default();
    let ring = |b: &mut Builder, pts: &[[f64; 2]], z: f64| -> Result<Vec<VertexId>> {
        pts.iter()
            .map(|&[x, y]| b.vertex(place.point([x, y, z])))
            .collect()
    };
    let (sq0, sq1) = (ring(&mut b, &square, 0.0)?, ring(&mut b, &square, h)?);
    let (ho0, ho1) = (ring(&mut b, &hole, 0.0)?, ring(&mut b, &hole, h)?);
    walls(&mut b, &mut lines, sides, &sq0, &sq1)?;
    // The hole's walls face its axis: the rings reversed.
    let rev = |r: &[VertexId]| r.iter().rev().copied().collect::<Vec<_>>();
    walls(&mut b, &mut lines, sides, &rev(&ho0), &rev(&ho1))?;
    polygon(&mut b, &mut lines, sides, &[sq1, rev(&ho1)])?;
    polygon(&mut b, &mut lines, sides, &[rev(&sq0), ho0])?;
    Ok(b.d)
}

/// A block with a block-shaped void inside, each `n x n` cells a face: the solid region
/// has two shells, the void its own region and shell. Seed 0: `[0, 3]^3` round `[1, 2]^3`.
pub(crate) fn cavity(dice: &mut Dice, n: usize) -> Result<Draft> {
    let place = Place {
        frame: dice.frame(),
    };
    let outer_dims = [0; 3].map(|_| dice.range(3.0, 5.0, 3.0));
    let lo = [0; 3].map(|_| dice.range(0.8, 1.2, 1.0));
    let inner_dims = [0; 3].map(|_| dice.range(0.8, 1.2, 1.0));
    let outer_axes = outer_dims.map(|l| breaks(dice, n, l));
    let inner_axes = inner_dims.map(|l| breaks(dice, n, l));
    let mut b = Builder::new();
    let (outside, wall) = b.solid_shells()?;
    let solid =
        b.d.shell(wall)
            .map(|s| s.record().region)
            .ok_or(GenError::Degenerate("shell"))?;
    let lining = b.d.add_shell(ShellRecord::new(solid))?;
    let void = b.region_shell(false)?;
    let mut lines = Lines::default();
    lattice_box(
        &mut b,
        &mut lines,
        &place,
        [0.0; 3],
        &outer_axes,
        [outside, wall],
        false,
    )?;
    lattice_box(
        &mut b,
        &mut lines,
        &place,
        lo,
        &inner_axes,
        [void, lining],
        true,
    )?;
    Ok(b.d)
}

/// `n + 1` blocks stacked along z, each its own solid region and shell, neighbours sharing
/// a face: the shared faces' edges have three fins, put in radial order. Seed 0: unit
/// cubes.
pub(crate) fn stack(dice: &mut Dice, n: usize) -> Result<Draft> {
    let place = Place {
        frame: dice.frame(),
    };
    let (w, d) = (dice.range(0.5, 3.0, 1.0), dice.range(0.5, 3.0, 1.0));
    let mut z = vec![0.0];
    let mut top = 0.0;
    for _ in 0..=n {
        top += dice.range(0.5, 2.0, 1.0);
        z.push(top);
    }
    let mut b = Builder::new();
    let outside = b.d.add_shell(ShellRecord::new(RegionId::OUTSIDE))?;
    let shells = (0..=n)
        .map(|_| b.region_shell(true))
        .collect::<Result<Vec<_>>>()?;
    let square = [[0.0, 0.0], [w, 0.0], [w, d], [0.0, d]];
    let rings = z
        .iter()
        .map(|&h| {
            square
                .iter()
                .map(|&[x, y]| b.vertex(place.point([x, y, h])))
                .collect::<Result<Vec<_>>>()
        })
        .collect::<Result<Vec<_>>>()?;
    let mut lines = Lines::default();
    let shell = |t: usize| shells.get(t).copied().ok_or(GenError::Degenerate("shell"));
    let level = |t: usize| rings.get(t).cloned().ok_or(GenError::Degenerate("ring"));
    for t in 0..=n {
        walls(
            &mut b,
            &mut lines,
            [outside, shell(t)?],
            &level(t)?,
            &level(t + 1)?,
        )?;
    }
    let mut floor = level(0)?;
    floor.reverse();
    polygon(&mut b, &mut lines, [outside, shell(0)?], &[floor])?;
    polygon(&mut b, &mut lines, [outside, shell(n)?], &[level(n + 1)?])?;
    // Between block t below and t + 1 above: the front up, into t + 1.
    for t in 0..n {
        polygon(
            &mut b,
            &mut lines,
            [shell(t + 1)?, shell(t)?],
            &[level(t + 1)?],
        )?;
    }
    b.order_fins()?;
    Ok(b.d)
}

/// An `(n + 1) x (n + 1)` checkerboard of blocks one layer high, the black cells (`i + j`
/// even) filled: diagonal neighbours share the vertical edge at their common corner, so
/// each of the `n^2` inner lattice edges has four fins, put in radial order. Each block is
/// its own solid region and shell, or, when `one`, every block is one solid region of one
/// shell, the blocks touching along those edges: the sectors about such an edge are then
/// the solid and the outside by turns, so the sides say nothing of the order and only the
/// geometry fixes it. Seed 0: unit cells.
pub(crate) fn checker(dice: &mut Dice, n: usize, one: bool) -> Result<Draft> {
    lattice_blocks(dice, [n + 1, n + 1, 1], |[i, j, _]| (i + j) % 2 == 0, one)
}

/// Two unit-ish blocks that meet at one corner vertex and nothing else (a pinched vertex:
/// its link is two circles), each its own solid region and shell, or, when `one`, one solid
/// region of one shell. Seed 0: unit cells.
pub(crate) fn corner_touch(dice: &mut Dice, one: bool) -> Result<Draft> {
    lattice_blocks(dice, [2, 2, 2], |[i, j, k]| i == j && j == k, one)
}

/// The blocks of an `nx x ny x nz` lattice whose cell `keep` accepts, cells meeting at their
/// shared lattice vertices and edges (see [`checker`]).
fn lattice_blocks(
    dice: &mut Dice,
    dims: [usize; 3],
    keep: impl Fn([usize; 3]) -> bool,
    one: bool,
) -> Result<Draft> {
    let place = Place {
        frame: dice.frame(),
    };
    let [mx, my, mz] = dims;
    let (w, d) = (dice.range(0.5, 3.0, 1.0), dice.range(0.5, 3.0, 1.0));
    let h = dice.range(0.5, 3.0, 1.0);
    let axes = [
        breaks(dice, mx, w * mx as f64),
        breaks(dice, my, d * my as f64),
        if mz == 1 {
            vec![0.0, h]
        } else {
            breaks(dice, mz, h * mz as f64)
        },
    ];
    let mut b = Builder::new();
    let outside = b.d.add_shell(ShellRecord::new(RegionId::OUTSIDE))?;
    let welded = if one {
        Some(b.region_shell(true)?)
    } else {
        None
    };
    let mut vertices: BTreeMap<[usize; 3], VertexId> = BTreeMap::new();
    let mut lines = Lines::default();
    for (i, j, k) in
        (0..mx).flat_map(|i| (0..my).flat_map(move |j| (0..mz).map(move |k| (i, j, k))))
    {
        if !keep([i, j, k]) {
            continue;
        }
        let shell = match welded {
            Some(s) => s,
            None => b.region_shell(true)?,
        };
        let lo = [i, j, k];
        let mut corner = |b: &mut Builder, c: [usize; 3]| -> Result<VertexId> {
            let idx = [lo[0] + c[0], lo[1] + c[1], lo[2] + c[2]];
            if let Some(&v) = vertices.get(&idx) {
                return Ok(v);
            }
            let mut l = [0.0; 3];
            for (a, slot) in l.iter_mut().enumerate() {
                let at = idx.get(a).copied().unwrap_or(0);
                *slot = axes
                    .get(a)
                    .and_then(|x| x.get(at))
                    .copied()
                    .ok_or(GenError::Degenerate("checker"))?;
            }
            let v = b.vertex(place.point(l))?;
            vertices.insert(idx, v);
            Ok(v)
        };
        // The block's six faces, fronts outward, as `lattice_box` makes a cell's.
        for axis in 0..3 {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            for side in [0, 1] {
                let mut ring = Vec::with_capacity(4);
                for (du, dv) in [(0, 0), (1, 0), (1, 1), (0, 1)] {
                    let mut c = [0; 3];
                    for (a, value) in [(axis, side), (u, du), (v, dv)] {
                        if let Some(slot) = c.get_mut(a) {
                            *slot = value;
                        }
                    }
                    ring.push(corner(&mut b, c)?);
                }
                if side == 0 {
                    ring.reverse();
                }
                polygon(&mut b, &mut lines, [outside, shell], &[ring])?;
            }
        }
    }
    b.order_fins()?;
    Ok(b.d)
}
