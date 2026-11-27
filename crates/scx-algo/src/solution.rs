// Moved from solidcore `solidcore/crates/solidcore/src/geom/solution_array.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8.
// Upstream record (verbatim from that file): Upstream: `source/SMLib/inc/SmSolutionArray.h`,
// `source/SMLib/src/SmSolutionArray.cpp`.
// Changes: (research 57 M10) only the solution, its ends, its classification, its equality
// tests and the array's sorted insertion moved; the queries that evaluate geometry
// (`GetIndex`, `GetPoint`, the `SolutionObject` geometry hooks, `SideRecord`) stay with the
// curve and surface crates. A solution's counts cannot exceed its fixed arrays (the
// constructors take arrays of at most `MAX_PARAMS` and `MAX_OBJECTS`), so nothing here fails;
// the pin returned `InvalidInput` for counts past them. `SolutionType` is `SolutionKind`
// without `PolySingleValue` (no solver makes it) and `Unknown` (no solution is
// uninitialised); `compare_start: bool` is `End`. `SolutionObject` is three required methods
// (parameter count, interval, tolerance), where the pin had an `ObjectKind` and failing
// geometry defaults, and `ParameterDomains` (a resolver beside the handle) is gone: a handle
// answers for itself. `object_index_for_parameter_index` reads each object in turn (the pin
// read object 0 on every pass, so a parameter past the first object of a mixed solution
// took the wrong axis) and finds no object for a parameter past them (the pin answered
// object 0, axis 0). `add_sorted` no longer skips a `BIG_DOUBLE` value (a sentinel for an
// uninitialised solution; none exists here).
//! Solver results: a solution end (one point of a solution space), a solution (a point or
//! a range over a set of objects), how the end of one solution lies against another, the
//! duplicate tests, and the sorted, de-duplicating insertion into a list of solutions.
//!
//! | Rust | Upstream |
//! |------|----------|
//! | [`MAX_OBJECTS`], [`MAX_PARAMS`] | `SM_SA_MAX_TREES`, `SM_SA_MAX_TREES*2` |
//! | [`SolutionKind`] | `SmSolutionType` |
//! | [`SortKey`] | `SmSortKeyType` |
//! | [`Contact`], [`ContactId`] | (not upstream: solidcore's contact typing) |
//! | [`SolutionEnd`] | `SmSolutionEnd` |
//! | [`Solution`], [`Solution::point`], [`Solution::range`], [`Solution::nodes`] | `SmSolution` and its fields |
//! | [`Solution::classify_end`], [`EndClass`], [`End`] | `SmSolution::ClassifySolutionEnd` and its return values |
//! | [`Solution::object_for_parameter`] | `SmSolution::GetObjectIndexForParameterIndex` |
//! | [`Solution::parameter_interval`] | `SmSolution::GetParameterInterval` |
//! | [`Solution::object_tolerance`] | `SmSolution::GetObjectTolerance` |
//! | [`Solution::is_equal`] | `SmSolution::IsEqual` |
//! | [`identical`], [`Pairing`], [`Prefer`] | `SmGlobalSolver::IdenticalSolutions`, `SmSolutionPairType` |
//! | [`add_sorted`] | `SmSolutionArray::AddSortedSolution` |
//! | `are_sequential` (private) | `sm_AreSequential` |

use crate::split_tree::NodeId;
use scx_num::extent1d::Extent1d;
use scx_num::math::{EFF_ZERO, EFF_ZERO_SQRT};

/// Most objects (and tree nodes) a solution refers to (`SM_SA_MAX_TREES`).
pub const MAX_OBJECTS: usize = 5;
/// Most parameters a solution end holds (`SM_SA_MAX_TREES*2`).
pub const MAX_PARAMS: usize = 2 * MAX_OBJECTS;

/// What a solution is (`SmSolutionType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SolutionKind {
    /// Only the tree nodes are meaningful (`SM_ST_NODES_ONLY`).
    Nodes,
    /// One point: only the start holds values (`SM_ST_SINGLE_VALUE`).
    Point,
    /// Every point from the start to the end (`SM_ST_RANGE_OF_VALUES`).
    Range,
}

