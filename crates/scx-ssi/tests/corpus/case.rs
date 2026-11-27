// Written for solidcorex. Apache-2.0.
//! A corpus case: its two surfaces as scx surfaces over their extents, and the truth stored
//! with it (research 73 §9, §16). The data is the ssi-corpus (MIT OR Apache-2.0), read from
//! the directory `SCX_SSI_CORPUS` names; none of it is in this repository.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use scx_num::extent2d::Extent2d;
use scx_num::frame::{Frame, Sense};
use scx_num::units::Length;
use scx_num::vector2d::Point2;
use scx_num::vector3d::{Point3, Vec3};
use scx_nurbs::KnotVector;
use scx_nurbs_surface::{Net, NurbsSurface};
use scx_surface::{Cone, Cylinder, Plane, Sphere, Surface, Torus};

use crate::json::{self, Json};

pub type P3 = [f64; 3];

/// The six cases built from Yang, Jia and Yan's published data, excluded until the paper's
/// licence is confirmed (04, 11-27, R73). The paper's other 19 are reconstructions.
pub const YANG_DATA: [&str; 6] = [
    "y23_appendix_ex2",
    "y23_t3_ex1_torus_ellipsoid",
    "y23_t3_ex2_torus_ellipsoid",
    "y23_t3_ex3_torus_ellipsoid",
    "y23_t3_ex4_torus_ellipsoid",
    "y23_t3_ex5_torus_ellipsoid",
];

pub struct Side {
    pub surface: Surface,
    pub extent: Extent2d,
    /// A sphere's centre and radius, for the closed-form truth.
    pub sphere: Option<(P3, f64)>,
}

pub struct TruthBranch {
    pub closed: bool,
    pub kind: String,
    pub points: Vec<P3>,
}

pub struct Case {
    pub id: String,
    pub cov_tol: f64,
    pub ill_cond: f64,
    pub feature_size: Option<f64>,
    pub sides: [Side; 2],
    pub branches: Vec<TruthBranch>,
    pub isolated: Vec<P3>,
    pub singular: Vec<P3>,
    /// The boundary polyline of each coincident region.
    pub coincident: Vec<Vec<P3>>,
}

fn p3(j: Option<&Json>, what: &str) -> Result<P3, String> {
    j.and_then(Json::nums)
        .and_then(|v| <P3>::try_from(v).ok())
        .ok_or_else(|| format!("{what} is not a 3-vector"))
}

fn pair(j: Option<&Json>, what: &str) -> Result<[f64; 2], String> {
    j.and_then(Json::nums)
        .and_then(|v| <[f64; 2]>::try_from(v).ok())
        .ok_or_else(|| format!("{what} is not a pair"))
}

fn num(j: Option<&Json>, what: &str) -> Result<f64, String> {
    j.and_then(Json::num)
        .ok_or_else(|| format!("{what} is not a number"))
}

fn frame(origin: P3, x: P3, y: P3) -> Result<Frame, String> {
    let z = Vec3::new(x[0], x[1], x[2]).cross(Vec3::new(y[0], y[1], y[2]));
    Frame::new(
        Point3::new(origin[0], origin[1], origin[2]),
        z,
        Vec3::new(x[0], x[1], x[2]),
    )
    .ok_or_else(|| "a degenerate frame".to_string())
}

fn rect(a: Point2, b: Point2) -> Result<Extent2d, String> {
    Extent2d::spanning(a, b).ok_or_else(|| "a bad extent".to_string())
}

