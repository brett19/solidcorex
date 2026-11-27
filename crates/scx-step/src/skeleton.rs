// Written for solidcorex.
//! The entity plan of a body: its shells and their faces as entity ids, read from the exchange
//! without building geometry. SP2 to SP4 fill each face with its surface, loops and edges.

use scx_p21::error::EntityCause;
use scx_p21::product::BodyKind;
use scx_p21::{Exchange, Instance, P21Error, Param};

use crate::SolidOrSheet;

/// What a shell is to its body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellRole {
    /// The outer shell of a solid.
    Outer,
    /// A void of a solid (`BREP_WITH_VOIDS`).
    Void,
    /// A shell of a sheet model (`SHELL_BASED_SURFACE_MODEL`), open or closed.
    Sheet,
}

/// One face of a shell.
#[derive(Clone, Debug, PartialEq)]
pub struct FacePlan {
    /// The `ADVANCED_FACE` or `FACE_SURFACE`.
    pub entity: u64,
    /// Its surface entity.
    pub surface: u64,
    /// The surface's entity type, as the file names it (the finite pending list of 04 is made
    /// of these).
    pub surface_kind: String,
    /// The face's `same_sense`: whether its normal is the surface's.
    pub same_sense: bool,
    /// How many bounds (loops) it lists.
    pub bounds: usize,
}

/// One shell of a body.
#[derive(Clone, Debug, PartialEq)]
pub struct ShellPlan {
    /// The `CLOSED_SHELL` or `OPEN_SHELL` (the shell an oriented shell wraps).
    pub entity: u64,
    /// Its role in the body.
    pub role: ShellRole,
    /// Whether the file calls the shell closed.
    pub closed: bool,
    /// Whether an `ORIENTED_*_SHELL` reverses it.
    pub reversed: bool,
    /// Its faces, in the file's order.
    pub faces: Vec<FacePlan>,
}

type Result<T> = core::result::Result<T, P21Error>;

fn fail(id: u64, cause: EntityCause) -> P21Error {
    P21Error::entity(id, cause)
}

fn get(ex: &Exchange, id: u64) -> Result<&Instance> {
    ex.get(id).ok_or_else(|| fail(id, EntityCause::Missing))
}

fn wrong(e: &Instance, id: u64, expected: &'static str) -> P21Error {
    fail(
        id,
        EntityCause::WrongType {
            expected,
            found: e.name().to_owned(),
        },
    )
}

fn param<'a>(id: u64, p: &'a [Param], index: usize, expected: &'static str) -> Result<&'a Param> {
    p.get(index)
        .ok_or_else(|| fail(id, EntityCause::BadParam { index, expected }))
}

fn reference(id: u64, p: &[Param], index: usize) -> Result<u64> {
    param(id, p, index, "an entity reference")?
        .as_ref_id()
        .ok_or_else(|| {
            fail(
                id,
                EntityCause::BadParam {
                    index,
                    expected: "an entity reference",
                },
            )
        })
}

fn refs(id: u64, p: &[Param], index: usize) -> Result<Vec<u64>> {
    let list = param(id, p, index, "a list of entity references")?
        .as_list()
        .ok_or_else(|| {
            fail(
                id,
                EntityCause::BadParam {
                    index,
                    expected: "a list of entity references",
                },
            )
        })?;
    list.iter()
        .map(|x| {
            x.as_ref_id().ok_or_else(|| {
                fail(
                    id,
                    EntityCause::BadParam {
                        index,
                        expected: "a list of entity references",
                    },
                )
            })
        })
        .collect()
}

/// A face's plan.
fn face(ex: &Exchange, id: u64) -> Result<FacePlan> {
    let e = get(ex, id)?;
    let p = e
        .record("ADVANCED_FACE")
        .or_else(|| e.record("FACE_SURFACE"))
        .ok_or_else(|| wrong(e, id, "a face with a surface"))?;
    let bounds = refs(id, p, 1)?.len();
    let surface = reference(id, p, 2)?;
    let same_sense = param(id, p, 3, "a boolean")?.as_bool().ok_or_else(|| {
        fail(
            id,
            EntityCause::BadParam {
                index: 3,
                expected: "a boolean",
            },
        )
    })?;
    let surface_kind = get(ex, surface)?.name().to_owned();
    Ok(FacePlan {
        entity: id,
        surface,
        surface_kind,
        same_sense,
        bounds,
    })
}

/// A shell's plan: `CLOSED_SHELL`, `OPEN_SHELL`, or an `ORIENTED_*_SHELL` of one.
fn shell(ex: &Exchange, id: u64, role: ShellRole) -> Result<ShellPlan> {
    let e = get(ex, id)?;
    if let Some(p) = e
        .record("ORIENTED_CLOSED_SHELL")
        .or_else(|| e.record("ORIENTED_OPEN_SHELL"))
    {
        let inner = reference(id, p, 2)?;
        let same = param(id, p, 3, "a boolean")?.as_bool().ok_or_else(|| {
            fail(
                id,
                EntityCause::BadParam {
                    index: 3,
                    expected: "a boolean",
                },
            )
        })?;
        // One level only: an oriented shell of an oriented shell is not a shell, and this
        // keeps a cycle of them from recursing.
        let mut s = plain(ex, inner, role).map_err(|x| x.within(id))?;
        s.reversed ^= !same;
        return Ok(s);
    }
    plain(ex, id, role)
}

/// A `CLOSED_SHELL` or `OPEN_SHELL`.
fn plain(ex: &Exchange, id: u64, role: ShellRole) -> Result<ShellPlan> {
    let e = get(ex, id)?;
    let (p, closed) = if let Some(p) = e.record("CLOSED_SHELL") {
        (p, true)
    } else if let Some(p) = e.record("OPEN_SHELL") {
        (p, false)
    } else {
        return Err(wrong(e, id, "a shell"));
    };
    let ids = refs(id, p, 1)?;
    let faces = ids
        .into_iter()
        .map(|f| face(ex, f).map_err(|x| x.within(id)))
        .collect::<Result<Vec<_>>>()?;
    Ok(ShellPlan {
        entity: id,
        role,
        closed,
        reversed: false,
        faces,
    })
}

/// A body's kind and shells. A solid's outer shell and voids must be closed.
pub(crate) fn body(
    ex: &Exchange,
    id: u64,
    kind: BodyKind,
) -> Result<(SolidOrSheet, Vec<ShellPlan>)> {
    let e = get(ex, id)?;
    let p = e.params();
    match kind {
        BodyKind::ManifoldSolidBrep | BodyKind::BrepWithVoids => {
            let mut shells =
                vec![shell(ex, reference(id, p, 1)?, ShellRole::Outer).map_err(|x| x.within(id))?];
            if kind == BodyKind::BrepWithVoids {
                for v in refs(id, p, 2)? {
                    shells.push(shell(ex, v, ShellRole::Void).map_err(|x| x.within(id))?);
                }
            }
            for s in &shells {
                if !s.closed {
                    return Err(fail(
                        s.entity,
                        EntityCause::WrongType {
                            expected: "a closed shell",
                            found: "OPEN_SHELL".to_owned(),
                        },
                    )
                    .within(id));
                }
            }
            Ok((SolidOrSheet::Solid, shells))
        }
        BodyKind::ShellBasedSurfaceModel => {
            let shells = refs(id, p, 1)?
                .into_iter()
                .map(|s| shell(ex, s, ShellRole::Sheet).map_err(|x| x.within(id)))
                .collect::<Result<Vec<_>>>()?;
            Ok((SolidOrSheet::Sheet, shells))
        }
        _ => Err(wrong(e, id, "a body this crate reads")),
    }
}