/// The order [`add_sorted`] and the solver's accumulator keep (`SmSortKeyType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortKey {
    /// By solution value only (`SM_SK_BY_SOLUTION_VALUE`).
    BySolutionValue,
    /// By the first parameter only (`SM_SK_BY_FIRST_PARAMETER`).
    ByFirstParameter,
    /// By the first parameter, ties resolved by the second (`SM_SK_BY_FIRST_TWO_PARAMETERS`).
    ByFirstTwoParameters,
    /// By value, ties resolved by the parameters (`SM_SK_BY_VALUE_WITH_PARAMETER`).
    ByValueWithParameter,
    /// By the first parameter, then the next ones up to the first that differs
    /// (`SM_SK_BY_ALL_PARAMETERS`).
    ByAllParameters,
}

/// How a line or curve solution meets the surface (not upstream). A solver sets it only
/// from a certificate; [`Contact::Unclassified`] is a solution no typing has looked at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Contact {
    /// No typing has looked at the solution.
    #[default]
    Unclassified,
    /// The line passes from one side of the surface to the other here.
    Crossing,
    /// The line stays on one side of the surface here.
    Touching,
    /// Typing looked at the solution and could not decide.
    Undecided,
}

impl Contact {
    /// The contact of one solution kept for two merged into it (a duplicate merged into the
    /// solution it repeats): unclassified for two unclassified, touching for two touching,
    /// undecided otherwise. A merge is by a tolerance, never by a certificate that the two are
    /// one contact, so two crossings merged are undecided (they may be two).
    #[must_use]
    pub const fn merged(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unclassified, Self::Unclassified) => Self::Unclassified,
            (Self::Touching, Self::Touching) => Self::Touching,
            _ => Self::Undecided,
        }
    }

    /// The contact of a range that takes in another solution (a point it absorbs, a range it
    /// is joined with, the point a range is made from): unclassified when both are, and
    /// undecided once a typed solution goes into it.
    #[must_use]
    pub const fn absorbed(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unclassified, Self::Unclassified) => Self::Unclassified,
            _ => Self::Undecided,
        }
    }
}

/// One contact two solutions of one solve both stand for (not upstream): two solutions of
/// one solve with the same id are one contact, counted once. A solution merged with or
/// absorbed into another loses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContactId(pub core::num::NonZeroU32);

/// A handle to an object a solution refers to (upstream an `SmObject*`). Two handles are
/// the same object when they compare equal.
pub trait SolutionObject: Copy + PartialEq {
    /// The parameters the object takes in a solution, in order: 0 for a vertex, 1 for a
    /// curve or an edge, 2 for a surface or a face, 3 for a volume. `None` for an object
    /// the parameters of a solution cannot be read past (upstream's `SM_ERR` for an object
    /// of no known kind).
    fn parameter_count(&self) -> Option<usize>;

    /// The interval of the object's parameter `axis` (`GetNaturalInterval`, the edge
    /// interval, an axis of the UV domain); `None` when the object has no such parameter.
    fn parameter_interval(&self, axis: usize) -> Option<Extent1d>;

    /// The object's zone tolerance (`SmTol::GetZoneTol3d`).
    fn tolerance(&self) -> f64;
}

/// One point of a solution space (`SmSolutionEnd`): a solution value (typically a distance)
/// and the parameters of the objects there, concatenated in object order. Entries past the
/// solution's parameter count are 0.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SolutionEnd {
    /// The solution value (`m_dSolutionValue`).
    pub value: f64,
    /// The parameters (`m_adParameters`).
    pub params: [f64; MAX_PARAMS],
}

impl SolutionEnd {
    /// An end with `value` and `params` (the rest 0).
    #[must_use]
    pub fn new<const V: usize>(value: f64, params: [f64; V]) -> Self {
        const {
            assert!(
                V <= MAX_PARAMS,
                "a solution end holds at most MAX_PARAMS parameters"
            );
        };
        let mut all = [0.0; MAX_PARAMS];
        for (a, p) in all.iter_mut().zip(params) {
            *a = p;
        }
        Self { value, params: all }
    }

    /// Parameter `i`, 0 past [`MAX_PARAMS`].
    #[must_use]
    pub fn param(&self, i: usize) -> f64 {
        self.params.get(i).copied().unwrap_or(0.0)
    }
}

/// Which end of a solution [`Solution::classify_end`] reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum End {
    /// The start (`compare_start` true).
    Start,
    /// The end; a point's start (`compare_start` false).
    End,
}

