// Derived from solidcore `solidcore/crates/solidcore-step/src/write.rs` at commit
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8 (its own origin: SolidCraft's `crates/step`,
// MIT OR Apache-2.0).
// Upstream record (verbatim from that file): Writing ISO 10303-21: a DATA section built
// entity by entity, and [`crate::geom`] curves and surfaces as ISO 10303-42 entities.
// Identical geometric entities are written once and shared, as the reader shares them;
// topological ones are distinct ([`Writer::add_distinct`]). [`topology`] writes a schema 4
// record's solids.
// Changes: only the Part 21 layer of that file moves here (its Part 42 geometry is
// `crate::part42::write`; `topology` and `product` go with the body mapping to `scx-step`).
// `param` and `params` write the whole parameter model, refusing (was writing `0.` and
// counting, `non_finite`) a real that is not finite and checking names, enumerations and
// binaries against §5.4. `Writer::new`, `Writer::add`, `Writer::add_distinct` and
// `Writer::add_complex` (now `Instance::complex` passed to `Writer::add`) build a
// `DataSection` of `Instance` values, `Writer::finish` in place of `Writer::data` and
// `Writer::file`; the shared-instance map hashes with `FixedState` (was the seeded default).
// `write`, `write_into`, `instance`, `record`, `checked`, `section` and `raw` are new: the
// whole exchange is written, every section and HEADER record as read.
//! Writing an exchange structure (ISO 10303-21:2016 §5 to §11).
//!
//! What [`write`] gives reads back as the same [`Exchange`]: `parse(write(x)) == x` for
//! every exchange `write` accepts. One record or instance per line.

use std::collections::HashMap;

use scx_num::hash::FixedState;

use crate::error::{P21Error, Refusal, Unwritable};
use crate::exchange::{DataSection, Exchange, Instance, Param, Record};
use crate::text;

/// `x` with the instance it is in.
fn at(id: Option<u64>) -> impl Fn(Unwritable) -> P21Error {
    move |cause| Refusal::Unwritable { id, cause }.into()
}

/// A checked name.
fn checked(name: &str, o: &mut String) -> Result<(), Unwritable> {
    if !text::keyword_ok(name) {
        return Err(Unwritable::BadKeyword(name.to_string()));
    }
    o.push_str(name);
    Ok(())
}

fn param(p: &Param, o: &mut String) -> Result<(), Unwritable> {
    use core::fmt::Write as _;
    match p {
        Param::Int(i) => {
            let _ = write!(o, "{i}");
        }
        Param::Real(x) => text::real_into(*x, o)?,
        Param::Str(s) => text::encode(s, o),
        Param::Enum(e) => {
            if !text::enumeration_ok(e) {
                return Err(Unwritable::BadEnumeration(e.clone()));
            }
            o.push('.');
            o.push_str(e);
            o.push('.');
        }
        Param::Ref(r) => {
            let _ = write!(o, "#{r}");
        }
        Param::List(v) => {
            o.push('(');
            params(v, o)?;
            o.push(')');
        }
        Param::Typed { name, value } => {
            checked(name, o)?;
            o.push('(');
            param(value, o)?;
            o.push(')');
        }
        Param::Binary(b) => {
            if !text::binary_ok(b) {
                return Err(Unwritable::BadBinary(b.clone()));
            }
            o.push('"');
            o.push_str(b);
            o.push('"');
        }
        Param::Unset => o.push('$'),
        Param::Derived => o.push('*'),
    }
    Ok(())
}

fn params(v: &[Param], o: &mut String) -> Result<(), Unwritable> {
    for (i, p) in v.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        param(p, o)?;
    }
    Ok(())
}

fn record(r: &Record, o: &mut String) -> Result<(), Unwritable> {
    checked(&r.name, o)?;
    o.push('(');
    params(&r.params, o)?;
    o.push(')');
    Ok(())
}

/// An instance's text after `#id=`, without its `;`.
fn instance(e: &Instance, o: &mut String) -> Result<(), Unwritable> {
    match e {
        Instance::Simple(r) => record(r, o),
        Instance::Complex(parts) => {
            if parts.is_empty() {
                return Err(Unwritable::EmptyComplex);
            }
            o.push('(');
            for r in parts {
                record(r, o)?;
            }
            o.push(')');
            Ok(())
        }
    }
}

