// Written for solidcorex.
//! The skeleton on small synthetic AP203, AP214 and AP242 files: units, uncertainty, the
//! result, the errors, and the entity plan of solids and sheets. No external files.

#![allow(
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use scx_num::error::ErrorKind;
use scx_p21::protocol::{Ap242Edition, Doubt, Protocol};
use scx_p21::units::Units;
use scx_p21::{Exchange, parse};
use scx_step::{
    ImportWarning, MissingUnits, Pending, ReadOptions, ShellRole, SkipReason, SolidOrSheet,
    StepError, Uncertainty, read,
};

const AP203: &str = "'CONFIG_CONTROL_DESIGN'";
const AP214: &str = "'AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'";
const AP242: &str = "'AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF { 1 0 10303 442 3 1 4 }'";

const MM: &str = "#1=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));";
const METRE: &str = "#1=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT($,.METRE.));";
const INCH: &str = "#1=(CONVERSION_BASED_UNIT('INCH',#6)LENGTH_UNIT()NAMED_UNIT(#7));\
#6=LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(25.4),#8);\
#7=DIMENSIONAL_EXPONENTS(1.,0.,0.,0.,0.,0.,0.);\
#8=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));";
const RAD: &str = "#2=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.));";
const DEG: &str = "#2=(CONVERSION_BASED_UNIT('DEGREE',#91)NAMED_UNIT(#92)PLANE_ANGLE_UNIT());\
#91=PLANE_ANGLE_MEASURE_WITH_UNIT(PLANE_ANGLE_MEASURE(0.017453292519943295),#93);\
#92=DIMENSIONAL_EXPONENTS(0.,0.,0.,0.,0.,0.,0.);\
#93=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.));";

fn exchange(schema: &str, data: &str) -> Exchange {
    let t = format!(
        "ISO-10303-21;\nHEADER;FILE_DESCRIPTION(('t'),'2;1');FILE_NAME('t','',(''),(''),'','','');\
         FILE_SCHEMA(({schema}));ENDSEC;\nDATA;\n{data}\nENDSEC;\nEND-ISO-10303-21;\n"
    );
    match parse(&t) {
        Ok(p) => p.exchange,
        Err(e) => panic!("{}\n{t}", e.one_line()),
    }
}

/// A context #100 over the unit entities `units` (ids), and `unc` (an uncertainty entity list).
fn context(units: &str, unc: &str) -> String {
    format!(
        "#100=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT(({unc}))\
         GLOBAL_UNIT_ASSIGNED_CONTEXT(({units}))REPRESENTATION_CONTEXT('',''));"
    )
}

fn uncertainty(id: u32, v: &str, unit: &str) -> String {
    format!(
        "#{id}=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE({v}),{unit},'distance_accuracy_value','');"
    )
}

/// A face on a plane.
fn faces(first: u32, n: u32) -> String {
    (0..n)
        .map(|i| format!("#{}=ADVANCED_FACE('',(),#30,.T.);", first + i))
        .collect()
}

fn ids(first: u32, n: u32) -> String {
    (0..n)
        .map(|i| format!("#{}", first + i))
        .collect::<Vec<_>>()
        .join(",")
}

/// Common entities: the plane #30 and a solid #40 'Cube' of a closed shell #10 of six faces.
const GEOMETRY: &str = "#30=PLANE('',#31);#31=AXIS2_PLACEMENT_3D('',#32,$,$);\
#32=CARTESIAN_POINT('',(0.,0.,0.));";

fn cube() -> String {
    format!(
        "{GEOMETRY}{}#10=CLOSED_SHELL('',({}));#40=MANIFOLD_SOLID_BREP('Cube',#10);\
         #50=ADVANCED_BREP_SHAPE_REPRESENTATION('',(#40),#100);",
        faces(11, 6),
        ids(11, 6)
    )
}

fn mm_file(schema: &str) -> Exchange {
    exchange(
        schema,
        &format!(
            "{MM}{RAD}{}{}{}",
            uncertainty(3, "1.E-07", "#1"),
            context("#1,#2", "#3"),
            cube()
        ),
    )
}

