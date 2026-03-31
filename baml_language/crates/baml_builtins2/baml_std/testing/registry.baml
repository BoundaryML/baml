// testing.Registry — the root collector for all tests and testsets.
// The runtime creates a Registry, calls $init_test(registry), then reads
// registry.tests and registry.testsets.

class TestRegistration {
  name string
  body () -> null
  runner TestRunner?
}

class TestSetRegistration {
  name string
  collector (TestSetCollector) -> null
  runner TestSetRunner?
}

class Registry {
  tests TestRegistration[]
  testsets TestSetRegistration[]

  //baml:mut_self
  function register_test(self, name: string, body: () -> null, runner: TestRunner?) -> null {
    $rust_function
  }

  //baml:mut_self
  function register_test_set(self, name: string, collector: (TestSetCollector) -> null, runner: TestSetRunner?) -> null {
    $rust_function
  }
}

class TestSetCollector {
  tests TestRegistration[]
  testsets TestSetRegistration[]

  //baml:mut_self
  function register_test(self, name: string, body: () -> null, runner: TestRunner?) -> null {
    $rust_function
  }

  //baml:mut_self
  function register_test_set(self, name: string, collector: (TestSetCollector) -> null, runner: TestSetRunner?) -> null {
    $rust_function
  }
}
