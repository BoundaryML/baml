//! Checked adapter registration followed by real engine callback dispatch,
//! completion validation and owned interface transport. SDK binding and the
//! C completion decoder remain outside this in-process test.

use std::sync::{Arc, Weak};

use crate::{HostAdapterImplementation, HostAdapterTypeDescriptor, HostDeclaration};
use baml_type::{Name, TyTemplate, TypeName};
use bex_resource_types::{HostValueArc, HostValueKind};
use bridge_ctypes::baml_bridge::cffi::{BamlToHostCall, baml_outbound_value};
use prost::Message;
use sys_native::SysOpsExt;

use super::{BexEngine, BexExternalValue, FunctionCallContextBuilder};

const SOURCE: &str = r#"
interface Echo<A> {
    type Output
    function echo<T, U>(self, value: T) -> T throws never
    function produce(self, value: A) -> Self.Output throws never
    function again<T>(self, value: T) -> T throws never { self.echo<T, int>(value) }
}
function exercise(value: Echo<string, Output=int>) -> int {
    assert.equal(value.echo<string, int>("Ada"), "Ada");
    assert.equal(value.echo<int, string>(7), 7);
    assert.equal(value.again<string>("default"), "default");
    let bound = value.echo<string, int>;
    assert.equal(bound("bound"), "bound");
    value.produce("answer")
}
"#;

// The only callbacks in this test carry primitive values, so no transferred
// object leases need adoption. Decode the actual engine-generated wire call.
extern "C" fn dispatch(key: u64, call_id: u32, bytes: *const u8, len: usize) {
    // SAFETY: sys_native keeps the nonempty encoded call alive during dispatch.
    let call = unsafe { std::slice::from_raw_parts(bytes, len) };
    let value = BamlToHostCall::decode(call)
        .ok()
        .and_then(|call| call.args.into_iter().next())
        .and_then(|arg| arg.value)
        .and_then(|value| value.value);
    let returned = match (key, value) {
        (1, Some(baml_outbound_value::Value::StringValue(value))) => {
            BexExternalValue::String(value.into())
        }
        (1, Some(baml_outbound_value::Value::IntValue(value))) => BexExternalValue::Int(value),
        (2, Some(baml_outbound_value::Value::StringValue(_))) => BexExternalValue::Int(73),
        (3, _) => BexExternalValue::String("wrong output".into()),
        (4, _) => {
            sys_native::host_dispatch::complete_with_throw(call_id, BexExternalValue::Int(9));
            return;
        }
        _ => BexExternalValue::Null, // Causes a contract failure without unwinding through C.
    };
    sys_native::host_dispatch::complete_with_value(call_id, returned);
}

async fn adapter(
    engine: &Arc<BexEngine>,
    produce_key: u64,
) -> (BexExternalValue, Weak<HostValueArc>) {
    let registered = engine
        .register_host_adapter(HostAdapterTypeDescriptor {
            name: Name::new("HostAdapter"),
            implementations: vec![HostAdapterImplementation {
                interface: TyTemplate::interface(
                    HostDeclaration::Named(TypeName::from_dotted_path("user.Echo")),
                    vec![TyTemplate::String {
                        attr: Default::default(),
                    }],
                    vec![(
                        Name::new("Output"),
                        TyTemplate::Int {
                            attr: Default::default(),
                        },
                    )],
                ),
                methods: vec![Name::new("produce"), Name::new("echo")],
            }],
        })
        .await
        .unwrap();
    // The engine API owns its registration across suspension and moving GC.
    engine
        .collect_garbage(bex_heap::CollectionLevel::Major)
        .await;
    assert_eq!(
        registered
            .callbacks()
            .iter()
            .map(|slot| slot.method.as_str())
            .collect::<Vec<_>>(),
        ["echo", "produce"]
    );
    let callback = HostValueArc::new(1, HostValueKind::Callable);
    let weak = Arc::downgrade(&callback);
    let receiver = engine
        .create_host_adapter_instance(
            &registered,
            HostValueArc::new(900, HostValueKind::Opaque),
            vec![
                callback,
                HostValueArc::new(produce_key, HostValueKind::Callable),
            ],
        )
        .await
        .unwrap();
    let exported = engine
        .project_interface_with_type_argument(
            BexExternalValue::Handle(receiver),
            bex_external_types::TypeArgument::Reference(registered.interfaces()[0].clone()),
        )
        .await
        .unwrap();
    (
        BexExternalValue::Adt(bex_external_types::BexExternalAdt::Interface(exported)),
        weak,
    )
}

#[tokio::test]
async fn host_methods_use_real_dispatch_validate_completions_and_release_their_receiver() {
    sys_native::host_dispatch::set_dispatch_fn(dispatch);
    let engine = Arc::new(
        BexEngine::new(
            baml_db::testing::compile_source(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .unwrap(),
    );
    for key in [2, 3, 4] {
        let (value, weak) = adapter(&engine, key).await;
        engine
            .collect_garbage(bex_heap::CollectionLevel::Major)
            .await;
        assert!(
            weak.upgrade().is_some(),
            "exported interface retains its implementation"
        );
        let result = engine
            .call_function(
                "exercise",
                vec![value],
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true,
            )
            .await;
        if key == 2 {
            assert_eq!(result.unwrap(), BexExternalValue::Int(73));
        } else {
            let error = result.unwrap_err();
            assert!(
                error.to_string().contains("HostContractViolation"),
                "invalid return/throw must fail the realized method contract: {error:?}"
            );
        }
        engine
            .collect_garbage(bex_heap::CollectionLevel::Major)
            .await;
        assert!(
            weak.upgrade().is_none(),
            "an idle, still-live engine must release the last callback owner after GC"
        );
    }
}
