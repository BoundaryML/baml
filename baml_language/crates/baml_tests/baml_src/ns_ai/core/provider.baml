// BEP-064 v2 reference implementation, intentionally local to baml_tests.
// Scenario code uses root.ai exclusively; compiler desugaring is deferred, so
// task_request is the hand-written companion seam.

interface Provider {
  function display_name(self) -> string throws never { "provider" }
  function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
  // True when blindly repeating Generate may duplicate externally visible
  // work. Reliability wrappers refuse effectful providers by default.
  function is_effectful(self) -> bool throws never { false }
  function with_retry(self, max: int) -> Retry throws never {
    Retry { inner: self, max: max }
  }
  function fallback_to(self, other: Provider) -> Fallback throws never {
    Fallback { members: [self, other] }
  }
  function traced(self, meter: UsageMeter) -> Traced throws never {
    Traced { inner: self, meter: meter }
  }
}
