// Written for solidcorex. Apache-2.0.
//! The global solver against the pin's (targets `algo` and `algo_ic`): `global_solve::solve`
//! and `GlobalSolver::solve_it` over the same two trees of point pieces, with one local
//! solve shared by both sides (scx-difftest's `t_gsolve` builds the same scenarios and
//! checks the results agree).
//!
//! A scenario is two clouds of points, each point a cube of its own radius, halved into a
//! tree in the same node order on both sides; the trees are built before timing. A local
//! solve of a leaf pair is the distance of the two points (projected, for
//! `ProjectedMinimize`), added through each side's accumulator; for `Intersect` every pair
//! the culling keeps is a range. The cases, `algo/gsolve_<operation>`: `intersect`,
//! `minimize`, `minimize_best` (one best answer kept), `maximize` and `projected_minimize`
//! over trees of up to 9 and 12 leaves, and `minimize_large` over up to 64 and 96. The
//! results consumed are every solution's kind, values and parameters, the best value and
//! the leaf pairs solved, in order.

use crate::case::{Case, both};
use crate::{Rng, seeded};

use scx_num::vector3d::sqrt;

type P = [f64; 3];

/// One node of the build plan: its box, and its two children (plan indices) or the leaf's
/// piece.
#[derive(Clone, Debug)]
struct Plan {
    lo: P,
    hi: P,
    kids: Option<(usize, usize)>,
    piece: Option<usize>,
}

/// Two clouds and their plans.
#[derive(Clone, Debug)]
struct Scenario {
    pts: [Vec<P>; 2],
    plan: [Vec<Plan>; 2],
    tol: f64,
    /// A unit vector: the projection normal.
    n: P,
}

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn plan_of(pts: &[P], rad: &[f64]) -> Vec<Plan> {
    fn go(out: &mut Vec<Plan>, items: &[(usize, P, f64)]) -> usize {
        if let [(i, p, r)] = items {
            out.push(Plan {
                lo: [p[0] - r, p[1] - r, p[2] - r],
                hi: [p[0] + r, p[1] + r, p[2] + r],
                kids: None,
                piece: Some(*i),
            });
            return out.len() - 1;
        }
        let (l, h) = items.split_at(items.len() / 2);
        let a = go(out, l);
        let b = go(out, h);
        let (pa, pb) = (&out[a], &out[b]);
        let lo = [0, 1, 2].map(|k| pa.lo[k].min(pb.lo[k]));
        let hi = [0, 1, 2].map(|k| pa.hi[k].max(pb.hi[k]));
        out.push(Plan {
            lo,
            hi,
            kids: Some((a, b)),
            piece: None,
        });
        out.len() - 1
    }
    let items: Vec<(usize, P, f64)> = pts
        .iter()
        .zip(rad)
        .enumerate()
        .map(|(i, (p, r))| (i, *p, *r))
        .collect();
    let mut out = Vec::new();
    go(&mut out, &items);
    out
}

/// A scenario of up to `max` pieces in each tree.
fn scenario(r: &mut Rng, max: [usize; 2]) -> Scenario {
    let n1 = 1 + r.below(max[0]);
    let n2 = 1 + r.below(max[1]);
    let off = [r.range(-2.0, 6.0), r.range(-1.0, 1.0), r.range(-1.0, 1.0)];
    let cloud = |n: usize, o: P, r: &mut Rng| -> (Vec<P>, Vec<f64>) {
        let pts = (0..n)
            .map(|_| [0, 1, 2].map(|k| o[k] + r.range(0.0, 4.0)))
            .collect();
        let rad = (0..n).map(|_| r.range(0.0, 0.6)).collect();
        (pts, rad)
    };
    let (p1, r1) = cloud(n1, [0.0; 3], r);
    let (p2, r2) = cloud(n2, off, r);
    let tol = [0.0, 1e-3, 0.05][r.below(3)];
    let n: P = r.array(-1.0, 1.0);
    let len = sqrt(dot(n, n));
    let n = if len > 1e-3 {
        n.map(|x| x / len)
    } else {
        [0.0, 0.0, 1.0]
    };
    Scenario {
        plan: [plan_of(&p1, &r1), plan_of(&p2, &r2)],
        pts: [p1, p2],
        tol,
        n,
    }
}

/// The operations timed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Intersect,
    Minimize,
    MinimizeBest,
    Maximize,
    ProjectedMinimize,
}

