//! Closed-world type values. The compiler freezes identity and display tables;
//! managed values only carry an index into those immutable tables.
use crate::{
    Dyn, R,
    strings::{Cx, Str},
    telemetry::capture::Type,
};
use baml_rt_gc::{Descriptor, Kind};
pub use baml_type::type_kind::TypeKind;

pub struct StaticType {
    pub ty: &'static Type,
    pub display: &'static str,
    /// Equivalence is computed with the compiler's complete type facts. Keep
    /// the spelled type separately: union order remains visible when rendered.
    pub identity: usize,
    pub kind: TypeKind,
    /// Static indices of the interface types this type implements, as the
    /// interpreter's resolver answered at pack time.
    pub implements: &'static [usize],
    /// `to_baml()`, or `None` where the interpreter rejects rendering.
    pub to_baml: Option<&'static str>,
}

pub const fn descriptor(type_id: u32) -> Descriptor {
    Descriptor {
        kind: Kind::Opaque,
        size: 16,
        ptr_slots: (0, 0),
        tagged_slots: (0, 0),
        elem: None,
        type_id,
        fields: &[],
        finalizer: None,
    }
}

pub fn new(desc: u32, index: usize) -> R<usize> {
    let object = unsafe { crate::try_alloc(desc, 16)? };
    unsafe {
        object.add(8).cast::<usize>().write(index);
    }
    Ok(object as usize)
}

/// # Safety
/// `value` is a live reflected type allocation from this program.
pub unsafe fn index(value: usize) -> usize {
    unsafe { *((value + 8) as *const usize) }
}

/// A declaration head at run time: identity is the declaration's type tag,
/// spelling is the linked declaration name, exactly as the interpreter's
/// `TypeHead` names itself.
#[derive(Clone, Copy)]
pub enum RtHead {
    Declared(fn() -> &'static crate::telemetry::capture::Declaration),
    Named(i64, &'static [u8]),
}
impl RtHead {
    pub fn tag(&self) -> i64 {
        match self {
            Self::Declared(d) => d().tag.as_i64(),
            Self::Named(tag, _) => *tag,
        }
    }
}
impl PartialEq for RtHead {
    fn eq(&self, other: &Self) -> bool {
        self.tag() == other.tag()
    }
}
impl Eq for RtHead {}
impl PartialOrd for RtHead {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for RtHead {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.tag().cmp(&other.tag())
    }
}
impl std::hash::Hash for RtHead {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.tag().hash(state)
    }
}
impl std::fmt::Debug for RtHead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.tag())
    }
}
impl baml_type::HeadDisplay for RtHead {
    fn head_display_name(&self) -> String {
        match self {
            Self::Declared(d) => d().name.display_name().to_string(),
            Self::Named(_, bytes) => {
                let name: baml_type::DeclarationName =
                    borsh::from_slice(bytes).expect("frozen type name");
                name.display_name().to_string()
            }
        }
    }
}

pub type RtTy = baml_type::RealizedTy<RtHead>;
type Ty = baml_type::Ty<RtHead>;

fn param(p: &crate::telemetry::capture::Parameter) -> baml_type::RealizedFunctionParamTy<RtHead> {
    baml_type::RealizedFunctionParamTy {
        name: p.name.map(baml_type::Name::new),
        mode: if p.optional {
            baml_type::FunctionParamMode::Optional
        } else {
            baml_type::FunctionParamMode::Required
        },
        ty: from_capture(p.ty),
    }
}