fn side(s: &Json) -> Result<Side, String> {
    let kind = s
        .get("type")
        .and_then(Json::str)
        .ok_or("a surface has no type")?;
    let mm = Length::from_mm;
    if kind == "nurbs" {
        let degree = pair(s.get("degree"), "degree")?;
        let (nu, nv) = (
            num(s.get("nu"), "nu")? as usize,
            num(s.get("nv"), "nv")? as usize,
        );
        let ku = s.get("knots_u").and_then(Json::nums).ok_or("knots_u")?;
        let kv = s.get("knots_v").and_then(Json::nums).ok_or("knots_v")?;
        let points: Vec<[f64; 3]> = s
            .get("points")
            .and_then(Json::arr)
            .ok_or("points")?
            .iter()
            .map(|p| p3(Some(p), "a control point"))
            .collect::<Result<_, _>>()?;
        let weights = s.get("weights").and_then(Json::nums);
        let e = |e: scx_nurbs::NurbsError| format!("{e:?}");
        let u = KnotVector::new(degree[0] as usize, ku).map_err(e)?;
        let v = KnotVector::new(degree[1] as usize, kv).map_err(e)?;
        let net = Net::new(nu, nv, points).map_err(|e| format!("{e:?}"))?;
        let w = match weights {
            Some(w) => Some(Net::new(nu, nv, w).map_err(|e| format!("{e:?}"))?),
            None => None,
        };
        let n = NurbsSurface::new(u, v, net, w).map_err(|e| format!("{e:?}"))?;
        let extent = n.domain();
        return Ok(Side {
            surface: Surface::Nurbs(Arc::new(n)),
            extent,
            sphere: None,
        });
    }
    let (x, y) = (p3(s.get("xaxis"), "xaxis")?, p3(s.get("yaxis"), "yaxis")?);
    let rad = |d: f64| d.to_radians();
    let se = |e: scx_surface::SurfaceError| format!("{e:?}");
    match kind {
        "plane" => {
            let f = frame(p3(s.get("origin"), "origin")?, x, y)?;
            let (u, v) = (pair(s.get("u"), "u")?, pair(s.get("v"), "v")?);
            Ok(Side {
                surface: Surface::Plane(Plane::new(f)),
                extent: rect(Point2::new(u[0], v[0]), Point2::new(u[1], v[1]))?,
                sphere: None,
            })
        }
        "cylinder" => {
            let f = frame(p3(s.get("origin"), "origin")?, x, y)?;
            let k = Cylinder::new(f, mm(num(s.get("radius"), "radius")?), Sense::Forward)
                .map_err(se)?;
            let (a, h) = (
                pair(s.get("angle"), "angle")?,
                pair(s.get("height"), "height")?,
            );
            let extent = rect(
                k.from_step(Point2::new(rad(a[0]), h[0])),
                k.from_step(Point2::new(rad(a[1]), h[1])),
            )?;
            Ok(Side {
                surface: Surface::Cylinder(k),
                extent,
                sphere: None,
            })
        }
        "cone" => {
            // The frame sits at the apex: radius 0 there, `v` the height along the axis.
            let f = frame(p3(s.get("apex"), "apex")?, x, y)?;
            let half = rad(num(s.get("half_angle_deg"), "half_angle_deg")?);
            let k = Cone::new(f, mm(0.0), half, Sense::Forward).map_err(se)?;
            let h = pair(s.get("height"), "height")?;
            let extent = rect(
                k.from_step(Point2::new(0.0, h[0])),
                k.from_step(Point2::new(core::f64::consts::TAU, h[1])),
            )?;
            Ok(Side {
                surface: Surface::Cone(k),
                extent,
                sphere: None,
            })
        }
        "sphere" => {
            let centre = p3(s.get("center"), "center")?;
            let radius = num(s.get("radius"), "radius")?;
            let f = frame(centre, x, y)?;
            let k = Sphere::new(f, mm(radius), Sense::Forward).map_err(se)?;
            let (lo, la) = (pair(s.get("lon"), "lon")?, pair(s.get("lat"), "lat")?);
            let extent = rect(
                k.from_step(Point2::new(rad(lo[0]), rad(la[0]))),
                k.from_step(Point2::new(rad(lo[1]), rad(la[1]))),
            )?;
            Ok(Side {
                surface: Surface::Sphere(k),
                extent,
                sphere: Some((centre, radius)),
            })
        }
        "torus" => {
            let f = frame(p3(s.get("center"), "center")?, x, y)?;
            let k = Torus::new(
                f,
                mm(num(s.get("R"), "R")?),
                mm(num(s.get("r"), "r")?),
                Sense::Forward,
            )
            .map_err(se)?;
            let (ma, mi) = (
                pair(s.get("major"), "major")?,
                pair(s.get("minor"), "minor")?,
            );
            let extent = rect(
                k.from_step(Point2::new(rad(ma[0]), rad(mi[0]))),
                k.from_step(Point2::new(rad(ma[1]), rad(mi[1]))),
            )?;
            Ok(Side {
                surface: Surface::Torus(k),
                extent,
                sphere: None,
            })
        }
        other => Err(format!("unknown surface type: {other}")),
    }
}

