// Moved from solidcore `solidcore/crates/solidcore/src/geom/global_solver.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/src/SmGlobalSolver.cpp`,
// `source/SMLib/inc/SmGlobalSolver.h`.
// Changes: (research 57 M10) the mutable `GlobalSolver` (setters, a solutions array swapped in
// and out, a recursion depth, a cancel token) is a pure call: `solve(trees, accumulator,
// hooks)` over an `Operation` that carries the vectors its tests read (`opt_vectors[i]` and
// `OperationCategory` are gone), the options value `SolveOptions`, and the `BranchSolver`
// hooks, which get the branch and the accumulator as arguments where the pin's virtuals read
// solver fields. The traversal is an explicit stack in the order of the pin's recursion, so
// there is no depth bound. A hook's `Result` is its own error type; cancellation is such an
// error. "No best answer yet" is `None`, which starts the maximising operations at 0 (the pin
// reset `SM_BIG_DOUBLE` for `Maximize` and `DirectedMaximize` but not `ProjectedMaximize`) and
// the others at infinity (the pin's 1e20 culled boxes more than 1e10 apart). A solution value
// of `SM_BIG_DOUBLE` is no longer skipped (no solver here makes the sentinel). The rayfire
// child order reads the children of the tree it splits (the pin read tree 1's whatever tree it
// split, and failed for a leaf there); the two-tree orderings leave a third tree's children in
// order (the pin failed). `SolutionRequested::FindAmbiguities` and
// `SolverOperation::{SurfaceProjectedIntersect}`'s curve-on-surface test stay out: the first
// is unused, the second reads curves (the curve crate's hook).
//! A global solver: a depth-first search over the branches of up to [`MAX_OBJECTS`]
//! [`SplitTree`]s (one node of each), which culls a branch by its boxes, solves it locally
//! once every node is a leaf, and otherwise splits its largest node and visits the two
//! halves, nearer one first. The caller's [`BranchSolver`] does the local solves (and may
//! cull, decide readiness and choose the split itself); the [`Accumulator`] keeps the
//! solutions, sorted, without duplicates, joined into ranges, and the best value so far.
//!
//! | Rust | Upstream |
//! |------|----------|
//! | [`solve`] | `SmGlobalSolver::SolveIt`, `SolveBranch`, `Subdivide` |
//! | [`Operation`], [`Goal`] | `SmSolverOperationType` with `cpOptVectors`, `SmOperationCategory` |
//! | [`Keep`] | `SmSolutionRequested` |
//! | [`SolveOptions`] | `SolveIt`'s arguments |
//! | [`Accumulator`], [`Accumulator::add_sorted`] | `m_cSolutions`, `m_dBestAnswerSoFarSq`, `AddSortedSolution` |
//! | [`Join`] | `bCombineContiguousRanges` |
//! | [`Branch`] | `SmTreeNode* apBranch[]` |
//! | [`BranchSolver`] | the virtual methods of `SmGlobalSolver` |
//! | [`may_contain`] | `SmGlobalSolver::BranchMayContainAnswers` |
//! | [`split_choice`], [`Split`], [`ChildOrder`] | `FindBestSubdivisionNode`, `sm_FindLargestNonLeaf` |
//! | [`record_nodes`] | `SmGlobalSolver::LocalSolve` |
//! | [`MAX_BRANCHES`] | (not upstream: solidcore `MAX_SOLVE_BRANCHES`) |

// The box tests keep a branch when a bound is NaN, as upstream: `!(a > b)`, not `a <= b`.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::error::{SolveError, SolveFailure};
use crate::geom;
use crate::geom::plane::point_project_to_plane;
use crate::solution::{
    identical, End, EndClass, Pairing, Prefer, Solution, SolutionKind, SolutionObject, SortKey,
    MAX_OBJECTS,
};
use crate::split_tree::{Node, NodeId, SplitTree};
use scx_num::extent3d::Extent3d;
use scx_num::math::{EFF_ZERO, EFF_ZERO_SQRT};
use scx_num::tol::is_zero;
use scx_num::vector3d::{sqrt, Point3, UnitVec3, Vec3};

/// The most branches one solve visits (not upstream: a bound on the work, as solidcore's
/// `MAX_SOLVE_BRANCHES`); past it the solve fails with [`SolveError::InvalidInput`].
pub const MAX_BRANCHES: u64 = 1 << 23;

const OP: &str = "global_solve::solve";

