//! Tests of the walk over a mock host with Python's rules and a host that
//! records every effect: what ships, what is stated, what is refused and
//! why, and that nothing is created for a value that does not ship.

use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
};

use baml_sdkgen_types::{
    ClassDescriptor, EnumDescriptor, FieldDescriptor, FunctionDescriptor, ParamDescriptor,
    ProgramDescriptor, Ty, TypeAliasDescriptor,
};
use baml_type::{Freshness, FunctionParamMode, Int63, Literal, MediaKind, TypeName};
use bridge_ctypes::{
    HostKey, TableKey,
    baml_bridge::cffi::{
        BamlHandle, BamlTyDef, BamlValueMedia, BamlValuePromptAst, baml_ty::Ty as WireTy,
        inbound_value::Value,
    },
    proto_ty_to_runtime_ty,
};
use num_bigint::BigInt;

use super::*;

/// A host value with Python's rules: `bool` is not `int`, an `int` fits a
/// `float` by exact conversion and a `bigint` by widening, a string names a
/// variant, a dict stands in for a class.
#[derive(Debug, Clone)]
enum Mock {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(&'static str),
    Bytes(Vec<u8>),
    List(Vec<Mock>),
    Map(Vec<(Mock, Mock)>),
    Instance {
        class: &'static str,
        args: Option<Vec<RuntimeTy>>,
        fields: Vec<(&'static str, Mock)>,
    },
    Member(&'static str, &'static str),
    Callable,
    Handle,
    Media(MediaKind),
    /// A value whose `shape` raises.
    Raising,
}

thread_local! {
    static DESCRIBES: Cell<usize> = const { Cell::new(0) };
}

fn describes() -> usize {
    DESCRIBES.with(Cell::get)
}

fn tn(name: &str) -> TypeName {
    TypeName::new(Name::new("user"), Vec::new(), Name::new(name))
}

impl HostValue for Mock {
    type Error = String;

    const RULES: Rules = Rules::Python;

    fn shape(&self) -> Result<Shape<Self>, String> {
        Ok(match self {
            Mock::Null => Shape::Null,
            Mock::Int(int) => Shape::Scalar(match Int63::new(*int) {
                Some(int) => Scalar::Int(int),
                None => Scalar::BigInt(BigInt::from(*int)),
            }),
            Mock::Float(float) => Shape::Scalar(Scalar::Float(*float)),
            Mock::Bool(bool) => Shape::Scalar(Scalar::Bool(*bool)),
            Mock::Str(_) => Shape::Scalar(Scalar::String),
            Mock::Bytes(_) => Shape::Scalar(Scalar::Bytes),
            Mock::List(items) => Shape::List(items.clone()),
            Mock::Map(entries) => Shape::Map(entries.clone()),
            Mock::Instance {
                class,
                args,
                fields,
            } => Shape::Instance {
                class: tn(class),
                type_args: args.clone(),
                fields: fields
                    .iter()
                    .map(|(name, value)| ((*name).to_string(), value.clone()))
                    .collect(),
            },
            Mock::Member(enumeration, variant) => Shape::EnumMember {
                enumeration: tn(enumeration),
                variant: (*variant).to_string(),
            },
            Mock::Callable => Shape::Callable,
            Mock::Handle => Shape::Leaf(LeafKind::Handle),
            Mock::Media(kind) => Shape::Leaf(LeafKind::Media(*kind)),
            Mock::Raising => return Err("the host raised".to_string()),
        })
    }

    fn text(&self) -> Result<Cow<'_, str>, String> {
        match self {
            Mock::Str(text) => Ok(Cow::Borrowed(text)),
            other => Err(format!("{other:?} has no text")),
        }
    }

    fn bytes(&self) -> Result<Cow<'_, [u8]>, String> {
        match self {
            Mock::Bytes(bytes) => Ok(Cow::Borrowed(bytes)),
            other => Err(format!("{other:?} has no bytes")),
        }
    }

    fn describe(&self) -> String {
        DESCRIBES.with(|describes| describes.set(describes.get() + 1));
        format!("{self:?}")
    }
}

fn host_key(key: u64) -> HostKey {
    HostKey::new(key).expect("test keys are nonzero")
}

fn table_key(key: u64) -> TableKey {
    TableKey::new(key).expect("test keys are nonzero")
}

/// A host that records every effect and can fail a registration on demand.
#[derive(Default)]
struct Recording {
    registered: RefCell<Vec<(HostKey, RuntimeTy)>>,
    leaves: Cell<usize>,
    released: RefCell<Vec<Owner>>,
    /// Fail the n-th registration (1-based).
    fail_register_at: Cell<Option<usize>>,
    next_key: Cell<u64>,
}

impl Host<Mock> for Recording {
    fn register(&self, _callable: &Mock, signature: &RuntimeTy) -> Result<HostKey, String> {
        let nth = self.registered.borrow().len() + 1;
        if self.fail_register_at.get() == Some(nth) {
            return Err("registration failed".to_string());
        }
        let key = host_key(100 + self.next_key.get());
        self.next_key.set(self.next_key.get() + 1);
        self.registered.borrow_mut().push((key, signature.clone()));
        Ok(key)
    }

    fn leaf(&self, _leaf: &Mock, kind: LeafKind) -> Result<(LeafWire, Option<Owner>), String> {
        self.leaves.set(self.leaves.get() + 1);
        Ok(match kind {
            LeafKind::Handle => (
                LeafWire::Handle(BamlHandle {
                    key: 9,
                    ..Default::default()
                }),
                Some(Owner::Handle(table_key(9))),
            ),
            LeafKind::Media(_) => (LeafWire::Media(BamlValueMedia::default()), None),
            LeafKind::Prompt => (LeafWire::Prompt(BamlValuePromptAst::default()), None),
            LeafKind::TypeValue => (LeafWire::TypeValue(BamlTyDef::default()), None),
        })
    }

    fn release(&self, owner: Owner) {
        self.released.borrow_mut().push(owner);
    }
}

// ── The program the tests encode against ────────────────────────────────────

fn class(name: &str, args: Vec<Ty>) -> Ty {
    Ty::Class(tn(name), args.into())
}

fn var(index: u32, name: &str) -> Ty {
    Ty::TypeVar(ParamTy::new(index, Name::new(name)))
}

fn union(members: Vec<Ty>) -> Ty {
    Ty::Union(members.into())
}

fn function(params: Vec<Ty>, ret: Ty, throws: Ty) -> Ty {
    Ty::Function {
        params: params
            .into_iter()
            .map(|ty| baml_type::CodegenFunctionParamTy {
                name: None,
                ty,
                mode: FunctionParamMode::Required,
            })
            .collect(),
        ret: Box::new(ret),
        throws: Box::new(throws),
    }
}

fn param(name: &str, ty: Ty) -> ParamDescriptor {
    ParamDescriptor {
        name: Name::new(name),
        ty,
        has_default: false,
    }
}

fn field(name: &str, ty: Ty) -> FieldDescriptor {
    FieldDescriptor {
        name: Name::new(name),
        ty,
    }
}

