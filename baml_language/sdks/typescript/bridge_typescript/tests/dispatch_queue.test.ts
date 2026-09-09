import { expect, test } from 'vitest';
import { _probeOwnedDispatch, type BamlHandle, type BamlEncodedResult } from '../dist/native.js';

import { baml_bridge } from '../dist/proto/baml_cffi.js';

const tick = () => new Promise<void>(resolve => setImmediate(resolve));

test('a full native queue releases rejected messages immediately', async () => {
    let delivered = 0;
    const probe = _probeOwnedDispatch((_call, args) => {
        delivered++;
        args._discard();
    }, 32, 1, false);
    expect(probe.accepted).toBe(1);
    expect(probe.rejected).toBe(31);
    expect(probe.pending).toBe(1);
    expect(probe.retained).toBe(1);
    for (let i = 0; i < 100 && delivered !== 1; i++) await tick();
    expect(delivered).toBe(1);
    expect(probe.pending).toBe(0);
    expect(probe.retained).toBe(0);
});

test('shutdown delivery frees queued resources without calling JS', () => {
    let called = false;
    const probe = _probeOwnedDispatch(() => { called = true; }, 32, 1, true);
    expect(called).toBe(false);
    expect(probe.pending).toBe(0);
    expect(probe.retained).toBe(0);
});

test('a broken dispatch wrapper cannot retain an unadopted argument result', async () => {
    let escaped: BamlEncodedResult | undefined;
    const probe = _probeOwnedDispatch((_call, args) => {
        escaped = args;
        throw new Error('broken bridge handler');
    }, 1, 1, false);
    for (let i = 0; i < 100 && !escaped; i++) await tick();
    expect(escaped).toBeDefined();
    expect(probe.pending).toBe(0);
    expect(probe.retained).toBe(0);
    expect(() => escaped!._adopt()).toThrow();
    escaped!._discard();
});


test('a broken wrapper cannot revoke already adopted argument ownership', async () => {
    let retained: BamlHandle | undefined;
    const probe = _probeOwnedDispatch((_call, args) => {
        const wire = baml_bridge.cffi.v1.BamlToHostCall.decode(args.payload).args[0].value!.handleValue!;
        retained = args._wrapHandle(wire.key!, wire.handleType!);
        args._adopt();
        throw new Error('handler broke after adoption');
    }, 1, 1, false);
    for (let i = 0; i < 100 && !retained; i++) await tick();
    expect(retained).toBeDefined();
    expect(probe.pending).toBe(0);
    expect(probe.retained).toBe(1);
    expect(() => retained!.clone()).not.toThrow();
});
