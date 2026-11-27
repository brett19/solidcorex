// Written for solidcorex (docs/research/58-p21.md §3). The accessors of `Param` and of an
// instance's records are those of solidcore's `step/p21.rs` at
// 03e16c9ee766de5b9b1ced845e5149d62ff493b8 (`Param::as_f64` .. `Entity::params`), on a model
// that keeps everything a file says.
//! The exchange structure as plain data (ISO 10303-21:2016 §5.5): the HEADER records, the
//! DATA sections and their instances, and the sections this crate carries as text.
//!
//! What a read keeps, so that writing it and reading it back gives the same value:
//!
//! - every HEADER record in order, user-defined ones (`!NAME`) included (§8);
//! - every DATA section with its parameters (§11.1), its instances sorted by id;
//! - a simple instance apart from a complex one of one part (§5.5 `SIMPLE_ENTITY_INSTANCE`,
//!   `COMPLEX_ENTITY_INSTANCE`), and a complex one's parts in file order;
//! - ANCHOR, REFERENCE and SIGNATURE sections (§9, §10, §14) as their text.
//!
//! What a read normalises: comments are gone (§5.6); strings are decoded (§6.4.3);
//! keywords, enumerations and binaries are upper case; the AP203 short entity names read as
//! their long names (§12.2.11); a repeated instance id keeps its first definition.

/// A parameter value (ISO 10303-21:2016 §5.5 `PARAMETER`).
#[derive(Clone, Debug, PartialEq)]
pub enum Param {
    /// An integer (§6.4.1).
    Int(i64),
    /// A real (§6.4.2): `1.`, `1.5E1`, `1.E-3`.
    Real(f64),
    /// A string, its control directives decoded (§6.4.3).
    Str(String),
    /// An enumeration value without its dots, `T` and `F` included (§6.4.5).
    Enum(String),
    /// An entity instance name, `#n` (§6.4.4).
    Ref(u64),
    /// A list, `(a, b, ...)`: an aggregate (§12.1.3).
    List(Vec<Param>),
    /// A typed parameter, `NAME(value)` (§5.5 `TYPED_PARAMETER`, §12.1.8): exactly one value.
    Typed {
        /// The type's keyword.
        name: String,
        /// The value.
        value: Box<Param>,
    },
    /// A binary (§6.4.6): the digit giving the unused bits of the first hexadecimal digit,
    /// then the hexadecimal digits, upper case, as written between the quotes.
    Binary(String),
    /// `$`: an optional attribute with no value (§12.2.2).
    Unset,
    /// `*`: an attribute redeclared as derived (§12.2.6, §5.5 `OMITTED_PARAMETER`).
    Derived,
}

impl Param {
    /// A typed parameter.
    pub fn typed(name: impl Into<String>, value: Param) -> Self {
        Self::Typed {
            name: name.into(),
            value: Box::new(value),
        }
    }

    /// A finite real, an integer as a real, or a typed parameter holding one of them.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Real(x) => Some(*x).filter(|x| x.is_finite()),
            Self::Int(i) => Some(*i as f64),
            Self::Typed { value, .. } => value.as_f64(),
            _ => None,
        }
    }

    /// An integer, or a real with no fraction below `9e15` in magnitude.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            Self::Real(x) if x.fract() == 0.0 && x.abs() < 9e15 => Some(*x as i64),
            _ => None,
        }
    }

    /// The id of an entity reference.
    pub fn as_ref_id(&self) -> Option<u64> {
        match self {
            Self::Ref(r) => Some(*r),
            _ => None,
        }
    }

    /// The text of a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    /// The value of an enumeration, without its dots.
    pub fn as_enum(&self) -> Option<&str> {
        match self {
            Self::Enum(s) => Some(s),
            _ => None,
        }
    }

    /// A BOOLEAN or LOGICAL (§12.1.1.3, §12.1.1.4): `.T.` is true, `.F.` false, and `.U.` or
    /// anything else `None`.
    pub fn as_bool(&self) -> Option<bool> {
        match self.as_enum()? {
            "T" => Some(true),
            "F" => Some(false),
            _ => None,
        }
    }

    /// The items of a list.
    pub fn as_list(&self) -> Option<&[Param]> {
        match self {
            Self::List(v) => Some(v),
            _ => None,
        }
    }
}

/// One record: the entity type's name and its parameters (§5.5 `SIMPLE_RECORD`). A HEADER
/// entry is one too.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    /// The entity type's name, upper case (`!` first for a user-defined one).
    pub name: String,
    /// The parameters, in file order.
    pub params: Vec<Param>,
}