/// What a solve looks for (`SmSolverOperationType`), with the vectors of the generic tests
/// (upstream `cpOptVectors`). An operation whose vectors only a [`BranchSolver`] reads
/// carries none: the hooks hold them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Operation {
    /// Intersect the objects (`SM_SO_INTERSECT`).
    Intersect,
    /// Brep or curve intersections limited to vertices and edges (`SM_SO_INTERSECT_WIREFRAME`).
    IntersectWireframe,
    /// Whether two trees may intersect (`SM_SO_INTERSECTION_TEST`).
    IntersectionTest,
    /// Minimise the distance between two objects (`SM_SO_MINIMIZE`).
    Minimize,
    /// Maximise the distance between two objects (`SM_SO_MAXIMIZE`).
    Maximize,
    /// Points that project along the objects' normals (`SM_SO_NORMALIZE`).
    Normalize,
    /// Points of two objects at a distance (`SM_SO_AT_DISTANCE`).
    AtDistance(f64),
    /// A plain search (`SM_SO_FIND`).
    Find,
    /// The nearest intersections along a ray from the first tree's box (`SM_SO_RAYFIRE`).
    Rayfire {
        /// The ray direction (`opt_vectors[0]`).
        direction: Vec3,
    },
    /// Minimise the 3D signed distance along `direction` (`SM_SO_3D_SIGNED_DIRECTED_MINIMIZE`).
    Signed3dDirectedMinimize {
        /// The direction (`opt_vectors[0]`).
        direction: Vec3,
    },
    /// Maximise the 3D signed distance along a vector (`SM_SO_3D_SIGNED_DIRECTED_MAXIMIZE`).
    Signed3dDirectedMaximize,
    /// Minimise the signed distance along `direction` projected onto the plane of `normal`
    /// (`SM_SO_SIGNED_DIRECTED_MINIMIZE`).
    SignedDirectedMinimize {
        /// The projection plane's normal (`opt_vectors[0]`).
        normal: Vec3,
        /// The direction (`opt_vectors[1]`).
        direction: Vec3,
    },
    /// Minimise the absolute projected distance along `direction` (`SM_SO_DIRECTED_MINIMIZE`).
    DirectedMinimize {
        /// The projection plane's normal (`opt_vectors[0]`).
        normal: Vec3,
        /// The direction (`opt_vectors[1]`).
        direction: Vec3,
    },
    /// Maximise the absolute projected distance along `direction` (`SM_SO_DIRECTED_MAXIMIZE`).
    DirectedMaximize {
        /// The projection plane's normal (`opt_vectors[0]`).
        normal: Vec3,
        /// The direction (`opt_vectors[1]`).
        direction: Vec3,
    },
    /// Minimise the distance projected along `normal`, measured as `|N x V x N|`
    /// (`SM_SO_PROJECTED_MINIMIZE`).
    ProjectedMinimize {
        /// The projection direction, not necessarily unit (`opt_vectors[0]`).
        normal: Vec3,
    },
    /// Maximise the distance projected along `normal` (`SM_SO_PROJECTED_MAXIMIZE`).
    ProjectedMaximize {
        /// The projection direction, not necessarily unit (`opt_vectors[0]`).
        normal: Vec3,
    },
    /// Intersect the objects projected along `normal` (`SM_SO_PROJECTED_INTERSECT`).
    ProjectedIntersect {
        /// The view plane's normal (`opt_vectors[0]`).
        normal: UnitVec3,
    },
    /// Intersect the objects rotated about an axis into a plane
    /// (`SM_SO_ROTATED_PROJECTED_INTERSECT`).
    RotatedProjectedIntersect {
        /// A point of the axis (`opt_vectors[0]`).
        point: Point3,
        /// The axis (`opt_vectors[1]`).
        axis: UnitVec3,
        /// The plane's x axis (`opt_vectors[2]`).
        x_axis: UnitVec3,
    },
    /// Intersect the objects' perspective projections (`SM_SO_PERSPECTIVE_INTERSECT`).
    PerspectiveIntersect {
        /// A point of the view plane (`opt_vectors[0]`).
        plane_point: Point3,
        /// The view plane's normal (`opt_vectors[1]`).
        normal: UnitVec3,
        /// The eye (`opt_vectors[2]`).
        eye: Point3,
    },
    /// Intersect the first curve projected onto the surface of the second
    /// (`SM_SO_SURFACE_PROJECTED_INTERSECT`); its test reads the curves, so it is the curve
    /// crate's hook.
    SurfaceProjectedIntersect,
    /// The minimum absolute angle to a plane (`SM_SO_ANGLE_MINIMIZE`).
    AngleMinimize,
    /// The minimum signed angle against a reference (`SM_SO_SIGNED_ANGLE_MINIMIZE`).
    SignedAngleMinimize,
    /// Projected tangency points of two curves (`SM_SO_PROJECTED_TANGENCY`).
    ProjectedTangency,
    /// The minimum pivot angle about `pivot` (`SM_SO_SIGNED_PIVOT_MINIMIZE`).
    SignedPivotMinimize {
        /// The pivot axis (`opt_vectors[0]`).
        axis: Vec3,
        /// The pivot point (`opt_vectors[1]`).
        pivot: Point3,
    },
    /// Curve points whose projected tangents pass through a point
    /// (`SM_SO_PROJECTED_TANGENT_THROUGH_POINT`).
    ProjectedTangentThroughPoint,
}

/// What kind of best an operation looks for (`SmOperationCategory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Goal {
    /// The smallest value (`SM_OC_MINIMIZE`).
    Minimize,
    /// The largest value (`SM_OC_MAXIMIZE`).
    Maximize,
    /// The smallest signed value (`SM_OC_SIGNED`).
    Signed,
    /// No best: every solution (`SM_OC_OTHER`).
    Other,
}

impl Operation {
    /// Whether the best answer is the largest (`Maximize`, `DirectedMaximize`,
    /// `ProjectedMaximize`; solidcore WR-21).
    #[must_use]
    pub const fn maximizes(&self) -> bool {
        matches!(
            self,
            Self::Maximize | Self::DirectedMaximize { .. } | Self::ProjectedMaximize { .. }
        )
    }

    /// The kind of best the operation looks for (upstream `m_eOperationCategory`).
    #[must_use]
    pub const fn goal(&self) -> Goal {
        match self {
            Self::DirectedMinimize { .. }
            | Self::ProjectedMinimize { .. }
            | Self::Minimize
            | Self::AngleMinimize
            | Self::Rayfire { .. } => Goal::Minimize,
            Self::DirectedMaximize { .. } | Self::ProjectedMaximize { .. } | Self::Maximize => {
                Goal::Maximize
            }
            Self::SignedDirectedMinimize { .. }
            | Self::SignedAngleMinimize
            | Self::SignedPivotMinimize { .. }
            | Self::Signed3dDirectedMinimize { .. }
            | Self::Signed3dDirectedMaximize => Goal::Signed,
            _ => Goal::Other,
        }
    }

    /// Whether the operation measures a projection (upstream `m_bProjectedOperation`).
    #[must_use]
    pub const fn is_projected(&self) -> bool {
        matches!(
            self,
            Self::SignedDirectedMinimize { .. }
                | Self::DirectedMinimize { .. }
                | Self::ProjectedMinimize { .. }
                | Self::SignedAngleMinimize
                | Self::DirectedMaximize { .. }
                | Self::ProjectedMaximize { .. }
                | Self::SignedPivotMinimize { .. }
                | Self::Signed3dDirectedMaximize
        )
    }

    /// Whether [`Accumulator::add_sorted`] keeps only the solutions within tolerance of the
    /// best.
    const fn trims_to_best(&self) -> bool {
        matches!(
            self,
            Self::Minimize
                | Self::Maximize
                | Self::Rayfire { .. }
                | Self::Signed3dDirectedMinimize { .. }
                | Self::DirectedMinimize { .. }
                | Self::DirectedMaximize { .. }
                | Self::ProjectedMinimize { .. }
                | Self::ProjectedMaximize { .. }
                | Self::SignedDirectedMinimize { .. }
                | Self::SignedPivotMinimize { .. }
        )
    }
}

/// Which solutions a solve keeps (`SmSolutionRequested`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Keep {
    /// Every solution within tolerance (`SM_SR_ALL`).
    #[default]
    All,
    /// Only the best (`SM_SR_SINGLE`): the first in sort order.
    Best,
    /// Only the tree nodes where solutions may lie (`SM_SR_NODES`); a [`BranchSolver`] reads
    /// it to record nodes ([`record_nodes`]) rather than solve.
    Nodes,
}

