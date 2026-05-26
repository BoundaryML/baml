// host_callable.test.ts — mirrors sdks/python/tests/test_host_callable.py.
//
// Exercises the JS host-callable bridge: encoder auto-registration in
// proto.ts's `setInboundValue`, the C ABI round-trip via
// `bridge_cffi::complete_host_call`, the ThreadsafeFunction dispatch, and
// the JS-side decode/invoke/encode wrapper. No generated SDK — the tests
// build an in-memory BAML runtime with a function that takes a callable
// of the relevant signature.
//
// Why we never call `callFunctionSync` here: that path blocks the Node
// main thread on a tokio `block_on`, while the host-value dispatch tries
// to schedule the user callback onto the libuv event loop via
// ThreadsafeFunction. The main thread is blocked → libuv can't run → the
// callback never fires → deadlock. The async `callFunction` path leaves
// the event loop free, so the tsfn dispatch completes promptly. Python's
// bridge sidesteps this by running async callables on a fresh asyncio
// loop in the dispatch thread (different I/O architecture).
//
// Why the suite runs jest with `forceExit` (see package.json): each
// registered callable holds a strong (`weak::<false>`) ThreadsafeFunction
// ref that pins the libuv loop until the engine releases it, and release
// is GC/drain-driven (`host_release_callback`). A runtime dropped at the
// end of a test isn't guaranteed to have collected its `HostClosure`
// before the process exits, so the ref can outlive the test and keep jest
// from exiting on its own. `forceExit` lets jest terminate once the tests
// themselves have completed.

import { BamlRuntime } from '../native';
import { callFunction, callFunctionSync } from '../index';
import { encodeCallArgs } from '../proto';

const CALLBACK_BAML = `
function CallCb(callback: (int) -> string, x: int) -> string {
    callback(x)
}

function CallIntCb(callback: (int) -> int, x: int) -> int {
    callback(x)
}
`;

function makeRuntime(): BamlRuntime {
    return BamlRuntime.fromFiles('.', { 'main.baml': CALLBACK_BAML });
}

describe('host-callable round-trip', () => {
    test('plain function callback returns a string', async () => {
        const rt = makeRuntime();
        const cb = (x: number) => `got ${x}`;
        const result = await callFunction(rt, 'CallCb', { callback: cb, x: 5 });
        expect(result.result()).toBe('got 5');
    });

    test('int-returning callback', async () => {
        const rt = makeRuntime();
        const cb = (x: number) => x + 1;
        const result = await callFunction(rt, 'CallIntCb', { callback: cb, x: 41 });
        expect(result.result()).toBe(42);
    });

    test('arrow function (lambda) callback', async () => {
        const rt = makeRuntime();
        const result = await callFunction(rt, 'CallCb', {
            callback: (x: number) => `arrow-${x}`,
            x: 12,
        });
        expect(result.result()).toBe('arrow-12');
    });

    test('multiple callables produce distinct registry entries', async () => {
        const rt = makeRuntime();
        const seen: Record<string, number> = { a: 0, b: 0 };
        const cbA = (x: number) => {
            seen.a += 1;
            return `a:${x}`;
        };
        const cbB = (x: number) => {
            seen.b += 1;
            return `b:${x}`;
        };
        const ra = await callFunction(rt, 'CallCb', { callback: cbA, x: 1 });
        expect(ra.result()).toBe('a:1');
        const rb = await callFunction(rt, 'CallCb', { callback: cbB, x: 2 });
        expect(rb.result()).toBe('b:2');
        expect(seen).toEqual({ a: 1, b: 1 });
    });
});

describe('host-callable error surfacing', () => {
    test('throwing callback surfaces as a BAML error containing the message', async () => {
        const rt = makeRuntime();
        const cb = (_x: number): string => {
            throw new Error('oops');
        };
        await expect(
            callFunction(rt, 'CallCb', { callback: cb, x: 1 })
        ).rejects.toThrow(/oops|Error/);
    });

    test('TypeError surfaces with the message', async () => {
        const rt = makeRuntime();
        const cb = (_x: number): string => {
            throw new TypeError('bad');
        };
        await expect(
            callFunction(rt, 'CallCb', { callback: cb, x: 2 })
        ).rejects.toThrow(/bad|TypeError/);
    });
});