#[test]
fn reads_a_solid_under_each_protocol() {
    for (schema, protocol) in [
        (AP203, Protocol::Ap203),
        (AP214, Protocol::Ap214),
        (AP242, Protocol::Ap242(Some(Ap242Edition::Second))),
    ] {
        let got = read(&mm_file(schema), &ReadOptions::default()).unwrap();
        assert_eq!(got.report.declared.protocol, protocol, "{schema}");
        assert!(got.report.warnings.is_empty(), "{schema}");
        assert!(got.skipped.is_empty());
        assert_eq!(got.bodies.len(), 1);
        let b = &got.bodies[0];
        assert_eq!(
            (b.entity, b.name.as_str(), b.kind),
            (40, "Cube", SolidOrSheet::Solid)
        );
        assert_eq!(b.representation, 50);
        assert_eq!(b.shells.len(), 1);
        let s = &b.shells[0];
        assert_eq!(
            (s.entity, s.role, s.closed, s.reversed),
            (10, ShellRole::Outer, true, false)
        );
        assert_eq!(s.faces.len(), 6);
        assert_eq!(s.faces[0].surface_kind, "PLANE");
        assert!(s.faces[0].same_sense);
        assert!(got.report.assumed.is_empty() && got.report.raised.is_empty());
    }
}

#[test]
fn units_and_uncertainty_come_from_the_context() {
    let got = read(&mm_file(AP214), &ReadOptions::default()).unwrap();
    let c = got.bodies[0].context;
    assert_eq!(c.units, Units { len: 1.0, ang: 1.0 });
    // 1e-7 times MILLI times METRE, as the file writes it.
    assert_eq!(c.uncertainty, Uncertainty::Stated(1e-7 * (1e-3 * 1000.0)));
    assert!(!c.assumed.any());
}

#[test]
fn inches_degrees_and_metres_are_converted_by_the_files_own_numbers() {
    let data = format!(
        "{INCH}{DEG}{}{}{}",
        uncertainty(3, "1.E-05", "#1"),
        context("#1,#2", "#3"),
        cube()
    );
    let c = read(&exchange(AP203, &data), &ReadOptions::default())
        .unwrap()
        .bodies[0]
        .context;
    assert_eq!(c.units.len, 25.4);
    assert_eq!(c.units.ang, 0.017453292519943295);
    assert_eq!(c.uncertainty, Uncertainty::Stated(1e-5 * 25.4));

    let data = format!("{METRE}{RAD}{}{}", context("#1,#2", ""), cube());
    let c = read(&exchange(AP242, &data), &ReadOptions::default())
        .unwrap()
        .bodies[0]
        .context;
    assert_eq!(c.units.len, 1000.0);
    assert_eq!(c.uncertainty, Uncertainty::Absent);
}

#[test]
fn an_uncertainty_without_a_unit_is_in_the_contexts_length_unit() {
    let data = format!(
        "{INCH}{RAD}#3=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.001),$,'','');{}{}",
        context("#1,#2", "#3"),
        cube()
    );
    let c = read(&exchange(AP203, &data), &ReadOptions::default())
        .unwrap()
        .bodies[0]
        .context;
    assert_eq!(c.uncertainty, Uncertainty::Stated(0.001 * 25.4));
}

#[test]
fn several_uncertainties_take_the_largest_and_say_so() {
    let data = format!(
        "{MM}{RAD}{}{}{}{}",
        uncertainty(3, "1.E-07", "#1"),
        uncertainty(4, "1.E-03", "#1"),
        context("#1,#2", "#3,#4"),
        cube()
    );
    let got = read(&exchange(AP214, &data), &ReadOptions::default()).unwrap();
    assert_eq!(
        got.bodies[0].context.uncertainty,
        Uncertainty::Stated(1e-3 * (1e-3 * 1000.0))
    );
    assert_eq!(
        got.report.warnings,
        vec![ImportWarning::SeveralUncertainties { representation: 50 }]
    );
}