/// The options of a solve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SolveOptions {
    /// What to look for.
    pub operation: Operation,
    /// Which solutions to keep.
    pub keep: Keep,
    /// The 3D distance tolerance (`d3dTolerance`).
    pub tolerance: f64,
    /// The best squared value known before the solve (`dBestAnswerSoFarSq`); `None` for
    /// none, which is 0 for an operation that [maximises](Operation::maximizes) and
    /// infinity otherwise.
    pub incumbent_sq: Option<f64>,
}

impl SolveOptions {
    /// `operation`, keeping every solution, at `tolerance`, with no incumbent.
    #[must_use]
    pub const fn new(operation: Operation, tolerance: f64) -> Self {
        Self {
            operation,
            keep: Keep::All,
            tolerance,
            incumbent_sq: None,
        }
    }
}

/// Whether [`Accumulator::add_sorted`] joins ranges that only touch (`bCombineContiguousRanges`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Join {
    /// Join overlapping ranges and the points in them; keep touching ranges apart.
    Overlapping,
    /// Join touching ranges too.
    Touching,
}

/// The solutions of a solve and the best squared value so far (`m_cSolutions`,
/// `m_dBestAnswerSoFarSq`).
#[derive(Debug, Clone, PartialEq)]
pub struct Accumulator<O> {
    options: SolveOptions,
    best_sq: f64,
    solutions: Vec<Solution<O>>,
}

impl<O: SolutionObject> Accumulator<O> {
    /// An empty accumulator for `options`.
    #[must_use]
    pub fn new(options: SolveOptions) -> Self {
        Self::with_solutions(options, Vec::new())
    }

    /// An accumulator for `options` holding `solutions` already (a caller that solves in
    /// pieces). They are kept as given: [`Accumulator::add_sorted`] places new ones among
    /// them.
    #[must_use]
    pub fn with_solutions(options: SolveOptions, solutions: Vec<Solution<O>>) -> Self {
        let none = if options.operation.maximizes() {
            0.0
        } else {
            f64::INFINITY
        };
        let best_sq = options.incumbent_sq.unwrap_or(none);
        Self {
            options,
            best_sq,
            solutions,
        }
    }

    /// The options.
    #[must_use]
    pub const fn options(&self) -> &SolveOptions {
        &self.options
    }

    /// The best squared value so far (`m_dBestAnswerSoFarSq`); signed (`v |v|`) for a
    /// [`Goal::Signed`] operation.
    #[must_use]
    pub const fn best_sq(&self) -> f64 {
        self.best_sq
    }

    /// The solutions, in sort order.
    #[must_use]
    pub fn solutions(&self) -> &[Solution<O>] {
        &self.solutions
    }

    /// The solutions, in sort order.
    #[must_use]
    pub fn into_solutions(self) -> Vec<Solution<O>> {
        self.solutions
    }

    /// Appends `solution` unsorted and unchecked (`m_cSolutions.Add`, as
    /// [`record_nodes`] does).
    pub fn push(&mut self, solution: Solution<O>) {
        self.solutions.push(solution);
    }

    /// [`Accumulator::add_sorted_by`] with the default duplicate test ([`identical`]).
    pub fn add_sorted(&mut self, solution: &Solution<O>, key: SortKey, join: Join) {
        let larger = self.options.operation.maximizes();
        self.add_sorted_by(solution, key, join, |a, b| identical(a, b, larger));
    }

    /// Adds `solution` (`AddSortedSolution`): drops it when it is farther than the
    /// tolerance from the best so far (and otherwise tracks the best), replaces a held
    /// solution it duplicates by `same` with the one `same` prefers, and otherwise inserts it
    /// in `key` order; then joins ranges with the ranges and points they meet, keeps only the
    /// first for [`Keep::Best`], and for the operations that look for a best drops the
    /// solutions at the end that are not within tolerance of the first.
    ///
    /// `same(new, held)` is the duplicate test (upstream's `IdenticalSolutions` override);
    /// [`Prefer::First`] keeps `new`.
    pub fn add_sorted_by(
        &mut self,
        solution: &Solution<O>,
        key: SortKey,
        join: Join,
        same: impl Fn(&Solution<O>, &Solution<O>) -> Pairing,
    ) {
        let op = self.options.operation;
        let tol = self.options.tolerance;
        if !self.track_best(solution.value()) {
            return;
        }
        // duplicates (values within SM_EFF_ZERO_SQRT and parameters within a scaled zero)
        for held in &mut self.solutions {
            if let Pairing::Identical(prefer) = same(solution, held) {
                let mut kept = match prefer {
                    Prefer::First => *solution,
                    Prefer::Second => *held,
                };
                kept.contact = held.contact.merged(solution.contact);
                kept.shared_contact = None;
                *held = kept;
                return;
            }
        }
        let at = self
            .solutions
            .iter()
            .position(|held| goes_before(op.maximizes(), key, solution, held))
            .unwrap_or(self.solutions.len());
        self.solutions.insert(at, *solution);
        self.join_ranges(join);
        if self.options.keep == Keep::Best {
            self.solutions.truncate(1);
        }
        if op.trims_to_best() {
            self.trim_to_best(tol);
        }
    }

    /// Updates the best for a solution of value `v`; false when the solution is too far
    /// from the best to keep.
    fn track_best(&mut self, v: f64) -> bool {
        let op = self.options.operation;
        let tol = self.options.tolerance;
        let v_sq = v * v;
        // one square root here saves several below
        let best = sqrt(self.best_sq.abs());
        let (better, far) = match op.goal() {
            Goal::Signed => {
                let sgn = if v >= 0.0 { 1.0 } else { -1.0 };
                let best_sign = if self.best_sq < 0.0 { -1.0 } else { 1.0 };
                ((sgn * v_sq) < self.best_sq, v > best_sign * best + tol)
            }
            // (upstream tests the angle and pivot minimisations by category only)
            Goal::Minimize if !matches!(op, Operation::AngleMinimize) => {
                (v_sq < self.best_sq, v > best + tol)
            }
            Goal::Maximize => (v_sq > self.best_sq, v < best - tol),
            _ => (false, false),
        };
        if better {
            self.best_sq = v_sq;
        } else if far {
            return false;
        }
        true
    }