/// Where a solution end lies against a solution ([`Solution::classify_end`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndClass {
    /// Below the start of the range (or, against a point, smaller in the first parameter).
    BelowStart,
    /// At the start of the range.
    AtStart,
    /// At the point, or inside the range.
    Within,
    /// At the end of the range.
    AtEnd,
    /// Above the end of the range (or, against a point, larger in the first parameter).
    AboveEnd,
    /// No consistent answer: other counts or objects, unequal values at a matching point,
    /// or parameters that disagree.
    Inconsistent,
}

impl EndClass {
    /// At the start, within or at the end (upstream `abs(class) <= 1`).
    #[must_use]
    pub const fn is_at_or_within(self) -> bool {
        matches!(self, Self::AtStart | Self::Within | Self::AtEnd)
    }
}

/// The result of a solver (`SmSolution`): a point, or a range of points, in the joint
/// parameter space of up to [`MAX_OBJECTS`] objects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Solution<O> {
    kind: SolutionKind,
    variables: usize,
    start: SolutionEnd,
    end: SolutionEnd,
    object_count: usize,
    objects: [Option<O>; MAX_OBJECTS],
    nodes: [Option<NodeId>; MAX_OBJECTS],
    /// How the solution meets the surface (not upstream); see [`Contact`].
    pub contact: Contact,
    /// The `(u, v)` box `[[u0, u1], [v0, v1]]` a certified root is the only root in, for a
    /// solution typed a crossing by that root (not upstream).
    pub unique: Option<[[f64; 2]; 2]>,
    /// The one contact this solution and another of the same solve stand for (not
    /// upstream); see [`ContactId`].
    pub shared_contact: Option<ContactId>,
}

fn options<const K: usize, A: Copy>(items: [A; K]) -> [Option<A>; MAX_OBJECTS] {
    const {
        assert!(
            K <= MAX_OBJECTS,
            "a solution refers to at most MAX_OBJECTS objects"
        );
    };
    let mut all = [None; MAX_OBJECTS];
    for (a, i) in all.iter_mut().zip(items) {
        *a = Some(i);
    }
    all
}

impl<O: Copy> Solution<O> {
    fn blank(kind: SolutionKind, variables: usize, start: SolutionEnd, end: SolutionEnd) -> Self {
        Self {
            kind,
            variables,
            start,
            end,
            object_count: 0,
            objects: [None; MAX_OBJECTS],
            nodes: [None; MAX_OBJECTS],
            contact: Contact::Unclassified,
            unique: None,
            shared_contact: None,
        }
    }

    /// A point solution: value and parameters (`SM_ST_SINGLE_VALUE`).
    #[must_use]
    pub fn point<const V: usize>(value: f64, params: [f64; V]) -> Self {
        let e = SolutionEnd::new(value, params);
        Self::blank(SolutionKind::Point, V, e, e)
    }

    /// A range solution from `start` to `end` (`SM_ST_RANGE_OF_VALUES`).
    #[must_use]
    pub fn range<const V: usize>(start: (f64, [f64; V]), end: (f64, [f64; V])) -> Self {
        Self::blank(
            SolutionKind::Range,
            V,
            SolutionEnd::new(start.0, start.1),
            SolutionEnd::new(end.0, end.1),
        )
    }

    /// The nodes a solution may lie in (`SM_ST_NODES_ONLY`), one per tree.
    #[must_use]
    pub fn nodes<const K: usize>(nodes: [NodeId; K]) -> Self {
        let mut s = Self::blank(
            SolutionKind::Nodes,
            0,
            SolutionEnd::default(),
            SolutionEnd::default(),
        );
        s.nodes = options(nodes);
        s
    }

    /// The solution with its objects (`None` for one a solver does not name).
    #[must_use]
    pub fn with_objects<const K: usize>(mut self, objects: [Option<O>; K]) -> Self {
        const {
            assert!(
                K <= MAX_OBJECTS,
                "a solution refers to at most MAX_OBJECTS objects"
            );
        };
        let mut all = [None; MAX_OBJECTS];
        for (a, o) in all.iter_mut().zip(objects) {
            *a = o;
        }
        self.objects = all;
        self.object_count = K;
        self
    }

    /// The solution with the tree nodes it was found in.
    #[must_use]
    pub fn with_nodes<const K: usize>(mut self, nodes: [NodeId; K]) -> Self {
        self.nodes = options(nodes);
        self
    }

