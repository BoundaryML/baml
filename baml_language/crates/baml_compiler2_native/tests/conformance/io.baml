// Natives that wait for I/O: the task is parked while a thread of the
// runtime's own does the work, and other tasks run meanwhile. The paths
// are relative to the directory the tests run in, which is this crate's.

function io_env_set() -> bool {
  baml.env.get(rep("PA", 1) + "TH") != null
}
function io_env_unset() -> bool {
  baml.env.get(rep("BAML_NO_SUCH_VARIABLE_", 2)) == null
}
function io_file_read() -> string {
  let text = baml.fs.read(rep("tests/conformance/", 1) + "support.baml");
  text.slice(0, 9)
}
function io_file_missing() -> string {
  let text = baml.fs.read(rep("/no/such/directory", 1) + "/file") catch (e) {
    baml.errors.ParseError => "parse",
    baml.errors.Io => e.message,
  };
  text
}
function io_file_missing_uncaught() -> int {
  baml.fs.read(rep("/no/such/directory", 1) + "/file").length()
}
// Three tasks wait on files at once while a fourth allocates: each goes
// on with what it held before it waited.
function io_files_read_by_tasks() -> string {
  let name = rep("tests/conformance/", 1);
  let a = spawn { let kept = rep("a", 20); kept.length() + baml.fs.read(name + "support.baml").length() * 0 };
  let b = spawn { let kept = task_ints(30); baml.fs.read(name + "tasks.baml").slice(0, 2) + `${kept[29]}` };
  let c = spawn { baml.env.get(rep("PA", 1) + "TH") != null };
  let busy = spawn { task_churn(80) };
  let n = await busy;
  `${await a} ${await b} ${await c} ${n}`
}
// A `to_json` that waits for a file, inside `baml.json`: a native that
// stopped for compiled code, which stopped for I/O.
class IoReadsAFile {
  path: string
  implements baml.ToJson {
    function to_json(self) -> baml.json.json throws baml.json.SerializationError {
      let text = baml.fs.read(self.path) catch (e) {
        _ => { throw baml.json.SerializationError { message: "unreadable", path: self.path, reason: "io" } },
      };
      baml.json.from(text.slice(0, 2))
    }
  }
}
function io_inside_a_to_json() -> string {
  let name = rep("tests/conformance/", 1);
  baml.json.stringify([IoReadsAFile { path: name + "support.baml" }, IoReadsAFile { path: name + "tasks.baml" }])
}
