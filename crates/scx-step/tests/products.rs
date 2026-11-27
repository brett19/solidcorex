// Written for solidcorex (docs/research/74-step6-idiom-prep.md §27).
//! The product tree on small synthetic files: placements of occurrences and mapped items, the
//! composition order, shared bodies, the placements a file does not give, units across
//! representations. The truths are closed forms (a quarter turn is `(x, y) -> (-y, x)`).

#![allow(
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use scx_num::math::Affine;
use scx_num::vector3d::{Point3, Vec3};
use scx_p21::parse;
use scx_p21::units::{MissingUnits, UnitsFault};
use scx_step::{ProductTree, ReadOptions, StepError, Unplaced, World, read};

const HEADER: &str = "ISO-10303-21;\nHEADER;FILE_DESCRIPTION(('t'),'2;1');\
FILE_NAME('t','',(''),(''),'','','');FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));ENDSEC;\nDATA;\n\
#8=PRODUCT_DEFINITION_CONTEXT('part definition',#9,'design');\n#9=APPLICATION_CONTEXT('');\n\
#1=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));\n\
#2=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.));\n\
#3=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNIT_ASSIGNED_CONTEXT((#1,#2))\
REPRESENTATION_CONTEXT('',''));\n\
#70=(CONVERSION_BASED_UNIT('INCH',#71)LENGTH_UNIT()NAMED_UNIT(#72));\n\
#71=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(25.4),#1);\n\
#72=DIMENSIONAL_EXPONENTS(1.,0.,0.,0.,0.,0.,0.);\n\
#4=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNIT_ASSIGNED_CONTEXT((#70,#2))\
REPRESENTATION_CONTEXT('',''));\n\
#5=(GEOMETRIC_REPRESENTATION_CONTEXT(3)REPRESENTATION_CONTEXT('',''));\n\
#90=CLOSED_SHELL('',(#91));#91=ADVANCED_FACE('',(),#92,.T.);#92=PLANE('',#93);\n\
#93=AXIS2_PLACEMENT_3D('',#94,$,$);#94=CARTESIAN_POINT('',(0.,0.,0.));\n";

fn tree_with(data: &str, opts: &ReadOptions) -> Result<ProductTree, StepError> {
    let text = format!("{HEADER}{data}\nENDSEC;\nEND-ISO-10303-21;\n");
    let parsed = parse(&text).unwrap_or_else(|e| panic!("{}\n{text}", e.one_line()));
    read(&parsed.exchange, opts).map(|i| i.products)
}

fn tree(data: &str) -> ProductTree {
    tree_with(data, &ReadOptions::default()).unwrap()
}

fn axis(k: u64, origin: [f64; 3], z: [f64; 3], x: [f64; 3]) -> String {
    format!(
        "#{k}=AXIS2_PLACEMENT_3D('',#{},#{},#{});\n#{}=CARTESIAN_POINT('',({:?},{:?},{:?}));\n\
         #{}=DIRECTION('',({:?},{:?},{:?}));\n#{}=DIRECTION('',({:?},{:?},{:?}));\n",
        k + 1,
        k + 2,
        k + 3,
        k + 1,
        origin[0],
        origin[1],
        origin[2],
        k + 2,
        z[0],
        z[1],
        z[2],
        k + 3,
        x[0],
        x[1],
        x[2]
    )
}

fn at(k: u64, origin: [f64; 3]) -> String {
    axis(k, origin, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0])
}

/// A placement with no reference direction.
fn axis_only(k: u64, z: [f64; 3]) -> String {
    format!(
        "#{k}=AXIS2_PLACEMENT_3D('',#{},#{},$);\n#{}=CARTESIAN_POINT('',(0.,0.,0.));\n\
         #{}=DIRECTION('',({:?},{:?},{:?}));\n",
        k + 1,
        k + 2,
        k + 1,
        k + 2,
        z[0],
        z[1],
        z[2]
    )
}

