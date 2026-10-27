//! Which values of a type can come from JSON.
//!
//! A JSON reader builds only some values of a type. `uint8array` has no JSON
//! form, `never` has no values at all, and a class whose every value is
//! cyclic has none that JSON, which holds finite trees only, can carry. Each
//! reader drops what it cannot build: `uint8array | string` reads as
//! `string`, an optional `uint8array?` field is only `null`, and `never[]`
//! is only `[]`.
//!
//! [`JsonInhabitance`] answers that question once for every reader:
//! `baml.json.schema`, the LLM output check, the prompt renderer, and
//! schema-aligned parsing. A reader describes its definitions through
//! [`JsonDefs`], which also decides the types whose JSON form is the
//! reader's own call (see [`JsonDefs::opaque`]), and the types a model
//! writes in another form (see [`JsonDefs::shape`]).

use rustc_hash::{FxHashMap, FxHashSet};

use crate::{Head, HeadDisplay, RuntimeTy};

/// Which names a class's fields and an enum's variants go by in JSON: the
/// runtime form of `baml.json.Naming`. The JSON Schema `baml.json.schema`
/// writes, and the JSON the other `baml.json` conversions read and write, all
/// follow it, through [`Self::key`] and [`Self::shows`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JsonNaming {
    /// Declared names. `@alias` and `@skip` have no effect: the interchange
    /// form of BEP-038.
    Declared,
    /// The names a model is shown: a member's `@alias` if it has one, and no
    /// `@skip` member at all.
    Aliased,
}

impl JsonNaming {
    /// `Aliased` when `aliased` holds, else `Declared`.
    pub fn new(aliased: bool) -> Self {
        if aliased {
            JsonNaming::Aliased
        } else {
            JsonNaming::Declared
        }
    }

    /// The JSON key of a field, or the JSON string of a variant, declared as
    /// `name` with `alias`.
    pub fn key<'a>(self, name: &'a str, alias: Option<&'a str>) -> &'a str {
        match self {
            JsonNaming::Declared => name,
            JsonNaming::Aliased => alias.unwrap_or(name),
        }
    }

    /// Whether a member with this `@skip` setting appears in JSON at all.
    pub fn shows(self, skip: bool) -> bool {
        match self {
            JsonNaming::Declared => true,
            JsonNaming::Aliased => !skip,
        }
    }
}

/// What a type's values look like as JSON once the values with no JSON form
/// are set aside.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum JsonStatus {
    /// Some values have a JSON form, and a reader builds exactly those.
    Json,
    /// The type has values, but none has a JSON form: `uint8array`, a
    /// function, a class that requires one, or a class whose every value is
    /// cyclic. A union drops it like `never`, but a list or map of it has no
    /// JSON form either, rather than being only the empty one.
    NoJson,
    /// The type has no values at all: `never`, an enum with no variants, and
    /// whatever requires one of them. A list or map of it is only the empty
    /// one.
    Empty,
}

/// A class as a reader's definitions describe it, at the type arguments it
/// was asked for.
#[derive(Clone, Debug)]
pub struct JsonClass<H: Head> {
    /// Every field the class declares, `@skip` ones included, in declaration
    /// order.
    pub fields: Vec<JsonField<H>>,
}

/// One field of a [`JsonClass`].
#[derive(Clone, Debug)]
pub struct JsonField<H: Head> {
    pub name: String,
    pub alias: Option<String>,
    pub skip: bool,
    pub ty: RuntimeTy<H>,
}

/// One variant of an enum, as a reader's definitions describe it.
#[derive(Clone, Debug)]
pub struct JsonVariant {
    pub name: String,
    pub skip: bool,
}

/// The definitions a type reaches, as one reader sees them.
pub trait JsonDefs<H: Head> {
    /// Why a definition cannot be looked up, or why the reader rejects a type.
    type Error;

    /// The class `head` instantiated at `args`, its field types realized at
    /// those arguments.
    fn class(&self, head: &H, args: &[RuntimeTy<H>]) -> Result<JsonClass<H>, Self::Error>;

    /// The variants of the enum `head`, `@skip` ones included, in declaration
    /// order.
    fn variants(&self, head: &H) -> Result<Vec<JsonVariant>, Self::Error>;

    /// The type the recursive alias `head` stands for.
    fn alias_target(&self, head: &H) -> Result<RuntimeTy<H>, Self::Error>;

    /// The status of a type whose JSON form is the reader's own call:
    /// `unknown`, an interface, a type parameter or projection, and media. No
    /// schema describes the first ones, and a reader may or may not build
    /// media. An error stops the analysis.
    fn opaque(&self, ty: &RuntimeTy<H>) -> Result<JsonStatus, Self::Error>;

    /// Rejects a type the reader cannot take, wherever the analysis meets it
    /// in a part of a value JSON carries. Accepts every type by default.
    fn check(&self, _ty: &RuntimeTy<H>) -> Result<(), Self::Error> {
        Ok(())
    }