    /// What the solution is.
    #[must_use]
    pub const fn kind(&self) -> SolutionKind {
        self.kind
    }

    /// The number of parameters in each end.
    #[must_use]
    pub const fn variables(&self) -> usize {
        self.variables
    }

    /// The point, or the start of the range.
    #[must_use]
    pub const fn start(&self) -> &SolutionEnd {
        &self.start
    }

    /// The end of the range (a point's is its start as made).
    #[must_use]
    pub const fn end(&self) -> &SolutionEnd {
        &self.end
    }

    /// The start, to change its value or parameters (a caller rescaling them).
    pub fn start_mut(&mut self) -> &mut SolutionEnd {
        &mut self.start
    }

    /// The end, to change its value or parameters.
    pub fn end_mut(&mut self) -> &mut SolutionEnd {
        &mut self.end
    }

    /// The value of the start (`m_vStart.m_dSolutionValue`).
    #[must_use]
    pub const fn value(&self) -> f64 {
        self.start.value
    }

    /// The objects, `None` where the solver named none.
    #[must_use]
    pub fn objects(&self) -> &[Option<O>] {
        self.objects.get(..self.object_count).unwrap_or(&[])
    }

    /// The tree nodes, one slot per tree (`None` past the solve's trees).
    #[must_use]
    pub const fn tree_nodes(&self) -> &[Option<NodeId>; MAX_OBJECTS] {
        &self.nodes
    }

    /// The solution turned into a range from its start to `end`.
    pub(crate) fn make_range(&mut self, end: SolutionEnd) {
        self.kind = SolutionKind::Range;
        self.end = end;
    }

    /// The range ends replaced.
    pub(crate) fn set_ends(&mut self, start: SolutionEnd, end: SolutionEnd) {
        self.start = start;
        self.end = end;
    }
}

/// `sm_AreSequential`: strictly ascending or strictly descending.
fn are_sequential(a: f64, b: f64, c: f64) -> bool {
    (a < b && b < c) || (a > b && b > c)
}

/// `smos_Max` of three: `a > b ? (a > c ? a : c) : (b > c ? b : c)`.
pub(crate) fn max3(a: f64, b: f64, c: f64) -> f64 {
    if a > b {
        if a > c { a } else { c }
    } else if b > c {
        b
    } else {
        c
    }
}

impl<O: SolutionObject> Solution<O> {
    fn same_objects(&self, other: &Self) -> bool {
        self.objects()
            .iter()
            .zip(other.objects())
            .all(|(a, b)| a == b)
    }