    /// Joins every pair that combines, until no pair does (or 100 solutions are held).
    fn join_ranges(&mut self, join: Join) {
        let op = self.options.operation;
        let mut done = false;
        while !done && self.solutions.len() < 100 && !self.solutions.is_empty() {
            done = true;
            'outer: for k in 0..self.solutions.len() {
                for j in k + 1..self.solutions.len() {
                    let Some((head, tail)) = self.solutions.split_at_mut_checked(j) else {
                        break;
                    };
                    let (Some(min), Some(max)) = (head.get_mut(k), tail.first_mut()) else {
                        break;
                    };
                    if let Some(combined) = combine(op, min, max, join) {
                        *min = combined;
                        self.solutions.remove(j);
                        done = false;
                        break 'outer;
                    }
                }
            }
        }
    }

    /// Drops the solutions at the end whose value differs from the first's by more than
    /// the tolerance (for rayfire, the objects' tolerances).
    fn trim_to_best(&mut self, tol: f64) {
        let (Some(first), Some(last)) = (self.solutions.first(), self.solutions.last()) else {
            return;
        };
        let first_value = first.value();
        let this_tol = if matches!(self.options.operation, Operation::Rayfire { .. }) {
            // to match the old behaviour, the distance between the points on the ray
            max2(tol, first.object_tolerance(0) + last.object_tolerance(0))
        } else {
            tol
        };
        while self.solutions.len() > 1
            && self
                .solutions
                .last()
                .is_some_and(|s| (first_value - s.value()).abs() > this_tol)
        {
            self.solutions.pop();
        }
    }
}

/// `smos_Max` of two: `a > b ? a : b`.
const fn max2(a: f64, b: f64) -> f64 {
    if a > b {
        a
    } else {
        b
    }
}

/// Whether `new` is inserted before `held` (the insertion sort of `AddSortedSolution`).
fn goes_before<O>(maximizes: bool, key: SortKey, new: &Solution<O>, held: &Solution<O>) -> bool
where
    O: Copy,
{
    let by_value = |held_v: f64, new_v: f64| {
        if maximizes {
            // the best answer is the largest
            held_v < new_v
        } else {
            held_v > new_v
        }
    };
    let (held_v, new_v) = (held.value(), new.value());
    let (h, n) = (held.start(), new.start());
    match key {
        SortKey::BySolutionValue => by_value(held_v, new_v),
        SortKey::ByFirstTwoParameters => h.param(0) > n.param(0) && h.param(1) > n.param(1),
        SortKey::ByFirstParameter => {
            let (t0, s0) = (h.param(0), n.param(0));
            // a point goes before a range with the same start (closed curves)
            let scale = (1.0 + t0) * EFF_ZERO;
            t0 > s0 || ((t0 - s0).abs() < scale && held.kind() == SolutionKind::Range)
        }
        SortKey::ByValueWithParameter => {
            if is_zero(held_v - new_v) {
                by_all_parameters(held, new)
            } else {
                by_value(held_v, new_v)
            }
        }
        SortKey::ByAllParameters => by_all_parameters(held, new),
    }
}

/// The parameter order of `AddSortedSolution`: by the first parameter, a tie (within a
/// scaled `SM_EFF_ZERO_SQRT`) settled by the next one that differs.
fn by_all_parameters<O: Copy>(held: &Solution<O>, new: &Solution<O>) -> bool {
    let (h, n) = (held.start(), new.start());
    let mut before = false;
    let mut decided = false;
    for j in 0..held.variables() {
        let (tj, sj) = (h.param(j), n.param(j));
        // the first parameter that differs at all gives the default answer
        if !decided {
            if tj > sj {
                decided = true;
                before = true;
            } else if tj < sj {
                decided = true;
                before = false;
            }
        }
        // a bigger tolerance than the duplicate test's
        let scale = (1.0 + tj) * EFF_ZERO_SQRT;
        if (tj - sj).abs() > scale {
            break;
        }
        // a tie so far: the next parameter may settle it
        if j + 1 < held.variables() {
            let (tn, sn) = (h.param(j + 1), n.param(j + 1));
            let scale = (1.0 + tn) * EFF_ZERO_SQRT;
            if (tn - sn).abs() > scale {
                before = tn > sn;
                break;
            }
        }
    }
    before
}

/// `sm_CombineMinMax`: the union of two solutions when they combine (a range taking in a
/// point at or inside it, overlapping ranges, and touching ones for [`Join::Touching`]).
/// For an intersection, a point inside a range that is tighter than the range's ends
/// instead splits the range there, in place, into two touching ranges, and `None` is
/// returned.
fn combine<O: SolutionObject>(
    op: Operation,
    min: &mut Solution<O>,
    max: &mut Solution<O>,
    join: Join,
) -> Option<Solution<O>> {
    // (not upstream) the contact of a range the two make, or one takes the other into
    let both = min.contact.absorbed(max.contact);
    let ranged = |mut s: Solution<O>| {
        s.contact = both;
        s.unique = None;
        s.shared_contact = None;
        s
    };
    if min.kind() == SolutionKind::Range {
        if max.kind() == SolutionKind::Point {
            let class = min.classify_end(max, End::Start);
            let intersect = matches!(op, Operation::Intersect);
            return match class {
                EndClass::AtStart | EndClass::AtEnd => Some(ranged(*min)),
                EndClass::Within if !intersect => Some(ranged(*min)),
                EndClass::Within => {
                    // an intersection point inside a range: when the point is tight,
                    // two contiguous ranges
                    let pt_dev = max.value();
                    let (min_dev, max_dev) = (min.start().value, min.end().value);
                    if pt_dev < min_dev * 100.0 || pt_dev < max_dev * 100.0 {
                        let (min_end, max_start) = (*min.end(), *max.start());
                        max.make_range(min_end);
                        max.contact = both;
                        max.unique = None;
                        max.shared_contact = None;
                        min.set_ends(*min.start(), max_start);
                        min.contact = both;
                        min.unique = None;
                        min.shared_contact = None;
                        None
                    } else {
                        Some(ranged(*min))
                    }
                }
                _ => None,
            };
        }
        // both ranges: where max's start and end lie against min
        let class1 = min.classify_end(max, End::Start);
        let class2 = min.classify_end(max, End::End);
        if class1 == EndClass::Inconsistent || class2 == EndClass::Inconsistent {
            return None;
        }
        // disjoint
        if class1 == EndClass::AboveEnd || class2 == EndClass::BelowStart {
            return None;
        }
        // touching, unless asked
        if join == Join::Overlapping
            && ((class1 == EndClass::AtEnd && class2 == EndClass::AboveEnd)
                || (class1 == EndClass::BelowStart && class2 == EndClass::AtStart))
        {
            return None;
        }
        let mut combined = ranged(*min);
        if class1 == EndClass::Within && class2 == EndClass::Within {
            return Some(combined);
        }
        let start = if class1 == EndClass::BelowStart {
            *max.start()
        } else {
            *min.start()
        };
        let end = if class2 == EndClass::AboveEnd {
            *max.end()
        } else {
            *min.end()
        };
        combined.set_ends(start, end);
        return Some(combined);
    }
    // a point (or nodes) in a range
    if max.kind() == SolutionKind::Range && max.classify_end(min, End::Start).is_at_or_within() {
        return Some(ranged(*max));
    }
    None
}

