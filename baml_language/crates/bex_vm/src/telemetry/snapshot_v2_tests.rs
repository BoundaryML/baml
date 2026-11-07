//! Compare the two heap traversals using the existing VM capture as oracle.
use super::*;
use bex_v2_heap::{Field, SlotKind, TaggedSlot as Dyn};
// Compile the actual traversal in this test, without linking two fiber runtimes.
#[allow(dead_code, unreachable_pub)]
#[path = "../../../bex_v2_runtime/src/telemetry/capture.rs"]
mod v2;
use std::sync::OnceLock;
static CLASS: OnceLock<v2::Declaration> = OnceLock::new();
static ENUM: OnceLock<v2::Declaration> = OnceLock::new();
fn class() -> &'static v2::Declaration {
    CLASS.get().unwrap()
}
fn enm() -> &'static v2::Declaration {
    ENUM.get().unwrap()
}
fn frozen(ptr: HeapPtr) -> v2::Declaration {
    let (name, tag, is_enum) = match unsafe { ptr.get() } {
        Object::Class(c) => (c.name(), c.type_tag, false),
        Object::Enum(e) => (e.name(), e.type_tag, true),
        _ => unreachable!(),
    };
    v2::Declaration {
        name,
        tag,
        is_enum,
        definition: unsafe { definitions::of(ptr) }.unwrap().clone(),
    }
}
fn oracle(input: Input<'_>) -> Snapshot {
    unsafe {
        Scratch::default().capture(
            btel_snapshot::SnapshotPool::unbounded(btel_snapshot::Limits::default())
                .acquire_beyond_budget(),
            input,
        )
    }
}
#[test]
fn v2_captures_match_vm_content_ids_and_definition_blobs() {
    let mut vm = crate::vm::tests::test_vm(Vec::new());
    let class_ptr = vm.tlab.alloc(Object::Class(Box::new(bex_vm_types::Class {
        declared_crossing: baml_type::ClassCrossing::Fields,
        path: bex_vm_types::types::LocalName::new(Vec::new(), baml_type::Name::new("TestClass")),
        fields: vec![
            bex_vm_types::ClassField {
                name: "x".to_string(),
                field_type: baml_type::RuntimeTy::Int,
                field_template: baml_type::TyTemplate::from(baml_type::RealizedTy::Int),
                must_exist: false,
                stream_done: false,
                description: None,
                alias: None,
                docstring: None,
                other: indexmap::IndexMap::default(),
                skip: false,
                runtime_type: None,
            },
            bex_vm_types::ClassField {
                name: "y".to_string(),
                field_type: baml_type::RuntimeTy::Int,
                field_template: baml_type::TyTemplate::from(baml_type::RealizedTy::Int),
                must_exist: false,
                stream_done: false,
                description: None,
                alias: None,
                docstring: None,
                other: indexmap::IndexMap::default(),
                skip: false,
                runtime_type: None,
            },
        ],
        description: None,
        alias: None,
        docstring: None,
        other: indexmap::IndexMap::default(),
        type_tag: baml_type::typetag::TypeTag::from_i64(100),
        stream_done: false,
        cleanup: None,
        methods: indexmap::IndexMap::new(),
        generic_param_count: 0,
        owner: bex_vm_types::types::Owner::anonymous(),
        telemetry_definition: bex_vm_types::DefinitionCell::default(),
    })));
    let enum_ptr = vm.tlab.alloc(Object::Enum(Box::new(bex_vm_types::Enum {
        type_tag: baml_type::typetag::TypeTag::from_i64(200),
        path: bex_vm_types::types::LocalName::new(Vec::new(), baml_type::Name::new("Color")),
        variants: vec![
            bex_vm_types::EnumVariant {
                name: "Red".to_string(),
                description: None,
                alias: None,
                docstring: None,
                other: indexmap::IndexMap::default(),
                skip: false,
            },
            bex_vm_types::EnumVariant {
                name: "Green".to_string(),
                description: None,
                alias: None,
                docstring: None,
                other: indexmap::IndexMap::default(),
                skip: false,
            },
            bex_vm_types::EnumVariant {
                name: "Blue".to_string(),
                description: None,
                alias: None,
                docstring: None,
                other: indexmap::IndexMap::default(),
                skip: false,
            },
        ],
        description: None,
        alias: None,
        docstring: None,
        other: indexmap::IndexMap::default(),
        owner: bex_vm_types::types::Owner::anonymous(),
        telemetry_definition: bex_vm_types::DefinitionCell::default(),
    })));

    assert!(CLASS.set(frozen(class_ptr)).is_ok());
    assert!(ENUM.set(frozen(enum_ptr)).is_ok());
    let shapes = [
        v2::Shape::String,
        v2::Shape::Class {
            declaration: class,
            fields: &[
                Field {
                    name: "x",
                    offset: 8,
                    slot: SlotKind::Scalar,
                    type_id: 0,
                },
                Field {
                    name: "y",
                    offset: 16,
                    slot: SlotKind::Scalar,
                    type_id: 0,
                },
            ],
        },
        v2::Shape::Enum {
            declaration: enm,
            variants: &["Red", "Green", "Blue"],
        },
        v2::Shape::List {
            growing: false,
            tag: 0,
            element: &v2::Type::Int,
        },
        v2::Shape::MapStringInt,
    ];
    let instance = vm.tlab.alloc_instance_with_type_args(
        class_ptr,
        Box::new([]),
        vec![Value::int(10), Value::int(20)],
    );
    let variant = vm.tlab.alloc_variant(enum_ptr, 1);
    let string = vm.tlab.alloc_string("hello");
    let list = vm.tlab.alloc_array(
        bex_vm_types::RealizedTy::Int,
        vec![Value::int(-9), Value::int(8)],
    );
    let map = vm.tlab.alloc_map(
        bex_vm_types::RealizedTy::String,
        bex_vm_types::RealizedTy::Int,
        indexmap::IndexMap::from([("hello".into(), Value::int(12))]),
    );
    let record = [0u64, 10, 20];
    let text = [0u64, 5, u64::from_le_bytes(*b"hello\0\0\0")];
    let array = [0u64, 2, (-9i64).cast_unsigned(), 8];
    let buffer = [0u64, 1, text.as_ptr() as u64, 12];
    let mapping = [0u64, buffer.as_ptr() as u64, 1, 1];
    let values = [
        Value::int(-7),
        Value::bool(true),
        Value::NULL,
        Value::OMITTED_ARG,
        Value::object(instance),
        Value::object(instance),
        Value::object(variant),
        Value::object(string),
        Value::object(list),
        Value::object(map),
    ];
    let native = [
        Dyn {
            tag: 0,
            payload: (-7i64).cast_unsigned(),
        },
        Dyn { tag: 1, payload: 1 },
        Dyn { tag: 2, payload: 0 },
        Dyn {
            tag: u64::MAX,
            payload: 0,
        },
        Dyn {
            tag: 4,
            payload: record.as_ptr() as u64,
        },
        Dyn {
            tag: 4,
            payload: record.as_ptr() as u64,
        },
        Dyn { tag: 5, payload: 1 },
        Dyn {
            tag: 3,
            payload: text.as_ptr() as u64,
        },
        Dyn {
            tag: 6,
            payload: array.as_ptr() as u64,
        },
        Dyn {
            tag: 7,
            payload: mapping.as_ptr() as u64,
        },
    ];
    let expected = oracle(Input::FunctionArgs(&values));
    let actual = unsafe { v2::Scratch::default().arguments(&shapes, &native) };
    assert_eq!(actual.root_id(), expected.root_id());
    for (value, native) in values.into_iter().zip(native) {
        assert_eq!(
            unsafe { v2::Scratch::default().value(&shapes, native) }.root_id(),
            oracle(Input::Value(value)).root_id()
        );
    }
    let vm_type =
        bex_vm_types::RealizedTy::Class(TypeHead::new(class_ptr, class().tag), Box::new([]));
    let expected = oracle(Input::TypeArgs(&[("T", &vm_type)]));
    let actual = v2::Scratch::default().type_args(&[("T", &v2::Type::Class(class))]);
    assert_eq!(actual.root_id(), expected.root_id());
}
