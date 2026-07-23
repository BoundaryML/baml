// Media remains structural in PromptAst and Request.messages(). Providers decide
// their own wire encoding and must reject unsupported media rather than drop it.
class s05_Caption {
  text: string,
  tags: string[],
}

function s05_image_prompt_template(
  instruction: string,
  img: image,
) -> (baml.llm.Context) -> baml.llm.PromptAst throws never {
  (ctx: baml.llm.Context) -> baml.llm.PromptAst {
    baml.llm.assemble_prompt_ast(
      ["", " ", " ", ""],
      [instruction, img, ctx.output_format],
    )
  }
}

function s05_pdf_prompt_template(
  instruction: string,
  document: pdf,
) -> (baml.llm.Context) -> baml.llm.PromptAst throws never {
  (ctx: baml.llm.Context) -> baml.llm.PromptAst {
    baml.llm.assemble_prompt_ast(
      ["", " ", ""],
      [instruction, document],
    )
  }
}
