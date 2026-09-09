// Probe BAML's in-runtime reference and field-aliasing semantics. ResultRecord
// deliberately has the same relevant shape as a generic RunResult<T> field.
class Counter {
    value: int,
}

class Holder {
    first: Counter,
    second: Counter,
}

class ResultRecord<T> {
    value: T,
}

class ProbeJournal {
    entries: int[],

    function append(self, value: int) -> void throws never {
        self.entries.push(value)
    }
}

class MixedResultRecord<T> {
    value: T,
    journal: ProbeJournal,
    usage: int,
}

class CallbackEnvelope {
    callback: () -> int throws never,
}

class IdentityObservation {
    after_local_alias_write: int,
    after_return_alias_write: int,
    holder_second_after_first_write: int,
    result_field_after_extracted_write: int,
    original_payload_after_result_alias_write: int,
    detached_old_payload_after_field_replacement: int,
    replacement_payload_value: int,
    original_journal_after_outer_copy_append: int[],
    original_journal_after_copy_field_replacement: int[],
    replacement_journal: int[],
    callback_first_copy_call: int,
    callback_second_copy_call: int,
}

function return_same(value: Counter) -> Counter throws never {
    value
}

function return_same_result<T>(result: ResultRecord<T>) -> ResultRecord<T> throws never {
    result
}

function copy_mixed_outer<T>(result: MixedResultRecord<T>) -> MixedResultRecord<T> throws never {
    MixedResultRecord<T> { value: result.value, journal: result.journal, usage: result.usage }
}

function copy_callback_outer(envelope: CallbackEnvelope) -> CallbackEnvelope throws never {
    CallbackEnvelope { callback: envelope.callback }
}

function observe_identity_and_result_fields() -> IdentityObservation throws never {
    let original = Counter { value: 1 };
    let local_alias = original;
    local_alias.value = 2;
    let after_local_alias_write = original.value;

    let returned_alias = return_same(original);
    returned_alias.value = 3;
    let after_return_alias_write = original.value;

    let holder = Holder { first: original, second: original };
    holder.first.value = 4;
    let holder_second_after_first_write = holder.second.value;

    let payload = Counter { value: 5 };
    let result = ResultRecord<Counter> { value: payload };
    let extracted = result.value;
    extracted.value = 6;
    let result_field_after_extracted_write = result.value.value;

    let result_alias = return_same_result(result);
    result_alias.value.value = 7;
    let original_payload_after_result_alias_write = payload.value;

    result.value = Counter { value: 8 };

    // Construct a distinct outer record while intentionally retaining the same
    // nested journal object. This is the mixed-envelope projection proposed for
    // RunResult: copied outer fields can still carry live child identity.
    let journal = ProbeJournal { entries: [1] };
    let mixed = MixedResultRecord<string> { value: "done", journal: journal, usage: 9 };
    let mixed_outer_copy = copy_mixed_outer(mixed);
    mixed_outer_copy.journal.append(2);
    let original_journal_after_outer_copy_append = mixed.journal.entries;
    mixed_outer_copy.journal = ProbeJournal { entries: [99] };
    mixed_outer_copy.journal.append(100);

    // The same construction works for callable children: the envelope is new,
    // while both fields still call one closure over one mutable captured object.
    let callback_state = Counter { value: 0 };
    let callback = () -> int throws never {
        callback_state.value += 1;
        callback_state.value
    };
    let callback_envelope = CallbackEnvelope { callback: callback };
    let callback_outer_copy = copy_callback_outer(callback_envelope);
    let callback_first_copy_call = callback_envelope.callback();
    let callback_second_copy_call = callback_outer_copy.callback();

    IdentityObservation {
        after_local_alias_write: after_local_alias_write,
        after_return_alias_write: after_return_alias_write,
        holder_second_after_first_write: holder_second_after_first_write,
        result_field_after_extracted_write: result_field_after_extracted_write,
        original_payload_after_result_alias_write: original_payload_after_result_alias_write,
        detached_old_payload_after_field_replacement: extracted.value,
        replacement_payload_value: result.value.value,
        original_journal_after_outer_copy_append: original_journal_after_outer_copy_append,
        original_journal_after_copy_field_replacement: mixed.journal.entries,
        replacement_journal: mixed_outer_copy.journal.entries,
        callback_first_copy_call: callback_first_copy_call,
        callback_second_copy_call: callback_second_copy_call,
    }
}

test "classes and result fields preserve aliases inside BAML" {
    let observed = observe_identity_and_result_fields();
    assert.equal(observed.after_local_alias_write, 2);
    assert.equal(observed.after_return_alias_write, 3);
    assert.equal(observed.holder_second_after_first_write, 4);
    assert.equal(observed.result_field_after_extracted_write, 6);
    assert.equal(observed.original_payload_after_result_alias_write, 7);
    assert.equal(observed.detached_old_payload_after_field_replacement, 7);
    assert.equal(observed.replacement_payload_value, 8);
    assert.equal(observed.original_journal_after_outer_copy_append, [1, 2]);
    assert.equal(observed.original_journal_after_copy_field_replacement, [1, 2]);
    assert.equal(observed.replacement_journal, [99, 100]);
    assert.equal(observed.callback_first_copy_call, 1);
    assert.equal(observed.callback_second_copy_call, 2)
}