    /// The type JSON holds in place of `ty`, a class or enum type: its
    /// `baml.FromLlmOutput` `Shape`, for a reader of a model's answer. The
    /// analysis then gives `ty` the status of that type, and never looks at
    /// the fields or variants of `ty`. `None` by default: the type is read by
    /// its fields or variants.
    fn shape(&self, _ty: &RuntimeTy<H>) -> Result<Option<RuntimeTy<H>>, Self::Error> {
        Ok(None)
    }
}

/// Why [`JsonInhabitance::of`] stopped: a reader's error, and where the
/// analysis met it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonError<E> {
    pub error: E,
    /// The declared names of the fields from the root type down to the type
    /// the error is about. Empty when it is about the root itself.
    pub path: Vec<String>,
}

/// A class instantiation, recursive alias, or shaped enum: a definition the
/// analysis gives one status, whose body can reach itself.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum DefId<H: Head> {
    /// Each argument's union is spelled flat, so `Box<(int | null) | null>`
    /// and `Box<int | null>` are one definition.
    Class(H, Box<[RuntimeTy<H>]>),
    Alias(H),
    /// An enum the reader gives a shape ([`JsonDefs::shape`]). Any other
    /// enum is a leaf, not a definition.
    Enum(H),
}

impl<H: Head> DefId<H> {
    fn class(head: &H, args: &[RuntimeTy<H>]) -> Self {
        DefId::Class(
            head.clone(),
            args.iter().cloned().map(RuntimeTy::flatten_union).collect(),
        )
    }

    fn head(&self) -> &H {
        match self {
            DefId::Class(head, _) | DefId::Alias(head) | DefId::Enum(head) => head,
        }
    }
}

/// A class's fields as JSON shows them under the analysis's [`JsonNaming`].
#[derive(Clone, Debug)]
struct ClassBody<H: Head> {
    /// The fields JSON carries, each with its declared name, in declaration
    /// order.
    fields: Vec<(String, RuntimeTy<H>)>,
    /// The first field JSON leaves out (`@skip`, under aliased names) whose
    /// type has no empty value, with that type. A field JSON leaves out holds
    /// its type's empty value, so with none to hold, the class has no JSON
    /// form.
    unfillable: Option<(String, RuntimeTy<H>)>,
}

#[derive(Clone, Debug)]
enum DefBody<H: Head> {
    Class(ClassBody<H>),
    Alias(RuntimeTy<H>),
    /// A class or enum JSON holds as this type instead
    /// ([`JsonDefs::shape`]): it has the status of that type.
    Shaped(RuntimeTy<H>),
}