describe('host-callable async (Promise) callbacks', () => {
    test('async function returning a string resolves through the dispatch path', async () => {
        const rt = makeRuntime();
        const cb = async (x: number): Promise<string> => {
            await new Promise<void>((resolve) => setImmediate(resolve));
            return `async-${x}`;
        };
        const result = await callFunction(rt, 'CallCb', { callback: cb, x: 4 });
        expect(result.result()).toBe('async-4');
    });

    test('callback returning a manually-resolved Promise also works', async () => {
        const rt = makeRuntime();
        const cb = (x: number): Promise<string> =>
            new Promise((resolve) => setImmediate(() => resolve(`promise-${x}`)));
        const result = await callFunction(rt, 'CallCb', { callback: cb, x: 7 });
        expect(result.result()).toBe('promise-7');
    });

    test('rejected Promise surfaces as a BAML error', async () => {
        const rt = makeRuntime();
        const cb = (_x: number): Promise<string> =>
            Promise.reject(new RangeError('out of range'));
        await expect(
            callFunction(rt, 'CallCb', { callback: cb, x: 9 })
        ).rejects.toThrow(/out of range|RangeError/);
    });
});

describe('host-callable sync-call guard', () => {
    test('a host callable on the sync path fast-fails instead of hanging', () => {
        const rt = makeRuntime();
        const cb = (x: number) => `got ${x}`;
        // Must throw synchronously (no await) — the whole point is that we
        // never reach the blocking native call, so this can never hang.
        expect(() => {
            callFunctionSync(rt, 'CallCb', { callback: cb, x: 5 });
        }).toThrow(/host callable/i);
    });

    test('the sync-guard error names the async API', () => {
        const rt = makeRuntime();
        let caught: unknown;
        try {
            callFunctionSync(rt, 'CallCb', { callback: (x: number) => `${x}`, x: 1 });
        } catch (err) {
            caught = err;
        }
        expect(caught).toBeInstanceOf(Error);
        expect((caught as Error).message).toMatch(/async/i);
    });

    test('the sync guard only fires for function args, not scalars', () => {
        // Encoding scalar-only kwargs in sync mode must succeed; the guard
        // only trips on a JS `function`. (We avoid encoding a function on
        // the async path here: that would register a `weak::<false>` tsfn
        // which pins the libuv loop and could keep jest from exiting.)
        expect(() => encodeCallArgs({ x: 1, s: 'ok' }, /* syncMode */ true)).not.toThrow();
        // A function in sync mode trips the guard before any registration.
        expect(() => encodeCallArgs({ cb: () => 0 }, /* syncMode */ true)).toThrow(/host callable/i);
    });
});

describe('host-callable always completes on abnormal paths', () => {
    test('a callback returning an unencodable value surfaces as an error, not a hang', async () => {
        const rt = makeRuntime();
        // A BigInt is not encodable by setInboundValue → the result-encode
        // path throws → must be reported as an error (and the call must
        // complete), not hang.
        const cb = (_x: number): unknown => 10n;
        await expect(
            callFunction(rt, 'CallCb', { callback: cb, x: 1 })
        ).rejects.toThrow();
    });

    test('a callback throwing a value with a hostile toString still completes', async () => {
        const rt = makeRuntime();
        const hostile = {
            get message(): string {
                throw new Error('cannot read message');
            },
            toString(): string {
                throw new Error('cannot stringify');
            },
        };
        const cb = (_x: number): string => {
            throw hostile;
        };
        // Even though describeError/String could throw on this object, the
        // dispatch must still complete the call with *some* error rather
        // than hanging forever.
        await expect(
            callFunction(rt, 'CallCb', { callback: cb, x: 1 })
        ).rejects.toThrow();
    });
});

describe('host-callable concurrent invocations', () => {
    test('multiple in-flight async callbacks resolve independently', async () => {
        const rt = makeRuntime();
        let active = 0;
        let maxActive = 0;
        const cb = async (x: number): Promise<string> => {
            active += 1;
            maxActive = Math.max(maxActive, active);
            // yield to the loop a couple of times so the callbacks are
            // genuinely interleaved.
            await new Promise<void>((resolve) => setImmediate(resolve));
            await new Promise<void>((resolve) => setImmediate(resolve));
            active -= 1;
            return `n${x}`;
        };
        const promises = Array.from({ length: 8 }, (_, i) =>
            callFunction(rt, 'CallCb', { callback: cb, x: i })
        );
        const results = (await Promise.all(promises)).map((r) => r.result());
        expect(results.sort()).toEqual(['n0', 'n1', 'n2', 'n3', 'n4', 'n5', 'n6', 'n7']);
        // We can't assert exact concurrency without flakiness, but at least
        // the callback should have run for every invocation.
        expect(maxActive).toBeGreaterThanOrEqual(1);
    });
});