fn descriptor() -> ProgramDescriptor {
    let int_fn = || function(vec![], Ty::Int, Ty::Never);
    let string_fn = || function(vec![], Ty::String, Ty::Never);
    ProgramDescriptor {
        functions: vec![FunctionDescriptor {
            fqn: "user.f".into(),
            class_generic_params: vec![],
            generic_params: vec![Name::new("T")],
            params: vec![
                param("a", Ty::Int),
                param("b", Ty::Float),
                param("c", Ty::Enum(tn("Mood"))),
                param("d", class("Box", vec![var(0, "T")])),
                param("e", union(vec![Ty::Int, Ty::String])),
                param("g", Ty::Unknown),
                param("h", function(vec![Ty::Int], Ty::String, Ty::Never)),
                param(
                    "i",
                    union(vec![
                        Ty::Literal(Literal::Int(1), Freshness::Regular),
                        Ty::Literal(Literal::Int(2), Freshness::Regular),
                    ]),
                ),
                param(
                    "j",
                    Ty::Map {
                        key: Box::new(Ty::String),
                        value: Box::new(Ty::Int),
                    },
                ),
                param(
                    "k",
                    union(vec![
                        Ty::List(Box::new(Ty::Int)),
                        Ty::List(Box::new(Ty::String)),
                    ]),
                ),
                param("l", union(vec![class("A", vec![]), class("B", vec![])])),
                param("m", Ty::TypeAlias(tn("Tree"))),
                param("n", union(vec![Ty::Int, Ty::Float])),
                param("o", union(vec![Ty::Enum(tn("Mood")), Ty::String])),
                param("p", Ty::List(Box::new(var(0, "T")))),
                param("q", Ty::Media(MediaKind::Image)),
                param("r", union(vec![Ty::Media(MediaKind::Pdf), Ty::Null])),
                param("s", Ty::List(Box::new(Ty::Int))),
                param("t", Ty::List(Box::new(union(vec![Ty::Int, Ty::String])))),
                param("u", union(vec![Ty::Int, int_fn()])),
                param("v", union(vec![int_fn(), string_fn()])),
                param("w", Ty::TypeAlias(tn("Loop"))),
                param("x", Ty::TypeAlias(tn("Tree2"))),
                param("y", Ty::List(Box::new(union(vec![var(0, "T"), Ty::Null])))),
                param("tf", union(vec![var(0, "T"), Ty::Float])),
                param("uf", union(vec![Ty::Unknown, Ty::Float])),
                param(
                    "z",
                    union(vec![
                        function(vec![var(0, "T")], Ty::Bool, Ty::Never),
                        Ty::Null,
                    ]),
                ),
            ],
            ret: Ty::Null,
            throws: Ty::Never,
        }],
        classes: vec![
            ClassDescriptor {
                name: tn("Box"),
                generic_params: vec![Name::new("T")],
                fields: vec![field("item", var(0, "T"))],
            },
            ClassDescriptor {
                name: tn("A"),
                generic_params: vec![],
                fields: vec![field("x", Ty::Int)],
            },
            ClassDescriptor {
                name: tn("B"),
                generic_params: vec![],
                fields: vec![
                    field("x", Ty::Int),
                    field("y", union(vec![Ty::Int, Ty::Null])),
                ],
            },
            ClassDescriptor {
                name: tn("OptList"),
                generic_params: vec![Name::new("T")],
                fields: vec![field(
                    "xs",
                    Ty::List(Box::new(union(vec![var(0, "T"), Ty::Null]))),
                )],
            },
            ClassDescriptor {
                name: tn("P2"),
                generic_params: vec![Name::new("T")],
                fields: vec![
                    field("a", Ty::List(Box::new(var(0, "T")))),
                    field("b", var(0, "T")),
                ],
            },
            ClassDescriptor {
                name: tn("Pair"),
                generic_params: vec![Name::new("A"), Name::new("B")],
                fields: vec![field("a", var(0, "A")), field("b", var(1, "B"))],
            },
            ClassDescriptor {
                name: tn("Holder"),
                generic_params: vec![],
                fields: vec![field("f", int_fn())],
            },
            ClassDescriptor {
                name: tn("Node"),
                generic_params: vec![],
                fields: vec![field("c", Ty::TypeAlias(tn("Tree2")))],
            },
        ],
        enums: vec![EnumDescriptor {
            name: tn("Mood"),
            variants: vec![Name::new("Happy"), Name::new("Sad")],
        }],
        type_aliases: vec![
            TypeAliasDescriptor {
                name: tn("Tree"),
                resolves_to: union(vec![Ty::Int, Ty::List(Box::new(Ty::TypeAlias(tn("Tree"))))]),
                recursive: true,
            },
            // Legal (the productivity rule): the unguarded member is vacuous.
            TypeAliasDescriptor {
                name: tn("Loop"),
                resolves_to: union(vec![
                    Ty::TypeAlias(tn("Loop")),
                    Ty::List(Box::new(Ty::TypeAlias(tn("Loop")))),
                ]),
                recursive: true,
            },
            TypeAliasDescriptor {
                name: tn("IntOrStr"),
                resolves_to: union(vec![Ty::Int, Ty::String]),
                recursive: false,
            },
            TypeAliasDescriptor {
                name: tn("Tree2"),
                resolves_to: union(vec![
                    class("Node", vec![]),
                    Ty::Map {
                        key: Box::new(Ty::String),
                        value: Box::new(Ty::TypeAlias(tn("Tree2"))),
                    },
                ]),
                recursive: true,
            },
        ],
    }
}

struct Fixture {
    facts: Facts,
    host: Recording,
}

impl Fixture {
    fn new() -> Self {
        DESCRIBES.with(|describes| describes.set(0));
        Self {
            facts: Facts::from_descriptor(&descriptor()).unwrap(),
            host: Recording::default(),
        }
    }

    fn slot(&self, name: &str) -> RuntimeTy {
        self.facts
            .function("user.f")
            .unwrap()
            .params
            .iter()
            .find(|p| p.name == name)
            .unwrap()
            .ty
            .clone()
    }

    /// Encode `value` under parameter `name`, keeping what ships.
    fn encode(&self, value: Mock, name: &str) -> Result<InboundValue, Error<String>> {
        self.encode_in(value, &self.slot(name))
    }

    fn encode_in(&self, value: Mock, slot: &RuntimeTy) -> Result<InboundValue, Error<String>> {
        let mut owners = Owners::new(&self.host);
        let result = encode(
            &self.facts,
            value,
            slot,
            Root::Argument("x".into()),
            &mut owners,
        );
        if result.is_ok() {
            owners.keep();
        }
        result
    }

    fn ok(&self, value: Mock, name: &str) -> InboundValue {
        self.encode(value, name).unwrap()
    }

    fn refusal(&self, value: Mock, name: &str) -> String {
        refusal(self.encode(value, name))
    }
}