/// The frozen capture spelling of a type, as the interpreter's realized type.
pub fn from_capture(ty: &Type) -> RtTy {
    use baml_type::Literal as L;
    match ty {
        Type::Type => RtTy::Type,
        Type::Null => RtTy::Null,
        Type::Never => RtTy::Never,
        Type::Unknown => RtTy::Unknown,
        Type::Bigint => RtTy::Bigint,
        Type::Int => RtTy::Int,
        Type::Float => RtTy::Float,
        Type::Bool => RtTy::Bool,
        Type::String => RtTy::String,
        Type::Uint8Array => RtTy::Uint8Array,
        Type::RustType => RtTy::RustType,
        Type::Future(v, e) => RtTy::Future(Box::new(from_capture(v)), Box::new(from_capture(e))),
        Type::Function {
            params,
            ret,
            throws,
        } => RtTy::Function {
            params: params.iter().map(param).collect(),
            ret: Box::new(from_capture(ret)),
            throws: Box::new(from_capture(throws)),
        },
        Type::LiteralInt(v, f) => RtTy::Literal(L::Int(*v), *f),
        Type::LiteralBool(v, f) => RtTy::Literal(L::Bool(*v), *f),
        Type::LiteralString(v, f) => RtTy::Literal(L::String((*v).into()), *f),
        Type::LiteralFloat(v, f) => RtTy::Literal(L::Float((*v).into()), *f),
        Type::Class(d) => RtTy::Class(RtHead::Declared(*d), Box::new([])),
        Type::GenericClass(d, args) => RtTy::Class(
            RtHead::Declared(*d),
            args.iter().map(from_capture).collect(),
        ),
        Type::Enum(d) => RtTy::Enum(RtHead::Declared(*d)),
        Type::EnumVariant(d, v) => RtTy::EnumVariant(RtHead::Declared(*d), (*v).into()),
        Type::Alias(tag, name) => RtTy::TypeAlias(RtHead::Named(*tag, name)),
        Type::Interface(tag, name, args, assoc) => RtTy::Interface(
            RtHead::Named(*tag, name),
            args.iter().map(from_capture).collect(),
            assoc
                .iter()
                .map(|(n, t)| ((*n).into(), from_capture(t)))
                .collect(),
        ),
        Type::Map(k, v) => RtTy::Map {
            key: Box::new(from_capture(k)),
            value: Box::new(from_capture(v)),
        },
        Type::List(e) => RtTy::List(Box::new(from_capture(e))),
        Type::Union(m) => RtTy::Union(m.iter().map(from_capture).collect()),
    }
}

fn leak<T>(value: T) -> &'static T {
    Box::leak(Box::new(value))
}
fn leak_slice<T>(values: Vec<T>) -> &'static [T] {
    Box::leak(values.into_boxed_slice())
}
fn leak_str(value: &str) -> &'static str {
    Box::leak(value.to_owned().into_boxed_str())
}

/// The capture spelling of a type built at run time. Runtime types are
/// interned, so each distinct spelling is frozen once per instance.
pub fn to_capture(ty: &RtTy) -> R<&'static Type> {
    use baml_type::Literal as L;
    let unsupported =
        || crate::Error::internal(format!("unsupported runtime reflected type {ty:?}"));
    Ok(leak(match ty {
        RtTy::Type => Type::Type,
        RtTy::Null => Type::Null,
        RtTy::Never => Type::Never,
        RtTy::Unknown => Type::Unknown,
        RtTy::Bigint => Type::Bigint,
        RtTy::Int => Type::Int,
        RtTy::Float => Type::Float,
        RtTy::Bool => Type::Bool,
        RtTy::String => Type::String,
        RtTy::Uint8Array => Type::Uint8Array,
        RtTy::RustType => Type::RustType,
        RtTy::Future(v, e) => Type::Future(to_capture(v)?, to_capture(e)?),
        RtTy::Function {
            params,
            ret,
            throws,
        } => Type::Function {
            params: leak_slice(
                params
                    .iter()
                    .map(|p| {
                        Ok(crate::telemetry::capture::Parameter {
                            name: p.name.as_ref().map(|n| leak_str(n.as_str())),
                            optional: p.mode == baml_type::FunctionParamMode::Optional,
                            ty: to_capture(&p.ty)?,
                        })
                    })
                    .collect::<R<Vec<_>>>()?,
            ),
            ret: to_capture(ret)?,
            throws: to_capture(throws)?,
        },
        RtTy::Literal(L::Int(v), f) => Type::LiteralInt(*v, *f),
        RtTy::Literal(L::Bool(v), f) => Type::LiteralBool(*v, *f),
        RtTy::Literal(L::String(v), f) => Type::LiteralString(leak_str(v.as_str()), *f),
        RtTy::Literal(L::Float(v), f) => Type::LiteralFloat(leak_str(v.as_ref()), *f),
        RtTy::Class(RtHead::Declared(d), args) if args.is_empty() => Type::Class(*d),
        RtTy::Class(RtHead::Declared(d), args) => Type::GenericClass(
            *d,
            leak_slice(
                args.iter()
                    .map(|a| to_capture(a).map(clone_capture))
                    .collect::<R<Vec<_>>>()?,
            ),
        ),
        RtTy::Enum(RtHead::Declared(d)) => Type::Enum(*d),
        RtTy::EnumVariant(RtHead::Declared(d), v) => Type::EnumVariant(*d, leak_str(v.as_str())),
        RtTy::TypeAlias(RtHead::Named(tag, name)) => Type::Alias(*tag, name),
        RtTy::Interface(RtHead::Named(tag, name), args, assoc) => Type::Interface(
            *tag,
            name,
            leak_slice(
                args.iter()
                    .map(|a| to_capture(a).map(clone_capture))
                    .collect::<R<Vec<_>>>()?,
            ),
            leak_slice(
                assoc
                    .iter()
                    .map(|(n, t)| Ok((leak_str(n.as_str()), clone_capture(to_capture(t)?))))
                    .collect::<R<Vec<_>>>()?,
            ),
        ),
        RtTy::Map { key, value } => Type::Map(to_capture(key)?, to_capture(value)?),
        RtTy::List(e) => Type::List(to_capture(e)?),
        RtTy::Union(m) => Type::Union(leak_slice(
            m.iter()
                .map(|t| to_capture(t).map(clone_capture))
                .collect::<R<Vec<_>>>()?,
        )),
        _ => return Err(unsupported()),
    }))
}

