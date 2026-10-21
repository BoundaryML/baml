// Regression: a `Call`/`VirtualCall` terminator's destination must be a
// `Place::Local`. The emitter materializes a terminator-call result with
// `emit_store_place`, which only handles locals — a `Place::Field`/`Index`/
// `Capture` destination hits `unreachable!`. MIR lowering must therefore route
// the result through a temp and copy it into the projection after the call's
// resume block.
//
// Converted from crates/bex_vm/tests/projection_dest_calls.rs, where each case
// compiled a source string and ran a no-arg `bool` function through the real VM.
// Those entry points survive as top-level helpers, so each assertion is
// `assert.is_true(<helper>())`: a test-block local is boxed (VM quirk), so
// nothing is bound in a test block here.
//
// The corpus is one project, so every class, interface and function carries a
// `Pdc` / `pdc_` prefix to stay clear of the other namespaces.

// ─── Driver-call result written into a field ────────────────────────────────
// `==`/`!=` on two class instances lowers to the `baml.ops.equals_equals` driver
// (a block-splitting `Call`). Assigning the result to a field gives that call a
// `Place::Field` destination, which must be normalized through a temp. The `==`
// (non-negated) path is the regression: it previously wrote straight into the
// projection. The `!=` path always used a temp, so it is the control.

class PdcTag {
    id: int
}

implement baml.ops.Equals for PdcTag {
    function eq(self, other: Self) -> bool throws never { self.id == other.id }
}

class PdcBoolBox {
    flag: bool
}

function pdc_tag(id: int) -> PdcTag { PdcTag { id: id } }

function pdc_eq_into_field_true() -> bool {
    let b = PdcBoolBox { flag: false };
    b.flag = (pdc_tag(1) == pdc_tag(1));
    b.flag
}

function pdc_eq_into_field_false() -> bool {
    let b = PdcBoolBox { flag: true };
    b.flag = (pdc_tag(1) == pdc_tag(2));
    b.flag
}

function pdc_ne_into_field_true() -> bool {
    let b = PdcBoolBox { flag: false };
    b.flag = (pdc_tag(1) != pdc_tag(2));
    b.flag
}

test "equality_result_assigned_to_field" {
    assert.is_true(pdc_eq_into_field_true())
    assert.is_true(!pdc_eq_into_field_false())
    assert.is_true(pdc_ne_into_field_true())
}

// ─── Virtual-call result written into a field ───────────────────────────────
// A virtual interface-method call on an existential receiver lowers to a
// `VirtualCall` terminator. Assigning its result to a field gives it a
// `Place::Field` destination — the same normalization requirement as a plain
// call. Two implementors keep the dispatch genuinely virtual.

interface PdcFlag {
    function flag(self) -> bool throws never
}

class PdcYes {
    implements PdcFlag {
        function flag(self) -> bool { true }
    }
}

class PdcNo {
    implements PdcFlag {
        function flag(self) -> bool { false }
    }
}

function pdc_flag_into_field_true() -> bool {
    let f: PdcFlag = PdcYes {};
    let b = PdcBoolBox { flag: false };
    b.flag = f.flag();
    b.flag
}

function pdc_flag_into_field_false() -> bool {
    let f: PdcFlag = PdcNo {};
    let b = PdcBoolBox { flag: true };
    b.flag = f.flag();
    b.flag
}

test "virtual_call_result_assigned_to_field" {
    assert.is_true(pdc_flag_into_field_true())
    assert.is_true(!pdc_flag_into_field_false())
}

// ─── Call expression as the base of a nested field assignment ───────────────
// The base of a projection is itself allowed to be a call expression. MIR must
// evaluate that call into a local before appending field projections.

class PdcInfo {
    title: string?
}

class PdcRecord {
    info: PdcInfo
}

class PdcStore {
    record: PdcRecord
    calls: int

    function require(self) -> PdcRecord {
        self.calls += 1;
        self.record
    }
}

function pdc_call_base_nested_assignment() -> bool {
    let store = PdcStore {
        record: PdcRecord { info: PdcInfo { title: null } },
        calls: 0,
    };
    store.require().info.title = "triage";
    store.record.info.title == "triage" && store.calls == 1
}

test "method_call_result_can_be_the_base_of_nested_field_assignment" {
    assert.is_true(pdc_call_base_nested_assignment())
}
