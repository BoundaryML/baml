implements Capabilities for AiGatewayImages {
  function structured_output(self) -> Support throws never { Support.No }
  function image_input(self) -> Support throws never { Support.No }
  function pdf_input(self) -> Support throws never { Support.No }
  function image_output(self) -> Support throws never { Support.Yes }
  function parallel_tools(self) -> Support throws never { Support.No }
  function max_input_tokens(self) -> int throws never { 0 }
}
