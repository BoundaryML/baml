test "trace defaults and factories" {
    assert.equal(
        trace.options().inspect(),
        trace.Settings {
            mode: null,
            capture: trace.Capture { inputs: null, output: null, error: null },
        },
    );
    assert.equal(trace.hidden().inspect().mode, trace.Mode.Hidden);
    assert.equal(trace.timing().inspect().mode, trace.Mode.Timing);
    assert.equal(trace.options(mode = trace.Mode.Span).inspect().mode, trace.Mode.Span);
    assert.equal(
        trace.span().inspect(),
        trace.Settings {
            mode: trace.Mode.Span,
            capture: trace.Capture { inputs: null, output: null, error: null },
        },
    );
}

function BuildersAreImmutable() -> bool {
    let base = trace.span(inputs = true, output = false, error = true);
    let hidden = base.mode(trace.Mode.Hidden);
    let automatic = hidden.mode(null);
    let replacement = base.span(error = false);
    base.mode(trace.Mode.Timing);
    base.inspect()
        == trace.Settings {
            mode: trace.Mode.Span,
            capture: trace.Capture { inputs: true, output: false, error: true },
        }
        && hidden.inspect().mode == trace.Mode.Hidden
        && automatic.inspect().mode == null
        && automatic.inspect().capture == base.inspect().capture
        && replacement.inspect()
            == trace.Settings {
                mode: trace.Mode.Span,
                capture: trace.Capture { inputs: null, output: null, error: false },
            }
}

test "trace builders are immutable and mode preserves capture" {
    assert.is_true(BuildersAreImmutable());
}

function InspectionIsDetached() -> bool {
    let options = trace.span(inputs = true, error = false);
    let reserved = options.reserve();
    let options_view = options.inspect();
    let reserved_view = reserved.inspect();
    options_view.mode = trace.Mode.Hidden;
    options_view.capture.inputs = false;
    reserved_view.capture.error = true;
    options.inspect()
        == trace.Settings {
            mode: trace.Mode.Span,
            capture: trace.Capture { inputs: true, output: null, error: false },
        }
        && reserved.inspect() == options.inspect()
}

test "trace inspection is detached" {
    assert.is_true(InspectionIsDetached());
}

function ReservationSnapshotsOptions() -> bool {
    let options = trace.span(output = true).mode(trace.Mode.Hidden);
    let reserved = options.reserve();
    options.inspect().mode == trace.Mode.Hidden
        && reserved.inspect()
            == trace.Settings {
                mode: trace.Mode.Span,
                capture: trace.Capture { inputs: null, output: true, error: null },
            }
}

test "trace reserve selects span without mutating options" {
    assert.is_true(ReservationSnapshotsOptions());
}

function SpanIdentityEquality() -> bool {
    let reservation = trace.options().reserve();
    let first = reservation.id();
    let second = reservation.id();
    let copied = baml.deep_copy(first);
    let other = trace.options().reserve().id();
    first == second
        && first == copied
        && !(first != second)
        && first != other
        && !(first == other)
        && [first] == [second]
}

test "trace span ids compare by identity not wrapper" {
    assert.is_true(SpanIdentityEquality());
}

function DefaultCallerHasNoSpan() -> bool {
    trace.current_span_id() == null
}

test "trace current span does not create an identity" {
    assert.is_true(DefaultCallerHasNoSpan());
}