    /// Where `other`'s start (or, for [`End::End`] and a range `other`, its end) lies against
    /// this solution (`ClassifySolutionEnd`).
    #[must_use]
    pub fn classify_end(&self, other: &Self, which: End) -> EndClass {
        // can't compare different variable or object counts, or different objects
        if self.variables != other.variables
            || self.object_count != other.object_count
            || !self.same_objects(other)
        {
            return EndClass::Inconsistent;
        }
        let e = if which == End::Start || other.kind == SolutionKind::Point {
            &other.start
        } else {
            &other.end
        };
        let n = self.variables;
        if self.kind == SolutionKind::Point {
            let mut ret = EndClass::Inconsistent;
            // can't compare unequal solution values
            if EFF_ZERO_SQRT < (self.start.value - e.value).abs() {
                return ret;
            }
            for (&s, &o) in self.start.params.iter().zip(&e.params).take(n) {
                let scaled_zero = EFF_ZERO * (1.0 + s.abs());
                if (s - o).abs() < scaled_zero {
                    if ret != EndClass::Inconsistent && ret != EndClass::Within {
                        continue;
                    }
                    ret = EndClass::Within;
                    continue;
                }
                // had a match and lost it
                if ret == EndClass::Within {
                    return EndClass::Inconsistent;
                }
                // lower or higher, by the first parameter (as upstream)
                ret = if e.param(0) < self.start.param(0) {
                    EndClass::BelowStart
                } else {
                    EndClass::AboveEnd
                };
                break;
            }
            return ret;
        }
        // a range (or a nodes-only solution, read as one, as upstream)
        let mut ret = EndClass::Inconsistent;
        for ((&s, &t), &o) in self
            .start
            .params
            .iter()
            .zip(&self.end.params)
            .zip(&e.params)
            .take(n)
        {
            let scaled_zero = (1.0 + o.abs()).abs() * EFF_ZERO;
            // a degenerate parameter range: ambiguous, go on
            if (s - o).abs() < scaled_zero && (t - o).abs() < scaled_zero {
                continue;
            }
            if (s - o).abs() < scaled_zero {
                if matches!(
                    ret,
                    EndClass::BelowStart | EndClass::AboveEnd | EndClass::AtEnd
                ) {
                    return EndClass::Inconsistent;
                }
                if ret != EndClass::Inconsistent && ret != EndClass::AtStart {
                    continue;
                }
                if (self.start.value - e.value).abs() > EFF_ZERO_SQRT {
                    return EndClass::Inconsistent;
                }
                ret = EndClass::AtStart;
                continue;
            }
            if (t - o).abs() < scaled_zero {
                if matches!(
                    ret,
                    EndClass::BelowStart | EndClass::AboveEnd | EndClass::AtStart
                ) {
                    return EndClass::Inconsistent;
                }
                if ret != EndClass::Inconsistent && ret != EndClass::AtEnd {
                    continue;
                }
                if (self.end.value - e.value).abs() > EFF_ZERO_SQRT {
                    return EndClass::Inconsistent;
                }
                ret = EndClass::AtEnd;
                continue;
            }
            let class = if are_sequential(s, o, t) {
                EndClass::Within
            } else if are_sequential(o, s, t) {
                EndClass::BelowStart
            } else if are_sequential(s, t, o) {
                EndClass::AboveEnd
            } else {
                return EndClass::Inconsistent;
            };
            if ret != EndClass::Inconsistent && ret != class {
                return EndClass::Inconsistent;
            }
            ret = class;
        }
        ret
    }

    /// The object behind parameter `index` and the axis of that object the parameter is
    /// (`GetObjectIndexForParameterIndex`); `None` for an index past the parameters, a
    /// missing object before it or an object of unknown parameter count.
    #[must_use]
    pub fn object_for_parameter(&self, index: usize) -> Option<(usize, usize)> {
        if index >= self.variables {
            return None;
        }
        let mut first = 0;
        for (i, object) in self.objects().iter().enumerate() {
            // some solvers do not store an object for every parameter
            let count = (*object)?.parameter_count()?;
            if index < first + count {
                return Some((i, index - first));
            }
            first += count;
        }
        None
    }

    /// The interval of the object parameter `index` belongs to (`GetParameterInterval`).
    #[must_use]
    pub fn parameter_interval(&self, index: usize) -> Option<Extent1d> {
        let (object, axis) = self.object_for_parameter(index)?;
        (*self.objects().get(object)?)?.parameter_interval(axis)
    }

    /// The zone tolerance of object `index`; 0 past the objects or for a missing one
    /// (`GetObjectTolerance`).
    #[must_use]
    pub fn object_tolerance(&self, index: usize) -> f64 {
        match self.objects().get(index) {
            Some(Some(o)) => o.tolerance(),
            _ => 0.0,
        }
    }

    /// Whether every parameter of the two is within `(1 + |p|) factor SM_EFF_ZERO` of the
    /// start parameter `p` or `1e-4` of either parameter's interval (both ends of a range).
    fn parameters_match(&self, other: &Self, factor: f64) -> bool {
        let n = self.variables;
        for i in 0..n {
            let length = |s: &Self| s.parameter_interval(i).map_or(0.0, |v| 0.0001 * v.length());
            let a = self.start.param(i);
            let scaled_zero = max3(
                (1.0 + a.abs()).abs() * factor * EFF_ZERO,
                length(self),
                length(other),
            );
            if (a - other.start.param(i)).abs() > scaled_zero
                || (self.kind == SolutionKind::Range
                    && (self.end.param(i) - other.end.param(i)).abs() > scaled_zero)
            {
                return false;
            }
        }
        true
    }

    /// Whether `other` is the same solution (`IsEqual`): the same counts, kind and objects,
    /// values within `SM_EFF_ZERO_SQRT`, and every parameter (both ends of a range) within
    /// the larger of `SM_EFF_ZERO` scaled and `1e-4` of its interval.
    #[must_use]
    pub fn is_equal(&self, other: &Self) -> bool {
        self.object_count == other.object_count
            && self.variables == other.variables
            && self.kind == other.kind
            && EFF_ZERO_SQRT >= (self.start.value - other.start.value).abs()
            && self.same_objects(other)
            && self.parameters_match(other, 1.0)
    }
}

