function scale(budget: float, factor: float? = null) -> float throws never {
    let f = 1.0;
    if (factor != null) {
        f = factor
    }
    return budget * f;
}

function takes_int(n: int) -> int throws never {
    return n;
}

function main() -> int throws never {
    let g: reflect.AnyFunction<Returns = float, Throws = never> = scale;
    // The one boundary conversion: an integral value widens for a
    // `float` parameter (JSON Schema's `number` admits integers)...
    let widened = reflect.call_any(g, { "budget": 150 }) catch (e) {
        reflect.InvalidArgumentError => -1.0,
        reflect.errors.CompilationError => -100.0,
    };
    // ...and for a `float?` parameter.
    let opt = reflect.call_any(g, { "budget": 2, "factor": 3 }) catch (e) {
        reflect.InvalidArgumentError => -1.0,
        reflect.errors.CompilationError => -100.0,
    };
    // Nothing else converts: a float does not narrow to `int`, and a
    // numeric string does not parse to `float`.
    let h: reflect.AnyFunction<Returns = int, Throws = never> = takes_int;
    let narrowed = reflect.call_any(h, { "n": 1.5 }) catch (e) {
        reflect.InvalidArgumentError => -1,
        reflect.errors.CompilationError => -100,
    };
    let stringy = reflect.call_any(g, { "budget": "150" }) catch (e) {
        reflect.InvalidArgumentError => -2.0,
        reflect.errors.CompilationError => -100.0,
    };
    // Widening is lossless-only: 2^53 is the last exactly
    // representable integer and widens; 2^53 + 1 would silently
    // round, so it stays an InvalidArgumentError.
    let exact = reflect.call_any(g, { "budget": 9007199254740992 }) catch (e) {
        reflect.InvalidArgumentError => -1.0,
        reflect.errors.CompilationError => -100.0,
    };
    let lossy = reflect.call_any(g, { "budget": 9007199254740993 }) catch (e) {
        reflect.InvalidArgumentError => -3.0,
        reflect.errors.CompilationError => -100.0,
    };
    let ok = 0;
    if (widened == 150.0) {
        ok = ok + 1
    }
    if (opt == 6.0) {
        ok = ok + 1
    }
    if (narrowed == -1) {
        ok = ok + 1
    }
    if (stringy == -2.0) {
        ok = ok + 1
    }
    if (exact == 9007199254740992.0) {
        ok = ok + 1
    }
    if (lossy == -3.0) {
        ok = ok + 1
    }
    return ok;
}

test "call_any_widens_int_for_float_param" {
    assert.equal(main(), 6)
}
