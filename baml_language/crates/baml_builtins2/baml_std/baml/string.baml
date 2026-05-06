class String {
  //baml:mut_vm
  function to_json(self) -> baml.json.json {
    $rust_function
  }

  function length(self) -> int {
    $rust_function
  }

  function to_lower_case(self) -> string {
    $rust_function
  }

  function to_upper_case(self) -> string {
    $rust_function
  }

  function trim(self) -> string {
    $rust_function
  }

  function includes(self, search: string) -> bool {
    $rust_function
  }

  function starts_with(self, prefix: string) -> bool {
    $rust_function
  }

  function ends_with(self, suffix: string) -> bool {
    $rust_function
  }

  function split(self, delimiter: string) -> string[] {
    $rust_function
  }

  function substring(self, start: int, end: int) -> string {
    $rust_function
  }

  function replace(self, search: string, replacement: string) -> string {
    $rust_function
  }

  function index_of(self, search: string) -> int {
    $rust_function
  }

  function char_at(self, index: int) -> string {
    $rust_function
  }

  function matches(self, pattern: string) -> bool {
    $rust_function
  }

  function replace_all(self, search: string, replacement: string) -> string {
    $rust_function
  }

  /// Encodes the string as UTF-8 bytes.
  function to_bytes(self) -> uint8array {
    $rust_function
  }
}