/// Product definition #pd with its shape representation #rep (in context #ctx) listing `items`.
fn part_in(ctx: u64, pd: u64, rep: u64, items: &str) -> String {
    format!(
        "#{}=PRODUCT('p{pd}','p{pd}','',());\n\
         #{}=PRODUCT_DEFINITION_FORMATION('','',#{});\n\
         #{pd}=PRODUCT_DEFINITION('design','',#{},#8);\n\
         #{}=PRODUCT_DEFINITION_SHAPE('','',#{pd});\n\
         #{rep}=SHAPE_REPRESENTATION('',({items}),#{ctx});\n\
         #{}=SHAPE_DEFINITION_REPRESENTATION(#{},#{rep});\n",
        pd + 1,
        pd + 2,
        pd + 1,
        pd + 2,
        pd + 3,
        pd + 4,
        pd + 3
    )
}

fn part(pd: u64, rep: u64, items: &str) -> String {
    part_in(3, pd, rep, items)
}

fn solid(id: u64) -> String {
    format!("#{id}=MANIFOLD_SOLID_BREP('b{id}',#90);\n")
}

/// Occurrence #n of definition `child` in `parent`, with a relationship between `reps` and a
/// transformation between `items`: PDS #n+1, transformation #n+2, relationship #n+3, CDSR #n+4.
fn occurrence(
    n: u64,
    (parent, child): (u64, u64),
    (r1, r2): (u64, u64),
    (i1, i2): (u64, u64),
) -> String {
    format!(
        "#{n}=NEXT_ASSEMBLY_USAGE_OCCURRENCE('o{n}','n{n}','',#{parent},#{child},'ref{n}');\n\
         #{}=PRODUCT_DEFINITION_SHAPE('','',#{n});\n\
         #{}=ITEM_DEFINED_TRANSFORMATION('','',#{i1},#{i2});\n\
         #{}=(REPRESENTATION_RELATIONSHIP('','',#{r1},#{r2})\
         REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION(#{})SHAPE_REPRESENTATION_RELATIONSHIP());\n\
         #{}=CONTEXT_DEPENDENT_SHAPE_REPRESENTATION(#{},#{});\n",
        n + 1,
        n + 2,
        n + 3,
        n + 2,
        n + 4,
        n + 3,
        n + 1
    )
}

/// Occurrence #n placed by a placement or operator #frame written as the transformation itself
/// (in `rep_2`'s units): relationship #n+3, CDSR #n+4.
fn occurrence_by_frame(
    n: u64,
    (parent, child): (u64, u64),
    (r1, r2): (u64, u64),
    frame: u64,
) -> String {
    format!(
        "#{n}=NEXT_ASSEMBLY_USAGE_OCCURRENCE('o{n}','n{n}','',#{parent},#{child},$);\n\
         #{}=PRODUCT_DEFINITION_SHAPE('','',#{n});\n\
         #{}=(REPRESENTATION_RELATIONSHIP('','',#{r1},#{r2})\
         REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION(#{frame})SHAPE_REPRESENTATION_RELATIONSHIP());\n\
         #{}=CONTEXT_DEPENDENT_SHAPE_REPRESENTATION(#{},#{});\n",
        n + 1,
        n + 3,
        n + 4,
        n + 3,
        n + 1
    )
}

/// An occurrence with no placement at all.
fn bare_occurrence(n: u64, (parent, child): (u64, u64)) -> String {
    format!("#{n}=NEXT_ASSEMBLY_USAGE_OCCURRENCE('o{n}','','',#{parent},#{child},$);\n")
}

const TOL: f64 = 1e-12;

fn placed(w: &World) -> Affine {
    match w {
        World::Placed(a) => *a,
        World::Unplaced(u) => panic!("not placed: {u:?}"),
    }
}

fn near(a: Vec3, b: [f64; 3]) -> bool {
    (a.x - b[0]).abs() < TOL && (a.y - b[1]).abs() < TOL && (a.z - b[2]).abs() < TOL
}

fn body_world(t: &ProductTree, body: u64) -> Affine {
    let mut all = t.placements_of(body);
    let first = all.next().unwrap_or_else(|| panic!("no body #{body}"));
    placed(&first.world)
}

#[track_caller]
fn assert_map(a: Affine, x: [f64; 3], y: [f64; 3], z: [f64; 3], t: [f64; 3]) {
    assert!(
        near(a.x_axis(), x)
            && near(a.y_axis(), y)
            && near(a.z_axis(), z)
            && near(a.translation_part(), t),
        "got {a:?}"
    );
}

