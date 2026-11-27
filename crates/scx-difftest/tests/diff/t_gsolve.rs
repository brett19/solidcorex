// Written for solidcorex. Apache-2.0.
//! `scx-algo`: `global_solve.rs` and `solution.rs`, against the pin's `GlobalSolver` and
//! `Solution`.
//!
//! A scenario, from two seeds, is two trees of point pieces (a point in a small cube), built
//! by halving in the same node order on both sides, so a node's index names the same piece.
//! Both sides' local solves are one function of the two points (`value`); each side adds its
//! solution through its own accumulator (`Accumulator::add_sorted`, the pin's
//! `add_sorted_solution`). The comparison is of the solutions (kind, value, parameters), the
//! best value and the order the leaf pairs were solved in, so the culling, the split choice
//! and the child order are compared too. "No best answer yet" is `None` for scx and
//! `SM_BIG_DOUBLE` for the pin, read back as infinity. `gsolve_truths` checks the answers
//! against brute force over every leaf pair.

use crate::harness::{SplitMix64, run, run_defect};
use scx_num::vector3d::sqrt;

type P = [f64; 3];

/// One node of the build plan: its box, its two children (plan indices) or the leaf's piece.
#[derive(Clone, Debug)]
pub struct Plan {
    lo: P,
    hi: P,
    kids: Option<(usize, usize)>,
    piece: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct Scenario {
    pts: [Vec<P>; 2],
    plan: [Vec<Plan>; 2],
    tol: f64,
    /// A unit vector: the projection normal, or the ray's direction.
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

/// The scenario of two seeds; `ray` makes every leaf of tree 1 the same point, the ray's
/// start (so its boxes' corners are the start, as the rayfire tests read them).
pub fn scenario(s1: u64, s2: u64, ray: bool) -> Scenario {
    scenario_sized(s1, s2, ray, [9, 12])
}

/// [`scenario`] with up to `max` pieces in each tree.
pub fn scenario_sized(s1: u64, s2: u64, ray: bool, max: [usize; 2]) -> Scenario {
    let mut rng = SplitMix64(s1 ^ s2.rotate_left(17) ^ 0x6A09_E667_F3BC_C908);
    let n1 = if ray {
        1 + rng.below(4)
    } else {
        1 + rng.below(max[0])
    };
    let n2 = 1 + rng.below(max[1]);
    let off = [
        rng.uniform(-2.0, 6.0),
        rng.uniform(-1.0, 1.0),
        rng.uniform(-1.0, 1.0),
    ];
    let cloud = |n: usize, o: P, rng: &mut SplitMix64| -> (Vec<P>, Vec<f64>) {
        let pts = (0..n)
            .map(|_| [0, 1, 2].map(|k| o[k] + rng.uniform(0.0, 4.0)))
            .collect();
        let rad = (0..n).map(|_| rng.uniform(0.0, 0.6)).collect();
        (pts, rad)
    };
    let (mut p1, mut r1) = cloud(n1, [0.0; 3], &mut rng);
    if ray {
        // every leaf of the ray tree is the ray's start, a point
        p1 = vec![p1[0]; n1];
        r1 = vec![0.0; n1];
    }
    let (p2, r2) = cloud(n2, off, &mut rng);
    let tol = [0.0, 1e-3, 0.05][rng.below(3)];
    let n = if ray {
        // toward the second cloud's middle, jittered
        let c = [0, 1, 2].map(|k| off[k] + 2.0 + rng.uniform(-1.5, 1.5));
        sub(c, p1[0])
    } else {
        [0, 1, 2].map(|_| rng.uniform(-1.0, 1.0))
    };
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

/// The operations compared.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Case {
    Intersect,
    IntersectNodes,
    Minimize,
    MinimizeBest,
    Maximize,
    ProjectedMinimize,
    ProjectedMaximize,
    Rayfire,
}

impl Case {
    const fn ray(self) -> bool {
        matches!(self, Self::Rayfire)
    }
}

/// The local solve of both sides: the solution value of the pair, or none.
fn value(case: Case, sc: &Scenario, p: P, q: P, rq: f64) -> Option<f64> {
    let v = sub(q, p);
    match case {
        Case::Intersect | Case::IntersectNodes => Some(0.0),
        Case::Minimize | Case::MinimizeBest | Case::Maximize => Some(sqrt(dot(v, v))),
        Case::ProjectedMinimize | Case::ProjectedMaximize => {
            let k = dot(v, sc.n);
            let w = [0, 1, 2].map(|i| v[i] - k * sc.n[i]);
            Some(sqrt(dot(w, w)))
        }
        Case::Rayfire => {
            // the ray parameter of the point, when the ray passes within its cube's radius
            let t = dot(v, sc.n);
            let miss = [0, 1, 2].map(|i| v[i] - t * sc.n[i]);
            (t >= 0.0 && sqrt(dot(miss, miss)) <= rq + sc.tol).then_some(t)
        }
    }
}

/// One solution: kind (0 nodes, 1 point, 2 range), start value and two parameters, end
/// value and two parameters (node indices for a nodes solution).
pub type Row = (usize, f64, f64, f64, f64, f64, f64);
/// The solutions, the best value, and the leaf pairs solved, in order.
pub type Res = Option<(Vec<Row>, f64, Vec<(usize, usize)>)>;

fn rad_of(plan: &[Plan], piece: usize) -> f64 {
    plan.iter()
        .find(|n| n.piece == Some(piece))
        .map_or(0.0, |n| n.hi[0] - (n.lo[0] + n.hi[0]) / 2.0)
}

pub mod xs {
    use super::{Case, Res, Row, Scenario, rad_of, value};
    use scx_algo::error::SolveError;
    use scx_algo::global_solve::{
        Accumulator, Branch, BranchSolver, Join, Keep, Next, Operation, SolveOptions, record_nodes,
        solve,
    };
    use scx_algo::solution::{Solution, SolutionKind, SolutionObject, SortKey};
    use scx_algo::split_tree::SplitTree;
    use scx_num::extent1d::Extent1d;
    use scx_num::extent3d::Extent3d;
    use scx_num::vector3d::{Point3, Vec3};

    /// Tree `tree`'s curve of `n` pieces, parameter `k + 0.5` on piece `k`.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Piece {
        pub tree: usize,
        pub n: usize,
    }

    impl SolutionObject for Piece {
        fn parameter_count(&self) -> Option<usize> {
            Some(1)
        }
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

    fn tree(plan: &[super::Plan]) -> SplitTree<Option<usize>> {
        let mut t = SplitTree::with_capacity(plan.len());
        let mut ids = Vec::with_capacity(plan.len());
        for n in plan {
            let b = Extent3d::spanning(
                Point3::new(n.lo[0], n.lo[1], n.lo[2]),
                Point3::new(n.hi[0], n.hi[1], n.hi[2]),
            )
            .unwrap();
            let id = match n.kids {
                None => t.leaf(b, n.piece),
                Some((a, c)) => t.split(b, None, [ids[a], ids[c]]),
            };
            ids.push(id.unwrap());
        }
        t
    }

    struct Hooks<'s> {
        sc: &'s Scenario,
        case: Case,
        log: Vec<(usize, usize)>,
    }

    impl BranchSolver<Option<usize>, Piece, 2> for Hooks<'_> {
        type Error = ();
        fn local_solve(
            &mut self,
            branch: &Branch<'_, Option<usize>, 2>,
            found: &mut Accumulator<Piece>,
        ) -> Result<Next, SolveError<()>> {
            let [a, b] = branch.nodes().map(|n| n.item().unwrap());
            self.log.push((a, b));
            if self.case == Case::IntersectNodes {
                return Ok(record_nodes(branch, found));
            }
            let sc = self.sc;
            let (p, q) = (sc.pts[0][a], sc.pts[1][b]);
            let Some(v) = value(self.case, sc, p, q, rad_of(&sc.plan[1], b)) else {
                return Ok(Next::Done);
            };
            let (pa, pb) = (a as f64 + 0.5, b as f64 + 0.5);
            let objects = [
                Some(Piece {
                    tree: 0,
                    n: sc.pts[0].len(),
                }),
                Some(Piece {
                    tree: 1,
                    n: sc.pts[1].len(),
                }),
            ];
            let (sol, key) = if matches!(self.case, Case::Intersect) {
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

    pub fn run(sc: &Scenario, case: Case) -> Res {
        let (t1, t2) = trees(sc);
        run_on(sc, case, &t1, &t2)
    }

    pub type Trees = (SplitTree<Option<usize>>, SplitTree<Option<usize>>);

    pub fn trees(sc: &Scenario) -> Trees {
        (tree(&sc.plan[0]), tree(&sc.plan[1]))
    }

    pub fn run_on(
        sc: &Scenario,
        case: Case,
        t1: &SplitTree<Option<usize>>,
        t2: &SplitTree<Option<usize>>,
    ) -> Res {
        let n = Vec3::new(sc.n[0], sc.n[1], sc.n[2]);
        let operation = match case {
            Case::Intersect | Case::IntersectNodes => Operation::Intersect,
            Case::Minimize | Case::MinimizeBest => Operation::Minimize,
            Case::Maximize => Operation::Maximize,
            Case::ProjectedMinimize => Operation::ProjectedMinimize { normal: n },
            Case::ProjectedMaximize => Operation::ProjectedMaximize { normal: n },
            Case::Rayfire => Operation::Rayfire { direction: n },
        };
        let mut options = SolveOptions::new(operation, sc.tol);
        options.keep = match case {
            Case::MinimizeBest => Keep::Best,
            Case::IntersectNodes => Keep::Nodes,
            _ => Keep::All,
        };
        let mut hooks = Hooks {
            sc,
            case,
            log: Vec::new(),
        };
        let found = solve([t1, t2], Accumulator::new(options), &mut hooks).ok()?;
        let best = found.best_sq();
        let rows = found
            .solutions()
            .iter()
            .map(|s| -> Row {
                let (st, en) = (s.start(), s.end());
                match s.kind() {
                    SolutionKind::Nodes => {
                        let ix = |i: usize| s.tree_nodes()[i].map_or(-1.0, |id| id.index() as f64);
                        (0, 0.0, ix(0), ix(1), 0.0, 0.0, 0.0)
                    }
                    SolutionKind::Point => (1, st.value, st.param(0), st.param(1), 0.0, 0.0, 0.0),
                    SolutionKind::Range => (
                        2,
                        st.value,
                        st.param(0),
                        st.param(1),
                        en.value,
                        en.param(0),
                        en.param(1),
                    ),
                }
            })
            .collect();
        Some((rows, best, hooks.log))
    }

    /// `Solution::object_for_parameter` over objects of the given parameter counts.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Kinded(pub usize);

    impl SolutionObject for Kinded {
        fn parameter_count(&self) -> Option<usize> {
            Some(self.0)
        }
        fn parameter_interval(&self, _: usize) -> Option<Extent1d> {
            None
        }
        fn tolerance(&self) -> f64 {
            0.0
        }
    }

    pub fn object_for_parameter(counts: &[usize], index: usize) -> Option<(usize, usize)> {
        let o = |i: usize| counts.get(i).map(|&c| Kinded(c));
        let variables: usize = counts.iter().sum();
        macro_rules! point {
            ($($v:literal)*) => {
                match variables {
                    $($v => Solution::<Kinded>::point(0.0, [0.0; $v]),)*
                    _ => Solution::point(0.0, [0.0; 10]),
                }
            };
        }
        let sol = point!(0 1 2 3 4 5 6 7 8 9);
        let sol = match counts.len() {
            1 => sol.with_objects([o(0)]),
            2 => sol.with_objects([o(0), o(1)]),
            _ => sol.with_objects([o(0), o(1), o(2)]),
        };
        sol.object_for_parameter(index)
    }
}

pub mod ss {
    use super::{Case, Res, Row, Scenario, rad_of, value};
    use solidcore::Result;
    use solidcore::geom::extent1d::Extent1d;
    use solidcore::geom::extent3d::Extent3d;
    use solidcore::geom::global_solver::{
        Branch, GlobalSolve, GlobalSolver, SolutionRequested, SolveRequest, SolverOperation,
    };
    use solidcore::geom::solution_array::{
        ObjectKind, Solution, SolutionArray, SolutionObject, SolutionType, SortKey,
    };
    use solidcore::geom::tree::{NodeId, Tree, TreeNode};
    use solidcore::math::{BIG_DOUBLE, Vec3};

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Piece {
        pub tree: usize,
        pub n: usize,
    }

    impl SolutionObject for Piece {
        fn kind(&self) -> ObjectKind {
            ObjectKind::Curve
        }
        fn natural_interval(&self) -> Option<Extent1d> {
            Some(Extent1d::from_min_max(0.0, self.n as f64))
        }
    }

    fn tree(plan: &[super::Plan]) -> Tree<Piece> {
        let mut t = Tree::new(None);
        let mut ids: Vec<NodeId> = Vec::with_capacity(plan.len());
        for n in plan {
            let mut node = TreeNode::new();
            node.bbox = Extent3d::from_points(
                Vec3::new(n.lo[0], n.lo[1], n.lo[2]),
                Vec3::new(n.hi[0], n.hi[1], n.hi[2]),
            );
            if let Some((a, c)) = n.kids {
                node.child1 = Some(ids[a]);
                node.child2 = Some(ids[c]);
            }
            ids.push(t.add_node(node));
        }
        t.set_top_node(ids.last().copied());
        t
    }

    struct Hooks<'t, 's> {
        base: GlobalSolver<'t, Piece>,
        sc: &'s Scenario,
        case: Case,
        log: Vec<(usize, usize)>,
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
            let a = sc.plan[0][branch[0].unwrap().0].piece.unwrap();
            let b = sc.plan[1][branch[1].unwrap().0].piece.unwrap();
            self.log.push((a, b));
            if self.case == Case::IntersectNodes {
                return self.base.local_solve(branch);
            }
            let (p, q) = (sc.pts[0][a], sc.pts[1][b]);
            let Some(v) = value(self.case, sc, p, q, rad_of(&sc.plan[1], b)) else {
                return Ok(false);
            };
            let (pa, pb) = (a as f64 + 0.5, b as f64 + 0.5);
            let mut s = Solution::new();
            s.num_variables = 2;
            s.num_objects = 2;
            s.objects[0] = Some(Piece {
                tree: 0,
                n: sc.pts[0].len(),
            });
            s.objects[1] = Some(Piece {
                tree: 1,
                n: sc.pts[1].len(),
            });
            s.start.solution_value = v;
            s.start.parameters[0] = pa;
            s.start.parameters[1] = pb;
            let key = if self.case == Case::Intersect {
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

    pub fn run(sc: &Scenario, case: Case) -> Res {
        let (t1, t2) = trees(sc);
        run_on(sc, case, &t1, &t2)
    }

    pub type Trees = (Tree<Piece>, Tree<Piece>);

    pub fn trees(sc: &Scenario) -> Trees {
        (tree(&sc.plan[0]), tree(&sc.plan[1]))
    }

    pub fn run_on(sc: &Scenario, case: Case, t1: &Tree<Piece>, t2: &Tree<Piece>) -> Res {
        let n = Vec3::new(sc.n[0], sc.n[1], sc.n[2]);
        let operation = match case {
            Case::Intersect | Case::IntersectNodes => SolverOperation::Intersect,
            Case::Minimize | Case::MinimizeBest => SolverOperation::Minimize,
            Case::Maximize => SolverOperation::Maximize,
            Case::ProjectedMinimize => SolverOperation::ProjectedMinimize,
            Case::ProjectedMaximize => SolverOperation::ProjectedMaximize,
            Case::Rayfire => SolverOperation::Rayfire,
        };
        let solution_requested = match case {
            Case::MinimizeBest => SolutionRequested::Single,
            Case::IntersectNodes => SolutionRequested::Nodes,
            _ => SolutionRequested::All,
        };
        let mut hooks = Hooks {
            base: GlobalSolver::with_trees(t1, t2, 2),
            sc,
            case,
            log: Vec::new(),
        };
        let vectors = [n];
        let request = SolveRequest {
            operation,
            solution_requested,
            tolerance_3d: sc.tol,
            best_answer_so_far_sq: BIG_DOUBLE,
            opt_vectors: &vectors,
        };
        let mut found = SolutionArray::new();
        hooks.solve_it(&request, &mut found).ok()?;
        let best = hooks.base.best_answer_so_far_sq;
        // "no best yet": the pin's sentinel is scx's infinity
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
                    SolutionType::NodesOnly => {
                        let ix = |i: usize| s.nodes[i].map_or(-1.0, |id| id.0 as f64);
                        (0, 0.0, ix(0), ix(1), 0.0, 0.0, 0.0)
                    }
                    SolutionType::RangeOfValues => (
                        2,
                        st.solution_value,
                        st.parameters[0],
                        st.parameters[1],
                        en.solution_value,
                        en.parameters[0],
                        en.parameters[1],
                    ),
                    _ => (
                        1,
                        st.solution_value,
                        st.parameters[0],
                        st.parameters[1],
                        0.0,
                        0.0,
                        0.0,
                    ),
                }
            })
            .collect();
        Some((rows, best, hooks.log))
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Kinded(pub usize);

    impl SolutionObject for Kinded {
        fn kind(&self) -> ObjectKind {
            match self.0 {
                0 => ObjectKind::Vertex,
                1 => ObjectKind::Curve,
                2 => ObjectKind::Surface,
                _ => ObjectKind::Volume,
            }
        }
    }

    pub fn object_for_parameter(counts: &[usize], index: usize) -> Option<(usize, usize)> {
        let mut s = Solution::<Kinded>::new();
        s.solution_type = SolutionType::SingleValue;
        s.num_variables = counts.iter().sum();
        s.num_objects = counts.len();
        for (slot, &c) in s.objects.iter_mut().zip(counts) {
            *slot = Some(Kinded(c));
        }
        s.object_index_for_parameter_index(index).ok().flatten()
    }
}

/// Runs `case` on both sides.
#[allow(clippy::type_complexity)]
fn both(case: Case) -> (impl Fn((u64, u64)) -> Res, impl Fn((u64, u64)) -> Res) {
    (
        move |(a, b): (u64, u64)| xs::run(&scenario(a, b, case.ray()), case),
        move |(a, b): (u64, u64)| ss::run(&scenario(a, b, case.ray()), case),
    )
}

macro_rules! gsolve {
    ($name:ident, $case:expr) => {
        #[test]
        fn $name() {
            let (x, s) = both($case);
            run::<(u64, u64), _, _>(concat!(module_path!(), "::", stringify!($name)), x, s);
        }
    };
}

gsolve!(gsolve_intersect, Case::Intersect);
gsolve!(gsolve_intersect_nodes, Case::IntersectNodes);
gsolve!(gsolve_minimize, Case::Minimize);
gsolve!(gsolve_minimize_best, Case::MinimizeBest);
gsolve!(gsolve_maximize, Case::Maximize);
gsolve!(gsolve_projected_minimize, Case::ProjectedMinimize);

/// The brute-force best of `case` over every leaf pair; `None` when no pair has a value.
fn brute(sc: &Scenario, case: Case) -> Option<f64> {
    let mut best: Option<f64> = None;
    for &p in &sc.pts[0] {
        for (b, &q) in sc.pts[1].iter().enumerate() {
            let Some(v) = value(case, sc, p, q, rad_of(&sc.plan[1], b)) else {
                continue;
            };
            let larger = matches!(case, Case::Maximize | Case::ProjectedMaximize);
            best = Some(match best {
                Some(x) if (larger && x >= v) || (!larger && x <= v) => x,
                _ => v,
            });
        }
    }
    best
}

/// Whether scx's result holds the brute-force best first, with the best value its square.
pub fn holds_the_best(sc: &Scenario, case: Case, r: &Res) -> Result<(), String> {
    let Some((rows, best_sq, _)) = r else {
        return Err("scx failed".to_owned());
    };
    let first = rows.first().map(|r| r.1);
    let truth = brute(sc, case);
    if first != truth {
        return Err(format!("first value {first:?}, the truth is {truth:?}"));
    }
    if let Some(t) = truth
        && *best_sq != t * t
    {
        return Err(format!("best_sq {best_sq}, the truth is {}", t * t));
    }
    Ok(())
}

#[test]
fn gsolve_projected_maximize() {
    let (x, s) = both(Case::ProjectedMaximize);
    run_defect::<(u64, u64), _, _>(
        concat!(module_path!(), "::gsolve_projected_maximize"),
        &crate::pin_defects::gsolve_projected_maximize(),
        x,
        s,
    );
}

#[test]
fn gsolve_rayfire() {
    let (x, s) = both(Case::Rayfire);
    run_defect::<(u64, u64), _, _>(
        concat!(module_path!(), "::gsolve_rayfire"),
        &crate::pin_defects::gsolve_rayfire(),
        x,
        s,
    );
}

/// The answers against brute force over every leaf pair: the best value is first and its
/// square is the best, for every goal operation; every solution of a minimise or maximise
/// is within the tolerance of the best; the intersection solves exactly the pairs whose
/// cubes meet within the tolerance (none lost to the tree's culling).
#[test]
fn gsolve_truths() {
    let mut rng = SplitMix64(0x5EED);
    let mut checked = 0;
    for _ in 0..300 {
        let (a, b) = (rng.next_u64(), rng.next_u64());
        for case in [
            Case::Minimize,
            Case::MinimizeBest,
            Case::Maximize,
            Case::ProjectedMinimize,
            Case::ProjectedMaximize,
            Case::Rayfire,
        ] {
            let sc = scenario(a, b, case.ray());
            let r = xs::run(&sc, case);
            holds_the_best(&sc, case, &r).unwrap_or_else(|e| panic!("{case:?} {a} {b}: {e}"));
            let (rows, _, _) = r.unwrap();
            let t = brute(&sc, case).unwrap_or(0.0);
            for row in &rows {
                assert!(
                    (row.1 - t).abs() <= sc.tol,
                    "{case:?} {a} {b}: {} is not within {} of {t}",
                    row.1,
                    sc.tol
                );
            }
            if case == Case::MinimizeBest {
                assert!(rows.len() <= 1, "{a} {b}: Keep::Best kept {}", rows.len());
            }
            checked += 1;
        }
        let sc = scenario(a, b, false);
        let (_, _, mut log) = xs::run(&sc, Case::Intersect).unwrap();
        log.sort_unstable();
        let mut want = Vec::new();
        for l1 in sc.plan[0].iter().filter(|n| n.piece.is_some()) {
            for l2 in sc.plan[1].iter().filter(|n| n.piece.is_some()) {
                let apart =
                    (0..3).any(|k| l1.lo[k] > l2.hi[k] + sc.tol || l2.lo[k] > l1.hi[k] + sc.tol);
                if !apart {
                    want.push((l1.piece.unwrap(), l2.piece.unwrap()));
                }
            }
        }
        want.sort_unstable();
        assert_eq!(log, want, "{a} {b}: the pairs solved");
        checked += 1;
    }
    println!("difftest gsolve_truths: {checked} scenarios checked against brute force");
}

/// The object counts, as parameter counts (0 a vertex, 1 a curve, 2 a surface, 3 a volume),
/// of every list of one to three objects.
fn kind_lists() -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    for n in 1..=3usize {
        for code in 0..4usize.pow(n as u32) {
            out.push((0..n).map(|i| (code / 4usize.pow(i as u32)) % 4).collect());
        }
    }
    out
}

/// The object behind parameter `index` by counting: the truth for `object_for_parameter`.
pub fn object_for_parameter_truth(counts: &[usize], index: usize) -> Option<(usize, usize)> {
    let mut first = 0;
    for (i, &c) in counts.iter().enumerate() {
        if index < first + c {
            return Some((i, index - first));
        }
        first += c;
    }
    None
}

/// The parameter counts and index of the inputs `(a, b, c)`: every list of `kind_lists`
/// (`14 a + b` covers them), each index from 0 to 9.
pub fn object_for_parameter_case((a, b, c): (usize, usize, usize)) -> (Vec<usize>, usize) {
    let lists = kind_lists();
    (lists[(a * 14 + b) % lists.len()].clone(), c % 10)
}

#[test]
fn sol_object_for_parameter() {
    run_defect::<(usize, usize, usize), _, _>(
        concat!(module_path!(), "::sol_object_for_parameter"),
        &crate::pin_defects::object_for_parameter(),
        |a| {
            let (counts, index) = object_for_parameter_case(a);
            xs::object_for_parameter(&counts, index)
        },
        |a| {
            let (counts, index) = object_for_parameter_case(a);
            ss::object_for_parameter(&counts, index)
        },
    );
}
