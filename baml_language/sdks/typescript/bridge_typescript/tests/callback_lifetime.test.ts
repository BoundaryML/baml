// Process lifetime is separate from registration/receiver ownership. These
// children use the public SDK and must finish without forced exit, manual GC,
// shutdown calls, or a JS timer keeping otherwise idle BAML work alive.
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { expect, test } from 'vitest';

const source = `
function use(callback: () -> int) -> int { callback() }
function hold(callback: () -> int throws never) -> () -> int throws never { callback }
function inc(value: int) -> int throws never { value + 1 }
function foreground(callback: () -> int) -> int {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
    callback()
}
function schedule(callback: () -> int) -> int {
    spawn {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(100n));
        callback()
    };
    1
}
`;

const cases = [
    {
        name: 'completed callbacks do not pin an idle Node process',
        body: `console.log('value', await use(() => 10));`,
        output: ['value 10'],
    },
    {
        name: 'retaining a callable value does not imply active work',
        body: `globalThis.retained = await hold(() => 10); console.log('retained');`,
        output: ['retained'],
    },
    {
        name: 'a pending foreground call keeps Node alive for its callback',
        body: `console.log('foreground', await foreground(() => 10));`,
        output: ['foreground 10'],
    },
    {
        name: 'background callbacks can reenter BAML during the exit drain',
        body: `
            await schedule(async () => {
                const value = await inc(9);
                console.log('reentered', value);
                return value;
            });
            console.log('scheduled');
        `,
        output: ['scheduled', 'reentered 10'],
    },
    {
        name: 'the exit drain also waits for a retiring runtime',
        body: `
            await schedule(() => { console.log('retired callback'); return 10; });
            BamlRuntime.initializeRuntime('.', {
                'replacement.baml': 'function marker() -> int throws never { 1 }',
            });
            console.log('replaced');
        `,
        output: ['replaced', 'retired callback'],
    },
];

for (const scenario of cases) {
    test(scenario.name, () => {
        const child = run(scenario.body);
        expect(child.error, child.stderr).toBeUndefined();
        expect(child.status, child.stderr).toBe(0);
        for (const output of scenario.output) expect(child.stdout).toContain(output);
    }, 20_000);
}

test('background errors are delivered before automatic exit', () => {
    const child = run(`
        await schedule(() => { throw new Error('background boom'); });
        console.log('scheduled error');
    `);
    expect(child.error, child.stderr).toBeUndefined();
    expect(child.status).not.toBe(0);
    expect(child.stdout).toContain('scheduled error');
    expect(child.stderr).toContain('background boom');
}, 20_000);

function run(body: string) {
    return spawnSync(process.execPath, ['--input-type=module', '--eval', `
        import { BamlRuntime, defineFunction } from './dist/index.js';
        BamlRuntime.initializeRuntime('.', { 'main.baml': ${JSON.stringify(source)} });
        const use = defineFunction('user.use', 'async', ['callback']);
        const hold = defineFunction('user.hold', 'async', ['callback']);
        const inc = defineFunction('user.inc', 'async', ['value']);
        const foreground = defineFunction('user.foreground', 'async', ['callback']);
        const schedule = defineFunction('user.schedule', 'async', ['callback']);
        ${body}
    `], { cwd: fileURLToPath(new URL('..', import.meta.url)), encoding: 'utf8', timeout: 15_000 });
}
