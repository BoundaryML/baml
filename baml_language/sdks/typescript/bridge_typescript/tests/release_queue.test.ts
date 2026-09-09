import { execFileSync } from 'node:child_process';
import { beforeAll, afterAll, expect, test } from 'vitest';
import {
    BamlRuntime, shutdownRuntime, _hostReleaseStats, _releaseHostReferencesForTest,
    _probeReleaseQueue, type HandleKey,
} from '../dist/native.js';
import { registerHostOpaque, lookupHostValue } from '../dist/host_value_registry.js';

const tick = () => new Promise<void>(resolve => setImmediate(resolve));
const keyValue = (key: HandleKey) => BigInt(key.high >>> 0) << 32n | BigInt(key.low >>> 0);
const keys = (start: number, length: number) => Array.from({length}, (_, i) => ({low: start + i, high: 0}));
async function until(check: () => boolean): Promise<void> {
    for (let i = 0; i < 1000 && !check(); i++) await tick();
    expect(check()).toBe(true);
}

beforeAll(() => { BamlRuntime.initializeRuntime('.', {'main.baml': 'function alive() -> int { 1 }'}); });
afterAll(async () => { await shutdownRuntime(); });

test('a release burst larger than the old queue loses no JS registrations', async () => {
    const registered = Array.from({length: 16384}, (_, index) => registerHostOpaque({index}));
    const before = _hostReleaseStats();
    _releaseHostReferencesForTest(registered);
    const queued = _hostReleaseStats();
    expect(queued.pending).toBe(registered.length);
    expect(queued.wakes - before.wakes).toBe(1);
    expect(queued.scheduled).toBe(true);
    await until(() => _hostReleaseStats().pending === 0);
    for (const key of registered) expect(lookupHostValue(keyValue(key))).toBeUndefined();
    expect(_hostReleaseStats().capacity).toBe(0);
    expect(_hostReleaseStats().scheduled).toBe(false);
    expect(_hostReleaseStats().closed).toBe(false);
});

test('repeated bursts leave no pending-key capacity in an idle singleton', async () => {
    for (let round = 0; round < 3; round++) {
        const registered = Array.from({length: 5000}, () => registerHostOpaque({round}));
        _releaseHostReferencesForTest(registered);
        await until(() => _hostReleaseStats().pending === 0);
        expect(_hostReleaseStats().capacity).toBe(0);
        for (const key of registered) expect(lookupHostValue(keyValue(key))).toBeUndefined();
    }
});

test('reentrant releases join the pending batch without a lost wakeup', async () => {
    const received = new Set<bigint>();
    let extra = false;
    const queue = _probeReleaseQueue(batch => {
        for (const key of batch) received.add(keyValue(key));
        if (!extra) {
            extra = true;
            queue.enqueue(keys(20000, 4000));
        }
    });
    queue.enqueue(keys(10000, 4000));
    await until(() => queue.stats.pending === 0);
    expect(received.size).toBe(8000);
    expect(queue.stats.capacity).toBe(0);
    expect(queue.stats.scheduled).toBe(false);
    queue.close();
});

test('a partial deletion failure retries the unacknowledged batch', async () => {
    let attempts = 0;
    const received = new Set<bigint>();
    const queue = _probeReleaseQueue(batch => {
        attempts++;
        received.add(keyValue(batch[0]));
        if (attempts === 1) throw new Error('transient SDK deletion failure');
        for (const key of batch) received.add(keyValue(key));
    });
    queue.enqueue(keys(100, 100));
    await until(() => queue.stats.pending === 0);
    expect(attempts).toBe(2);
    expect(received.size).toBe(100);
    expect(queue.stats.capacity).toBe(0);
    queue.close();
});

test('duplicate pending keys need only one deletion', async () => {
    let delivered = 0;
    const queue = _probeReleaseQueue(batch => { delivered += batch.length; });
    const same = keys(5, 1);
    for (let i = 0; i < 100; i++) queue.enqueue(same);
    expect(queue.stats.pending).toBe(1);
    expect(queue.stats.wakes).toBe(1);
    await until(() => queue.stats.pending === 0);
    expect(delivered).toBe(1);
    queue.close();
});

test('release batches preserve all 64 bits of distinct keys', async () => {
    const registered = [
        {low: 1, high: 0x200000},
        {low: 2, high: 0x200000},
        {low: 1, high: 0xffffffff},
    ];
    const received = new Set<bigint>();
    const queue = _probeReleaseQueue(batch => {
        for (const key of batch) received.add(keyValue(key));
    });
    queue.enqueue(registered);
    await until(() => queue.stats.pending === 0);
    expect(received).toEqual(new Set(registered.map(keyValue)));
    queue.close();
});

test('closing an isolated channel discards queued keys and rejects later enqueueing', async () => {
    let calls = 0;
    const queue = _probeReleaseQueue(() => { calls++; });
    queue.enqueue(keys(1, 3000));
    queue.close();
    queue.enqueue(keys(9000, 100));
    await tick();
    expect(calls).toBe(0);
    expect(queue.stats.closed).toBe(true);
    expect(queue.stats.pending).toBe(0);
    expect(queue.stats.capacity).toBe(0);
});

test('the release channel does not keep an otherwise finished Node process alive', () => {
    const registry = new URL('../dist/host_value_registry.js', import.meta.url).href;
    const native = new URL('../dist/native.js', import.meta.url).href;
    execFileSync(process.execPath, ['--input-type=module', '-e', `
        import { registerHostOpaque } from ${JSON.stringify(registry)};
        import { _releaseHostReferencesForTest } from ${JSON.stringify(native)};
        const keys = Array.from({length: 5000}, () => registerHostOpaque({}));
        _releaseHostReferencesForTest(keys);
    `], {timeout: 10000});
}, 15000);