/// One level of a type, as the analysis sees it. [`JsonInhabitance::node`] is
/// the one place that matches on [`RuntimeTy`] for the fixpoints.
enum Node<'t, H: Head> {
    /// A type whose every value has a JSON form.
    Leaf,
    /// A type none of whose values has a JSON form.
    NoJson,
    /// A type with no values.
    Never,
    /// A union's members, nested unions flattened.
    Union(Vec<&'t RuntimeTy<H>>),
    /// A list's element or a map's value.
    Container(&'t RuntimeTy<H>),
    Def(DefId<H>),
}

/// The [`JsonStatus`] of every class instantiation and recursive alias some
/// root types reach, under one [`JsonNaming`].
///
/// The statuses are fixpoints over the definition graph, computed in two
/// steps. Which definitions are [`JsonStatus::Empty`] is the greatest fixpoint
/// of "has a value": a cycle through fields does not empty a class, because a
/// BAML value can be cyclic. The rest are [`JsonStatus::Json`] unless
/// something forces [`JsonStatus::NoJson`]: a field that requires a value with
/// no JSON form, or having only cyclic values. JSON holds finite trees only,
/// so a definition must also have a finite value — the least fixpoint, where
/// an empty list or map is always finite — and one that has none becomes
/// `NoJson`, which can force others in turn.
///
/// Only the parts of a value JSON carries are looked at: the fields a naming
/// shows, and every union member. A field JSON leaves out only needs an empty
/// value to hold.
#[derive(Clone, Debug)]
pub struct JsonInhabitance<H: Head> {
    naming: JsonNaming,
    /// Every definition the roots reach, in the order they reach them.
    ids: Vec<DefId<H>>,
    index: FxHashMap<DefId<H>, usize>,
    bodies: Vec<DefBody<H>>,
    status: Vec<JsonStatus>,
    /// Whether each definition has a value; final before `status` is.
    inhabited: Vec<bool>,
    variants: FxHashMap<H, Vec<JsonVariant>>,
    opaque: FxHashMap<RuntimeTy<H>, JsonStatus>,
}

impl<H: Head> JsonInhabitance<H> {
    /// The analysis of everything `root` reaches.
    ///
    /// # Errors
    ///
    /// The first error `defs` reports on the way, with the fields that lead
    /// to it.
    pub fn of<D: JsonDefs<H>>(
        root: &RuntimeTy<H>,
        defs: &D,
        naming: JsonNaming,
    ) -> Result<Self, JsonError<D::Error>> {
        Self::of_all(std::iter::once(root), defs, naming)
    }

    /// The analysis of everything any of `roots` reaches.
    ///
    /// # Errors
    ///
    /// The first error `defs` reports on the way, with the fields that lead
    /// to it from the root it was met under.
    pub fn of_all<'r, D: JsonDefs<H>>(
        roots: impl IntoIterator<Item = &'r RuntimeTy<H>>,
        defs: &D,
        naming: JsonNaming,
    ) -> Result<Self, JsonError<D::Error>> {
        let mut analysis = JsonInhabitance {
            naming,
            ids: Vec::new(),
            index: FxHashMap::default(),
            bodies: Vec::new(),
            status: Vec::new(),
            inhabited: Vec::new(),
            variants: FxHashMap::default(),
            opaque: FxHashMap::default(),
        };
        for root in roots {
            analysis.discover(root, defs, &mut Vec::new())?;
        }
        analysis.solve();
        Ok(analysis)
    }

    /// The status of `ty`, a type the roots reach.
    ///
    /// A type the analysis has not seen is taken to be [`JsonStatus::Json`].
    pub fn status(&self, ty: &RuntimeTy<H>) -> JsonStatus {
        if !self.inhabited(ty) {
            return JsonStatus::Empty;
        }
        match self.node(ty) {
            Node::Leaf => JsonStatus::Json,
            Node::NoJson => JsonStatus::NoJson,
            Node::Never => JsonStatus::Empty,
            Node::Union(members) => {
                if members
                    .into_iter()
                    .any(|member| self.status(member) == JsonStatus::Json)
                {
                    JsonStatus::Json
                } else {
                    JsonStatus::NoJson
                }
            }
            Node::Container(inner) => match self.status(inner) {
                JsonStatus::Json | JsonStatus::Empty => JsonStatus::Json,
                JsonStatus::NoJson => JsonStatus::NoJson,
            },
            Node::Def(id) => self.def_status(&id),
        }
    }

    /// Why `ty`, a [`JsonStatus::NoJson`] or [`JsonStatus::Empty`] type, has
    /// no value with a JSON form: the fields that lead to the cause, and the
    /// cause. A union leads through its first member of the same status.
    pub fn witness<'a>(&'a self, ty: &'a RuntimeTy<H>) -> Witness<'a, H> {
        let mut path = Vec::new();
        let cause = self.witness_walk(ty, &mut path, &mut FxHashSet::default());
        Witness { path, cause }
    }

    fn witness_walk<'a>(
        &'a self,
        ty: &'a RuntimeTy<H>,
        path: &mut Vec<(&'a H, &'a str)>,
        seen: &mut FxHashSet<usize>,
    ) -> WitnessCause<'a, H> {
        let status = self.status(ty);
        match self.node(ty) {
            Node::Leaf | Node::NoJson | Node::Never => WitnessCause::Leaf(ty),
            Node::Union(members) => {
                match members
                    .into_iter()
                    .find(|member| self.status(member) == status)
                {
                    Some(member) => self.witness_walk(member, path, seen),
                    None => WitnessCause::Leaf(ty),
                }
            }
            Node::Container(inner) => self.witness_walk(inner, path, seen),
            Node::Def(id) => {
                let Some(&at) = self.index.get(&id) else {
                    return WitnessCause::Leaf(ty);
                };
                let head = self.ids[at].head();
                if !seen.insert(at) {
                    return WitnessCause::Cyclic {
                        head,
                        revisited: true,
                    };
                }
                match &self.bodies[at] {
                    DefBody::Class(body) => {
                        if status != JsonStatus::Empty
                            && let Some((field, field_ty)) = &body.unfillable
                        {
                            return WitnessCause::Unfillable {
                                class: head,
                                field,
                                ty: field_ty,
                            };
                        }
                        let culprit = body.fields.iter().find(|(_, field_ty)| {
                            if status == JsonStatus::Empty {
                                !self.inhabited(field_ty)
                            } else {
                                self.status(field_ty) == JsonStatus::NoJson
                            }
                        });
                        match culprit {
                            Some((field, field_ty)) => {
                                path.push((head, field));
                                self.witness_walk(field_ty, path, seen)
                            }
                            None => WitnessCause::Cyclic {
                                head,
                                revisited: false,
                            },
                        }
                    }
                    DefBody::Alias(target) | DefBody::Shaped(target) => {
                        self.witness_walk(target, path, seen)
                    }
                }
            }
        }
    }

    /// Records every definition `ty` reaches, with its body, and asks `defs`
    /// about every type on the way. Fields the naming leaves out are not
    /// visited.
    fn discover<D: JsonDefs<H>>(
        &mut self,
        ty: &RuntimeTy<H>,
        defs: &D,
        path: &mut Vec<String>,
    ) -> Result<(), JsonError<D::Error>> {
        let located = |path: &Vec<String>| {
            let path = path.clone();
            move |error| JsonError { error, path }
        };
        defs.check(ty).map_err(located(path))?;
        match ty {
            RuntimeTy::Unknown
            | RuntimeTy::Interface(..)
            | RuntimeTy::TypeVar(..)
            | RuntimeTy::AssociatedTypeProjection { .. }
            | RuntimeTy::Media(_) => {
                if !self.opaque.contains_key(ty) {
                    let status = defs.opaque(ty).map_err(located(path))?;
                    self.opaque.insert(ty.clone(), status);
                }
                Ok(())
            }
            RuntimeTy::Enum(head) | RuntimeTy::EnumVariant(head, _) => {
                let id = DefId::Enum(head.clone());
                if self.index.contains_key(&id) || self.variants.contains_key(head) {
                    return Ok(());
                }
                // A variant type of a shaped enum stays a variant: only the
                // enum itself is written as its shape.
                let enum_ty = RuntimeTy::Enum(head.clone());
                if let Some(shape) = defs.shape(&enum_ty).map_err(located(path))? {
                    self.insert(id, DefBody::Shaped(shape.clone()));
                    return self.discover(&shape, defs, path);
                }
                let variants = defs.variants(head).map_err(located(path))?;
                self.variants.insert(head.clone(), variants);
                Ok(())
            }
            RuntimeTy::Union(members) => {
                let mut flat = Vec::new();
                flatten_union(members, &mut flat);
                flat.into_iter()
                    .try_for_each(|member| self.discover(member, defs, path))
            }
            RuntimeTy::List(inner) | RuntimeTy::Map { value: inner, .. } => {
                self.discover(inner, defs, path)
            }
            RuntimeTy::Class(head, args) => {
                let id = DefId::class(head, args);
                if self.index.contains_key(&id) {
                    return Ok(());
                }
                let class_ty = RuntimeTy::Class(head.clone(), id_args(&id));
                if let Some(shape) = defs.shape(&class_ty).map_err(located(path))? {
                    self.insert(id, DefBody::Shaped(shape.clone()));
                    return self.discover(&shape, defs, path);
                }
                let class = defs.class(head, &id_args(&id)).map_err(located(path))?;
                let mut fields = Vec::new();
                let mut unfillable = None;
                for field in class.fields {
                    if self.naming.shows(field.skip) {
                        fields.push((field.name, field.ty));
                    } else if unfillable.is_none() {
                        path.push(field.name.clone());
                        let has = has_empty_value(defs, &field.ty).map_err(located(path));
                        path.pop();
                        if !has? {
                            unfillable = Some((field.name, field.ty));
                        }
                    }
                }
                let at = self.insert(id, DefBody::Class(ClassBody { fields, unfillable }));
                let DefBody::Class(body) = &self.bodies[at] else {
                    unreachable!("a class was just inserted at {at}")
                };
                let fields = body.fields.clone();
                for (name, field_ty) in &fields {
                    path.push(name.clone());
                    self.discover(field_ty, defs, path)?;
                    path.pop();
                }
                Ok(())
            }
            RuntimeTy::TypeAlias(head) => {
                let id = DefId::Alias(head.clone());
                if self.index.contains_key(&id) {
                    return Ok(());
                }
                let target = defs.alias_target(head).map_err(located(path))?;
                self.insert(id, DefBody::Alias(target.clone()));
                self.discover(&target, defs, path)
            }
            RuntimeTy::Int
            | RuntimeTy::Bigint
            | RuntimeTy::Float
            | RuntimeTy::String
            | RuntimeTy::Bool
            | RuntimeTy::Null
            | RuntimeTy::Literal(..)
            | RuntimeTy::Uint8Array
            | RuntimeTy::Function { .. }
            | RuntimeTy::Future(..)
            | RuntimeTy::RustType
            | RuntimeTy::Type
            | RuntimeTy::Resource
            | RuntimeTy::PromptAst
            | RuntimeTy::Never => Ok(()),
        }
    }

    fn insert(&mut self, id: DefId<H>, body: DefBody<H>) -> usize {
        let at = self.ids.len();
        self.index.insert(id.clone(), at);
        self.ids.push(id);
        self.bodies.push(body);
        // Provisional; `solve` computes the real values.
        self.status.push(JsonStatus::Json);
        self.inhabited.push(true);
        at
    }

    fn solve(&mut self) {
        let count = self.ids.len();

        // Greatest fixpoint of "has a value": start from every definition
        // having one, and clear the ones that cannot.
        loop {
            let mut changed = false;
            for at in 0..count {
                if self.inhabited[at] && !self.body_inhabited(at) {
                    self.inhabited[at] = false;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for at in 0..count {
            self.status[at] = if self.inhabited[at] {
                JsonStatus::Json
            } else {
                JsonStatus::Empty
            };
        }

        loop {
            // Greatest fixpoint of `Json` among the inhabited definitions.
            loop {
                let mut changed = false;
                for at in 0..count {
                    if self.status[at] == JsonStatus::Json
                        && self.body_status(at) == JsonStatus::NoJson
                    {
                        self.status[at] = JsonStatus::NoJson;
                        changed = true;
                    }
                }
                if !changed {
                    break;
                }
            }
            // Least fixpoint of "has a finite value" among the `Json`
            // definitions. One with none has only cyclic values.
            let mut finite = vec![false; count];
            loop {
                let mut changed = false;
                for at in 0..count {
                    if self.status[at] == JsonStatus::Json
                        && !finite[at]
                        && self.body_finite(at, &finite)
                    {
                        finite[at] = true;
                        changed = true;
                    }
                }
                if !changed {
                    break;
                }
            }
            let cyclic: Vec<usize> = (0..count)
                .filter(|&at| self.status[at] == JsonStatus::Json && !finite[at])
                .collect();
            if cyclic.is_empty() {
                break;
            }
            for at in cyclic {
                self.status[at] = JsonStatus::NoJson;
            }
        }
    }

    fn node<'t>(&self, ty: &'t RuntimeTy<H>) -> Node<'t, H> {
        match ty {
            RuntimeTy::Int
            | RuntimeTy::Bigint
            | RuntimeTy::Float
            | RuntimeTy::String
            | RuntimeTy::Bool
            | RuntimeTy::Null
            | RuntimeTy::Literal(..) => Node::Leaf,
            RuntimeTy::Enum(head) if self.index.contains_key(&DefId::Enum(head.clone())) => {
                Node::Def(DefId::Enum(head.clone()))
            }
            RuntimeTy::Enum(head) => match self.variants.get(head) {
                Some(variants) if variants.is_empty() => Node::Never,
                // Every variant `@skip` under aliased names: values exist,
                // but JSON names none of them.
                Some(variants)
                    if !variants
                        .iter()
                        .any(|variant| self.naming.shows(variant.skip)) =>
                {
                    Node::NoJson
                }
                _ => Node::Leaf,
            },
            RuntimeTy::EnumVariant(head, name) => {
                let shown = self.variants.get(head).is_none_or(|variants| {
                    variants.iter().any(|variant| {
                        variant.name == name.as_str() && self.naming.shows(variant.skip)
                    })
                });
                if shown { Node::Leaf } else { Node::NoJson }
            }
            RuntimeTy::Never => Node::Never,
            RuntimeTy::Uint8Array
            | RuntimeTy::Function { .. }
            | RuntimeTy::Future(..)
            | RuntimeTy::RustType
            | RuntimeTy::Type
            | RuntimeTy::Resource
            | RuntimeTy::PromptAst => Node::NoJson,
            RuntimeTy::Unknown
            | RuntimeTy::Interface(..)
            | RuntimeTy::TypeVar(..)
            | RuntimeTy::AssociatedTypeProjection { .. }
            | RuntimeTy::Media(_) => match self.opaque.get(ty) {
                Some(JsonStatus::NoJson) => Node::NoJson,
                Some(JsonStatus::Empty) => Node::Never,
                Some(JsonStatus::Json) | None => Node::Leaf,
            },
            RuntimeTy::Union(members) => {
                let mut flat = Vec::new();
                flatten_union(members, &mut flat);
                Node::Union(flat)
            }
            RuntimeTy::List(inner) | RuntimeTy::Map { value: inner, .. } => Node::Container(inner),
            RuntimeTy::Class(head, args) => Node::Def(DefId::class(head, args)),
            RuntimeTy::TypeAlias(head) => Node::Def(DefId::Alias(head.clone())),
        }
    }

    fn def_status(&self, id: &DefId<H>) -> JsonStatus {
        self.index
            .get(id)
            .map_or(JsonStatus::Json, |&at| self.status[at])
    }

    /// Whether `ty` has any value at all, JSON or not.
    fn inhabited(&self, ty: &RuntimeTy<H>) -> bool {
        match self.node(ty) {
            Node::Leaf | Node::NoJson | Node::Container(_) => true,
            Node::Never => false,
            Node::Union(members) => members.into_iter().any(|member| self.inhabited(member)),
            Node::Def(id) => self.index.get(&id).is_none_or(|&at| self.inhabited[at]),
        }
    }

    fn body_inhabited(&self, at: usize) -> bool {
        match &self.bodies[at] {
            DefBody::Class(body) => body
                .fields
                .iter()
                .all(|(_, field_ty)| self.inhabited(field_ty)),
            DefBody::Alias(target) | DefBody::Shaped(target) => self.inhabited(target),
        }
    }

    fn body_status(&self, at: usize) -> JsonStatus {
        match &self.bodies[at] {
            DefBody::Class(body) => {
                if body.unfillable.is_some() {
                    return JsonStatus::NoJson;
                }
                for (_, field_ty) in &body.fields {
                    let status = self.status(field_ty);
                    debug_assert!(
                        status != JsonStatus::Empty,
                        "an inhabited class has no empty field"
                    );
                    if status != JsonStatus::Json {
                        return JsonStatus::NoJson;
                    }
                }
                JsonStatus::Json
            }
            DefBody::Alias(target) | DefBody::Shaped(target) => self.status(target),
        }
    }

    /// Whether `ty`, a `Json` type, has a finite JSON value, given the
    /// definitions known to have one.
    fn finite(&self, ty: &RuntimeTy<H>, finite: &[bool]) -> bool {
        match self.node(ty) {
            Node::Leaf | Node::Container(_) => true,
            Node::NoJson | Node::Never => false,
            Node::Union(members) => members.into_iter().any(|member| {
                self.status(member) == JsonStatus::Json && self.finite(member, finite)
            }),
            Node::Def(id) => self.index.get(&id).is_none_or(|&at| finite[at]),
        }
    }

    fn body_finite(&self, at: usize, finite: &[bool]) -> bool {
        match &self.bodies[at] {
            DefBody::Class(body) => {
                body.unfillable.is_none()
                    && body
                        .fields
                        .iter()
                        .all(|(_, field_ty)| self.finite(field_ty, finite))
            }
            DefBody::Alias(target) | DefBody::Shaped(target) => self.finite(target, finite),
        }
    }
}