/// A two-part assembly: root #10 (rep 1000, a placement #300 at +754 X), child #20 (rep 2000,
/// body 101 and its own placement #400). `rep_1_is_child` and `item_1_in_parent` pick how the
/// relationship and the transformation are written.
fn two_parts(rep_1_is_child: bool, item_1_in_parent: bool) -> ProductTree {
    let items = if item_1_in_parent {
        (300, 400)
    } else {
        (400, 300)
    };
    let reps = if rep_1_is_child {
        (2000, 1000)
    } else {
        (1000, 2000)
    };
    tree(
        &[
            solid(101),
            at(300, [754.0, 0.0, 0.0]),
            at(400, [0.0; 3]),
            part(10, 1000, "#300"),
            part(20, 2000, "#101,#400"),
            occurrence(500, (10, 20), reps, items),
        ]
        .concat(),
    )
}

#[test]
fn the_child_sits_at_the_parent_placement_however_the_transformation_is_written() {
    for (rep_1_is_child, item_1_in_parent) in [(false, true), (true, true), (true, false)] {
        let t = two_parts(rep_1_is_child, item_1_in_parent);
        assert_eq!(t.placed.len(), 1);
        assert_map(
            body_world(&t, 101),
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [754.0, 0.0, 0.0],
        );
    }
}

#[test]
fn instances_are_the_expanded_assembly_with_their_parts_and_occurrences() {
    let t = two_parts(false, true);
    assert_eq!(t.parts.len(), 2);
    assert_eq!(
        (t.parts[0].definition, t.parts[0].name.as_str()),
        (10, "p10")
    );
    assert_eq!(t.instances.len(), 2);
    let (root, child) = (&t.instances[0], &t.instances[1]);
    assert_eq!((root.part, root.parent, root.by.is_none()), (0, None, true));
    assert_eq!(placed(&root.world), Affine::IDENTITY);
    assert_eq!((child.part, child.parent), (1, Some(0)));
    let by = child.by.as_ref().unwrap();
    assert_eq!(
        (
            by.entity,
            by.id.as_str(),
            by.name.as_str(),
            by.reference_designator.as_deref()
        ),
        (500, "o500", "n500", Some("ref500"))
    );
    assert_eq!(t.placed[0].instance, 1);
    assert_eq!(t.placed[0].representation, 2000);
    assert!(t.placed[0].mapped_by.is_empty());
}