/// A branch: one node of each tree.
#[derive(Debug)]
pub struct Branch<'a, T, const N: usize> {
    trees: [&'a SplitTree<T>; N],
    ids: [NodeId; N],
    nodes: [&'a Node<T>; N],
}

impl<'a, T, const N: usize> Branch<'a, T, N> {
    /// The branch of `ids` (one node of each tree); `None` when an id is not a node of its
    /// tree.
    #[must_use]
    pub fn new(trees: [&'a SplitTree<T>; N], ids: [NodeId; N]) -> Option<Self> {
        let mut found: [Option<&'a Node<T>>; N] = [None; N];
        for ((slot, tree), id) in found.iter_mut().zip(trees).zip(ids) {
            *slot = Some(tree.get(id)?);
        }
        let fallback = found.iter().flatten().next().copied()?;
        Some(Self {
            trees,
            ids,
            nodes: found.map(|n| n.unwrap_or(fallback)),
        })
    }

    /// The trees.
    #[must_use]
    pub const fn trees(&self) -> &[&'a SplitTree<T>; N] {
        &self.trees
    }

    /// The node ids, one per tree.
    #[must_use]
    pub const fn ids(&self) -> &[NodeId; N] {
        &self.ids
    }

    /// The nodes, one per tree.
    #[must_use]
    pub const fn nodes(&self) -> &[&'a Node<T>; N] {
        &self.nodes
    }

    /// The nodes' boxes.
    #[must_use]
    pub fn boxes(&self) -> [Extent3d; N] {
        self.nodes.map(|n| *n.bbox())
    }
}

/// Which child of a split node a solve visits first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChildOrder {
    /// The first child, then the second.
    Given,
    /// The second child, then the first.
    Swapped,
}

/// The node of a branch to split ([`split_choice`]): the index of its tree, and the order
/// to visit the two halves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Split {
    /// The tree whose node is split.
    pub tree: usize,
    /// Which half first.
    pub order: ChildOrder,
}

/// Whether a local solve settled the branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Next {
    /// The branch is done.
    Done,
    /// The branch needs splitting further (upstream `needs_more_subdivision`).
    Split,
}

/// The hooks of a solve over `N` trees of pieces `T` (the virtual methods of
/// `SmGlobalSolver`). Only [`BranchSolver::local_solve`] is required; the others default to
/// the generic tests of this module.
pub trait BranchSolver<T, O: SolutionObject, const N: usize> {
    /// The hooks' own error (an evaluation failure, a cancellation).
    type Error;

    /// Solves a branch whose nodes are all [ready](BranchSolver::is_ready), adding what it
    /// finds to `found` (`LocalSolve`).
    ///
    /// # Errors
    ///
    /// The hook's errors, which end the solve.
    fn local_solve(
        &mut self,
        branch: &Branch<'_, T, N>,
        found: &mut Accumulator<O>,
    ) -> Result<Next, SolveError<Self::Error>>;

    /// Whether the branch may hold solutions (`BranchMayContainAnswers`); by default the
    /// box tests of [`may_contain`].
    ///
    /// # Errors
    ///
    /// The hook's errors, which end the solve.
    fn may_contain(
        &mut self,
        branch: &Branch<'_, T, N>,
        found: &Accumulator<O>,
    ) -> Result<bool, SolveError<Self::Error>> {
        let o = found.options();
        Ok(may_contain(
            &o.operation,
            &branch.boxes(),
            o.tolerance,
            found.best_sq(),
        ))
    }

    /// Whether the branch is ready for a local solve (`AreReadyForLocalSolve`); by default,
    /// when every node is a leaf.
    fn is_ready(&self, branch: &Branch<'_, T, N>) -> bool {
        branch.nodes().iter().all(|n| n.is_leaf())
    }

    /// The node to split (`FindBestSubdivisionNode`); by default [`split_choice`].
    ///
    /// # Errors
    ///
    /// [`SolveFailure::NoNodeToSubdivide`] by default when every node is a leaf; the hook's
    /// errors.
    fn split_choice(
        &self,
        branch: &Branch<'_, T, N>,
        found: &Accumulator<O>,
    ) -> Result<Split, SolveError<Self::Error>> {
        split_choice(&found.options().operation, branch).ok_or(SolveError::Failed {
            operation: "global_solve::split_choice",
            failure: SolveFailure::NoNodeToSubdivide { trees: N },
        })
    }
}

/// Records the branch's nodes as a [`SolutionKind::Nodes`] solution, unsorted (the base
/// `LocalSolve`: with no knowledge of the pieces, the nodes are all a solve can say).
pub fn record_nodes<T, O: SolutionObject, const N: usize>(
    branch: &Branch<'_, T, N>,
    found: &mut Accumulator<O>,
) -> Next {
    found.push(Solution::nodes(*branch.ids()));
    Next::Done
}

/// `SmGlobalSolver::SolveIt`: searches the branches of `trees` from their roots, depth
/// first, through `hooks`, adding to `found`.
///
/// # Errors
///
/// [`SolveError::InvalidInput`] for an empty tree, past [`MAX_BRANCHES`] branches, or for a
/// split the hooks choose of a leaf or of no tree; the hooks' errors.
pub fn solve<T, O, S, const N: usize>(
    trees: [&SplitTree<T>; N],
    mut found: Accumulator<O>,
    hooks: &mut S,
) -> Result<Accumulator<O>, SolveError<S::Error>>
where
    O: SolutionObject,
    S: BranchSolver<T, O, N> + ?Sized,
{
    const {
        assert!(
            N >= 1 && N <= MAX_OBJECTS,
            "a solve has 1 to MAX_OBJECTS trees"
        );
    };
    let invalid = |reason| SolveError::InvalidInput {
        operation: OP,
        reason,
    };
    let mut root = [None; N];
    for (slot, tree) in root.iter_mut().zip(trees) {
        *slot = tree.root();
    }
    let Some(&any) = root.iter().flatten().next() else {
        return Err(invalid("a tree is empty"));
    };
    if root.iter().any(Option::is_none) {
        return Err(invalid("a tree is empty"));
    }
    let mut stack = vec![root.map(|r| r.unwrap_or(any))];
    let mut visited: u64 = 0;
    while let Some(ids) = stack.pop() {
        visited += 1;
        if visited > MAX_BRANCHES {
            return Err(invalid("solve visits more than MAX_BRANCHES branches"));
        }
        let branch = Branch::new(trees, ids).ok_or_else(|| invalid("a node is not in its tree"))?;
        // no answers in this branch
        if !hooks.may_contain(&branch, &found)? {
            continue;
        }
        if hooks.is_ready(&branch) && hooks.local_solve(&branch, &mut found)? == Next::Done {
            continue;
        }
        let Split { tree, order } = hooks.split_choice(&branch, &found)?;
        let [a, b] = branch
            .nodes()
            .get(tree)
            .and_then(|n| n.children())
            .ok_or_else(|| invalid("the split chosen is of a leaf or of no tree"))?;
        let (first, second) = match order {
            ChildOrder::Given => (a, b),
            ChildOrder::Swapped => (b, a),
        };
        let mut first_ids = ids;
        let mut second_ids = ids;
        if let (Some(f), Some(s)) = (first_ids.get_mut(tree), second_ids.get_mut(tree)) {
            *f = first;
            *s = second;
        }
        // the first half is visited (all of it) first, as the recursion upstream
        stack.push(second_ids);
        stack.push(first_ids);
    }
    Ok(found)
}