/// Why a type has no value with a JSON form ([`JsonInhabitance::witness`]).
#[derive(Debug)]
pub struct Witness<'a, H: Head> {
    /// The fields from the type down to the cause: each field's class, and the
    /// field's declared name.
    pub path: Vec<(&'a H, &'a str)>,
    pub cause: WitnessCause<'a, H>,
}

/// What a [`Witness`] leads to.
#[derive(Debug)]
pub enum WitnessCause<'a, H: Head> {
    /// A type with no JSON form at all, such as `uint8array`, or with no
    /// values, such as `never`.
    Leaf(&'a RuntimeTy<H>),
    /// `field` of `class`, which JSON leaves out (`@skip`, under aliased
    /// names), has type `ty`, which has no empty value for it to hold.
    Unfillable {
        class: &'a H,
        field: &'a str,
        ty: &'a RuntimeTy<H>,
    },
    /// Every value of the class or alias `head` is cyclic. `revisited` when
    /// the path met `head` again, rather than ending at a body that names no
    /// other cause.
    Cyclic { head: &'a H, revisited: bool },
}

impl<H: Head + HeadDisplay> Witness<'_, H> {
    /// The witness in words, as `baml.json.schema` reports it: one `field
    /// `Class.name`: ` per step, then the cause.
    pub fn reason(&self) -> String {
        let mut out = String::new();
        for (class, field) in &self.path {
            out.push_str(&format!("field `{}.{field}`: ", class.head_display_name()));
        }
        match &self.cause {
            WitnessCause::Leaf(ty) => out.push_str(&format!("`{ty}` has no JSON form")),
            WitnessCause::Unfillable { class, field, .. } => out.push_str(&format!(
                "field `{}.{field}` is `@skip`, so JSON leaves it out, and its type has no \
                 empty value to hold",
                class.head_display_name()
            )),
            WitnessCause::Cyclic { head, revisited } => {
                out.push_str(&format!(
                    "every value of `{}` is cyclic",
                    head.head_display_name()
                ));
                if !revisited {
                    out.push_str(", and JSON holds only finite values");
                }
            }
        }
        out
    }
}