/// The local solve of both sides: the solution value of the pair.
fn value(op: Op, sc: &Scenario, p: P, q: P) -> f64 {
    let v = sub(q, p);
    match op {
        Op::Intersect => 0.0,
        Op::Minimize | Op::MinimizeBest | Op::Maximize => sqrt(dot(v, v)),
        Op::ProjectedMinimize => {
            let k = dot(v, sc.n);
            let w = [0, 1, 2].map(|i| v[i] - k * sc.n[i]);
            sqrt(dot(w, w))
        }
    }
}

/// A solution: kind (1 point, 2 range), start value and two parameters, end value and
/// two parameters.
type Row = [f64; 7];
/// The solutions and the best value, then the leaf pairs solved, in order; `None` when
/// the solve failed.
type Found = Option<((Vec<Row>, f64), Vec<[usize; 2]>)>;

#[allow(clippy::cast_precision_loss)] // piece indices are small
const fn at(i: usize) -> f64 {
    i as f64 + 0.5
}

mod xs {
    use super::{Found, Op, Row, Scenario, at, value};
    use scx_algo::error::SolveError;
    use scx_algo::global_solve::{
        Accumulator, Branch, BranchSolver, Join, Keep, Next, Operation, SolveOptions, solve,
    };
    use scx_algo::solution::{Solution, SolutionKind, SolutionObject, SortKey};
    use scx_algo::split_tree::SplitTree;
    use scx_num::budget::{Budget, Stop};
    use scx_num::extent1d::Extent1d;
    use scx_num::extent3d::Extent3d;
    use scx_num::vector3d::{Point3, Vec3};

    /// A tree's curve of `n` pieces, parameter `k + 0.5` on piece `k`.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Piece {
        n: usize,
    }

    impl SolutionObject for Piece {
        fn parameter_count(&self) -> Option<usize> {
            Some(1)
        }
        #[allow(clippy::cast_precision_loss)] // piece counts are small
        fn parameter_interval(&self, axis: usize) -> Option<Extent1d> {
            if axis == 0 {
                Extent1d::spanning(0.0, self.n as f64)
            } else {
                None
            }
        }
        fn tolerance(&self) -> f64 {
            0.0
        }
    }

    pub type Tree = SplitTree<Option<usize>>;

    /// The tree of a plan: `None` if the plan is malformed.
    pub fn tree(plan: &[super::Plan]) -> Option<Tree> {
        let mut t = SplitTree::with_capacity(plan.len());
        let mut ids = Vec::with_capacity(plan.len());
        for n in plan {
            let b = Extent3d::spanning(
                Point3::new(n.lo[0], n.lo[1], n.lo[2]),
                Point3::new(n.hi[0], n.hi[1], n.hi[2]),
            )?;
            let id = match n.kids {
                None => t.leaf(b, n.piece),
                Some((a, c)) => t.split(b, None, [*ids.get(a)?, *ids.get(c)?]),
            };
            ids.push(id?);
        }
        Some(t)
    }

    struct Hooks<'s> {
        sc: &'s Scenario,
        op: Op,
        log: Vec<[usize; 2]>,
    }

    impl BranchSolver<Option<usize>, Piece, 2> for Hooks<'_> {
        type Error = ();
        fn local_solve(
            &mut self,
            branch: &Branch<'_, Option<usize>, 2>,
            found: &mut Accumulator<Piece>,
            _: &mut Budget,
        ) -> Result<Next, SolveError<()>> {
            let [Some(a), Some(b)] = branch.nodes().map(|n| *n.item()) else {
                return Ok(Next::Done);
            };
            self.log.push([a, b]);
            let sc = self.sc;
            let v = value(self.op, sc, sc.pts[0][a], sc.pts[1][b]);
            let (pa, pb) = (at(a), at(b));
            let objects = [
                Some(Piece { n: sc.pts[0].len() }),
                Some(Piece { n: sc.pts[1].len() }),
            ];
            let (sol, key) = if self.op == Op::Intersect {
                (
                    Solution::range((v, [pa, pb]), (v, [pa + 1.0, pb + 1.0])),
                    SortKey::ByAllParameters,
                )
            } else {
                (Solution::point(v, [pa, pb]), SortKey::BySolutionValue)
            };
            found.add_sorted(&sol.with_objects(objects), key, Join::Touching);
            Ok(Next::Done)
        }
    }

    pub fn run(sc: &Scenario, op: Op, t1: &Tree, t2: &Tree) -> Found {
        let operation = match op {
            Op::Intersect => Operation::Intersect,
            Op::Minimize | Op::MinimizeBest => Operation::Minimize,
            Op::Maximize => Operation::Maximize,
            Op::ProjectedMinimize => Operation::ProjectedMinimize {
                normal: Vec3::new(sc.n[0], sc.n[1], sc.n[2]),
            },
        };
        let mut options = SolveOptions::new(operation, sc.tol);
        if op == Op::MinimizeBest {
            options.keep = Keep::Best;
        }
        let mut hooks = Hooks {
            sc,
            op,
            log: Vec::new(),
        };
        let found = solve(
            [t1, t2],
            Accumulator::new(options),
            &mut hooks,
            &mut Budget::unlimited(),
            &Stop::new(),
        )
        .ok()?;
        let rows = found
            .solutions()
            .iter()
            .map(|s| -> Row {
                let (st, en) = (s.start(), s.end());
                match s.kind() {
                    SolutionKind::Range => [
                        2.0,
                        st.value,
                        st.param(0),
                        st.param(1),
                        en.value,
                        en.param(0),
                        en.param(1),
                    ],
                    _ => [1.0, st.value, st.param(0), st.param(1), 0.0, 0.0, 0.0],
                }
            })
            .collect();
        Some(((rows, found.best_sq()), hooks.log))
    }
}

