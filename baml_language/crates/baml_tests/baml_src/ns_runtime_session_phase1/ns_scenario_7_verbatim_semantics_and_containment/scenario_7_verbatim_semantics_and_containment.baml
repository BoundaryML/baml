function host_dispatch_session_value(value: baml.ToString) -> string throws never {
    value.to_string()
}

function scenario_app() -> reflect.Package {
    reflect.Package.compile(
        {
            "app.baml": `
                class ValidationError { message string }
                function ImprovePost(text: string) -> string { text + " polished" }
                function LoadNotes() -> string { "raw notes" }
                function SaveDraft(title: string, body: string) -> string {
                  log.info("SAVE:" + title + ":" + body)
                  "draft-1"
                }
                function ValidateOrThrow(title: string, body: string) -> null throws ValidationError {
                  throw ValidationError { message: title + ":" + body }
                }
                function Validate(n: int) -> int throws ValidationError {
                  if (n < 0) { throw ValidationError { message: "negative" } }
                  n
                }
            `,
        },
    )
}

function scenario_7() -> bool {
    let s = reflect.Session.new(packages = { "app": scenario_app() });

    // Submission 1: declarations hoist and execute nothing.
    s.eval(
        `
            class Draft {
              title: string,
              body: string,
            }

            function Polish(d: Draft) -> Draft {
              Draft { title: d.title, body: app.ImprovePost(d.body) }
            }
        `,
    );

    // Submission 2: the binding becomes visible only after its initializer.
    s.eval(`let draft = Draft { title: "Eval in BAML", body: app.LoadNotes() }`);
    let title = s.eval<string>(`Polish(draft).title`);

    // Containment is a committed prefix, not rollback.
    let _ = s.eval(
        `
            let saved = app.SaveDraft(draft.title, draft.body)
            app.ValidateOrThrow(draft.title, draft.body)
            let approved = true
        `,
    ) catch (e) {
        reflect.errors.EvaluationError => null,
        _ => throw e,
    };
    let saved = s.eval<string>(`saved`);
    let approved_missing = s.eval<bool>(`approved`) catch (_) {
        reflect.errors.CompilationError => true,
        _ => false,
    };

    let _ = s.eval(
        `
            let x = 10
            let checked = app.Validate(-1)
            let y = "hi"
        `,
    ) catch (e) {
        reflect.errors.EvaluationError => null,
        _ => throw e,
    };
    let x = s.eval<int>(`x`);
    let y_missing = (s.eval<string>(`y`) == "") catch (_) {
        reflect.errors.CompilationError => true,
        _ => false,
    };

    // Assignment updates the old cell; shadowing allocates a new one.
    s.eval(`let greeting = "hello"`);
    s.eval(``let shout = (name: string) -> { `\${greeting}, \${name}!` }``);
    s.eval(`greeting = "howdy"`);
    let a = s.eval<string>(`shout("Ada")`);
    s.eval(`let greeting = "goodbye"`);
    let b = s.eval<string>(`shout("Ada")`);

    // A failed compile never poisons the Session.
    let compile_failed = false;
    let _ = s.eval(`let broken: MissingType = null`) catch (_) {
        reflect.errors.CompilationError => {
            compile_failed = true
        },
        _ => null,
    };
    let continued = s.eval<int>(`x + 1`);

    title == "Eval in BAML"
        && saved == "draft-1"
        && approved_missing
        && x == 10
        && y_missing
        && a == "howdy, Ada!"
        && b == "howdy, Ada!"
        && compile_failed
        && continued == 11
}

function diagnostic_submission_name() -> string {
    let s = reflect.Session.new();
    let _ = s.eval(`let bad: MissingType = null`) catch (e) {
        reflect.errors.CompilationError => {
            let span = e.diagnostics[0].span ?? throw "missing diagnostic span";
            return span.file ?? "";
        },
        _ => {
            return "wrong error";
        },
    };
    "unexpected success"
}

function package_current_is_rejected() -> bool {
    let s = reflect.Session.new();
    let _ = s.eval(`reflect.Package.current()`) catch (_) {
        reflect.errors.CompilationError => {
            return true;
        },
        _ => {
            return false;
        },
    };
    false
}