/// Capture types are immutable trees of `'static` references; a shallow copy
/// shares every child.
fn clone_capture(ty: &'static Type) -> Type {
    match ty {
        Type::Type => Type::Type,
        Type::Null => Type::Null,
        Type::Never => Type::Never,
        Type::Unknown => Type::Unknown,
        Type::Bigint => Type::Bigint,
        Type::Int => Type::Int,
        Type::Float => Type::Float,
        Type::Bool => Type::Bool,
        Type::String => Type::String,
        Type::Uint8Array => Type::Uint8Array,
        Type::RustType => Type::RustType,
        Type::Future(v, e) => Type::Future(v, e),
        Type::Function {
            params,
            ret,
            throws,
        } => Type::Function {
            params,
            ret,
            throws,
        },
        Type::LiteralInt(v, f) => Type::LiteralInt(*v, *f),
        Type::LiteralBool(v, f) => Type::LiteralBool(*v, *f),
        Type::LiteralString(v, f) => Type::LiteralString(v, *f),
        Type::LiteralFloat(v, f) => Type::LiteralFloat(v, *f),
        Type::Class(d) => Type::Class(*d),
        Type::GenericClass(d, a) => Type::GenericClass(*d, a),
        Type::Enum(d) => Type::Enum(*d),
        Type::EnumVariant(d, v) => Type::EnumVariant(*d, v),
        Type::Alias(t, n) => Type::Alias(*t, n),
        Type::Interface(t, n, a, s) => Type::Interface(*t, n, a, s),
        Type::Map(k, v) => Type::Map(k, v),
        Type::List(e) => Type::List(e),
        Type::Union(m) => Type::Union(m),
    }
}

