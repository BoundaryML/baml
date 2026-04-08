// test_tracing.test.ts — mirrors bridge_python/tests/test_tracing.py

import { BamlRuntime, HostSpanManager, CtxManager } from '../index';

const BAML_SRC = `
function ReturnOne() -> int {
  1
}
`;

describe('HostSpanManager', () => {
    test('new starts with zero depth', () => {
        const mgr = new HostSpanManager();
        expect(mgr.contextDepth()).toBe(0);
    });

    test('enter increments depth', () => {
        const mgr = new HostSpanManager();
        mgr.enter('test', {});
        expect(mgr.contextDepth()).toBe(1);
    });

    test('enter then exitOk restores depth', () => {
        const mgr = new HostSpanManager();
        mgr.enter('test', {});
        mgr.exitOk();
        expect(mgr.contextDepth()).toBe(0);
    });

    test('deepClone is independent', () => {
        const mgr = new HostSpanManager();
        mgr.enter('parent', {});
        const clone = mgr.deepClone();
        clone.enter('child', {});
        expect(mgr.contextDepth()).toBe(1);
        expect(clone.contextDepth()).toBe(2);
    });

    test('upsertTags does not throw', () => {
        const mgr = new HostSpanManager();
        expect(() => mgr.upsertTags({ key: 'value' })).not.toThrow();
    });
});

describe('CtxManager', () => {
    const rt = BamlRuntime.fromFiles('.', { 'main.baml': BAML_SRC });

    test('provides isolated contexts', async () => {
        const ctxMgr = new CtxManager(rt);
        const results: number[] = [];

        const task = async (depth: number) => {
            const mgr = ctxMgr.cloneContext();
            for (let i = 0; i < depth; i++) {
                mgr.enter(`level${i}`, {});
            }
            results.push(mgr.contextDepth());
            for (let i = 0; i < depth; i++) {
                mgr.exitOk();
            }
        };

        await Promise.all([task(1), task(2), task(3)]);
        expect(results.sort()).toEqual([1, 2, 3]);
    });

    test('traceFn wraps sync function', () => {
        const ctxMgr = new CtxManager(rt);
        const wrapped = ctxMgr.traceFn('myFunc', (x: unknown) => x);
        expect(wrapped(42)).toBe(42);
    });

    test('traceFnAsync wraps async function', async () => {
        const ctxMgr = new CtxManager(rt);
        const wrapped = ctxMgr.traceFnAsync('myAsyncFunc', async (x: unknown) => x);
        expect(await wrapped(42)).toBe(42);
    });
});
