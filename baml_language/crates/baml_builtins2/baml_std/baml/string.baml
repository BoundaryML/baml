class String {
  function length(self) -> int {
    $rust_function
  }

  function toLowerCase(self) -> string {
    $rust_function
  }

  function toUpperCase(self) -> string {
    $rust_function
  }

  function trim(self) -> string {
    $rust_function
  }

  function includes(self, search: string) -> bool {
    $rust_function
  }

  function startsWith(self, prefix: string) -> bool {
    $rust_function
  }

  function endsWith(self, suffix: string) -> bool {
    $rust_function
  }

  @@internal.uses(vm)
  function split(self, delimiter: string) -> string[] {
    $rust_function
  }

  function substring(self, start: int, end: int) -> string {
    $rust_function
  }

  function replace(self, search: string, replacement: string) -> string {
    $rust_function
  }

  function indexOf(self, search: string) -> int {
    $rust_function
  }

  function charAt(self, index: int) -> string {
    $rust_function
  }

  function matches(self, pattern: string) -> bool {
    $rust_function
  }

  function replaceAll(self, search: string, replacement: string) -> string {
    $rust_function
  }
}
