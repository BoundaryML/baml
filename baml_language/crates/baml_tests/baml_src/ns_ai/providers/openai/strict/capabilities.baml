implements Capabilities for OpenAiStrict {
  function structured_output(self) -> Support throws never { Support.Yes }
  function image_input(self) -> Support throws never { Support.Conditional }
  function pdf_input(self) -> Support throws never { Support.Conditional }
  function image_output(self) -> Support throws never { Support.No }
  function parallel_tools(self) -> Support throws never { Support.Yes }
  function max_input_tokens(self) -> int throws never { 128000 }
}
