//! The vendored Cranelift must report a reference that is live into an
//! exception handler in the stack map of the `try_call` it is live across.
//! Upstream 0.130.2 spills it and leaves the map empty; the collector would
//! then miss a root of every frame that catches.

use cranelift_codegen::{
    Context,
    ir::{
        self, AbiParam, BlockArg, ExceptionTableData, ExceptionTableItem, InstBuilder, MemFlags,
        Signature, UserFuncName, types::I64,
    },
    isa::CallConv,
    settings::{self, Configurable},
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

#[test]
fn a_reference_live_into_a_handler_is_in_the_try_call_stack_map() {
    let mut flags = settings::builder();
    flags.set("opt_level", "speed").unwrap();
    flags.set("preserve_frame_pointers", "true").unwrap();
    let isa = cranelift_native::builder()
        .unwrap()
        .finish(settings::Flags::new(flags))
        .unwrap();

    let mut signature = Signature::new(CallConv::Tail);
    signature.params.push(AbiParam::new(I64));
    signature.params.push(AbiParam::new(I64));
    signature.returns.push(AbiParam::new(I64));
    let mut function =
        ir::Function::with_name_signature(UserFuncName::testcase("catches"), signature.clone());
    let mut context = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut function, &mut context);
    let entry = builder.create_block();
    let resume = builder.create_block();
    let handler = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.append_block_param(resume, I64);
    builder.append_block_param(handler, I64);

    builder.switch_to_block(entry);
    let object = builder.block_params(entry)[0];
    let callee = builder.block_params(entry)[1];
    builder.declare_value_needs_stack_map(object);
    let callee_signature = builder.import_signature(signature);
    let returned = builder
        .func
        .dfg
        .block_call(resume, &[BlockArg::TryCallRet(0)]);
    let thrown = builder
        .func
        .dfg
        .block_call(handler, &[BlockArg::TryCallExn(0)]);
    let table = builder
        .func
        .dfg
        .exception_tables
        .push(ExceptionTableData::new(
            callee_signature,
            returned,
            [ExceptionTableItem::Default(thrown)],
        ));
    builder
        .ins()
        .try_call_indirect(callee, &[object, callee], table);

    builder.switch_to_block(resume);
    let value = builder.block_params(resume)[0];
    builder.ins().return_(&[value]);

    // The handler is the only use of `object` after the call.
    builder.switch_to_block(handler);
    let field = builder.ins().load(I64, MemFlags::trusted(), object, 8);
    builder.ins().return_(&[field]);
    builder.seal_all_blocks();
    builder.finalize();

    let mut compilation = Context::for_function(function);
    let code = compilation
        .compile(
            &*isa,
            &mut cranelift_codegen::control::ControlPlane::default(),
        )
        .expect("the function compiles");
    let site = code
        .buffer
        .call_sites()
        .find(|site| !site.exception_handlers.is_empty())
        .expect("the try_call has a call site record");
    let slots = code
        .buffer
        .user_stack_maps()
        .iter()
        .find(|(return_address, _, _)| *return_address == site.ret_addr)
        .map_or(0, |(_, _, map)| map.entries().count());
    assert_eq!(slots, 1, "the try_call's stack map names the reference");
}