impl Record {
    /// A record.
    pub fn new(name: impl Into<String>, params: Vec<Param>) -> Self {
        Self {
            name: name.into(),
            params,
        }
    }
}

/// An entity instance (§5.5 `ENTITY_INSTANCE`).
#[derive(Clone, Debug, PartialEq)]
pub enum Instance {
    /// `#n=NAME(...);`
    Simple(Record),
    /// `#n=(A(...)B(...));`, its parts in file order. A writer lists them in ascending order
    /// of name (§12.2.5.3); [`Instance::complex`] sorts them so.
    Complex(Vec<Record>),
}

impl Instance {
    /// A simple instance.
    pub fn simple(name: impl Into<String>, params: Vec<Param>) -> Self {
        Self::Simple(Record::new(name, params))
    }

    /// A complex instance, its parts sorted by name as §12.2.5.3 orders them.
    pub fn complex(mut parts: Vec<Record>) -> Self {
        parts.sort_by(|a, b| a.name.cmp(&b.name));
        Self::Complex(parts)
    }

    /// Its records: one for a simple instance, the parts of a complex one.
    pub fn records(&self) -> &[Record] {
        match self {
            Self::Simple(r) => core::slice::from_ref(r),
            Self::Complex(rs) => rs,
        }
    }

    /// Whether it is written as a complex instance.
    pub fn is_complex(&self) -> bool {
        matches!(self, Self::Complex(_))
    }

    /// The name of a simple instance (of a complex one's first part).
    pub fn name(&self) -> &str {
        self.records().first().map_or("", |r| r.name.as_str())
    }

    /// The parameters of the record called `name`.
    pub fn record(&self, name: &str) -> Option<&[Param]> {
        self.records()
            .iter()
            .find(|r| r.name == name)
            .map(|r| r.params.as_slice())
    }

    /// Whether it has a record called `name`.
    pub fn has(&self, name: &str) -> bool {
        self.records().iter().any(|r| r.name == name)
    }

    /// The parameters of a simple instance (of a complex one's first part).
    pub fn params(&self) -> &[Param] {
        self.records().first().map_or(&[], |r| r.params.as_slice())
    }
}

/// `FILE_DESCRIPTION` (§8.2.1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileDescription {
    /// `description`: its lines.
    pub description: Vec<String>,
    /// `implementation_level`, such as `2;1`.
    pub implementation_level: String,
}

/// `FILE_NAME` (§8.2.2).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileName {
    /// `name`.
    pub name: String,
    /// `time_stamp` (ISO 8601).
    pub time_stamp: String,
    /// `author`.
    pub author: Vec<String>,
    /// `organization`.
    pub organization: Vec<String>,
    /// `preprocessor_version`.
    pub preprocessor_version: String,
    /// `originating_system`.
    pub originating_system: String,
    /// `authorization`.
    pub authorization: String,
}

/// The HEADER section (§8): its records in file order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Header {
    /// Every record, the three required ones (§8.1) and any others, user-defined included.
    pub records: Vec<Record>,
}

/// Strings of a list parameter (non-strings skipped).
fn strings(p: Option<&Param>) -> Vec<String> {
    p.and_then(Param::as_list)
        .unwrap_or(&[])
        .iter()
        .filter_map(|p| p.as_str().map(str::to_string))
        .collect()
}

/// A string parameter (`""` when it is not one).
fn string(p: Option<&Param>) -> String {
    p.and_then(Param::as_str).unwrap_or("").to_string()
}

/// A list of strings as a parameter.
fn string_list(v: &[String]) -> Param {
    Param::List(v.iter().map(|s| Param::Str(s.clone())).collect())
}

impl Header {
    /// The three records §8.1 requires, in its order.
    pub fn new(description: &FileDescription, name: &FileName, schemas: &[String]) -> Self {
        Self {
            records: vec![
                Record::new(
                    "FILE_DESCRIPTION",
                    vec![
                        string_list(&description.description),
                        Param::Str(description.implementation_level.clone()),
                    ],
                ),
                Record::new(
                    "FILE_NAME",
                    vec![
                        Param::Str(name.name.clone()),
                        Param::Str(name.time_stamp.clone()),
                        string_list(&name.author),
                        string_list(&name.organization),
                        Param::Str(name.preprocessor_version.clone()),
                        Param::Str(name.originating_system.clone()),
                        Param::Str(name.authorization.clone()),
                    ],
                ),
                Record::new("FILE_SCHEMA", vec![string_list(schemas)]),
            ],
        }
    }