/// The parameter of the point of the line `p + t v` nearest `q`, 0 for a line with no
/// direction (the global solver's reading of [`geom::line::line_closest_point`]).
fn closest_t(p: Point3, v: Vec3, q: Point3) -> f64 {
    geom::line::line_closest_point(p, v, q).unwrap_or(0.0)
}

fn unit_or_z(v: Vec3) -> Vec3 {
    v.normalized().map_or(Vec3::Z, |u| *u)
}

/// The largest squared length of the offsets `center ± dir r` perspective projected
/// relative to `center` (the radius loops of the perspective test).
fn max_projected_radius_sq(
    center: Point3,
    dirs: [Vec3; 2],
    radius: f64,
    view: (Point3, Vec3, Point3),
) -> f64 {
    let [d1, d2] = dirs;
    let (plane_point, normal, eye) = view;
    let mut max_sq = 0.0;
    for offset in [d1 * radius, -d1 * radius, d2 * radius, -d2 * radius] {
        // (solidcore WR-608: the vector form already adds its base to the offset)
        let r = offset.perspective_project_to_plane(center, plane_point, normal, eye);
        let r_sq = r.length_squared();
        if r_sq > max_sq {
            max_sq = r_sq;
        }
    }
    max_sq
}

/// `BranchMayContainAnswers`: whether nodes with `boxes` may hold solutions of `op` at
/// `tolerance` better than `best_sq`, by box and sphere tests. True for one box, for
/// operations with no test, and wherever a test cannot decide.
#[must_use]
pub fn may_contain(op: &Operation, boxes: &[Extent3d], tolerance: f64, best_sq: f64) -> bool {
    // no natural test for a single tree: a hook's job
    let [b0, b1, ..] = boxes else {
        return true;
    };
    let tol = tolerance;
    match *op {
        Operation::Intersect | Operation::IntersectionTest => {
            for (i, bi) in boxes.iter().enumerate() {
                let grown = bi.expanded(tol);
                if boxes
                    .iter()
                    .skip(i + 1)
                    .any(|bj| grown.are_disjoint(bj, 0.0))
                {
                    return false;
                }
            }
            true
        }
        Operation::Minimize => {
            let d_sq = b0.min_distance_squared(b1);
            !(d_sq > best_sq && sqrt(d_sq) - tol > sqrt(best_sq))
        }
        Operation::Maximize => {
            let d_sq = b0.max_distance_squared(b1);
            !(d_sq < best_sq && sqrt(d_sq) + tol < sqrt(best_sq))
        }
        Operation::AtDistance(d) => {
            let at_sq = d * d;
            let max_sq = b0.max_distance_squared(b1);
            if max_sq < at_sq && sqrt(max_sq) - tol < sqrt(at_sq) {
                return false;
            }
            let min_sq = b0.min_distance_squared(b1);
            !(min_sq > at_sq && sqrt(min_sq) + tol > sqrt(at_sq))
        }
        Operation::ProjectedMinimize { normal } | Operation::ProjectedMaximize { normal } => {
            projected_extreme(op.maximizes(), normal, b0, b1, tol, best_sq)
        }
        Operation::ProjectedIntersect { .. }
        | Operation::RotatedProjectedIntersect { .. }
        | Operation::PerspectiveIntersect { .. } => projected_intersect(op, b0, b1, tol),
        Operation::DirectedMinimize { normal, direction }
        | Operation::SignedDirectedMinimize { normal, direction }
        | Operation::DirectedMaximize { normal, direction } => {
            directed(op, normal, direction, b0, b1, best_sq)
        }
        Operation::SignedPivotMinimize { axis, pivot } => {
            // the distances of the sphere centres from the pivot axis differ by more than
            // the radii: no pivot brings one sphere onto the other
            let s1 = b0.sphere_bound();
            let s2 = b1.sphere_bound();
            let v1 = axis.cross(s1.center - pivot).cross(axis);
            let v2 = axis.cross(s2.center - pivot).cross(axis);
            !((v1.length() - v2.length()).abs() - tol > s1.radius + s2.radius)
        }
        Operation::Rayfire { direction } => rayfire(direction, b0, b1, tol, best_sq),
        Operation::Signed3dDirectedMinimize { direction } => {
            let s1 = b0.sphere_bound();
            let s2 = b1.sphere_bound();
            let t = closest_t(s1.center, direction, s2.center);
            let closest = s1.center + t * direction;
            // the spheres do not meet along the direction
            if closest.distance(s2.center) > s1.radius + s2.radius + tol {
                return false;
            }
            let v = closest - s1.center;
            let along = if v.dot(direction) < 0.0 {
                -v.length()
            } else {
                v.length()
            };
            let min = along - s1.radius - s2.radius - tol;
            !(min * min.abs() > best_sq)
        }
        // no box test: normalise and find have none yet; a point of the tangent-through-
        // point solve may be arbitrarily far; the others are not handled upstream
        Operation::Normalize
        | Operation::Find
        | Operation::ProjectedTangentThroughPoint
        | Operation::IntersectWireframe
        | Operation::Signed3dDirectedMaximize
        | Operation::AngleMinimize
        | Operation::SignedAngleMinimize
        | Operation::ProjectedTangency
        | Operation::SurfaceProjectedIntersect => true,
    }
}