#[test]
fn missing_units_are_refused() {
    // No context entity at all, a context listing no units, and one listing only a length.
    let none = exchange(AP203, &format!("{MM}{RAD}{}", cube()));
    let empty = exchange(AP203, &format!("{MM}{RAD}{}{}", context("", ""), cube()));
    let length_only = exchange(AP203, &format!("{MM}{RAD}{}{}", context("#1", ""), cube()));
    for (ex, missing) in [
        (
            none,
            MissingUnits {
                length: true,
                angle: true,
            },
        ),
        (
            empty,
            MissingUnits {
                length: true,
                angle: true,
            },
        ),
        (
            length_only,
            MissingUnits {
                length: false,
                angle: true,
            },
        ),
    ] {
        let e = read(&ex, &ReadOptions::default()).unwrap_err();
        assert_eq!(
            e,
            StepError::NoUnits {
                representation: 50,
                missing
            }
        );
        assert_eq!((e.kind(), e.name()), (ErrorKind::Refused, "no_units"));
        assert!(e.to_string().contains("assume units explicitly"));
    }
}

#[test]
fn missing_units_are_read_only_when_assumed_and_the_assumption_is_recorded() {
    let inch = Units {
        len: 25.4,
        ang: 1.0,
    };
    let opts = ReadOptions::default().assuming_units(inch);
    // A length-only context: the angle is assumed, the length is the file's.
    let ex = exchange(
        AP203,
        &format!("{INCH}{RAD}{}{}", context("#1", "#3"), cube()).replace(
            "#100=(",
            "#3=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(0.5),$,'','');#100=(",
        ),
    );
    let got = read(&ex, &opts).unwrap();
    let c = got.bodies[0].context;
    assert_eq!(
        c.units,
        Units {
            len: 25.4,
            ang: 1.0
        }
    );
    assert_eq!(
        c.assumed,
        MissingUnits {
            length: false,
            angle: true
        }
    );
    // The uncertainty has no unit: it is in the (file's) length unit.
    assert_eq!(c.uncertainty, Uncertainty::Stated(0.5 * 25.4));
    assert_eq!(got.report.assumed.len(), 1);
    assert_eq!(got.report.assumed[0].representation, 50);
    assert_eq!(got.report.assumed[0].which, c.assumed);

    // No context: both assumed, and an unitless uncertainty cannot exist.
    let got = read(&exchange(AP203, &cube()), &opts).unwrap();
    assert_eq!(got.bodies[0].context.units, inch);
    assert_eq!(
        got.bodies[0].context.assumed,
        MissingUnits {
            length: true,
            angle: true
        }
    );
    assert_eq!(got.bodies[0].context.uncertainty, Uncertainty::Absent);

    // The file's units win over the assumption, which is then not recorded.
    let got = read(&mm_file(AP203), &opts).unwrap();
    assert_eq!(got.bodies[0].context.units, Units { len: 1.0, ang: 1.0 });
    assert!(got.report.assumed.is_empty());
}

#[test]
fn unusable_assumed_units_are_refused() {
    for bad in [
        Units { len: 0.0, ang: 1.0 },
        Units {
            len: 1.0,
            ang: f64::NAN,
        },
        Units {
            len: -1.0,
            ang: 1.0,
        },
    ] {
        let e = read(&mm_file(AP203), &ReadOptions::default().assuming_units(bad)).unwrap_err();
        assert_eq!(e, StepError::BadAssumedUnits);
    }
}

#[test]
fn two_different_length_units_are_refused() {
    let data = format!(
        "{MM}{RAD}#12=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT($,.METRE.));{}{}",
        context("#1,#2,#12", ""),
        cube()
    );
    let e = read(&exchange(AP203, &data), &ReadOptions::default()).unwrap_err();
    assert_eq!(
        e,
        StepError::ConflictingUnits {
            context: 100,
            which: MissingUnits {
                length: true,
                angle: false
            }
        }
    );
    // The same unit listed twice is not a conflict.
    let data = format!("{MM}{RAD}{}{}", context("#1,#2,#1", ""), cube());
    assert!(read(&exchange(AP203, &data), &ReadOptions::default()).is_ok());
}

