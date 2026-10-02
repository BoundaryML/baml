function worker_startup_span_identity() -> bool {
  let reserved = trace.span().reserve();
  let id = reserved.id();
  id == reserved.id() && id != trace.span().reserve().id()
}