/// The projected minimise and maximise tests, in the objective's units: the objective is
/// `|N x V x N| = (N.N) |P(V)|` for the unscaled orthogonal projection `P`, which sends a
/// sphere of radius `r` inside radius `r (N.N)`.
fn projected_extreme(
    maximizes: bool,
    normal: Vec3,
    b0: &Extent3d,
    b1: &Extent3d,
    tol: f64,
    best_sq: f64,
) -> bool {
    let s1 = b0.sphere_bound();
    let s2 = b1.sphere_bound();
    let scale = normal.dot(normal);
    if scale <= 0.0 || !scale.is_finite() {
        return true;
    }
    // a normal too short to project along prunes nothing
    let Some(projected) = point_project_to_plane(s2.center, s1.center, normal) else {
        return true;
    };
    let centres = s1.center.distance(projected) * scale;
    let r1 = s1.radius * scale + tol;
    let r2 = s2.radius * scale;
    let bound = if maximizes {
        centres + r1 + r2
    } else {
        centres - r1 - r2
    };
    if !bound.is_finite() {
        return true;
    }
    let excluded = if maximizes {
        bound * bound < best_sq
    } else {
        bound > 0.0 && bound * bound > best_sq
    };
    !excluded
}

/// The projected, rotated and perspective intersection tests: the spheres projected to a
/// common plane do not meet.
fn projected_intersect(op: &Operation, b0: &Extent3d, b1: &Extent3d, tol: f64) -> bool {
    let s1 = b0.sphere_bound();
    let s2 = b1.sphere_bound();
    // the tolerance goes on one radius
    let mut r1 = s1.radius + tol;
    let mut r2 = s2.radius;
    let (p1, p2) = match *op {
        Operation::RotatedProjectedIntersect {
            point,
            axis,
            x_axis,
        } => (
            s1.center
                .rotate_project_point_to_plane(point, *axis, *x_axis),
            s2.center
                .rotate_project_point_to_plane(point, *axis, *x_axis),
        ),
        Operation::PerspectiveIntersect {
            plane_point,
            normal,
            eye,
        } => {
            let n = *normal;
            let p1 = s1
                .center
                .perspective_project_point_to_plane(plane_point, n, eye);
            let p2 = s2
                .center
                .perspective_project_point_to_plane(plane_point, n, eye);
            // a sphere projects to an ellipse: bound its largest radius
            let dirs = |c: Point3| {
                let eye_vec = c - eye;
                let d1 = unit_or_z(eye_vec.cross(n)); // across the eye vector and the normal
                let d2 = unit_or_z(d1.cross(eye_vec)); // across the eye vector, in the normal's plane
                [d1, d2]
            };
            r1 -= tol;
            let view = (plane_point, n, eye);
            let max1 = max_projected_radius_sq(s1.center, dirs(s1.center), r1, view);
            let max2 = max_projected_radius_sq(s2.center, dirs(s2.center), r2, view);
            r1 = sqrt(max1) + tol;
            r2 = sqrt(max2);
            (p1, p2)
        }
        Operation::ProjectedIntersect { normal } => {
            match point_project_to_plane(s2.center, s1.center, *normal) {
                Some(p2) => (s1.center, p2),
                None => return true,
            }
        }
        _ => return true,
    };
    !(p1.distance(p2) - r1 - r2 > 0.0)
}

/// The directed tests: the sphere centres project apart along the direction by more than
/// the radii, or nearer or farther than the best allows.
fn directed(
    op: &Operation,
    normal: Vec3,
    direction: Vec3,
    b0: &Extent3d,
    b1: &Extent3d,
    best_sq: f64,
) -> bool {
    // a plane through one centre whose normal is across the direction: the other centre is
    // farther from it than the radii
    let plane_normal = direction.cross(normal);
    let s1 = b0.sphere_bound();
    let s2 = b1.sphere_bound();
    // a direction along the normal, or a normal too short, prunes nothing (upstream's NaN
    // projection fails every test below)
    let Some(proj) = point_project_to_plane(s2.center, s1.center, plane_normal) else {
        return true;
    };
    let sum = s1.radius + s2.radius;
    if proj.distance_squared(s2.center) > sum * sum + EFF_ZERO_SQRT {
        return false;
    }
    // then onto the projection plane through the first centre
    let Some(proj) = point_project_to_plane(proj, s1.center, normal) else {
        return true;
    };
    match op {
        Operation::SignedDirectedMinimize { .. } => {
            let v = proj - s1.center;
            let along = if v.dot(direction) < 0.0 {
                -v.length()
            } else {
                v.length()
            };
            let min = along - s1.radius - s2.radius;
            !(min * min.abs() > best_sq)
        }
        Operation::DirectedMinimize { .. } => {
            let min = proj.distance(s1.center) - s1.radius - s2.radius;
            !(min > 0.0 && min * min > best_sq)
        }
        Operation::DirectedMaximize { .. } => {
            let max = proj.distance(s1.center) + s1.radius + s2.radius;
            !(max * max < best_sq)
        }
        _ => true,
    }
}

/// The ray tests: the ray from the first box's corner misses the second box's sphere or
/// box, starts past it, or reaches it only beyond the best.
fn rayfire(direction: Vec3, b0: &Extent3d, b1: &Extent3d, tol: f64, best_sq: f64) -> bool {
    let start = b0.min();
    let s = b1.sphere_bound();
    let t = closest_t(start, direction, s.center);
    let closest = start + t * direction;
    // the sphere misses the line
    if closest.distance(s.center) > s.radius + tol {
        return false;
    }
    // the sphere is behind the start
    if t + s.radius < 0.0 {
        return false;
    }
    // the sphere is farther along than the best
    let nearest = t - s.radius - tol;
    if nearest > 0.0 && nearest * nearest > best_sq {
        return false;
    }
    // the ray's box misses the second box
    let length = 2.0 * (t.abs() + s.radius);
    let ray_box =
        Extent3d::spanning(start - length * direction, start + length * direction).expanded(tol);
    !ray_box.are_disjoint(b1, 0.0)
}