#[test]
fn each_representation_has_its_own_units() {
    // An inch part and a millimetre part in one file: the uncertainty of neither leaks.
    let data = format!(
        "{MM}{RAD}{}{}{GEOMETRY}{}#10=CLOSED_SHELL('',({}));#40=MANIFOLD_SOLID_BREP('A',#10);\
         #50=ADVANCED_BREP_SHAPE_REPRESENTATION('',(#40),#100);\
         #20=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT($,.METRE.));\
         #101=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNIT_ASSIGNED_CONTEXT((#20,#2))REPRESENTATION_CONTEXT('',''));\
         {}#13=CLOSED_SHELL('',({}));#41=MANIFOLD_SOLID_BREP('B',#13);\
         #51=ADVANCED_BREP_SHAPE_REPRESENTATION('',(#41),#101);",
        uncertainty(3, "1.E-07", "#1"),
        context("#1,#2", "#3"),
        faces(11, 2),
        ids(11, 2),
        faces(14, 2),
        ids(14, 2),
    );
    let got = read(&exchange(AP214, &data), &ReadOptions::default()).unwrap();
    assert_eq!(got.bodies.len(), 2);
    assert_eq!(got.bodies[0].context.units.len, 1.0);
    assert_eq!(
        got.bodies[0].context.uncertainty,
        Uncertainty::Stated(1e-7 * (1e-3 * 1000.0))
    );
    assert_eq!(got.bodies[1].context.units.len, 1000.0);
    assert_eq!(got.bodies[1].context.uncertainty, Uncertainty::Absent);
}

#[test]
fn solids_with_voids_and_sheets() {
    let data = format!(
        "{MM}{RAD}{}{GEOMETRY}{}{}{}\
         #10=CLOSED_SHELL('',({}));#13=CLOSED_SHELL('',({}));\
         #14=ORIENTED_CLOSED_SHELL('',*,#13,.F.);\
         #40=BREP_WITH_VOIDS('Hollow',#10,(#14));\
         #20=OPEN_SHELL('',({}));\
         #41=SHELL_BASED_SURFACE_MODEL('Sheet',(#20));\
         #50=ADVANCED_BREP_SHAPE_REPRESENTATION('',(#40),#100);\
         #51=SHAPE_REPRESENTATION('',(#41),#100);",
        context("#1,#2", ""),
        faces(11, 2),
        faces(15, 2),
        faces(21, 3),
        ids(11, 2),
        ids(15, 2),
        ids(21, 3)
    );
    let got = read(&exchange(AP242, &data), &ReadOptions::default()).unwrap();
    assert!(got.skipped.is_empty(), "{:?}", got.skipped);
    let (hollow, sheet) = (&got.bodies[0], &got.bodies[1]);
    assert_eq!((hollow.kind, hollow.shells.len()), (SolidOrSheet::Solid, 2));
    assert_eq!(hollow.shells[0].role, ShellRole::Outer);
    let void = &hollow.shells[1];
    assert_eq!(
        (void.entity, void.role, void.reversed, void.faces.len()),
        (13, ShellRole::Void, true, 2)
    );
    assert_eq!((sheet.kind, sheet.shells.len()), (SolidOrSheet::Sheet, 1));
    let s = &sheet.shells[0];
    assert_eq!(
        (s.role, s.closed, s.faces.len()),
        (ShellRole::Sheet, false, 3)
    );
}