/// Runtime facts for equivalence of types the program builds at run time.
/// Recursive aliases stay opaque and interface membership is not claimed,
/// the interpreter's fail-safe answers when a fact is unavailable.
struct Facts {
    enums: &'static [(i64, &'static [&'static str], u64)],
}
impl baml_type::normalize::TypeContext<RtHead> for Facts {
    fn well_known(&self, _head: baml_type::normalize::WellKnownHead) -> Option<RtHead> {
        None
    }
    fn alias_def(&self, _name: &RtHead) -> Option<Ty> {
        None
    }
    fn implements_interface(
        &self,
        _concrete: &Ty,
        _interface: &baml_type::Interface<RtHead>,
    ) -> bool {
        false
    }
    fn type_var_bound(&self, _param: &baml_type::ParamTy) -> Vec<baml_type::Interface<RtHead>> {
        Vec::new()
    }
    fn interface_requires(
        &self,
        _sub: &baml_type::Interface<RtHead>,
        _sup: &baml_type::Interface<RtHead>,
    ) -> bool {
        false
    }
    fn enum_variants(&self, name: &RtHead) -> Option<Vec<baml_type::Name>> {
        self.enums
            .iter()
            .find(|(tag, _, _)| *tag == name.tag())
            .map(|(_, v, _)| v.iter().map(|v| baml_type::Name::new(*v)).collect())
    }
    fn associated_type_bound(
        &self,
        _interface: &baml_type::Interface<RtHead>,
        _assoc: baml_type::Name,
    ) -> Vec<baml_type::Interface<RtHead>> {
        Vec::new()
    }
    fn project(
        &self,
        _base: &Ty,
        _interface: &baml_type::Interface<RtHead>,
        _member: &baml_type::Name,
        _fuel: u32,
    ) -> baml_type::normalize::ProjectionStep<RtHead> {
        baml_type::normalize::ProjectionStep::Opaque
    }
}

/// Closed-world reflection metadata of one program.
pub struct Program {
    pub types: &'static [StaticType],
    /// Enum type tag, variant names and the value tag of its carriers.
    pub enums: &'static [(i64, &'static [&'static str], u64)],
    /// Field descriptors of reflected class types, keyed by static entry.
    pub classes: &'static [ClassMeta],
    /// Variant descriptors of reflected enums, keyed by enum type tag.
    pub enum_meta: &'static [EnumMeta],
}

/// Schema metadata exactly as the linked declaration carries it.
pub struct Meta {
    pub alias: Option<&'static str>,
    pub description: Option<&'static str>,
    pub docstring: Option<&'static str>,
    pub other: &'static [(&'static str, &'static str)],
    pub skip: bool,
}
pub struct FieldMeta {
    pub name: &'static str,
    pub ty: &'static Type,
    pub meta: Meta,
}
pub struct ClassMeta {
    pub entry: usize,
    pub fields: &'static [FieldMeta],
    pub meta: Meta,
}
pub struct EnumMeta {
    pub tag: i64,
    pub variants: &'static [(&'static str, Meta)],
    pub meta: Meta,
}

/// The class descriptors of a reflected class type.
/// # Safety
/// `value` is a live reflected class type.
pub unsafe fn class_meta(program: &'static Program, value: usize) -> R<&'static ClassMeta> {
    let a = unsafe { index(crate::nonnull(value)?) };
    let entry = with(program, |r| static_entry(r, program, a));
    entry
        .and_then(|entry| {
            with(program, |r| {
                let identity = r.entries[entry].identity;
                program
                    .classes
                    .iter()
                    .find(|c| r.entries[c.entry].identity == identity)
            })
        })
        .ok_or_else(|| crate::Error::internal("class fields outside the closed-world table".into()))
}

/// The variant descriptors of a reflected enum type.
/// # Safety
/// `value` is a live reflected enum type.
pub unsafe fn enum_meta(program: &'static Program, value: usize) -> R<&'static EnumMeta> {
    let tag = match unsafe { ty(program, value) }? {
        RtTy::Enum(head) => head.tag(),
        other => {
            return Err(crate::Error::internal(format!(
                "enum metadata of {other:?}"
            )));
        }
    };
    program
        .enum_meta
        .iter()
        .find(|e| e.tag == tag)
        .ok_or_else(|| {
            crate::Error::internal("enum metadata outside the closed-world table".into())
        })
}

/// Intern a frozen capture type (a field's declared type) as a type value.
pub fn intern_capture(program: &'static Program, desc: u32, ty: &'static Type) -> R<usize> {
    intern(program, desc, from_capture(ty))
}

struct Entry {
    ty: RtTy,
    display: String,
    identity: usize,
    kind: TypeKind,
    capture: &'static Type,
}
struct Registry {
    program: *const Program,
    entries: Vec<Entry>,
    representatives: Vec<usize>,
}
thread_local! {
    static REGISTRY: std::cell::RefCell<Option<Registry>> = const { std::cell::RefCell::new(None) };
}

fn with<T>(program: &'static Program, f: impl FnOnce(&mut Registry) -> T) -> T {
    REGISTRY.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot
            .as_ref()
            .is_none_or(|r| !std::ptr::eq(r.program, program))
        {
            let entries: Vec<Entry> = program
                .types
                .iter()
                .map(|t| Entry {
                    ty: from_capture(t.ty),
                    display: t.display.to_owned(),
                    identity: t.identity,
                    kind: t.kind,
                    capture: t.ty,
                })
                .collect();
            let mut representatives = Vec::new();
            for (i, e) in entries.iter().enumerate() {
                if !representatives
                    .iter()
                    .any(|&r: &usize| entries[r].identity == e.identity)
                {
                    representatives.push(i);
                }
            }
            *slot = Some(Registry {
                program,
                entries,
                representatives,
            });
        }
        f(slot.as_mut().unwrap())
    })
}