/// `FindBestSubdivisionNode`: the node of the branch with the largest box diagonal that
/// has children (the last on ties), and which half to visit first — the one nearer the
/// other box for the minimising operations, farther for the maximising ones, nearer the
/// ray's start along the ray for rayfire. `None` when every node is a leaf.
#[must_use]
pub fn split_choice<T, const N: usize>(op: &Operation, branch: &Branch<'_, T, N>) -> Option<Split> {
    let mut best = None;
    let mut max_size = 0.0;
    for (i, node) in branch.nodes().iter().enumerate() {
        if node.is_leaf() {
            continue;
        }
        let size = (node.bbox().max() - node.bbox().min()).length_squared();
        if size >= max_size {
            max_size = size;
            best = Some(i);
        }
    }
    let tree = best?;
    let nodes = branch.nodes();
    let boxes = branch.boxes();
    let halves = nodes
        .get(tree)
        .and_then(|n| n.children())
        .and_then(|[a, b]| {
            let t = branch.trees().get(tree)?;
            Some((*t.get(a)?.bbox(), *t.get(b)?.bbox()))
        });
    // the other box of a two-tree ordering
    let other = if tree < 2 { boxes.get(1 - tree) } else { None };
    let mid = |b: &Extent3d| b.evaluate(0.5, 0.5, 0.5);
    let swapped = match (*op, halves) {
        (Operation::Rayfire { direction }, Some((h1, h2))) => {
            let start = boxes.first().map(Extent3d::min);
            start.is_some_and(|p| {
                closest_t(p, direction, mid(&h2))
                    < closest_t(p, direction, mid(&h1))
            })
        }
        (Operation::Signed3dDirectedMinimize { direction }, Some((h1, h2))) => {
            other.is_some_and(|o| {
                let p = mid(o);
                closest_t(p, direction, mid(&h2))
                    < closest_t(p, direction, mid(&h1))
            })
        }
        (
            Operation::Minimize
            | Operation::DirectedMinimize { .. }
            | Operation::SignedDirectedMinimize { .. }
            | Operation::ProjectedMinimize { .. },
            Some((h1, h2)),
        ) => other.is_some_and(|o| {
            let p = mid(o);
            (mid(&h2) - p).length_squared() < (mid(&h1) - p).length_squared()
        }),
        (_, Some((h1, h2))) if op.maximizes() => other.is_some_and(|o| {
            let p = mid(o);
            (mid(&h2) - p).length_squared() > (mid(&h1) - p).length_squared()
        }),
        _ => false,
    };
    Some(Split {
        tree,
        order: if swapped {
            ChildOrder::Swapped
        } else {
            ChildOrder::Given
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use scx_num::extent1d::Extent1d;

    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Piece(usize);

    impl SolutionObject for Piece {
        fn parameter_count(&self) -> Option<usize> {
            Some(0)
        }
        fn parameter_interval(&self, _: usize) -> Option<Extent1d> {
            None
        }
        fn tolerance(&self) -> f64 {
            0.0
        }
    }

    /// A tree over unit cubes at the given x, balanced by halving.
    fn row(xs: &[f64]) -> SplitTree<usize> {
        fn build(t: &mut SplitTree<usize>, xs: &[(usize, f64)]) -> NodeId {
            let b = |x: f64| {
                Extent3d::spanning(Point3::new(x, 0.0, 0.0), Point3::new(x + 1.0, 1.0, 1.0))
            };
            if let [(i, x)] = xs {
                return t.leaf(b(*x), *i).unwrap();
            }
            let (l, r) = xs.split_at(xs.len() / 2);
            let a = build(t, l);
            let c = build(t, r);
            let bbox = t.get(a).unwrap().bbox().union(*t.get(c).unwrap().bbox());
            t.split(bbox, usize::MAX, [a, c]).unwrap()
        }
        let mut t = SplitTree::new();
        let v: Vec<(usize, f64)> = xs.iter().copied().enumerate().collect();
        build(&mut t, &v);
        t
    }

    struct Nodes;
    impl BranchSolver<usize, Piece, 2> for Nodes {
        type Error = ();
        fn local_solve(
            &mut self,
            branch: &Branch<'_, usize, 2>,
            found: &mut Accumulator<Piece>,
        ) -> Result<Next, SolveError<()>> {
            Ok(record_nodes(branch, found))
        }
    }

    #[test]
    fn intersect_finds_the_overlapping_leaves() {
        let a = row(&[0.0, 2.0, 4.0, 6.0]);
        let b = row(&[4.5, 10.0]);
        let found = solve(
            [&a, &b],
            Accumulator::new(SolveOptions::new(Operation::Intersect, 0.0)),
            &mut Nodes,
        )
        .unwrap();
        let pairs: Vec<(usize, usize)> = found
            .solutions()
            .iter()
            .map(|s| {
                let n = s.tree_nodes();
                let (x, y) = (n[0].unwrap(), n[1].unwrap());
                (*a.get(x).unwrap().item(), *b.get(y).unwrap().item())
            })
            .collect();
        assert_eq!(pairs, [(2, 0)]);
    }

    struct Distance<'a>(&'a [f64], &'a [f64]);
    impl BranchSolver<usize, Piece, 2> for Distance<'_> {
        type Error = ();
        fn local_solve(
            &mut self,
            branch: &Branch<'_, usize, 2>,
            found: &mut Accumulator<Piece>,
        ) -> Result<Next, SolveError<()>> {
            let [i, j] = branch.nodes().map(|n| *n.item());
            let d = (self.0[i] - self.1[j]).abs().max(1.0) - 1.0;
            let s = Solution::point(d, []).with_objects([Some(Piece(i)), Some(Piece(j))]);
            found.add_sorted(&s, SortKey::BySolutionValue, Join::Overlapping);
            Ok(Next::Done)
        }
    }

    #[test]
    fn minimize_keeps_the_nearest_pair() {
        let xa = [0.0, 3.0, 9.0, 20.0];
        let xb = [14.0, 30.0];
        let (a, b) = (row(&xa), row(&xb));
        let found = solve(
            [&a, &b],
            Accumulator::new(SolveOptions::new(Operation::Minimize, 1e-9)),
            &mut Distance(&xa, &xb),
        )
        .unwrap();
        assert_eq!(found.best_sq(), 16.0);
        let s = found.solutions();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].objects(), [Some(Piece(2)), Some(Piece(0))]);
    }

    #[test]
    fn a_leaf_only_branch_with_more_to_split_fails() {
        struct Again;
        impl BranchSolver<usize, Piece, 2> for Again {
            type Error = ();
            fn local_solve(
                &mut self,
                _: &Branch<'_, usize, 2>,
                _: &mut Accumulator<Piece>,
            ) -> Result<Next, SolveError<()>> {
                Ok(Next::Split)
            }
        }
        let a = row(&[0.0]);
        let r = solve(
            [&a, &a],
            Accumulator::new(SolveOptions::new(Operation::Intersect, 0.0)),
            &mut Again,
        );
        assert!(matches!(
            r,
            Err(SolveError::Failed {
                failure: SolveFailure::NoNodeToSubdivide { trees: 2 },
                ..
            })
        ));
    }
}