/// A in the root turned a quarter turn about Z, B moved 10 along X inside A: B's origin goes to
/// (10,0,0) in A and to (0,10,0) in the world. The other composition order would give (10,0,0).
#[test]
fn nested_placements_compose_the_child_before_the_parent() {
    let t = tree(
        &[
            solid(101),
            solid(102),
            axis(300, [0.0; 3], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
            at(310, [0.0; 3]),
            at(320, [10.0, 0.0, 0.0]),
            at(330, [0.0; 3]),
            part(10, 1000, "#300"),
            part(20, 2000, "#101,#310,#320"),
            part(30, 3000, "#102,#330"),
            occurrence(500, (10, 20), (2000, 1000), (310, 300)),
            occurrence(510, (20, 30), (3000, 2000), (330, 320)),
        ]
        .concat(),
    );
    assert_eq!(t.placed.len(), 2);
    let turn = ([0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    assert_map(body_world(&t, 101), turn.0, turn.1, turn.2, [0.0; 3]);
    assert_map(
        body_world(&t, 102),
        turn.0,
        turn.1,
        turn.2,
        [0.0, 10.0, 0.0],
    );
}

/// The map origin (translation by (1,0,0)) goes onto the target (a quarter turn at (5,0,0)):
/// `p -> (p - (1,0,0))` turned, plus (5,0,0); the origin lands on (5,0,0). The other order,
/// target first, would put it at (4,0,0).
#[test]
fn a_mapped_item_places_its_origin_onto_its_target() {
    let t = tree(
        &[
            solid(101),
            at(300, [1.0, 0.0, 0.0]),
            axis(310, [5.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
            "#400=SHAPE_REPRESENTATION('',(#101,#300),#3);\n#401=REPRESENTATION_MAP(#300,#400);\n\
         #402=MAPPED_ITEM('',#401,#310);\n"
                .to_string(),
            part(10, 1000, "#402,#310"),
        ]
        .concat(),
    );
    assert_eq!(t.placed.len(), 1);
    let p = &t.placed[0];
    assert_eq!(
        (p.body, p.representation, p.mapped_by.clone()),
        (101, 400, vec![402])
    );
    let w = placed(&p.world);
    assert!(near(
        w.apply(Point3::new(1.0, 0.0, 0.0)).to_vec(),
        [5.0, 0.0, 0.0]
    ));
    assert_map(
        w,
        [0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        [5.0, -1.0, 0.0],
    );
}

/// Two uses of one part: its body is placed twice, once per instance, in occurrence order.
#[test]
fn a_shared_part_is_placed_once_per_use() {
    let t = tree(
        &[
            solid(101),
            at(300, [1.0, 0.0, 0.0]),
            at(310, [2.0, 0.0, 0.0]),
            at(400, [0.0; 3]),
            part(10, 1000, "#300,#310"),
            part(20, 2000, "#101,#400"),
            occurrence(500, (10, 20), (1000, 2000), (300, 400)),
            occurrence(510, (10, 20), (1000, 2000), (310, 400)),
        ]
        .concat(),
    );
    let uses: Vec<_> = t.placements_of(101).collect();
    assert_eq!(uses.len(), 2);
    assert_eq!(t.instances.len(), 3);
    assert_ne!((uses[0].instance, uses[1].instance), (0, 0));
    assert!(
        near(placed(&uses[0].world).translation_part(), [1.0, 0.0, 0.0])
            || near(placed(&uses[0].world).translation_part(), [2.0, 0.0, 0.0])
    );
    let mut xs: Vec<f64> = uses
        .iter()
        .map(|u| placed(&u.world).translation_part().x)
        .collect();
    xs.sort_by(f64::total_cmp);
    assert_eq!(xs, vec![1.0, 2.0]);
    assert_eq!(t.bodies_of(uses[0].instance).count(), 1);
}

#[test]
fn an_occurrence_without_a_placement_is_unplaced_not_the_identity() {
    let t = tree(
        &[
            solid(101),
            solid(102),
            part(10, 1000, ""),
            part(20, 2000, "#101"),
            part(30, 3000, "#102"),
            bare_occurrence(500, (10, 20)),
            // The grandchild has a placement of its own, but its parent has none.
            at(310, [0.0; 3]),
            at(320, [0.0; 3]),
            occurrence(510, (20, 30), (2000, 3000), (310, 320)),
        ]
        .concat(),
    );
    let none = World::Unplaced(Unplaced::NoRelationship { occurrence: 500 });
    assert_eq!(t.instances[1].world, none);
    // Listed, with the cause; the grandchild inherits it.
    assert_eq!(t.placements_of(101).next().unwrap().world, none);
    assert_eq!(t.placements_of(102).next().unwrap().world, none);
    assert_eq!(t.instances[2].world, none);
}

#[test]
fn a_relationship_that_is_not_about_the_child_is_unplaced() {
    let t = tree(
        &[
            solid(101),
            at(300, [754.0, 0.0, 0.0]),
            at(400, [0.0; 3]),
            part(10, 1000, "#300"),
            part(20, 2000, "#101,#400"),
            // Relates the root to itself: neither side is the child's.
            occurrence(500, (10, 20), (1000, 1000), (300, 300)),
        ]
        .concat(),
    );
    assert_eq!(
        t.placed[0].world,
        World::Unplaced(Unplaced::NotAboutChild { link: 503 })
    );
}

#[test]
fn a_relationship_without_a_transformation_is_not_a_placement() {
    let t = tree(
        &[
            solid(101),
            part(10, 1000, ""),
            part(20, 2000, "#101"),
            "#500=NEXT_ASSEMBLY_USAGE_OCCURRENCE('o','','',#10,#20,$);\n\
         #501=PRODUCT_DEFINITION_SHAPE('','',#500);\n\
         #503=SHAPE_REPRESENTATION_RELATIONSHIP('','',#2000,#1000);\n\
         #504=CONTEXT_DEPENDENT_SHAPE_REPRESENTATION(#503,#501);\n"
                .to_string(),
        ]
        .concat(),
    );
    assert_eq!(t.placed.len(), 1);
    assert_eq!(
        t.placed[0].world,
        World::Unplaced(Unplaced::NoTransformation { link: 503 })
    );
}

/// Two relationships for one occurrence: equal maps are one placement, different maps are
/// `Several`, never the first of them.
#[test]
fn several_relationships_agree_or_are_refused_as_unplaced() {
    let base = |second: [f64; 3]| {
        [
            solid(101),
            at(300, [5.0, 0.0, 0.0]),
            at(310, second),
            at(400, [0.0; 3]),
            part(10, 1000, "#300,#310"),
            part(20, 2000, "#101,#400"),
            occurrence(500, (10, 20), (1000, 2000), (300, 400)),
            // A second relationship and CDSR for the same occurrence, from #310.
            "#600=ITEM_DEFINED_TRANSFORMATION('','',#310,#400);\n\
             #601=(REPRESENTATION_RELATIONSHIP('','',#1000,#2000)\
             REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION(#600)SHAPE_REPRESENTATION_RELATIONSHIP());\n\
             #602=CONTEXT_DEPENDENT_SHAPE_REPRESENTATION(#601,#501);\n"
                .to_string(),
        ]
        .concat()
    };
    let same = tree(&base([5.0, 0.0, 0.0]));
    assert!(near(
        placed(&same.placed[0].world).translation_part(),
        [5.0, 0.0, 0.0]
    ));
    let differ = tree(&base([6.0, 0.0, 0.0]));
    assert_eq!(
        differ.placed[0].world,
        World::Unplaced(Unplaced::Several { occurrence: 500 })
    );
}

#[test]
fn a_placement_with_no_regular_frame_is_unplaced_and_the_file_is_read() {
    // The reference direction lies along the axis: no X.
    let t = tree(
        &[
            solid(101),
            axis(300, [0.0; 3], [0.0, 0.0, 1.0], [0.0, 0.0, 2.0]),
            at(400, [0.0; 3]),
            part(10, 1000, "#300"),
            part(20, 2000, "#101,#400"),
            occurrence(500, (10, 20), (1000, 2000), (300, 400)),
        ]
        .concat(),
    );
    assert_eq!(
        t.placed[0].world,
        World::Unplaced(Unplaced::Degenerate { entity: 502 })
    );
}

/// ISO 10303-42: an absent reference direction is +Z made square to the axis (+X when the axis
/// is along Z). For the axis +X that is +Z, so the frame is (x, y, z) = (+Z, +Y... z x x),
/// right-handed: x = (0,0,1), z = (1,0,0), y = z × x = (0,-1,0).
#[test]
fn an_absent_reference_direction_is_the_standards() {
    let t = tree(
        &[
            solid(101),
            axis_only(300, [1.0, 0.0, 0.0]),
            axis_only(310, [0.0, 0.0, -1.0]),
            at(400, [0.0; 3]),
            part(10, 1000, "#300"),
            part(20, 2000, "#101,#400"),
            occurrence(500, (10, 20), (1000, 2000), (300, 400)),
            solid(102),
            part(30, 3000, "#310"),
            part(40, 4000, "#102,#400"),
            occurrence(520, (30, 40), (3000, 4000), (310, 400)),
        ]
        .concat(),
    );
    assert_map(
        body_world(&t, 101),
        [0.0, 0.0, 1.0],
        [0.0, -1.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0; 3],
    );
    // An axis along -Z: X, and y = z × x = (0,-1,0).
    assert_map(
        body_world(&t, 102),
        [1.0, 0.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, -1.0],
        [0.0; 3],
    );
}

/// A mapped item whose target is a uniform scale by 2 at (1,0,0): the map's origin is at the
/// world origin, so the body's point (1,0,0) goes to (3,0,0).
#[test]
fn a_transformation_operator_scales() {
    let t = tree(
        &[
            solid(101),
            at(300, [0.0; 3]),
            "#310=CARTESIAN_POINT('',(1.,0.,0.));\n\
         #311=CARTESIAN_TRANSFORMATION_OPERATOR_3D('','',$,$,#310,2.,$);\n\
         #400=SHAPE_REPRESENTATION('',(#101,#300),#3);\n#401=REPRESENTATION_MAP(#300,#400);\n\
         #402=MAPPED_ITEM('',#401,#311);\n"
                .to_string(),
            part(10, 1000, "#402"),
        ]
        .concat(),
    );
    let w = placed(&t.placed[0].world);
    assert!(near(
        w.apply(Point3::new(1.0, 0.0, 0.0)).to_vec(),
        [3.0, 0.0, 0.0]
    ));
    assert_eq!(w.axis_lengths(), [2.0, 2.0, 2.0]);
}

/// Body #101 is read in the inch representation 2000 (the first, in id order, that lists it)
/// and also listed by the millimetre representation 3000: its numbers mean 25.4 times less
/// there, so that placement is scaled by 1/25.4 and the first is not.
#[test]
fn a_body_listed_in_another_length_unit_is_scaled_by_the_ratio() {
    let t = tree(
        &[
            solid(101),
            at(300, [0.0; 3]),
            at(310, [0.0; 3]),
            at(400, [0.0; 3]),
            at(410, [0.0; 3]),
            part_in(3, 10, 1000, "#300,#310"),
            part_in(4, 20, 2000, "#101,#400"),
            part_in(3, 30, 3000, "#101,#410"),
            occurrence(500, (10, 20), (1000, 2000), (300, 400)),
            occurrence(510, (10, 30), (1000, 3000), (310, 410)),
        ]
        .concat(),
    );
    let uses: Vec<_> = t.placements_of(101).collect();
    assert_eq!(uses.len(), 2);
    let (inch, mm) = (&uses[0], &uses[1]);
    assert_eq!((inch.representation, mm.representation), (2000, 3000));
    assert_eq!(placed(&inch.world), Affine::IDENTITY);
    let s = 1.0 / 25.4;
    assert_eq!(placed(&mm.world).axis_lengths(), [s, s, s]);
}

#[test]
fn a_body_listed_where_the_length_unit_is_missing_is_unplaced_unless_assumed() {
    let data = [
        solid(101),
        at(300, [0.0; 3]),
        at(310, [0.0; 3]),
        at(400, [0.0; 3]),
        at(410, [0.0; 3]),
        part_in(3, 10, 1000, "#300,#310"),
        part_in(3, 20, 2000, "#101,#400"),
        // No frames here: a frame in a context with no length unit is refused by the reader.
        part_in(5, 30, 3000, "#101"),
        occurrence(500, (10, 20), (1000, 2000), (300, 400)),
        occurrence_by_frame(510, (10, 30), (3000, 1000), 310),
    ]
    .concat();
    let t = tree(&data);
    assert_eq!(
        t.placed[1].world,
        World::Unplaced(Unplaced::Units {
            representation: 3000,
            fault: UnitsFault::Missing(MissingUnits {
                length: true,
                angle: true
            }),
        })
    );
    // Assuming inches for it: the file's own mm wins where it names one, so 3000 is inches.
    let units = scx_p21::units::Units {
        len: 25.4,
        ang: 1.0,
    };
    let t = tree_with(&data, &ReadOptions::default().assuming_units(units)).unwrap();
    assert_eq!(
        placed(&t.placed[1].world).axis_lengths(),
        [25.4, 25.4, 25.4]
    );
}

#[test]
fn an_assembly_that_uses_itself_is_a_structure_error() {
    let err = tree_with(
        &[
            solid(101),
            at(300, [0.0; 3]),
            part(10, 1000, "#101,#300"),
            occurrence(500, (10, 10), (1000, 1000), (300, 300)),
        ]
        .concat(),
        &ReadOptions::default(),
    )
    .unwrap_err();
    assert!(matches!(err, StepError::Structure(_)), "{err:?}");
}

#[test]
fn reading_twice_gives_the_same_tree() {
    let data = [
        solid(101),
        at(300, [754.0, 0.0, 0.0]),
        at(400, [0.0; 3]),
        part(10, 1000, "#300"),
        part(20, 2000, "#101,#400"),
        occurrence(500, (10, 20), (1000, 2000), (300, 400)),
    ]
    .concat();
    assert_eq!(tree(&data), tree(&data));
}

/// One shape written in two representations related without a transformation lists its body in
/// both; it is one body placed once, not twice.
#[test]
fn one_shape_in_two_related_representations_is_placed_once() {
    let t = tree(
        &[
            solid(101),
            part(10, 1000, "#101"),
            "#600=SHAPE_REPRESENTATION('',(#101),#3);\n\
         #601=SHAPE_REPRESENTATION_RELATIONSHIP('','',#1000,#600);\n"
                .to_string(),
        ]
        .concat(),
    );
    assert_eq!(t.placed.len(), 1);
    assert_eq!(t.placed[0].representation, 600);
    assert_eq!(placed(&t.placed[0].world), Affine::IDENTITY);
}