impl Registry {
    fn intern(&mut self, ty: RtTy) -> R<usize> {
        if let Some(i) = self.entries.iter().position(|e| e.ty == ty) {
            return Ok(i);
        }
        let program = unsafe { &*self.program };
        let facts = Facts {
            enums: program.enums,
        };
        let comparable = Ty::from(ty.clone());
        let identity = self
            .representatives
            .iter()
            .find(|&&r| {
                baml_type::normalize::equivalent(
                    &Ty::from(self.entries[r].ty.clone()),
                    &comparable,
                    &facts,
                )
            })
            .map(|&r| self.entries[r].identity);
        let identity = match identity {
            Some(identity) => identity,
            None => {
                let identity = self
                    .entries
                    .iter()
                    .map(|e| e.identity + 1)
                    .max()
                    .unwrap_or(0);
                self.representatives.push(self.entries.len());
                identity
            }
        };
        let capture = to_capture(&ty)?;
        self.entries.push(Entry {
            display: ty.to_string(),
            kind: baml_type::type_kind::classify_type(&ty),
            ty,
            identity,
            capture,
        });
        Ok(self.entries.len() - 1)
    }
}

/// # Safety
/// `value` is a live reflected type allocation from this program.
pub unsafe fn ty(program: &'static Program, value: usize) -> R<RtTy> {
    let value = crate::nonnull(value)?;
    Ok(with(program, |r| {
        r.entries[unsafe { index(value) }].ty.clone()
    }))
}

/// Allocate the carrier of `ty`, interning a runtime-built type.
pub fn intern(program: &'static Program, desc: u32, ty: RtTy) -> R<usize> {
    let index = with(program, |r| r.intern(ty))?;
    new(desc, index)
}