#[track_caller]
fn refusal(result: Result<InboundValue, Error<String>>) -> String {
    match result {
        Err(Error::Value(error)) => error.to_string(),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// The type a node states, if it states one.
fn stated(value: &InboundValue) -> Option<RuntimeTy> {
    value
        .value_type
        .as_ref()
        .map(|ty| proto_ty_to_runtime_ty(ty).unwrap())
}

fn list_ty(inner: RuntimeTy) -> RuntimeTy {
    RuntimeTy::List(Box::new(inner))
}

/// `() -> ret throws never`.
fn fn_ty(ret: RuntimeTy) -> RuntimeTy {
    RuntimeTy::Function {
        params: Box::new([]),
        ret: Box::new(ret),
        throws: Box::new(RuntimeTy::Never),
    }
}

fn inst(class: &'static str, fields: Vec<(&'static str, Mock)>) -> Mock {
    Mock::Instance {
        class,
        args: None,
        fields,
    }
}

// ── Scalars, enums, literals ────────────────────────────────────────────────

#[test]
fn scalars_follow_the_slot_and_the_hosts_rules() {
    let f = Fixture::new();
    assert!(matches!(
        f.ok(Mock::Int(7), "a").value,
        Some(Value::IntValue(7))
    ));
    // Python's int fits a float by exact conversion.
    assert!(
        matches!(f.ok(Mock::Int(1), "b").value, Some(Value::FloatValue(v)) if v.to_bits() == 1.0f64.to_bits())
    );
    assert_eq!(
        f.refusal(Mock::Float(1.0), "a"),
        "argument `x`: expected int, found Float(1.0)"
    );
    assert_eq!(
        f.refusal(Mock::Bool(true), "a"),
        "argument `x`: expected int, found Bool(true)"
    );
    // Beyond the VM's integers the host classifies a bigint.
    assert!(matches!(
        f.encode_in(Mock::Int(1 << 62), &RuntimeTy::Bigint).unwrap().value,
        Some(Value::BigintValue(hex)) if hex == "4000000000000000"
    ));
    assert_eq!(
        stated(&f.ok(Mock::List(vec![Mock::Int(1 << 62)]), "g")),
        Some(list_ty(RuntimeTy::Bigint))
    );
    assert!(matches!(
        f.encode_in(Mock::Bytes(vec![1, 2]), &RuntimeTy::Uint8Array).unwrap().value,
        Some(Value::Uint8arrayValue(bytes)) if bytes == [1, 2]
    ));
}

#[test]
fn enums_take_members_as_is_and_strings_by_conversion() {
    let f = Fixture::new();
    for value in [Mock::Member("Mood", "Happy"), Mock::Str("Happy")] {
        let encoded = f.ok(value, "c");
        assert!(matches!(encoded.value, Some(Value::EnumValue(ref v)) if v.value == "Happy"));
        assert_eq!(stated(&encoded), None, "an enum variant implies its type");
    }
    assert_eq!(
        f.refusal(Mock::Str("Grumpy"), "c"),
        "argument `x`: enum user.Mood has no variant `Grumpy`"
    );
    assert_eq!(
        f.refusal(Mock::Member("Other", "Happy"), "c"),
        "argument `x`: expected Mood, found Member(\"Other\", \"Happy\")"
    );
    // A string is a string before it is a variant: identity beats conversion.
    assert!(matches!(
        f.ok(Mock::Str("Happy"), "o").value,
        Some(Value::StringValue(_))
    ));
    assert!(matches!(
        f.ok(Mock::Member("Mood", "Sad"), "o").value,
        Some(Value::EnumValue(_))
    ));
}

#[test]
fn literals_are_matched_exactly() {
    let f = Fixture::new();
    assert!(matches!(
        f.ok(Mock::Int(2), "i").value,
        Some(Value::IntValue(2))
    ));
    assert_eq!(
        f.refusal(Mock::Int(3), "i"),
        "argument `x`: expected one of 1 | 2, found Int(3)"
    );
    let lit = RuntimeTy::Literal(Literal::String("a".into()), Freshness::Regular);
    assert!(f.encode_in(Mock::Str("a"), &lit).is_ok());
    assert_eq!(
        refusal(f.encode_in(Mock::Str("b"), &lit)),
        "argument `x`: Str(\"b\") does not fit the literal type \"a\""
    );
}

// ── Unions and the tie-break ────────────────────────────────────────────────

#[test]
fn an_open_member_takes_the_value_as_it_is_before_a_conversion() {
    let f = Fixture::new();
    // `T | float` with `1`: the int goes as an int and binds `T`, as the
    // compiler types the literal; it is not converted to `float`.
    assert!(matches!(
        f.ok(Mock::Int(1), "tf").value,
        Some(Value::IntValue(1))
    ));
    assert!(matches!(
        f.ok(Mock::Float(1.5), "tf").value,
        Some(Value::FloatValue(_))
    ));
    // The same under `unknown | float`.
    assert!(matches!(
        f.ok(Mock::Int(1), "uf").value,
        Some(Value::IntValue(1))
    ));
    // A dict under `A | unknown` is the map it is, not the class it could be.
    let slot = RuntimeTy::Union(Box::new([
        RuntimeTy::Class(tn("A"), Box::new([])),
        RuntimeTy::Unknown,
    ]));
    let encoded = f
        .encode_in(Mock::Map(vec![(Mock::Str("x"), Mock::Int(1))]), &slot)
        .unwrap();
    assert_eq!(
        stated(&encoded),
        Some(RuntimeTy::Map {
            key: Box::new(RuntimeTy::String),
            value: Box::new(RuntimeTy::Int),
        })
    );
}

#[test]
fn unions_rank_identity_over_natural_shape_over_conversion() {
    let f = Fixture::new();
    // `int | float` takes an int as an int: identity beats conversion.
    assert!(matches!(
        f.ok(Mock::Int(1), "n").value,
        Some(Value::IntValue(1))
    ));
    assert!(matches!(
        f.ok(Mock::Float(1.5), "n").value,
        Some(Value::FloatValue(_))
    ));
    // A dict is a map before it is a class.
    let slot = RuntimeTy::Union(Box::new([
        RuntimeTy::Class(tn("A"), Box::new([])),
        RuntimeTy::Map {
            key: Box::new(RuntimeTy::String),
            value: Box::new(RuntimeTy::Int),
        },
    ]));
    let encoded = f
        .encode_in(Mock::Map(vec![(Mock::Str("x"), Mock::Int(1))]), &slot)
        .unwrap();
    assert!(matches!(stated(&encoded), Some(RuntimeTy::Map { .. })));
    // An instance is its class, whatever else the union offers.
    let encoded = f
        .encode_in(inst("A", vec![("x", Mock::Int(1))]), &slot)
        .unwrap();
    assert_eq!(
        stated(&encoded),
        Some(RuntimeTy::Class(tn("A"), Box::new([])))
    );
}

#[test]
fn several_fits_collapse_when_identical_and_are_ambiguous_otherwise() {
    let f = Fixture::new();
    // A list of ints is an `int[]` by its natural type.
    assert_eq!(
        stated(&f.ok(Mock::List(vec![Mock::Int(1)]), "k")),
        Some(list_ty(RuntimeTy::Int))
    );
    // An empty list fits both members as distinct types: ambiguous, as the
    // compiler says of `[]` with no annotation.
    assert_eq!(
        f.refusal(Mock::List(vec![]), "k"),
        "argument `x`: List([]) fits more than one of int[] | string[] and the host cannot yet \
         say which member is meant"
    );
    // The natural type breaks a tie among distinct fits, whatever the order.
    for members in [
        [
            list_ty(RuntimeTy::Int),
            list_ty(RuntimeTy::Union(Box::new([
                RuntimeTy::Int,
                RuntimeTy::String,
            ]))),
        ],
        [
            list_ty(RuntimeTy::Union(Box::new([
                RuntimeTy::Int,
                RuntimeTy::String,
            ]))),
            list_ty(RuntimeTy::Int),
        ],
    ] {
        let slot = RuntimeTy::Union(Box::new(members));
        let encoded = f.encode_in(Mock::List(vec![Mock::Int(1)]), &slot).unwrap();
        assert_eq!(stated(&encoded), Some(list_ty(RuntimeTy::Int)));
    }
    // `null` under `int? | string?` is one value either way.
    let slot = RuntimeTy::Union(Box::new([
        RuntimeTy::Union(Box::new([RuntimeTy::Int, RuntimeTy::Null])),
        RuntimeTy::Union(Box::new([RuntimeTy::String, RuntimeTy::Null])),
    ]));
    assert!(f.encode_in(Mock::Null, &slot).is_ok());
    // Two classes a dict converts into: ambiguous.
    assert_eq!(
        f.refusal(Mock::Map(vec![(Mock::Str("x"), Mock::Int(1))]), "l"),
        "argument `x`: Map([(Str(\"x\"), Int(1))]) fits more than one of A | B and the \
         host cannot yet say which member is meant"
    );
}

#[test]
fn a_union_reports_the_one_member_that_took_the_shape() {
    let f = Fixture::new();
    let slot = RuntimeTy::Union(Box::new([
        RuntimeTy::Class(tn("A"), Box::new([])),
        RuntimeTy::Null,
    ]));
    assert_eq!(
        refusal(f.encode_in(Mock::Map(vec![(Mock::Str("x"), Mock::Str("no"))]), &slot)),
        "argument `x`.x: expected int, found Str(\"no\")"
    );
}

#[test]
fn a_callable_binds_to_one_function_member_and_is_ambiguous_among_several() {
    let f = Fixture::new();
    let encoded = f.ok(Mock::Callable, "u");
    assert!(matches!(stated(&encoded), Some(RuntimeTy::Function { .. })));
    assert_eq!(f.host.registered.borrow().len(), 1);
    assert!(
        f.refusal(Mock::Callable, "v")
            .contains("fits more than one of")
    );
    // Nested too: a list of callables under two list-of-function members.
    let slot = RuntimeTy::Union(Box::new([
        list_ty(fn_ty(RuntimeTy::Int)),
        list_ty(fn_ty(RuntimeTy::String)),
    ]));
    assert!(
        refusal(f.encode_in(Mock::List(vec![Mock::Callable]), &slot))
            .contains("fits more than one of")
    );
    assert_eq!(
        f.host.registered.borrow().len(),
        1,
        "no registration for an ambiguous value"
    );
    // A callable has no type of its own under a slot that fixes none.
    assert_eq!(
        f.refusal(Mock::Callable, "g"),
        "argument `x`: cannot encode Callable: a callable has no type of its own; declare the \
         slot as a function type"
    );
    // A callable under a function type that leaves `T` open binds nothing.
    assert!(f.encode(Mock::Null, "z").is_ok());
    assert!(
        f.refusal(Mock::Callable, "z")
            .contains("could not infer a type for `T`")
    );
    assert_eq!(f.host.registered.borrow().len(), 1);
}

// ── Open slots and holes ────────────────────────────────────────────────────

#[test]
fn open_slots_take_the_values_own_type_and_fill_holes_with_unknown() {
    let f = Fixture::new();
    assert_eq!(stated(&f.ok(Mock::Int(1), "g")), None);
    assert_eq!(
        stated(&f.ok(Mock::List(vec![Mock::Int(1), Mock::Int(2)]), "g")),
        Some(list_ty(RuntimeTy::Int))
    );
    assert_eq!(
        stated(&f.ok(Mock::List(vec![Mock::Int(1), Mock::Str("a")]), "g")),
        Some(list_ty(RuntimeTy::Union(Box::new([
            RuntimeTy::Int,
            RuntimeTy::String
        ]))))
    );
    // An empty container's element is a hole: `unknown` here, as the compiler
    // types `[]`; filled by its siblings in `[[], [1]]`.
    assert_eq!(
        stated(&f.ok(Mock::List(vec![]), "g")),
        Some(list_ty(RuntimeTy::Unknown))
    );
    assert_eq!(
        stated(&f.ok(
            Mock::List(vec![Mock::List(vec![]), Mock::List(vec![Mock::Int(1)])]),
            "g"
        )),
        Some(list_ty(list_ty(RuntimeTy::Int)))
    );
    let encoded = f.ok(Mock::Map(vec![(Mock::Str("k"), Mock::Int(1))]), "g");
    assert_eq!(
        stated(&encoded),
        Some(RuntimeTy::Map {
            key: Box::new(RuntimeTy::String),
            value: Box::new(RuntimeTy::Int),
        })
    );
    // Under a bare type variable the hole is the variable left unbound.
    assert_eq!(
        stated(&f.ok(Mock::List(vec![Mock::Int(1)]), "p")),
        Some(list_ty(RuntimeTy::Int))
    );
    assert_eq!(
        f.refusal(Mock::List(vec![]), "p"),
        "argument `x`: BAML could not infer a type for `T` from the value; pass the call's type \
         arguments"
    );
    // A handle's type is the engine's to know: as evidence it is opaque,
    // which no sibling fills: `unknown` here, unbound under a type variable.
    assert_eq!(
        stated(&f.ok(Mock::List(vec![Mock::Handle]), "g")),
        Some(list_ty(RuntimeTy::Unknown))
    );
    assert_eq!(
        stated(&f.ok(Mock::List(vec![Mock::Handle, Mock::Int(1)]), "g")),
        Some(list_ty(RuntimeTy::Unknown))
    );
    assert_eq!(
        stated(&f.ok(
            Mock::List(vec![Mock::Handle, Mock::Media(MediaKind::Image)]),
            "g"
        )),
        Some(list_ty(RuntimeTy::Unknown))
    );
    assert!(
        f.refusal(Mock::List(vec![Mock::Int(1), Mock::Handle]), "p")
            .contains("could not infer a type for `T`")
    );
    assert!(
        f.refusal(inst("Box", vec![("item", Mock::Handle)]), "g")
            .contains("do not determine its type arguments")
    );
    assert_eq!(
        stated(&f.ok(
            Mock::List(vec![Mock::Int(1), Mock::Str("a"), Mock::Handle]),
            "g"
        )),
        Some(list_ty(RuntimeTy::Unknown))
    );
    assert!(
        f.refusal(Mock::List(vec![Mock::Handle]), "p")
            .contains("could not infer a type for `T`")
    );
    // A bare handle binds the slot's variables in the engine, from the live
    // value (`self: Stream<T>`), whatever the slot; inside a container it is
    // a hole like any other.
    assert!(f.encode(Mock::Handle, "d").is_ok());
    assert!(f.encode(Mock::Handle, "p").is_ok());
}

#[test]
fn an_empty_container_beside_other_types_states_its_own_type_under_unknown() {
    let f = Fixture::new();
    // The map's value type is a union the empty list is one member of; the
    // list states that member, not the union, and never panics.
    let encoded = f.ok(
        Mock::Map(vec![
            (Mock::Str("kind"), Mock::Str("ok")),
            (Mock::Str("status"), Mock::Int(202)),
            (Mock::Str("headers"), Mock::List(vec![])),
        ]),
        "g",
    );
    let Some(Value::MapValue(map)) = &encoded.value else {
        panic!()
    };
    assert_eq!(
        stated(map.entries[2].value.as_ref().unwrap()),
        Some(list_ty(RuntimeTy::Unknown))
    );
    assert_eq!(
        stated(&encoded),
        Some(RuntimeTy::Map {
            key: Box::new(RuntimeTy::String),
            value: Box::new(RuntimeTy::Union(Box::new([
                RuntimeTy::String,
                RuntimeTy::Int,
                list_ty(RuntimeTy::Unknown)
            ]))),
        })
    );
    // Beside a handle the value type collapses to `unknown`; the empty list
    // still states `unknown[]`.
    let encoded = f.ok(
        Mock::Map(vec![
            (Mock::Str("a"), Mock::List(vec![])),
            (Mock::Str("b"), Mock::Handle),
        ]),
        "g",
    );
    let Some(Value::MapValue(map)) = &encoded.value else {
        panic!()
    };
    assert_eq!(
        stated(map.entries[0].value.as_ref().unwrap()),
        Some(list_ty(RuntimeTy::Unknown))
    );
    assert_eq!(
        stated(&encoded),
        Some(RuntimeTy::Map {
            key: Box::new(RuntimeTy::String),
            value: Box::new(RuntimeTy::Unknown),
        })
    );
    // And a list of such: `[[], 1]` is `(unknown[] | int)[]` with `[]` as `unknown[]`.
    let encoded = f.ok(Mock::List(vec![Mock::List(vec![]), Mock::Int(1)]), "g");
    let Some(Value::ListValue(list)) = &encoded.value else {
        panic!()
    };
    assert_eq!(stated(&list.values[0]), Some(list_ty(RuntimeTy::Unknown)));
}

#[test]
fn leaves_fit_their_kind_any_open_slot_and_open_union_members() {
    let f = Fixture::new();
    assert!(f.encode(Mock::Media(MediaKind::Image), "q").is_ok());
    assert_eq!(
        f.refusal(Mock::Media(MediaKind::Image), "r"),
        "argument `x`: expected one of pdf | null, found Media(Image)"
    );
    assert!(f.encode(Mock::Media(MediaKind::Pdf), "r").is_ok());
    // Nested under `unknown`, and under `T?`.
    assert_eq!(
        stated(&f.ok(Mock::List(vec![Mock::Media(MediaKind::Image)]), "g")),
        Some(list_ty(RuntimeTy::Media(MediaKind::Image)))
    );
    let optional_var = RuntimeTy::Union(Box::new([
        RuntimeTy::TypeVar(ParamTy::new(0, Name::new("T"))),
        RuntimeTy::Null,
    ]));
    assert!(
        f.encode_in(Mock::Media(MediaKind::Image), &optional_var)
            .is_ok()
    );
    // A handle fits any realized slot; the engine judges it live.
    assert!(f.encode(Mock::Handle, "a").is_ok());
    assert_eq!(f.host.leaves.get(), 5);
}

// ── Classes and generic solving ─────────────────────────────────────────────

#[test]
fn classes_take_instances_as_is_and_mappings_strictly() {
    let f = Fixture::new();
    let slot = RuntimeTy::Class(tn("B"), Box::new([]));
    assert!(
        f.encode_in(
            inst("B", vec![("x", Mock::Int(1)), ("y", Mock::Null)]),
            &slot
        )
        .is_ok()
    );
    // A missing nullable field is null; a missing required one is refused.
    assert!(
        f.encode_in(Mock::Map(vec![(Mock::Str("x"), Mock::Int(1))]), &slot)
            .is_ok()
    );
    assert_eq!(
        refusal(f.encode_in(Mock::Map(vec![(Mock::Str("y"), Mock::Int(1))]), &slot)),
        "argument `x`.x: class user.B requires field `x`"
    );
    assert_eq!(
        refusal(f.encode_in(
            Mock::Map(vec![
                (Mock::Str("x"), Mock::Int(1)),
                (Mock::Str("z"), Mock::Int(1))
            ]),
            &slot
        )),
        "argument `x`: class user.B has no field `z`"
    );
    assert_eq!(
        refusal(f.encode_in(inst("A", vec![("x", Mock::Int(1))]), &slot)),
        "argument `x`: expected B, found Instance { class: \"A\", args: None, fields: [(\"x\", \
         Int(1))] }"
    );
}

#[test]
fn generic_class_arguments_come_from_the_slot_the_value_or_the_fields() {
    let f = Fixture::new();
    // The slot `Box<T>` with `T` bound by the call.
    let encoded = f.encode_in(
        inst("Box", vec![("item", Mock::Int(1))]),
        &RuntimeTy::Class(tn("Box"), Box::new([RuntimeTy::Int])),
    );
    assert_eq!(
        stated(&encoded.unwrap()),
        Some(RuntimeTy::Class(tn("Box"), Box::new([RuntimeTy::Int])))
    );
    // Under an open slot the fields decide, and the field is restated as the
    // class gives it: `(int | null)[]` for `T?[]`, not `int[]`.
    let encoded = f.ok(
        inst("OptList", vec![("xs", Mock::List(vec![Mock::Int(1)]))]),
        "g",
    );
    assert_eq!(
        stated(&encoded),
        Some(RuntimeTy::Class(tn("OptList"), Box::new([RuntimeTy::Int])))
    );
    let Some(Value::ClassValue(class)) = &encoded.value else {
        panic!()
    };
    assert_eq!(
        stated(class.fields[0].value.as_ref().unwrap()),
        Some(list_ty(RuntimeTy::Union(Box::new([
            RuntimeTy::Int,
            RuntimeTy::Null
        ]))))
    );
    // An empty container is a hole, filled by a sibling.
    let encoded = f.ok(
        inst("P2", vec![("a", Mock::List(vec![])), ("b", Mock::Int(1))]),
        "g",
    );
    assert_eq!(
        stated(&encoded),
        Some(RuntimeTy::Class(tn("P2"), Box::new([RuntimeTy::Int])))
    );
    let Some(Value::ClassValue(class)) = &encoded.value else {
        panic!()
    };
    assert_eq!(
        stated(class.fields[0].value.as_ref().unwrap()),
        Some(list_ty(RuntimeTy::Int))
    );
    // Nothing fills it: the arguments are missing, as the compiler says.
    assert_eq!(
        f.refusal(inst("OptList", vec![("xs", Mock::List(vec![]))]), "g"),
        "argument `x`: the fields of this user.OptList do not determine its type arguments; give \
         them explicitly"
    );
    // A partially realized slot keeps its realized argument: `b` is a float.
    let encoded = f
        .encode_in(
            inst("Pair", vec![("a", Mock::Str("x")), ("b", Mock::Int(1))]),
            &RuntimeTy::Class(
                tn("Pair"),
                Box::new([
                    RuntimeTy::TypeVar(ParamTy::new(0, Name::new("A"))),
                    RuntimeTy::Float,
                ]),
            ),
        )
        .unwrap();
    assert_eq!(
        stated(&encoded),
        Some(RuntimeTy::Class(
            tn("Pair"),
            Box::new([RuntimeTy::String, RuntimeTy::Float])
        ))
    );
    // The value's own arguments decide when it knows them, and must be the slot's.
    let typed = Mock::Instance {
        class: "Box",
        args: Some(vec![RuntimeTy::String]),
        fields: vec![("item", Mock::Str("s"))],
    };
    assert!(
        refusal(f.encode_in(
            typed,
            &RuntimeTy::Class(tn("Box"), Box::new([RuntimeTy::Int]))
        ))
        .contains("expected Box<int>")
    );
    // Instances of one head under a union: the fields decide between the members.
    let slot = RuntimeTy::Union(Box::new([
        RuntimeTy::Class(tn("Box"), Box::new([RuntimeTy::Int])),
        RuntimeTy::Class(tn("Box"), Box::new([RuntimeTy::String])),
    ]));
    assert_eq!(
        stated(
            &f.encode_in(inst("Box", vec![("item", Mock::Int(1))]), &slot)
                .unwrap()
        ),
        Some(RuntimeTy::Class(tn("Box"), Box::new([RuntimeTy::Int])))
    );
    assert!(
        refusal(f.encode_in(inst("Box", vec![("item", Mock::List(vec![]))]), &slot))
            .contains("expected one of")
    );
    let slot = RuntimeTy::Union(Box::new([
        RuntimeTy::Class(tn("Box"), Box::new([list_ty(RuntimeTy::Int)])),
        RuntimeTy::Class(tn("Box"), Box::new([list_ty(RuntimeTy::String)])),
    ]));
    assert!(
        refusal(f.encode_in(inst("Box", vec![("item", Mock::List(vec![]))]), &slot))
            .contains("fits more than one")
    );
}

#[test]
fn a_known_sibling_taking_the_value_leaves_the_variable_never() {
    let f = Fixture::new();
    // `OptList<T>{ xs: T?[] }` with `[None]`: `null` is a known member of
    // `T | null`, nothing pins `T`, so `T = never` and the class states it.
    let encoded = f.ok(
        inst("OptList", vec![("xs", Mock::List(vec![Mock::Null]))]),
        "g",
    );
    assert_eq!(
        stated(&encoded),
        Some(RuntimeTy::Class(
            tn("OptList"),
            Box::new([RuntimeTy::Never])
        ))
    );
    // A value that does pin it wins over the absorbed one.
    let encoded = f.ok(
        inst(
            "OptList",
            vec![("xs", Mock::List(vec![Mock::Null, Mock::Int(1)]))],
        ),
        "g",
    );
    assert_eq!(
        stated(&encoded),
        Some(RuntimeTy::Class(tn("OptList"), Box::new([RuntimeTy::Int])))
    );
    // The same under a function slot: `(T | null)[]` with `[None]`.
    assert_eq!(
        stated(&f.ok(Mock::List(vec![Mock::Null]), "y")),
        Some(list_ty(RuntimeTy::Union(Box::new([
            RuntimeTy::Never,
            RuntimeTy::Null
        ]))))
    );
    // The solver learns through the one open member of a union, bare or not.
    let slot = list_ty(RuntimeTy::Union(Box::new([
        RuntimeTy::Class(
            tn("Box"),
            Box::new([RuntimeTy::TypeVar(ParamTy::new(0, Name::new("T")))]),
        ),
        RuntimeTy::Null,
    ])));
    let encoded = f
        .encode_in(
            Mock::List(vec![inst("Box", vec![("item", Mock::Int(1))]), Mock::Null]),
            &slot,
        )
        .unwrap();
    assert_eq!(
        stated(&encoded),
        Some(list_ty(RuntimeTy::Union(Box::new([
            RuntimeTy::Class(tn("Box"), Box::new([RuntimeTy::Int])),
            RuntimeTy::Null
        ]))))
    );
}

#[test]
fn union_choice_does_not_depend_on_element_order() {
    let f = Fixture::new();
    let int_or_string = RuntimeTy::Union(Box::new([RuntimeTy::Int, RuntimeTy::String]));
    let wider = RuntimeTy::Union(Box::new([
        RuntimeTy::Int,
        RuntimeTy::String,
        RuntimeTy::Bool,
    ]));
    let slot = RuntimeTy::Union(Box::new([list_ty(int_or_string.clone()), list_ty(wider)]));
    for items in [
        vec![Mock::Int(1), Mock::Str("a")],
        vec![Mock::Str("a"), Mock::Int(1)],
    ] {
        let encoded = f.encode_in(Mock::List(items), &slot).unwrap();
        assert_eq!(stated(&encoded), Some(list_ty(int_or_string.clone())));
    }
}

#[test]
fn host_type_arguments_are_realized_and_in_stated_form() {
    let f = Fixture::new();
    // A type argument that names a type variable is the host's mistake, never
    // something the walker binds; it is refused at once, whatever its index.
    let var = RuntimeTy::TypeVar(ParamTy::new(u32::MAX - 7, Name::new("T")));
    let typed = Mock::Instance {
        class: "Box",
        args: Some(vec![var.clone()]),
        fields: vec![("item", Mock::Int(1))],
    };
    assert!(f.refusal(typed, "g").contains("must be realized types"));
    let signature = f.facts.function("user.f").unwrap();
    let mut owners = Owners::new(&f.host);
    let mut type_args = HashMap::new();
    type_args.insert(Name::new("T"), var);
    let error = encode_arguments(
        &f.facts,
        signature,
        &type_args,
        vec![("p".to_string(), Mock::List(vec![Mock::Int(1)]))],
        &mut owners,
    )
    .unwrap_err();
    assert!(
        matches!(error, Error::Value(ref e) if e.to_string().contains("must be realized types"))
    );
    // A type argument spelled with a non-recursive alias means its body.
    let aliased = Mock::Instance {
        class: "Box",
        args: Some(vec![RuntimeTy::TypeAlias(tn("IntOrStr"))]),
        fields: vec![("item", Mock::Int(1))],
    };
    assert_eq!(
        stated(&f.ok(aliased, "g")),
        Some(RuntimeTy::Class(
            tn("Box"),
            Box::new([RuntimeTy::Union(Box::new([
                RuntimeTy::Int,
                RuntimeTy::String
            ]))])
        ))
    );
}

#[test]
fn a_function_slot_with_an_open_element_type_is_solved_and_restated() {
    let f = Fixture::new();
    // Elements of several types bind the variable to their union, as the
    // compiler types the literal; a class's fields that disagree do not.
    assert_eq!(
        stated(&f.ok(Mock::List(vec![Mock::Int(1), Mock::Str("a")]), "p")),
        Some(list_ty(RuntimeTy::Union(Box::new([
            RuntimeTy::Int,
            RuntimeTy::String
        ]))))
    );
    assert_eq!(
        f.refusal(
            inst(
                "P2",
                vec![("a", Mock::List(vec![Mock::Int(1)])), ("b", Mock::Str("s"))]
            ),
            "g"
        ),
        "argument `x`: the fields of this user.P2 disagree about its type argument `T`"
    );
    for items in [
        vec![Mock::Int(1), Mock::Int(2)],
        vec![Mock::Int(1), Mock::Null],
    ] {
        assert_eq!(
            stated(&f.ok(Mock::List(items), "y")),
            Some(list_ty(RuntimeTy::Union(Box::new([
                RuntimeTy::Int,
                RuntimeTy::Null
            ]))))
        );
    }
}

// ── Aliases ─────────────────────────────────────────────────────────────────

#[test]
fn recursive_aliases_are_judged_by_their_bodies_and_terminate() {
    let f = Fixture::new();
    assert_eq!(stated(&f.ok(Mock::Int(1), "m")), None);
    let encoded = f.ok(
        Mock::List(vec![Mock::Int(1), Mock::List(vec![Mock::Int(2)])]),
        "m",
    );
    assert_eq!(
        stated(&encoded),
        Some(list_ty(RuntimeTy::TypeAlias(tn("Tree"))))
    );
    // `type Loop = Loop | Loop[]` is the type of `Loop[]`; the unguarded member is vacuous.
    assert_eq!(
        stated(&f.ok(Mock::List(vec![]), "w")),
        Some(list_ty(RuntimeTy::TypeAlias(tn("Loop"))))
    );
    assert!(f.refusal(Mock::Str("s"), "w").contains("expected one of"));
    // `Node | map<string, Tree2>` nested deep: each level is planned once.
    let mut value = Mock::Map(vec![]);
    for _ in 0..30 {
        value = Mock::Map(vec![(Mock::Str("c"), value)]);
    }
    let started = std::time::Instant::now();
    assert!(f.encode(value, "x").is_ok());
    assert!(
        started.elapsed().as_secs() < 5,
        "planning is not exponential in depth"
    );
}

// ── Maps and keys ───────────────────────────────────────────────────────────

#[test]
fn map_keys_are_strings_and_errors_carry_the_entry() {
    let f = Fixture::new();
    assert_eq!(
        f.refusal(
            Mock::Map(vec![(Mock::Str("k"), Mock::Str("not an int"))]),
            "j"
        ),
        "argument `x`.k: expected int, found Str(\"not an int\")"
    );
    assert_eq!(
        f.refusal(Mock::Map(vec![(Mock::Int(1), Mock::Int(2))]), "j"),
        "argument `x`[key Int(1)]: map keys must be string, found Int(1)"
    );
    assert!(
        f.refusal(Mock::Map(vec![(Mock::Int(1), Mock::Int(2))]), "g")
            .contains("map keys must be")
    );
}

// ── Effects: only what ships is created ─────────────────────────────────────

#[test]
fn a_losing_member_creates_nothing_and_a_refusal_creates_nothing() {
    let f = Fixture::new();
    let slot = RuntimeTy::Union(Box::new([
        RuntimeTy::Class(tn("Holder"), Box::new([])),
        RuntimeTy::Map {
            key: Box::new(RuntimeTy::String),
            value: Box::new(fn_ty(RuntimeTy::Int)),
        },
    ]));
    let encoded = f
        .encode_in(Mock::Map(vec![(Mock::Str("f"), Mock::Callable)]), &slot)
        .unwrap();
    assert!(matches!(stated(&encoded), Some(RuntimeTy::Map { .. })));
    assert_eq!(
        f.host.registered.borrow().len(),
        1,
        "the shipped callable only"
    );
    assert!(f.host.released.borrow().is_empty());
    // A refused value: a list with a callable and an int fits neither member.
    let slot = RuntimeTy::Union(Box::new([
        list_ty(fn_ty(RuntimeTy::Int)),
        list_ty(RuntimeTy::Int),
    ]));
    assert!(
        f.encode_in(Mock::List(vec![Mock::Callable, Mock::Int(1)]), &slot)
            .is_err()
    );
    assert_eq!(f.host.registered.borrow().len(), 1);
}

#[test]
fn a_failed_commit_releases_what_it_created_and_a_dropped_guard_releases_all() {
    let f = Fixture::new();
    f.host.fail_register_at.set(Some(2));
    let slot = list_ty(fn_ty(RuntimeTy::Int));
    let mut owners = Owners::new(&f.host);
    let result = encode(
        &f.facts,
        Mock::List(vec![Mock::Callable, Mock::Callable, Mock::Callable]),
        &slot,
        Root::Argument("x".into()),
        &mut owners,
    );
    // A failure while committing is the bridge's, not the callable's.
    assert!(matches!(result, Err(Error::Commit(ref message)) if message == "registration failed"));
    assert_eq!(owners.created(), 1);
    drop(owners);
    assert_eq!(
        *f.host.released.borrow(),
        vec![Owner::Callable(host_key(100))]
    );
    // A guard dropped without `keep` releases a shipped encode too: the
    // handle is judged by the engine, so it fits the function slot here.
    f.host.fail_register_at.set(None);
    let mut owners = Owners::new(&f.host);
    encode(
        &f.facts,
        Mock::List(vec![Mock::Callable, Mock::Handle]),
        &slot,
        Root::Argument("x".into()),
        &mut owners,
    )
    .unwrap();
    assert_eq!(owners.created(), 2);
    drop(owners);
    assert_eq!(
        *f.host.released.borrow(),
        vec![
            Owner::Callable(host_key(100)),
            Owner::Callable(host_key(101)),
            Owner::Handle(table_key(9))
        ]
    );
}

#[test]
fn discarded_trials_describe_nothing() {
    let f = Fixture::new();
    let slot = RuntimeTy::Union(Box::new([
        RuntimeTy::Int,
        RuntimeTy::Float,
        RuntimeTy::String,
    ]));
    f.encode_in(Mock::Str("s"), &slot).unwrap();
    assert_eq!(describes(), 0);
    f.encode_in(Mock::Bool(true), &slot).unwrap_err();
    assert_eq!(describes(), 1);
}

// ── Depth, size, host errors ────────────────────────────────────────────────

#[test]
fn a_value_the_wire_cannot_carry_is_refused_before_anything_is_created() {
    let f = Fixture::new();
    let mut value = Mock::Callable;
    for _ in 0..60 {
        value = Mock::List(vec![value]);
    }
    assert!(f.refusal(value, "g").contains("nested too deeply"));
    let mut value = Mock::Int(1);
    for _ in 0..40 {
        value = Mock::List(vec![value]);
    }
    assert!(f.encode(value, "g").is_ok());
    // A shallow value whose stated type is deep is measured too.
    let mut slot = RuntimeTy::Int;
    for _ in 0..55 {
        slot = list_ty(slot);
    }
    assert!(refusal(f.encode_in(Mock::List(vec![]), &slot)).contains("nested too deeply"));
    // The plan is measured before anything is created: a callable whose
    // function type nests too deeply is never registered.
    let mut signature = RuntimeTy::Int;
    for _ in 0..50 {
        signature = list_ty(signature);
    }
    let slot = RuntimeTy::Function {
        params: Box::new([]),
        ret: Box::new(signature),
        throws: Box::new(RuntimeTy::Never),
    };
    assert!(refusal(f.encode_in(Mock::Callable, &slot)).contains("nested too deeply"));
    assert_eq!(f.host.registered.borrow().len(), 0);
    assert!(f.host.released.borrow().is_empty());
}

#[test]
fn a_hosts_own_exception_is_raised_as_it_is() {
    let f = Fixture::new();
    assert!(
        matches!(f.encode(Mock::Raising, "a"), Err(Error::Host(ref m)) if m == "the host raised")
    );
    assert!(matches!(
        f.encode(Mock::List(vec![Mock::Raising]), "g"),
        Err(Error::Host(_))
    ));
    // A thrown host error, a contract violation and a bridge failure are told apart.
    assert!(matches!(
        Completion::from(f.encode(Mock::Raising, "a")),
        Completion::HostError(_)
    ));
    assert!(matches!(
        Completion::from(f.encode(Mock::Str("s"), "a")),
        Completion::ContractViolation(_)
    ));
    let mut deep = Mock::Int(1);
    for _ in 0..60 {
        deep = Mock::List(vec![deep]);
    }
    assert!(matches!(
        Completion::from(f.encode(deep, "g")),
        Completion::BridgeFailure(_)
    ));
}

// ── Arguments and the wire ──────────────────────────────────────────────────

#[test]
fn arguments_are_keyed_by_declared_name_and_type_args_substitute() {
    let f = Fixture::new();
    let signature = f.facts.function("user.f").unwrap();
    let mut owners = Owners::new(&f.host);
    let mut type_args = HashMap::new();
    type_args.insert(Name::new("T"), RuntimeTy::String);
    let entries = encode_arguments(
        &f.facts,
        signature,
        &type_args,
        vec![
            ("a".to_string(), Mock::Int(1)),
            ("p".to_string(), Mock::List(vec![Mock::Str("s")])),
        ],
        &mut owners,
    )
    .unwrap();
    owners.keep();
    assert_eq!(entries.len(), 2);
    assert_eq!(
        stated(entries[1].value.as_ref().unwrap()),
        Some(list_ty(RuntimeTy::String))
    );
    // Bound `T = string`, an int element is refused with its path.
    let mut owners = Owners::new(&f.host);
    let error = encode_arguments(
        &f.facts,
        signature,
        &type_args,
        vec![("p".to_string(), Mock::List(vec![Mock::Int(1)]))],
        &mut owners,
    )
    .unwrap_err();
    assert!(
        matches!(error, Error::Value(ref e) if e.to_string() == "argument `p`[0]: expected string, found Int(1)")
    );
}

#[test]
fn stated_types_are_the_wire_types_the_engine_reads() {
    let f = Fixture::new();
    let a = f.ok(inst("A", vec![("x", Mock::Int(1))]), "g");
    assert!(
        matches!(a.value_type.as_ref().and_then(|ty| ty.ty.as_ref()), Some(WireTy::ClassTy(c)) if c.name == "user.A")
    );
    assert_eq!(
        stated(&f.ok(Mock::List(vec![Mock::Int(1), Mock::Str("a")]), "t")),
        Some(f.slot("t"))
    );
    assert_eq!(stated(&f.ok(Mock::Int(1), "a")), None);
}

#[test]
fn host_callable_fixture_descriptor_is_current() {
    // The program declares no `throws` on its callbacks, so each carries an
    // implicit effect parameter, which a host states as `unknown`.
    let callback = |params: Vec<Ty>, ret: Ty| {
        function(
            params,
            ret,
            Ty::TypeVar(ParamTy::new(0, Name::new("__effect_param_0"))),
        )
    };
    // A nested function type declares its `throws` explicitly.
    let never_callback = |params: Vec<Ty>, ret: Ty| function(params, ret, Ty::Never);
    let function_desc = |fqn: &str, params: Vec<ParamDescriptor>, ret: Ty| FunctionDescriptor {
        fqn: fqn.into(),
        class_generic_params: vec![],
        generic_params: vec![],
        params,
        ret,
        throws: Ty::Unknown,
    };
    let program = ProgramDescriptor {
        functions: vec![
            function_desc(
                "CallCb",
                vec![
                    param("callback", callback(vec![Ty::Int], Ty::String)),
                    param("x", Ty::Int),
                ],
                Ty::String,
            ),
            function_desc(
                "CallIntCb",
                vec![
                    param("callback", callback(vec![Ty::Int], Ty::Int)),
                    param("x", Ty::Int),
                ],
                Ty::Int,
            ),
            function_desc(
                "CallFloatCb",
                vec![param("callback", callback(vec![], Ty::Float))],
                Ty::Float,
            ),
            function_desc(
                "ConsumeUnknownCb",
                vec![
                    param("callback", callback(vec![Ty::Int], Ty::Unknown)),
                    param("x", Ty::Int),
                ],
                Ty::Int,
            ),
            function_desc(
                "UnionCbs",
                vec![param(
                    "callbacks",
                    union(vec![
                        Ty::List(Box::new(never_callback(vec![Ty::Int], Ty::Int))),
                        Ty::List(Box::new(Ty::String)),
                    ]),
                )],
                Ty::Int,
            ),
            function_desc(
                "AmbiguousCbs",
                vec![param(
                    "callbacks",
                    union(vec![
                        Ty::List(Box::new(never_callback(vec![Ty::Int], Ty::Int))),
                        Ty::List(Box::new(never_callback(vec![Ty::Int], Ty::String))),
                    ]),
                )],
                Ty::Int,
            ),
            function_desc("TakeUnknown", vec![param("value", Ty::Unknown)], Ty::Int),
        ],
        classes: vec![],
        enums: vec![],
        type_aliases: vec![],
    };
    let current = program.encode_embedded();
    let checked_in = include_str!("../fixtures/host_callable_program.descriptor").trim();
    assert_eq!(
        checked_in, current,
        "fixtures/host_callable_program.descriptor is stale; replace its content with:\n{current}"
    );
    assert_eq!(
        ProgramDescriptor::decode_embedded(checked_in.as_bytes()).unwrap(),
        program
    );
}