function runtime_and_failed_contracts() -> bool {
    let s = reflect.Session.new();
    let string_t = reflect.Type.of<string>();
    type StringT = unreflect(string_t)
    let value = s.eval<StringT>(`"ok"`);
    let rejected = (
        s.eval<string>(
            `
                let should_not_exist = 7
                42
            `,
        )
            == ""
    ) catch (_) {
        reflect.errors.CompilationError => true,
        _ => false,
    };
    let missing = (s.eval<int>(`should_not_exist`) == 0) catch (_) {
        reflect.errors.CompilationError => true,
        _ => false,
    };
    reflect.Type.of_value(value) == string_t && rejected && missing
}

function declaration_redefinition_keeps_earlier_resolution() -> bool {
    let s = reflect.Session.new();
    s.eval(`function Current() -> int { 1 }`);
    s.eval(`let old = () -> { Current() }`);
    s.eval(`function Current() -> int { 2 }`);
    s.eval<int>(`old()`) == 1 && s.eval<int>(`Current()`) == 2
}

function client_declaration_is_lazy() -> bool {
    let s = reflect.Session.new();
    s.eval(
        `
                client NeverContacted = openai.ResponsesClient.new(
                model = "no-network-during-declaration",
                api_key = "unused",
                base_url = "http://127.0.0.1:1",
            );
        `,
    );
    s.eval<int>(`1`) == 1
}

function runtime_type_binding_persists() -> bool {
    let s = reflect.Session.new();
    let first = s.eval<bool>(
        `
            type T = unreflect(reflect.Type.of<string>());
            reflect.Type.of<T>() == reflect.Type.of<string>()
        `,
    );
    let later = s.eval<bool>(`reflect.Type.of<T>() == reflect.Type.of<string>()`);
    s.eval(`type T = unreflect(reflect.Type.of<int>());`);
    let rebound = s.eval<bool>(`reflect.Type.of<T>() == reflect.Type.of<int>()`);
    first && later && rebound
}

// A step whose value is typed by a session binding publishes as `unknown`:
// the binding lives only inside the step's own block, and later submissions
// read the value through a global. Narrowing recovers it.
function binding_typed_step_value_leaves_as_unknown() -> bool {
    let s = reflect.Session.new();
    s.eval(`type T = unreflect(reflect.Type.of<int>());`);
    s.eval(`let v = match (1) { let x: T => x, _ => 0 }`);
    let narrowed = s.eval<int>(`match (v) { let n: int => n, _ => -1 }`);
    let unrelated = s.eval<string>(`"plain"`);
    narrowed == 1 && unrelated == "plain"
}

function session_declarations_are_generative() -> bool {
    let left = reflect.Session.new();
    let right = reflect.Session.new();
    left.eval(`class SameName { value string }`);
    right.eval(`class SameName { value string }`);
    let left_type = left.eval<reflect.Type>(`reflect.Type.of<SameName>()`);
    let right_type = right.eval<reflect.Type>(`reflect.Type.of<SameName>()`);
    left_type != right_type
}

function host_dispatch_recovers_session_class_provenance() -> bool {
    let s = reflect.Session.new();
    s.eval(
        `
            class SessionValue {
              value string
              implements baml.ToString {
                function to_string(self) -> string throws never {
                  self.value
                }
              }
            }
        `,
    );
    let value = s.eval<baml.ToString>(`SessionValue { value: "from session" }`);
    host_dispatch_session_value(value) == "from session"
}

test "scenario_7_verbatim_semantics_and_containment" {
    assert.equal(scenario_7(), true)
}

test "compilation_diagnostics_use_stable_submission_names" {
    assert.equal(diagnostic_submission_name(), "$submission_0.baml")
}

test "package_current_is_not_available_in_session_source" {
    assert.equal(package_current_is_rejected(), true)
}

test "runtime_contracts_check_before_execution" {
    assert.equal(runtime_and_failed_contracts(), true)
}

test "declaration_redefinition_is_newest_wins_without_relinking_old_code" {
    assert.equal(declaration_redefinition_keeps_earlier_resolution(), true)
}

test "session_client_declarations_do_not_perform_network_io" {
    assert.equal(client_declaration_is_lazy(), true)
}

test "binding_typed_session_step_values_publish_as_unknown" {
    assert.equal(binding_typed_step_value_leaves_as_unknown(), true)
}

test "scoped_runtime_type_bindings_persist_and_rebind_between_submissions" {
    assert.equal(runtime_type_binding_persists(), true)
}

test "identical_declarations_in_two_sessions_are_distinct_types" {
    assert.equal(session_declarations_are_generative(), true)
}

test "host_interface_dispatch_uses_session_class_provenance" {
    assert.equal(host_dispatch_recovers_session_class_provenance(), true)
}