    /// The parameters of the first record called `name`.
    pub fn record(&self, name: &str) -> Option<&[Param]> {
        self.records
            .iter()
            .find(|r| r.name == name)
            .map(|r| r.params.as_slice())
    }

    /// `FILE_DESCRIPTION`, its fields read where they are of their type.
    pub fn file_description(&self) -> Option<FileDescription> {
        let p = self.record("FILE_DESCRIPTION")?;
        Some(FileDescription {
            description: strings(p.first()),
            implementation_level: string(p.get(1)),
        })
    }

    /// `FILE_NAME`, its fields read where they are of their type.
    pub fn file_name(&self) -> Option<FileName> {
        let p = self.record("FILE_NAME")?;
        Some(FileName {
            name: string(p.first()),
            time_stamp: string(p.get(1)),
            author: strings(p.get(2)),
            organization: strings(p.get(3)),
            preprocessor_version: string(p.get(4)),
            originating_system: string(p.get(5)),
            authorization: string(p.get(6)),
        })
    }

    /// `FILE_SCHEMA`'s schema names (empty when there is none).
    pub fn schemas(&self) -> Vec<String> {
        strings(self.record("FILE_SCHEMA").and_then(<[Param]>::first))
    }
}

/// A DATA section (§11).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataSection {
    /// The section's parameters, `DATA('name', ('schema'));` (§11.1); `None` for `DATA;`.
    pub params: Option<Vec<Param>>,
    /// The instances, sorted by id, each id once.
    instances: Vec<(u64, Instance)>,
}

impl DataSection {
    /// A section of `instances`, sorted by id. A repeated id keeps its first instance; the
    /// ids repeated are returned with it.
    pub fn new(
        params: Option<Vec<Param>>,
        mut instances: Vec<(u64, Instance)>,
    ) -> (Self, Vec<u64>) {
        // Stable: of a repeated id, the first stays first.
        instances.sort_by_key(|(id, _)| *id);
        let mut repeated = Vec::new();
        instances.dedup_by(|b, a| {
            let same = a.0 == b.0;
            if same {
                repeated.push(b.0);
            }
            same
        });
        (Self { params, instances }, repeated)
    }

    /// The instance `#id`.
    pub fn get(&self, id: u64) -> Option<&Instance> {
        self.instances
            .binary_search_by_key(&id, |(i, _)| *i)
            .ok()
            .and_then(|at| self.instances.get(at))
            .map(|(_, e)| e)
    }

    /// The instances in id order.
    pub fn instances(&self) -> &[(u64, Instance)] {
        &self.instances
    }

    /// How many instances.
    pub fn len(&self) -> usize {
        self.instances.len()
    }

    /// Whether it holds no instance.
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }
}

/// An exchange structure (§5.5 `EXCHANGE_FILE`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Exchange {
    /// The HEADER section.
    pub header: Header,
    /// The ANCHOR section's text between `ANCHOR;` and `ENDSEC;` (§9), as written.
    pub anchor: Option<String>,
    /// The REFERENCE section's text between `REFERENCE;` and `ENDSEC;` (§10), as written.
    pub reference: Option<String>,
    /// The DATA sections, in file order.
    pub sections: Vec<DataSection>,
    /// The SIGNATURE sections' texts between `SIGNATURE;` and `ENDSEC;` (§14), as written.
    pub signatures: Vec<String>,
}

impl Exchange {
    /// An exchange of one DATA section.
    pub fn new(header: Header, data: DataSection) -> Self {
        Self {
            header,
            sections: vec![data],
            ..Self::default()
        }
    }

    /// The instance `#id` (from the first section that defines it).
    pub fn get(&self, id: u64) -> Option<&Instance> {
        self.sections.iter().find_map(|s| s.get(id))
    }

    /// Every instance, section by section, each in id order.
    pub fn instances(&self) -> impl Iterator<Item = (u64, &Instance)> {
        self.sections
            .iter()
            .flat_map(|s| s.instances.iter().map(|(id, e)| (*id, e)))
    }

    /// How many instances in all sections.
    pub fn len(&self) -> usize {
        self.sections.iter().map(DataSection::len).sum()
    }

    /// Whether no section holds an instance.
    pub fn is_empty(&self) -> bool {
        self.sections.iter().all(DataSection::is_empty)
    }
}