mod ss {
    use super::{Found, Op, Row, Scenario, at, value};
    use solidcore::Result;
    use solidcore::geom::extent1d::Extent1d;
    use solidcore::geom::extent3d::Extent3d;
    use solidcore::geom::global_solver::{
        Branch, GlobalSolve, GlobalSolver, SolutionRequested, SolveRequest, SolverOperation,
    };
    use solidcore::geom::solution_array::{
        ObjectKind, Solution, SolutionArray, SolutionObject, SolutionType, SortKey,
    };
    use solidcore::geom::tree::{NodeId, Tree as PTree, TreeNode};
    use solidcore::math::{BIG_DOUBLE, Vec3};

    /// A tree's curve of `n` pieces.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Piece {
        n: usize,
    }

    impl SolutionObject for Piece {
        fn kind(&self) -> ObjectKind {
            ObjectKind::Curve
        }
        #[allow(clippy::cast_precision_loss)] // piece counts are small
        fn natural_interval(&self) -> Option<Extent1d> {
            Some(Extent1d::from_min_max(0.0, self.n as f64))
        }
    }

    pub type Tree = PTree<Piece>;

    /// The tree of a plan: `None` if the plan is malformed.
    pub fn tree(plan: &[super::Plan]) -> Option<Tree> {
        let mut t = PTree::new(None);
        let mut ids: Vec<NodeId> = Vec::with_capacity(plan.len());
        for n in plan {
            let mut node = TreeNode::new();
            node.bbox = Extent3d::from_points(
                Vec3::new(n.lo[0], n.lo[1], n.lo[2]),
                Vec3::new(n.hi[0], n.hi[1], n.hi[2]),
            );
            if let Some((a, c)) = n.kids {
                node.child1 = Some(*ids.get(a)?);
                node.child2 = Some(*ids.get(c)?);
            }
            ids.push(t.add_node(node));
        }
        t.set_top_node(ids.last().copied());
        Some(t)
    }

    struct Hooks<'t, 's> {
        base: GlobalSolver<'t, Piece>,
        sc: &'s Scenario,
        op: Op,
        log: Vec<[usize; 2]>,
    }

    impl<'t> GlobalSolve<'t, Piece> for Hooks<'t, '_> {
        fn solver(&self) -> &GlobalSolver<'t, Piece> {
            &self.base
        }
        fn solver_mut(&mut self) -> &mut GlobalSolver<'t, Piece> {
            &mut self.base
        }
        fn object_zone_tol_3d(&self, _: Piece) -> Result<f64> {
            Ok(0.0)
        }
        fn local_solve(&mut self, branch: &Branch) -> Result<bool> {
            let sc = self.sc;
            let piece = |tree: usize| {
                let node = branch[tree]?;
                sc.plan[tree].get(node.0)?.piece
            };
            let (Some(a), Some(b)) = (piece(0), piece(1)) else {
                return Ok(false);
            };
            self.log.push([a, b]);
            let v = value(self.op, sc, sc.pts[0][a], sc.pts[1][b]);
            let (pa, pb) = (at(a), at(b));
            let mut s = Solution::new();
            s.num_variables = 2;
            s.num_objects = 2;
            s.objects[0] = Some(Piece { n: sc.pts[0].len() });
            s.objects[1] = Some(Piece { n: sc.pts[1].len() });
            s.start.solution_value = v;
            s.start.parameters[0] = pa;
            s.start.parameters[1] = pb;
            let key = if self.op == Op::Intersect {
                s.solution_type = SolutionType::RangeOfValues;
                s.end.solution_value = v;
                s.end.parameters[0] = pa + 1.0;
                s.end.parameters[1] = pb + 1.0;
                SortKey::ByAllParameters
            } else {
                s.solution_type = SolutionType::SingleValue;
                SortKey::BySolutionValue
            };
            self.add_sorted_solution(&s, key, true)?;
            Ok(false)
        }
    }

    pub fn run(sc: &Scenario, op: Op, t1: &Tree, t2: &Tree) -> Found {
        let operation = match op {
            Op::Intersect => SolverOperation::Intersect,
            Op::Minimize | Op::MinimizeBest => SolverOperation::Minimize,
            Op::Maximize => SolverOperation::Maximize,
            Op::ProjectedMinimize => SolverOperation::ProjectedMinimize,
        };
        let solution_requested = if op == Op::MinimizeBest {
            SolutionRequested::Single
        } else {
            SolutionRequested::All
        };
        let mut hooks = Hooks {
            base: GlobalSolver::with_trees(t1, t2, 2),
            sc,
            op,
            log: Vec::new(),
        };
        let vectors = [Vec3::new(sc.n[0], sc.n[1], sc.n[2])];
        let request = SolveRequest {
            operation,
            solution_requested,
            tolerance_3d: sc.tol,
            best_answer_so_far_sq: BIG_DOUBLE,
            opt_vectors: &vectors,
        };
        let mut found = SolutionArray::new();
        hooks.solve_it(&request, &mut found).ok()?;
        // "no best yet": the pin's sentinel is scx's infinity
        let best = hooks.base.best_answer_so_far_sq;
        let best = if best == BIG_DOUBLE {
            f64::INFINITY
        } else {
            best
        };
        let rows = found
            .as_slice()
            .iter()
            .map(|s| -> Row {
                let (st, en) = (&s.start, &s.end);
                match s.solution_type {
                    SolutionType::RangeOfValues => [
                        2.0,
                        st.solution_value,
                        st.parameters[0],
                        st.parameters[1],
                        en.solution_value,
                        en.parameters[0],
                        en.parameters[1],
                    ],
                    _ => [
                        1.0,
                        st.solution_value,
                        st.parameters[0],
                        st.parameters[1],
                        0.0,
                        0.0,
                        0.0,
                    ],
                }
            })
            .collect();
        Some(((rows, best), hooks.log))
    }
}

/// The global solver's cases, appended to `out`.
pub fn cases(out: &mut Vec<Case>) {
    let small = seeded(61, |r| scenario(r, [9, 12]));
    let large = seeded(62, |r| scenario(r, [64, 96]));
    for (name, op, set) in [
        ("algo/gsolve_intersect", Op::Intersect, &small),
        ("algo/gsolve_minimize", Op::Minimize, &small),
        ("algo/gsolve_minimize_best", Op::MinimizeBest, &small),
        ("algo/gsolve_maximize", Op::Maximize, &small),
        (
            "algo/gsolve_projected_minimize",
            Op::ProjectedMinimize,
            &small,
        ),
        ("algo/gsolve_minimize_large", Op::Minimize, &large),
    ] {
        let inputs = both(
            set,
            |sc| Some((sc.clone(), xs::tree(&sc.plan[0])?, xs::tree(&sc.plan[1])?)),
            |sc| Some((sc.clone(), ss::tree(&sc.plan[0])?, ss::tree(&sc.plan[1])?)),
        );
        out.push(Case::map(
            name,
            inputs,
            move |(sc, t1, t2)| xs::run(sc, op, t1, t2),
            move |(sc, t1, t2)| ss::run(sc, op, t1, t2),
        ));
    }
}