fn points(j: Option<&Json>, what: &str) -> Result<Vec<P3>, String> {
    j.and_then(Json::arr)
        .ok_or_else(|| format!("{what} is not a list"))?
        .iter()
        .map(|p| p3(Some(p), what))
        .collect()
}

impl Case {
    /// The tolerance the case is run at (mm): the pin's default zone, or a tenth of the
    /// case's feature size when that is finer, so the feature is resolved.
    pub fn tol(&self) -> f64 {
        self.feature_size
            .filter(|f| f.is_finite() && *f > 0.0)
            .map_or(crate::score::TOL, |f| crate::score::TOL.min(0.1 * f))
    }

    pub fn from_text(text: &str) -> Result<Self, String> {
        let j = json::parse(text)?;
        let id = j.get("id").and_then(Json::str).ok_or("no id")?.to_string();
        let surfaces = j.get("surfaces").and_then(Json::arr).ok_or("no surfaces")?;
        let [a, b] = surfaces else {
            return Err("a case has two surfaces".to_string());
        };
        let sides = [side(a)?, side(b)?];
        let exp = j.get("expected").ok_or("no expected")?;
        let mut branches = Vec::new();
        for b in exp.get("branches").and_then(Json::arr).ok_or("branches")? {
            branches.push(TruthBranch {
                closed: b.get("closed").is_some_and(Json::is_truthy_bool),
                kind: b.get("kind").and_then(Json::str).unwrap_or("").to_string(),
                points: points(b.get("points"), "a branch")?,
            });
        }
        let mut coincident = Vec::new();
        for r in exp
            .get("coincident_regions")
            .and_then(Json::arr)
            .ok_or("coincident_regions")?
        {
            coincident.push(points(r.get("boundary"), "a boundary")?);
        }
        let ill = j.get("ill_cond").and_then(Json::num);
        Ok(Self {
            id,
            cov_tol: num(j.get("cov_tol"), "cov_tol")?,
            feature_size: j.get("feature_size").and_then(Json::num),
            ill_cond: ill.filter(|x| x.is_finite() && *x >= 1.0).unwrap_or(1.0),
            sides,
            branches,
            isolated: points(exp.get("isolated_points"), "isolated_points")?,
            singular: points(exp.get("singular_points"), "singular_points")?,
            coincident,
        })
    }
}

/// The corpus directory, from `SCX_SSI_CORPUS`: the folder holding `cases/` (or the cases
/// themselves).
pub fn corpus_dir() -> Option<PathBuf> {
    let d = PathBuf::from(std::env::var_os("SCX_SSI_CORPUS")?);
    let cases = d.join("cases");
    Some(if cases.is_dir() { cases } else { d })
}

fn read_case_file(path: &Path) -> Result<String, String> {
    let name = path.to_string_lossy();
    if name.ends_with(".json.gz") {
        let out = Command::new("gzip")
            .arg("-dc")
            .arg(path)
            .output()
            .map_err(|e| format!("gzip: {e}"))?;
        if !out.status.success() {
            return Err(format!("{name}: gzip failed"));
        }
        String::from_utf8(out.stdout).map_err(|_| format!("{name}: not UTF-8"))
    } else {
        std::fs::read_to_string(path).map_err(|e| format!("{name}: {e}"))
    }
}

/// The case ids of the directory, in name order, with their file.
pub fn case_files(dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let mut v = Vec::new();
    for e in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = e.map_err(|e| e.to_string())?.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let id = name
            .strip_suffix(".json.gz")
            .or_else(|| name.strip_suffix(".json"));
        if let Some(id) = id {
            v.push((id.to_string(), path));
        }
    }
    v.sort();
    Ok(v)
}

pub fn load(path: &Path) -> Result<Case, String> {
    Case::from_text(&read_case_file(path)?)
}
