//! Jinja template rendering to `PromptAst`.

use std::sync::Arc;

use bex_heap::builtin_types;
use bex_vm_types::PromptAst;
use sys_types::OpErrorKind;

/// Execute the `render_prompt` LLM operation.
///
/// Arguments: `[PrimitiveClient, template: String, args: Map<String, Any>]`
/// `PrimitiveClient` is extracted from the heap via the builtin class accessor (heap-backed).
pub fn execute_render_prompt(
    heap: &Arc<bex_heap::BexHeap>,
    mut args: Vec<bex_heap::BexValue<'_>>,
) -> Result<PromptAst, OpErrorKind> {
    if args.len() != 3 {
        return Err(OpErrorKind::InvalidArgumentCount {
            expected: 3,
            actual: args.len(),
        });
    }

    let arg0 = args.remove(0);
    let arg1 = args.remove(0);
    let arg2 = args.remove(0);

    let (client, template, template_args) = heap.with_gc_protection(|protected| {
        let client = arg0
            .as_builtin_class::<builtin_types::PrimitiveClient>(&protected)?
            .into_owned(&protected)?;
        let template = arg1.as_string(&protected).cloned()?;
        let template_args = arg2
            .as_map(&protected)?
            .into_iter()
            .map(|(k, v)| Ok((k, v.as_owned_but_very_slow(&protected)?)))
            .collect::<Result<_, _>>()?;
        Ok::<_, bex_heap::AccessError>((client, template, template_args))
    })?;

    // Build render context from PrimitiveClient
    let render_ctx = llm_jinja::RenderContext {
        client: llm_jinja::RenderContextClient {
            name: client.name,
            provider: client.provider,
            default_role: client.default_role,
            allowed_roles: client.allowed_roles,
        },
        // TODO: output_format should come from somewhere (function return type?)
        output_format: llm_types::OutputFormatContent::new(bex_external_types::Ty::String),
        tags: indexmap::IndexMap::new(),
        // TODO: enums should be passed from the orchestrator or looked up from snapshot
        enums: std::collections::HashMap::new(),
    };

    // Call the Jinja runtime - RenderPromptError converts to OpErrorKind via From
    let prompt_ast = llm_jinja::render_prompt(template.as_str(), &template_args, &render_ctx)?;

    // Convert VM PromptAst to external PromptAst
    Ok(Arc::new(prompt_ast))
}