/// The capture spelling of a type value, for telemetry.
pub fn capture_type(program: &'static Program, index: usize) -> &'static Type {
    with(program, |r| r.entries[index].capture)
}

/// # Safety
/// Non-null values bearing `tag` have this program's reflected type layout.
pub unsafe fn equal(program: &'static Program, tag: u64, left: Dyn, right: Dyn) -> bool {
    if left.tag == 2 || right.tag == 2 {
        return left.tag == right.tag;
    }
    if left.tag != tag || right.tag != tag || left.payload == 0 || right.payload == 0 {
        return false;
    }
    let (a, b) = unsafe { (index(left.payload as usize), index(right.payload as usize)) };
    with(program, |r| r.entries[a].identity == r.entries[b].identity)
}

/// # Safety
/// `left` and `right` are live reflected type allocations.
pub unsafe fn same_identity(program: &'static Program, left: usize, right: usize) -> bool {
    let (a, b) = unsafe { (index(left), index(right)) };
    with(program, |r| r.entries[a].identity == r.entries[b].identity)
}

/// # Safety
/// `value` is a live type allocation from this program.
pub unsafe fn text(program: &'static Program, value: usize) -> R<Str> {
    let value = crate::nonnull(value)?;
    let display = with(program, |r| {
        r.entries[unsafe { index(value) }].display.clone()
    });
    Cx::current().string(&display)
}

/// # Safety
/// `value` is a live reflected type allocation from this program.
pub unsafe fn kind(program: &'static Program, value: usize) -> TypeKind {
    with(program, |r| r.entries[unsafe { index(value) }].kind)
}

/// Children a kind view exposes: element, key/value, return, union members.
/// # Safety
/// `value` is a type allocation of a kind exposing the requested child.
pub unsafe fn child(program: &'static Program, desc: u32, value: usize, child: usize) -> R<usize> {
    let ty = unsafe { ty(program, value) }?;
    let child = match (ty, child) {
        (RtTy::List(e), 0) => *e,
        (RtTy::Map { key, .. }, 0) => *key,
        (RtTy::Map { value, .. }, 1) => *value,
        (RtTy::Function { ret, .. }, 0) => *ret,
        (RtTy::Union(m), i) if i < m.len() => m[i].clone(),
        (ty, _) => {
            return Err(crate::Error::internal(format!(
                "reflected type {ty:?} has no child {child}"
            )));
        }
    };
    intern(program, desc, child)
}

/// The number of union members (or function parameters) a view exposes.
/// # Safety
/// `value` is a live reflected type allocation.
pub unsafe fn arity(program: &'static Program, value: usize) -> R<usize> {
    Ok(match unsafe { ty(program, value) }? {
        RtTy::Union(m) => m.len(),
        RtTy::Function { params, .. } => params.len(),
        _ => 0,
    })
}

/// A function parameter: name, optional flag and the carrier of its type.
/// # Safety
/// `value` is a live reflected function type.
pub unsafe fn parameter(
    program: &'static Program,
    desc: u32,
    value: usize,
    index: usize,
) -> R<(Option<String>, bool, usize)> {
    let RtTy::Function { params, .. } = (unsafe { ty(program, value) }?) else {
        return Err(crate::Error::internal(
            "parameters of a non-function type".into(),
        ));
    };
    let p = &params[index];
    let ty = intern(program, desc, p.ty.clone())?;
    Ok((
        p.name.as_ref().map(|n| n.as_str().to_owned()),
        p.mode == baml_type::FunctionParamMode::Optional,
        ty,
    ))
}

/// The static entry equivalent to a type value: itself, or the first frozen
/// type of the same identity (equivalent types share every pack-time fact).
fn static_entry(r: &Registry, program: &Program, index: usize) -> Option<usize> {
    if index < program.types.len() {
        return Some(index);
    }
    let identity = r.entries[index].identity;
    (0..program.types.len()).find(|&i| r.entries[i].identity == identity)
}

/// `t._implements(other)`.
/// # Safety
/// Both are live reflected type allocations.
pub unsafe fn implements(program: &'static Program, ty: usize, other: usize) -> R<bool> {
    let (a, b) = unsafe { (index(crate::nonnull(ty)?), index(crate::nonnull(other)?)) };
    with(program, |r| {
        if !matches!(r.entries[b].ty, RtTy::Interface(..)) {
            return Ok(false);
        }
        let (Some(a), Some(_)) = (static_entry(r, program, a), static_entry(r, program, b)) else {
            return Err(crate::Error::internal(
                "implements on a type outside the closed-world table".into(),
            ));
        };
        let identity = r.entries[b].identity;
        Ok(program.types[a]
            .implements
            .iter()
            .any(|&j| r.entries[j].identity == identity))
    })
}

/// `t.to_baml()`.
/// # Safety
/// `ty` is a live reflected type allocation.
pub unsafe fn to_baml(program: &'static Program, ty: usize) -> R<Str> {
    let a = unsafe { index(crate::nonnull(ty)?) };
    let text = if a < program.types.len() {
        program.types[a].to_baml
    } else {
        None
    };
    let text = text.ok_or_else(|| {
        crate::Error::internal("to_baml of a type without a frozen rendering".into())
    })?;
    Cx::current().string(text)
}

/// `t[]`.
/// # Safety
/// `value` is a live reflected type allocation.
pub unsafe fn array(program: &'static Program, desc: u32, value: usize) -> R<usize> {
    let ty = unsafe { ty(program, value) }?;
    intern(program, desc, RtTy::List(Box::new(ty)))
}

/// `t | null`, without duplicating a null member.
/// # Safety
/// `value` is a live reflected type allocation.
pub unsafe fn optional(program: &'static Program, desc: u32, value: usize) -> R<usize> {
    let ty = unsafe { ty(program, value) }?;
    let mut members = match ty {
        RtTy::Union(m) => m.to_vec(),
        other => vec![other],
    };
    if !members.iter().any(|m| matches!(m, RtTy::Null)) {
        members.push(RtTy::Null);
    }
    intern(program, desc, RtTy::Union(members.into()))
}

/// `map<key, value>`.
/// # Safety
/// Both are live reflected type allocations.
pub unsafe fn map(program: &'static Program, desc: u32, key: usize, value: usize) -> R<usize> {
    let key = unsafe { ty(program, key) }?;
    let value = unsafe { ty(program, value) }?;
    intern(
        program,
        desc,
        RtTy::Map {
            key: Box::new(key),
            value: Box::new(value),
        },
    )
}

/// `reflect.Literal.new(value)`: the singleton type of a scalar value.
/// # Safety
/// `value` is a live string, bigint, int or bool value of this program.
pub unsafe fn literal(program: &'static Program, desc: u32, value: Dyn) -> R<usize> {
    use baml_type::Literal as L;
    let literal = match value.tag {
        0 => L::Int(value.payload as i64),
        1 => L::Bool(value.payload != 0),
        3 => {
            L::String(unsafe { Str::from_raw(crate::nonnull(value.payload as usize)?) }.to_owned())
        }
        _ => L::Bigint(unsafe {
            crate::native::bigint_owned(crate::nonnull(value.payload as usize)?)
        }),
    };
    intern(
        program,
        desc,
        RtTy::Literal(literal, baml_type::Freshness::Regular),
    )
}

/// `reflect.Literal.value()`: the value a literal or enum-variant type denotes.
/// # Safety
/// `value` is a live reflected literal type.
pub unsafe fn literal_value(
    program: &'static Program,
    value: usize,
    bigint_tag: Option<u64>,
) -> R<Dyn> {
    use baml_type::Literal as L;
    Ok(match unsafe { ty(program, value) }? {
        RtTy::Literal(L::Int(v), _) => Dyn {
            tag: 0,
            payload: v as u64,
        },
        RtTy::Literal(L::Bool(v), _) => Dyn {
            tag: 1,
            payload: u64::from(v),
        },
        RtTy::Literal(L::String(v), _) => Dyn {
            tag: 3,
            payload: Cx::current().string(&v)?.as_raw() as u64,
        },
        RtTy::Literal(L::Float(v), _) => Dyn {
            tag: crate::FLOAT_TAG,
            payload: v
                .parse::<f64>()
                .map_err(|_| crate::Error::internal("float literal".into()))?
                .to_bits(),
        },
        RtTy::Literal(L::Bigint(v), _) => Dyn {
            tag: bigint_tag
                .ok_or_else(|| crate::Error::internal("bigint literal without bigints".into()))?,
            payload: crate::native::bigint(v)? as u64,
        },
        RtTy::EnumVariant(head, name) => {
            let (_, variants, tag) = program
                .enums
                .iter()
                .find(|(t, _, _)| *t == head.tag())
                .ok_or_else(|| crate::Error::internal("enum variant without a layout".into()))?;
            let index = variants
                .iter()
                .position(|v| *v == name.as_str())
                .ok_or_else(|| crate::Error::internal("unknown enum variant".into()))?;
            Dyn {
                tag: *tag,
                payload: index as u64,
            }
        }
        other => {
            return Err(crate::Error::internal(format!(
                "literal value of a non-literal type {other:?}"
            )));
        }
    })
}

/// The concrete type of a runtime value, as the interpreter reports it.
/// # Safety
/// `value` is a live value of the active program described by `shapes`.
pub unsafe fn of_value(
    program: &'static Program,
    desc: u32,
    shapes: &[crate::telemetry::capture::Shape],
    value: Dyn,
) -> R<usize> {
    use crate::telemetry::capture::Shape;
    let ty = match value.tag {
        0 => RtTy::Int,
        1 => RtTy::Bool,
        2 => RtTy::Null,
        crate::FLOAT_TAG => RtTy::Float,
        3 => RtTy::String,
        tag => {
            let shape = shapes
                .get((tag - 3) as usize)
                .ok_or_else(|| crate::Error::internal("type of an untyped value".into()))?;
            if value.payload == 0 && !matches!(shape, Shape::Enum { .. }) {
                RtTy::Null
            } else {
                match shape {
                    Shape::String => RtTy::String,
                    Shape::Bigint => RtTy::Bigint,
                    Shape::Uint8Array => RtTy::Uint8Array,
                    Shape::Reflected(_) => RtTy::Type,
                    Shape::RustType => RtTy::RustType,
                    Shape::Class { declaration, .. } => {
                        RtTy::Class(RtHead::Declared(*declaration), Box::new([]))
                    }
                    Shape::GenericClass {
                        declaration, args, ..
                    } => RtTy::Class(
                        RtHead::Declared(*declaration),
                        args.iter().map(from_capture).collect(),
                    ),
                    Shape::Enum { declaration, .. } => RtTy::Enum(RtHead::Declared(*declaration)),
                    Shape::List { element, .. } => {
                        let aux = unsafe { (*(value.payload as *const baml_rt_gc::Header)).aux };
                        let element = if aux == crate::telemetry::capture::UNKNOWN_ELEMENT_TYPE {
                            RtTy::Unknown
                        } else if aux == crate::telemetry::capture::OPTIONAL_ELEMENT_TYPE {
                            RtTy::Union(vec![from_capture(element), RtTy::Null].into())
                        } else {
                            from_capture(element)
                        };
                        RtTy::List(Box::new(element))
                    }
                    Shape::Map { key, value, .. } => RtTy::Map {
                        key: Box::new(from_capture(key)),
                        value: Box::new(from_capture(value)),
                    },
                    Shape::MapStringInt => RtTy::Map {
                        key: Box::new(RtTy::String),
                        value: Box::new(RtTy::Int),
                    },
                    Shape::MapStringPtr { value, .. } => RtTy::Map {
                        key: Box::new(RtTy::String),
                        value: Box::new(from_capture(value)),
                    },
                    _ => {
                        return Err(crate::Error::internal(
                            "type of a function, future or opaque value".into(),
                        ));
                    }
                }
            }
        }
    };
    intern(program, desc, ty)
}

#[cfg(test)]
mod tests {
    use super::*;

    static TYPES: &[StaticType] = &[
        StaticType {
            ty: &Type::Int,
            display: "int",
            identity: 0,
            kind: TypeKind::Primitive,
            implements: &[],
            to_baml: None,
        },
        StaticType {
            ty: &Type::Bool,
            display: "bool",
            identity: 1,
            kind: TypeKind::Primitive,
            implements: &[],
            to_baml: None,
        },
        StaticType {
            ty: &Type::Union(&[Type::String, Type::Int]),
            display: "string | int",
            identity: 2,
            kind: TypeKind::Union,
            implements: &[],
            to_baml: None,
        },
        StaticType {
            ty: &Type::Union(&[Type::Int, Type::String]),
            display: "int | string",
            identity: 2,
            kind: TypeKind::Union,
            implements: &[],
            to_baml: None,
        },
        StaticType {
            ty: &Type::List(&Type::Int),
            display: "int[]",
            identity: 3,
            kind: TypeKind::Array,
            implements: &[],
            to_baml: None,
        },
    ];
    static PROGRAM: Program = Program {
        types: TYPES,
        enums: &[],
        classes: &[],
        enum_meta: &[],
    };
    static DESCRIPTORS: &[Descriptor] = &[crate::STRING_DESC, descriptor(4)];

    #[test]
    fn identity_is_separate_from_spelling_and_survives_collection() {
        let mut instance = crate::Instance::new(DESCRIPTORS, &[false, false, false, true, true]);
        instance.set_stress(true);
        unsafe {
            instance.run_owned(|| -> R<()> {
                let first = new(1, 2)?;
                let second = new(1, 3)?;
                let boolean = new(1, 1)?;
                crate::tasks::collect()?;
                let tagged = |value| Dyn {
                    tag: 4,
                    payload: value as u64,
                };
                assert!(equal(&PROGRAM, 4, tagged(first), tagged(second)));
                assert!(!equal(&PROGRAM, 4, tagged(first), tagged(boolean)));
                assert!(!equal(
                    &PROGRAM,
                    4,
                    tagged(first),
                    Dyn { tag: 0, payload: 2 }
                ));
                assert!(equal(
                    &PROGRAM,
                    4,
                    Dyn { tag: 2, payload: 0 },
                    Dyn { tag: 2, payload: 0 }
                ));
                assert_eq!(text(&PROGRAM, first)?.to_owned(), "string | int");
                assert_eq!(text(&PROGRAM, second)?.to_owned(), "int | string");
                // Runtime-built types reuse static identity by equivalence and
                // render with the interpreter's display.
                let int = new(1, 0)?;
                let list = array(&PROGRAM, 1, int)?;
                assert_eq!(index(list), 4);
                let optional = optional(&PROGRAM, 1, second)?;
                assert_eq!(text(&PROGRAM, optional)?.to_owned(), "int | string | null");
                let nested = array(&PROGRAM, 1, optional)?;
                assert_eq!(
                    text(&PROGRAM, nested)?.to_owned(),
                    "(int | string | null)[]"
                );
                assert!(!same_identity(&PROGRAM, nested, list));
                let reordered = optional_of(&PROGRAM, first)?;
                assert!(same_identity(&PROGRAM, reordered, optional));
                Ok(())
            })
        }
        .unwrap();
    }

    unsafe fn optional_of(program: &'static Program, value: usize) -> R<usize> {
        unsafe { optional(program, 1, value) }
    }
}