/// How two solutions relate ([`identical`], `SmSolutionPairType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pairing {
    /// The same solution; keep the one named (`SM_SP_IDENTICAL`).
    Identical(Prefer),
    /// Another kind, object or variable count, or value (`SM_SP_DIFFCOUNTS`).
    DifferentCounts,
    /// Other objects (`SM_SP_DIFFOBJECTS`).
    DifferentObjects,
    /// Only the parameters differ (`SM_SP_DIFFPARAMETERS`).
    DifferentParameters,
}

/// Which of two identical solutions to keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Prefer {
    /// The first (the new solution, in the accumulator).
    First,
    /// The second (the one held).
    Second,
}

/// Whether two solutions are the same (`SmGlobalSolver::IdenticalSolutions`): the same
/// counts, kind and objects, values within `SM_EFF_ZERO_SQRT`, and parameters within
/// `100 SM_EFF_ZERO` scaled or `1e-4` of their interval; then the smaller value is
/// preferred, or the larger when `larger_is_better` (solidcore WR-21: the plain maximising
/// operations).
#[must_use]
pub fn identical<O: SolutionObject>(
    a: &Solution<O>,
    b: &Solution<O>,
    larger_is_better: bool,
) -> Pairing {
    if a.object_count != b.object_count
        || a.variables != b.variables
        || a.kind != b.kind
        || EFF_ZERO_SQRT < (a.start.value - b.start.value).abs()
    {
        return Pairing::DifferentCounts;
    }
    if !a.same_objects(b) {
        return Pairing::DifferentObjects;
    }
    // (SM_EFF_ZERO is too small to use here)
    if !a.parameters_match(b, 100.0) {
        return Pairing::DifferentParameters;
    }
    let first = if larger_is_better {
        a.start.value > b.start.value
    } else {
        a.start.value < b.start.value
    };
    Pairing::Identical(if first { Prefer::First } else { Prefer::Second })
}