/// A section kept as text.
fn raw(name: &str, body: &str, o: &mut String) {
    o.push_str(name);
    o.push(';');
    o.push_str(body);
    o.push_str("ENDSEC;\n");
}

fn section(s: &DataSection, o: &mut String) -> Result<(), P21Error> {
    use core::fmt::Write as _;
    o.push_str("DATA");
    if let Some(p) = &s.params {
        o.push('(');
        params(p, o).map_err(at(None))?;
        o.push(')');
    }
    o.push_str(";\n");
    for (id, e) in s.instances() {
        let _ = write!(o, "#{id}=");
        instance(e, o).map_err(at(Some(*id)))?;
        o.push_str(";\n");
    }
    o.push_str("ENDSEC;\n");
    Ok(())
}

/// `ex` as Part 21 text.
pub fn write(ex: &Exchange) -> Result<String, P21Error> {
    let mut o = String::new();
    write_into(ex, &mut o)?;
    Ok(o)
}

/// [`write`] appending to `o`. On an error, `o` holds what was written before it.
pub fn write_into(ex: &Exchange, o: &mut String) -> Result<(), P21Error> {
    o.push_str("ISO-10303-21;\nHEADER;\n");
    for r in &ex.header.records {
        record(r, o).map_err(at(None))?;
        o.push_str(";\n");
    }
    o.push_str("ENDSEC;\n");
    if let Some(a) = &ex.anchor {
        raw("ANCHOR", a, o);
    }
    if let Some(r) = &ex.reference {
        raw("REFERENCE", r, o);
    }
    for s in &ex.sections {
        section(s, o)?;
    }
    o.push_str("END-ISO-10303-21;\n");
    for s in &ex.signatures {
        raw("SIGNATURE", s, o);
    }
    Ok(())
}

/// A DATA section under construction. Instances are numbered from 1 in the order they are
/// added; [`Writer::add`] of an instance whose text is that of an earlier one returns the
/// earlier one's id, so identical geometry is written once and shared.
///
/// Every instance is checked as it is added, so a value with no Part 21 form is refused at
/// the call that gave it.
#[derive(Clone, Debug, Default)]
pub struct Writer {
    instances: Vec<(u64, Instance)>,
    shared: HashMap<String, u64, FixedState>,
    text: String,
}

impl Writer {
    /// An empty section.
    pub fn new() -> Self {
        Self::default()
    }

    /// The id the next instance gets.
    fn next_id(&self) -> u64 {
        self.instances.len() as u64 + 1
    }

    /// Add `e`, or return the id of an earlier instance with the same text.
    pub fn add(&mut self, e: Instance) -> Result<u64, P21Error> {
        let id = self.next_id();
        self.text.clear();
        instance(&e, &mut self.text).map_err(at(Some(id)))?;
        if let Some(&old) = self.shared.get(self.text.as_str()) {
            return Ok(old);
        }
        self.shared.insert(self.text.clone(), id);
        self.instances.push((id, e));
        Ok(id)
    }

    /// Add `e` as an instance of its own, even when an earlier one has the same text
    /// (topology: two faces on one surface are two faces).
    pub fn add_distinct(&mut self, e: Instance) -> Result<u64, P21Error> {
        let id = self.next_id();
        self.text.clear();
        instance(&e, &mut self.text).map_err(at(Some(id)))?;
        self.instances.push((id, e));
        Ok(id)
    }

    /// The instance `#id` added so far.
    pub fn get(&self, id: u64) -> Option<&Instance> {
        let at = usize::try_from(id).ok()?.checked_sub(1)?;
        self.instances.get(at).map(|(_, e)| e)
    }

    /// How many instances.
    pub fn len(&self) -> usize {
        self.instances.len()
    }

    /// Whether none is added.
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// The section, with its parameters.
    pub fn finish(self, params: Option<Vec<Param>>) -> DataSection {
        // The ids are 1, 2, ... in order: nothing is repeated.
        DataSection::new(params, self.instances).0
    }
}
