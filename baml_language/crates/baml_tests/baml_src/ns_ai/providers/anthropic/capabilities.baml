implements Capabilities for Anthropic {
  function structured_output(self) -> Support throws never { Support.Conditional }
  function image_input(self) -> Support throws never { Support.Yes }
  function pdf_input(self) -> Support throws never { Support.Yes }
  function parallel_tools(self) -> Support throws never { Support.Yes }
  function max_input_tokens(self) -> int throws never { 200000 }
}