/// Whether `ty` has an empty value: the value a `@skip` field holds when JSON
/// is read under aliased names. The VM builds that value (`bex_vm`'s
/// `sap::empty_value`); this is the same rule as a predicate: `null` for a
/// nullable type, zeroes, `false`, `""`, empty containers, a literal's own
/// value, an enum's first variant that is not `@skip`, and a class whose
/// fields all have one. A class or alias that needs its own empty value has
/// none.
///
/// # Errors
///
/// A definition `defs` cannot look up.
pub fn has_empty_value<H: Head, D: JsonDefs<H>>(
    defs: &D,
    ty: &RuntimeTy<H>,
) -> Result<bool, D::Error> {
    has_empty_value_within(defs, ty, &mut Vec::new())
}

fn has_empty_value_within<H: Head, D: JsonDefs<H>>(
    defs: &D,
    ty: &RuntimeTy<H>,
    within: &mut Vec<DefId<H>>,
) -> Result<bool, D::Error> {
    Ok(match ty {
        RuntimeTy::Null
        | RuntimeTy::Unknown
        | RuntimeTy::Int
        | RuntimeTy::Bigint
        | RuntimeTy::Float
        | RuntimeTy::Bool
        | RuntimeTy::String
        | RuntimeTy::Uint8Array
        | RuntimeTy::Literal(..)
        | RuntimeTy::List(_)
        | RuntimeTy::Map { .. } => true,
        RuntimeTy::Enum(head) => defs.variants(head)?.iter().any(|variant| !variant.skip),
        RuntimeTy::EnumVariant(head, name) => defs
            .variants(head)?
            .iter()
            .any(|variant| variant.name == name.as_str()),
        RuntimeTy::Union(members) => {
            if members.iter().any(RuntimeTy::is_null) {
                return Ok(true);
            }
            for member in members {
                if has_empty_value_within(defs, member, within)? {
                    return Ok(true);
                }
            }
            false
        }
        RuntimeTy::Class(head, args) => {
            let id = DefId::class(head, args);
            if within.contains(&id) {
                return Ok(false);
            }
            let class = defs.class(head, &id_args(&id))?;
            within.push(id);
            let mut all = true;
            for field in &class.fields {
                if !has_empty_value_within(defs, &field.ty, within)? {
                    all = false;
                    break;
                }
            }
            within.pop();
            all
        }
        RuntimeTy::TypeAlias(head) => {
            let id = DefId::Alias(head.clone());
            if within.contains(&id) {
                return Ok(false);
            }
            let target = defs.alias_target(head)?;
            within.push(id);
            let has = has_empty_value_within(defs, &target, within)?;
            within.pop();
            has
        }
        RuntimeTy::Media(_)
        | RuntimeTy::Function { .. }
        | RuntimeTy::Future(..)
        | RuntimeTy::Interface(..)
        | RuntimeTy::RustType
        | RuntimeTy::Type
        | RuntimeTy::Resource
        | RuntimeTy::PromptAst
        | RuntimeTy::Never
        | RuntimeTy::TypeVar(..)
        | RuntimeTy::AssociatedTypeProjection { .. } => false,
    })
}

