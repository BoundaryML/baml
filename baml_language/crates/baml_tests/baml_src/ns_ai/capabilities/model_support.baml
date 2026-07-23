// Declarative model/endpoint support. These values describe support; typed
// capability dispatch still happens through the dedicated capability interfaces.

enum Support {
  Yes
  No
  Conditional
}

interface Capabilities requires Provider {
  function structured_output(self) -> Support throws never
  function image_input(self) -> Support throws never
  function pdf_input(self) -> Support throws never { Support.No }
  function image_output(self) -> Support throws never { Support.No }
  function parallel_tools(self) -> Support throws never
  /// 0 means the endpoint does not publish a token-based input limit.
  function max_input_tokens(self) -> int throws never
}