/// Inserts `new` into `solutions` in `key` order, smallest first, unless it
/// [equals](Solution::is_equal) one held, which then keeps the smaller absolute value and
/// both contacts merged (`SmSolutionArray::AddSortedSolution`). The index it went to, or
/// `None` for a duplicate.
pub fn add_sorted<O: SolutionObject>(
    solutions: &mut Vec<Solution<O>>,
    new: &Solution<O>,
    key: SortKey,
) -> Option<usize> {
    let mut at = None;
    for (ii, old) in solutions.iter_mut().enumerate() {
        if old.is_equal(new) {
            let contact = old.contact.merged(new.contact);
            if new.start.value.abs() < old.start.value.abs() {
                *old = *new;
            }
            old.contact = contact;
            old.shared_contact = None;
            return None;
        }
        let (old_v, new_v) = (old.start.value, new.start.value);
        let by_value = |old_v: f64, new_v: f64| old_v > new_v;
        let by_parameters = |old: &Solution<O>| {
            for (&o, &n) in old
                .start
                .params
                .iter()
                .zip(&new.start.params)
                .take(old.variables)
            {
                let scale = (1.0 + o) * EFF_ZERO_SQRT;
                if o > n + scale {
                    return true;
                } else if o < n - scale {
                    return false;
                }
            }
            false
        };
        let before = match key {
            SortKey::BySolutionValue => by_value(old_v, new_v),
            SortKey::ByFirstParameter => old.start.param(0) > new.start.param(0),
            SortKey::ByFirstTwoParameters => {
                let (o0, n0) = (old.start.param(0), new.start.param(0));
                let scale = (1.0 + o0) * EFF_ZERO_SQRT;
                o0 > n0 + scale
                    || ((o0 - n0).abs() < scale && old.start.param(1) > new.start.param(1))
            }
            SortKey::ByValueWithParameter => {
                let scale = (1.0 + old_v) * EFF_ZERO_SQRT;
                if (old_v - new_v).abs() > scale {
                    by_value(old_v, new_v)
                } else {
                    by_parameters(old)
                }
            }
            SortKey::ByAllParameters => by_parameters(old),
        };
        if before {
            at = Some(ii);
            break;
        }
    }
    let at = at.unwrap_or(solutions.len());
    solutions.insert(at, *new);
    Some(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Obj {
        Curve(f64),
        Surface,
    }

    impl SolutionObject for Obj {
        fn parameter_count(&self) -> Option<usize> {
            Some(match self {
                Self::Curve(_) => 1,
                Self::Surface => 2,
            })
        }
        fn parameter_interval(&self, axis: usize) -> Option<Extent1d> {
            match (self, axis) {
                (Self::Curve(l), 0) => Some(Extent1d::spanning(0.0, *l)),
                (Self::Surface, 0) => Some(Extent1d::spanning(0.0, 1.0)),
                (Self::Surface, 1) => Some(Extent1d::spanning(0.0, 100.0)),
                _ => None,
            }
        }
        fn tolerance(&self) -> f64 {
            1e-6
        }
    }

    #[test]
    fn parameters_map_to_each_object_in_turn() {
        let s = Solution::point(0.0, [0.1, 0.2, 0.3])
            .with_objects([Some(Obj::Curve(1.0)), Some(Obj::Surface)]);
        assert_eq!(s.object_for_parameter(0), Some((0, 0)));
        assert_eq!(s.object_for_parameter(1), Some((1, 0)));
        assert_eq!(s.object_for_parameter(2), Some((1, 1)));
        assert_eq!(s.object_for_parameter(3), None);
        assert_eq!(s.parameter_interval(2).map(|v| v.length()), Some(100.0));
    }

    #[test]
    fn a_point_inside_a_range_is_within() {
        let o = [Some(Obj::Curve(1.0))];
        let r = Solution::range((0.0, [0.2]), (0.0, [0.6])).with_objects(o);
        let p = |t: f64| Solution::point(0.0, [t]).with_objects(o);
        assert_eq!(r.classify_end(&p(0.4), End::Start), EndClass::Within);
        assert_eq!(r.classify_end(&p(0.2), End::Start), EndClass::AtStart);
        assert_eq!(r.classify_end(&p(0.6), End::Start), EndClass::AtEnd);
        assert_eq!(r.classify_end(&p(0.1), End::Start), EndClass::BelowStart);
        assert_eq!(r.classify_end(&p(0.9), End::Start), EndClass::AboveEnd);
        let other = Solution::point(0.0, [0.4]).with_objects([Some(Obj::Curve(2.0))]);
        assert_eq!(r.classify_end(&other, End::Start), EndClass::Inconsistent);
    }

    #[test]
    fn identical_prefers_the_better_value() {
        let o = [Some(Obj::Curve(1.0))];
        let a = Solution::point(1e-9, [0.5]).with_objects(o);
        let b = Solution::point(2e-9, [0.5 + 1e-5]).with_objects(o);
        assert_eq!(identical(&a, &b, false), Pairing::Identical(Prefer::First));
        assert_eq!(identical(&a, &b, true), Pairing::Identical(Prefer::Second));
        let c = Solution::point(1e-9, [0.6]).with_objects(o);
        assert_eq!(identical(&a, &c, false), Pairing::DifferentParameters);
    }

    #[test]
    fn add_sorted_orders_and_drops_duplicates() {
        let o = [Some(Obj::Curve(1.0))];
        let mut v = Vec::new();
        assert_eq!(
            add_sorted(
                &mut v,
                &Solution::point(0.0, [0.5]).with_objects(o),
                SortKey::ByFirstParameter
            ),
            Some(0)
        );
        assert_eq!(
            add_sorted(
                &mut v,
                &Solution::point(0.0, [0.1]).with_objects(o),
                SortKey::ByFirstParameter
            ),
            Some(0)
        );
        assert_eq!(
            add_sorted(
                &mut v,
                &Solution::point(0.0, [0.9]).with_objects(o),
                SortKey::ByFirstParameter
            ),
            Some(2)
        );
        assert_eq!(
            add_sorted(
                &mut v,
                &Solution::point(0.0, [0.9]).with_objects(o),
                SortKey::ByFirstParameter
            ),
            None
        );
        let firsts: Vec<f64> = v.iter().map(|s| s.start().param(0)).collect();
        assert_eq!(firsts, [0.1, 0.5, 0.9]);
    }
}