#[test]
fn bodies_that_cannot_be_read_are_skipped_with_a_reason_and_the_rest_are_read() {
    let data = format!(
        "{MM}{RAD}{}{GEOMETRY}{}\
         #10=CLOSED_SHELL('',({}));#40=MANIFOLD_SOLID_BREP('Good',#10);\
         #20=OPEN_SHELL('',({}));#41=MANIFOLD_SOLID_BREP('Open',#20);\
         #42=FACETED_BREP('Facets',#10);\
         #43=MANIFOLD_SOLID_BREP('Missing',#999);\
         #44=SHELL_BASED_SURFACE_MODEL('NoSurface',(#21));\
         #21=OPEN_SHELL('',(#22));#22=ADVANCED_FACE('',(),#999,.T.);\
         #50=ADVANCED_BREP_SHAPE_REPRESENTATION('',(#40,#41,#42,#43,#44),#100);",
        context("#1,#2", ""),
        faces(11, 2),
        ids(11, 2),
        ids(11, 2),
    );
    let got = read(&exchange(AP214, &data), &ReadOptions::default()).unwrap();
    assert_eq!(got.bodies.len(), 1);
    assert_eq!(got.bodies[0].name, "Good");
    let why: Vec<(u64, &SkipReason)> = got.skipped.iter().map(|s| (s.entity, &s.why)).collect();
    assert_eq!(why.len(), 4);
    assert_eq!(why[0].0, 41);
    assert!(matches!(why[0].1, SkipReason::Malformed(e) if e.name() == "wrong_type"));
    assert_eq!(
        (why[1].0, why[1].1),
        (42, &SkipReason::BodyKind("FACETED_BREP".to_owned()))
    );
    assert!(matches!(why[2].1, SkipReason::Malformed(e) if e.name() == "missing"));
    assert!(matches!(why[3].1, SkipReason::Malformed(e) if e.id().is_some()));
}

#[test]
fn an_oriented_shell_cycle_is_a_skip_not_a_loop() {
    let data = format!(
        "{MM}{RAD}{}#14=ORIENTED_CLOSED_SHELL('',*,#14,.T.);\
         #40=MANIFOLD_SOLID_BREP('Loop',#14);\
         #50=ADVANCED_BREP_SHAPE_REPRESENTATION('',(#40),#100);",
        context("#1,#2", ""),
    );
    let got = read(&exchange(AP203, &data), &ReadOptions::default()).unwrap();
    assert!(got.bodies.is_empty());
    assert_eq!(got.skipped.len(), 1);
}

#[test]
fn doubtful_protocols_and_empty_files_warn() {
    let got = read(
        &exchange(
            "'SOME_OTHER_SCHEMA'",
            &format!("{MM}{RAD}{}", context("#1,#2", "")),
        ),
        &ReadOptions::default(),
    )
    .unwrap();
    assert_eq!(
        got.report.warnings,
        vec![
            ImportWarning::Protocol(Doubt::UnknownSchema("SOME_OTHER_SCHEMA".to_owned())),
            ImportWarning::NoBodies
        ]
    );
    assert_eq!(got.report.declared.protocol, Protocol::Unknown);
    assert!(got.bodies.is_empty() && got.skipped.is_empty());
}

#[test]
fn closing_gaps_is_pending_until_stitching_is_wired() {
    let e = read(&mm_file(AP203), &ReadOptions::default().closing_gaps()).unwrap_err();
    assert_eq!(e, StepError::Pending(Pending::CloseGaps));
    assert_eq!(
        (e.kind(), e.name()),
        (ErrorKind::Unsupported, "close_gaps_pending")
    );
}

#[test]
fn a_broken_product_structure_is_a_typed_structure_error() {
    let ex = exchange(
        AP203,
        &format!(
            "{MM}{RAD}{}{}#60=PRODUCT_DEFINITION('d','',#999,#998);",
            context("#1,#2", ""),
            cube()
        ),
    );
    let e = read(&ex, &ReadOptions::default()).unwrap_err();
    assert!(matches!(e, StepError::Structure(_)));
    assert_eq!(e.kind(), ErrorKind::Refused);
    assert_eq!(e.name(), "missing");
    assert!(e.one_line().to_string().contains("product structure"));
}

#[test]
fn reading_twice_gives_the_same_result() {
    let ex = mm_file(AP242);
    let a = read(&ex, &ReadOptions::default()).unwrap();
    let b = read(&ex, &ReadOptions::default()).unwrap();
    assert_eq!(a, b);
}
