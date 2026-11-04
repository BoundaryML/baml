import { spawnSync } from 'node:child_process';

// The package holds a reference to its host adapter. A process that ends
// through `process.exit()` or an uncaught exception skips the hook that lets
// it go, and must not print napi's leak warning for it.
test.each([
    ['process_exit', 'process.exit(0);', 0],
    ['an_uncaught_exception', "throw new Error('boom');", 1],
])('ending_through_%s_prints_no_leak_warning', (_, ending, status) => {
    const child = spawnSync(process.execPath, ['--input-type=module', '--eval', `
        import { writeSync } from 'node:fs';
        import { BamlRuntime, callFunctionSync } from './dist/index.js';
        const rt = BamlRuntime.initializeRuntime('.', { 'main.baml': 'function Echo(n: int) -> int { n }' });
        // process.exit() can cut off an asynchronous write to a pipe.
        writeSync(1, 'called ' + callFunctionSync(rt, 'Echo', { n: 1 }).result() + '\\n');
        ${ending}
    `], { cwd: process.cwd(), encoding: 'utf8', env: { ...process.env, BAML_TELEMETRY: 'off' } });
    // The bridge loaded and ran the call before the process ended.
    expect(child.stdout).toContain('called 1');
    expect(child.status).toBe(status);
    expect(child.stderr).not.toContain('is not unref');
});