fn id_args<H: Head>(id: &DefId<H>) -> Box<[RuntimeTy<H>]> {
    match id {
        DefId::Class(_, args) => args.clone(),
        DefId::Alias(_) | DefId::Enum(_) => Box::new([]),
    }
}

/// The members of a union, nested unions spliced in.
pub fn flatten_union<'t, H: Head>(members: &'t [RuntimeTy<H>], out: &mut Vec<&'t RuntimeTy<H>>) {
    for member in members {
        match member {
            RuntimeTy::Union(inner) => flatten_union(inner, out),
            _ => out.push(member),
        }
    }
}

#[cfg(test)]
mod tests {
    use rustc_hash::FxHashMap;

    use super::*;
    use crate::{Name, TypeName};

    /// Definitions written out by hand: classes by name, each with its fields
    /// as `(name, skip, type)`, and enums with their variants.
    #[derive(Default)]
    struct Defs {
        classes: FxHashMap<TypeName, Vec<(&'static str, bool, RuntimeTy)>>,
        enums: FxHashMap<TypeName, Vec<(&'static str, bool)>>,
    }

    impl Defs {
        fn class(mut self, name: &str, fields: Vec<(&'static str, bool, RuntimeTy)>) -> Self {
            self.classes.insert(head(name), fields);
            self
        }

        fn enumeration(mut self, name: &str, variants: Vec<(&'static str, bool)>) -> Self {
            self.enums.insert(head(name), variants);
            self
        }
    }

    impl JsonDefs<TypeName> for Defs {
        type Error = String;

        fn class(&self, head: &TypeName, _: &[RuntimeTy]) -> Result<JsonClass<TypeName>, String> {
            let fields = self.classes.get(head).ok_or("unknown class")?;
            Ok(JsonClass {
                fields: fields
                    .iter()
                    .map(|(name, skip, ty)| JsonField {
                        name: (*name).to_string(),
                        alias: None,
                        skip: *skip,
                        ty: ty.clone(),
                    })
                    .collect(),
            })
        }

        fn variants(&self, head: &TypeName) -> Result<Vec<JsonVariant>, String> {
            let variants = self.enums.get(head).ok_or("unknown enum")?;
            Ok(variants
                .iter()
                .map(|(name, skip)| JsonVariant {
                    name: (*name).to_string(),
                    skip: *skip,
                })
                .collect())
        }

        fn alias_target(&self, _: &TypeName) -> Result<RuntimeTy, String> {
            Err("no aliases here".to_string())
        }

        fn opaque(&self, ty: &RuntimeTy) -> Result<JsonStatus, String> {
            match ty {
                RuntimeTy::Media(_) => Ok(JsonStatus::Json),
                _ => Err(format!("`{ty}` has no schema")),
            }
        }
    }

    fn head(name: &str) -> TypeName {
        TypeName::local(Name::new(name))
    }

    fn class(name: &str) -> RuntimeTy {
        RuntimeTy::Class(head(name), Box::new([]))
    }

    fn status(defs: &Defs, ty: &RuntimeTy, naming: JsonNaming) -> JsonStatus {
        JsonInhabitance::of(ty, defs, naming)
            .unwrap_or_else(|e| panic!("analysis failed: {}", e.error))
            .status(ty)
    }

    fn reason(defs: &Defs, ty: &RuntimeTy) -> String {
        let analysis = JsonInhabitance::of(ty, defs, JsonNaming::Aliased)
            .unwrap_or_else(|e| panic!("analysis failed: {}", e.error));
        analysis.witness(ty).reason()
    }

    #[test]
    fn leaves_take_their_own_status() {
        let defs = Defs::default();
        let cases = [
            (RuntimeTy::string(), JsonStatus::Json),
            (RuntimeTy::uint8array(), JsonStatus::NoJson),
            (RuntimeTy::Never, JsonStatus::Empty),
            (RuntimeTy::Media(crate::MediaKind::Image), JsonStatus::Json),
        ];
        for (ty, expected) in cases {
            assert_eq!(status(&defs, &ty, JsonNaming::Aliased), expected, "{ty}");
        }
    }

    #[test]
    fn a_union_drops_what_has_no_json_form() {
        let defs = Defs::default().class("Blob", vec![("bytes", false, RuntimeTy::uint8array())]);
        let bytes_or_text = RuntimeTy::union([RuntimeTy::uint8array(), RuntimeTy::string()]);
        let blob_or_text = RuntimeTy::union([class("Blob"), RuntimeTy::string()]);
        let never_or_bytes = RuntimeTy::union([RuntimeTy::Never, RuntimeTy::uint8array()]);
        for (ty, expected) in [
            (bytes_or_text, JsonStatus::Json),
            (blob_or_text, JsonStatus::Json),
            (
                RuntimeTy::optional(RuntimeTy::uint8array()),
                JsonStatus::Json,
            ),
            (never_or_bytes, JsonStatus::NoJson),
            (class("Blob"), JsonStatus::NoJson),
        ] {
            assert_eq!(status(&defs, &ty, JsonNaming::Aliased), expected, "{ty}");
        }
    }

    #[test]
    fn a_container_of_nothing_is_only_the_empty_one() {
        let defs = Defs::default().class("Holds", vec![("nothing", false, RuntimeTy::Never)]);
        for (ty, expected) in [
            (RuntimeTy::list(RuntimeTy::Never), JsonStatus::Json),
            (
                RuntimeTy::map(RuntimeTy::string(), RuntimeTy::Never),
                JsonStatus::Json,
            ),
            (RuntimeTy::list(class("Holds")), JsonStatus::Json),
            (class("Holds"), JsonStatus::Empty),
            (RuntimeTy::list(RuntimeTy::uint8array()), JsonStatus::NoJson),
        ] {
            assert_eq!(status(&defs, &ty, JsonNaming::Aliased), expected, "{ty}");
        }
    }

    #[test]
    fn a_class_needs_a_finite_value() {
        let defs = Defs::default()
            .class("Loop", vec![("next", false, class("Loop"))])
            .class(
                "Chain",
                vec![("next", false, RuntimeTy::optional(class("Chain")))],
            );
        assert_eq!(
            status(&defs, &class("Loop"), JsonNaming::Aliased),
            JsonStatus::NoJson
        );
        assert_eq!(
            status(&defs, &class("Chain"), JsonNaming::Aliased),
            JsonStatus::Json
        );
        assert_eq!(
            reason(&defs, &class("Loop")),
            "field `Loop.next`: every value of `Loop` is cyclic"
        );
    }

    #[test]
    fn a_skipped_field_needs_an_empty_value_only_when_json_leaves_it_out() {
        let defs = Defs::default()
            .class(
                "Tagged",
                vec![
                    ("name", false, RuntimeTy::string()),
                    ("kind", true, RuntimeTy::type_type()),
                ],
            )
            .class(
                "Fillable",
                vec![
                    ("name", false, RuntimeTy::string()),
                    ("raw", true, RuntimeTy::uint8array()),
                ],
            );
        assert_eq!(
            status(&defs, &class("Tagged"), JsonNaming::Aliased),
            JsonStatus::NoJson
        );
        assert_eq!(
            reason(&defs, &class("Tagged")),
            "field `Tagged.kind` is `@skip`, so JSON leaves it out, and its type has no empty \
             value to hold"
        );
        assert_eq!(
            status(&defs, &class("Tagged"), JsonNaming::Declared),
            JsonStatus::NoJson
        );
        assert_eq!(
            status(&defs, &class("Fillable"), JsonNaming::Aliased),
            JsonStatus::Json
        );
        assert_eq!(
            status(&defs, &class("Fillable"), JsonNaming::Declared),
            JsonStatus::NoJson
        );
    }

    #[test]
    fn an_enum_needs_a_variant_the_naming_shows() {
        let defs = Defs::default()
            .enumeration("None", vec![])
            .enumeration("Hidden", vec![("A", true)])
            .enumeration("Shown", vec![("A", true), ("B", false)]);
        let enumeration = |name: &str| RuntimeTy::Enum(head(name));
        assert_eq!(
            status(&defs, &enumeration("None"), JsonNaming::Aliased),
            JsonStatus::Empty
        );
        assert_eq!(
            status(&defs, &enumeration("Hidden"), JsonNaming::Aliased),
            JsonStatus::NoJson
        );
        assert_eq!(
            status(&defs, &enumeration("Hidden"), JsonNaming::Declared),
            JsonStatus::Json
        );
        assert_eq!(
            status(&defs, &enumeration("Shown"), JsonNaming::Aliased),
            JsonStatus::Json
        );
    }

    #[test]
    fn a_reader_error_carries_the_fields_that_lead_to_it() {
        let defs = Defs::default()
            .class("Outer", vec![("inner", false, class("Inner"))])
            .class(
                "Inner",
                vec![
                    ("bytes", false, RuntimeTy::uint8array()),
                    ("value", false, RuntimeTy::unknown()),
                ],
            );
        let ty = RuntimeTy::union([class("Outer"), RuntimeTy::string()]);
        let error = JsonInhabitance::of(&ty, &defs, JsonNaming::Aliased)
            .expect_err("`unknown` has no schema here");
        assert_eq!(error.error, "`unknown` has no schema");
        assert_eq!(error.path, ["inner", "value"]);
    }

    #[test]
    fn empty_values_follow_the_rule_the_vm_builds_them_by() {
        let defs = Defs::default()
            .class("Fills", vec![("n", false, RuntimeTy::int())])
            .class("Recurs", vec![("me", false, class("Recurs"))]);
        for (ty, expected) in [
            (RuntimeTy::optional(RuntimeTy::type_type()), true),
            (RuntimeTy::list(RuntimeTy::Never), true),
            (RuntimeTy::type_type(), false),
            (RuntimeTy::Never, false),
            (class("Fills"), true),
            (class("Recurs"), false),
        ] {
            assert_eq!(has_empty_value(&defs, &ty), Ok(expected), "{ty}");
        }
    }
}
