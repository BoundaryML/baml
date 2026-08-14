import { spawnSync } from 'node:child_process';

test('unhandled_spawn_error_uses_host_default', () => {
    const child = spawnSync(process.execPath, ['--input-type=module', '--eval', `
        import {
            BamlRuntime,
            callFunctionSync,
        } from './dist/index.js';
        import { shutdownRuntime } from './dist/native.js';

        const runtime = BamlRuntime.initializeRuntime('.', {
            'main.baml': \`
                function bad() -> int throws string { throw "boom" }
                function main() -> int {
                    spawn { bad() };
                    baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
                    1
                }
            \`,
        });
        callFunctionSync(runtime, 'main', {}).result();
        await shutdownRuntime();
    `], {
        cwd: process.cwd(),
        encoding: 'utf8',
        timeout: 10_000,
    });

    expect(child.status).not.toBe(0);
    expect(child.stderr).toContain('baml error');
});
