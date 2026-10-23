import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import * as bridge from '../dist/index.js';

test('package_root_exports_shutdownRuntime', () => {
    expect(typeof bridge.shutdownRuntime).toBe('function');
});

// `process.exit()` skips Node's `beforeExit`, so only an explicit
// `shutdownRuntime()` seals the telemetry recording before the process ends.
test('shutdownRuntime_seals_the_recording_before_process_exit', () => {
    const home = mkdtempSync(join(tmpdir(), 'baml-ts-shutdown-'));
    try {
        // Record telemetry to local files under a private BAML_HOME only.
        const env: NodeJS.ProcessEnv = { ...process.env, BAML_HOME: home, BAML_TELEMETRY: 'medium', BOUNDARY_API_KEY: 'local' };
        delete env.BOUNDARY_API_URL;
        delete env.BOUNDARY_PROJECT;
        const child = spawnSync(process.execPath, ['--input-type=module', '--eval', `
            import { readdirSync } from 'node:fs';
            import { join } from 'node:path';
            import { BamlRuntime, callFunctionSync, shutdownRuntime } from './dist/index.js';

            // Completed files are immutable, so sealing at shutdown adds one;
            // periodic flushes add files only while events are buffered.
            const recordings = join(process.env.BAML_HOME, 'btel', 'recordings');
            const completedFiles = () => {
                try {
                    return readdirSync(recordings, { withFileTypes: true })
                        .filter((entry) => entry.isDirectory())
                        .flatMap((entry) => readdirSync(join(recordings, entry.name)))
                        .filter((file) => file.endsWith('.btel')).length;
                } catch {
                    return 0;
                }
            };

            const runtime = BamlRuntime.initializeRuntime('.', {
                'main.baml': 'function answer() -> int { 42 }',
            });
            const answer = callFunctionSync(runtime, 'answer', {}).result();
            if (answer !== 42) throw new Error('unexpected answer ' + answer);
            const before = completedFiles();
            await shutdownRuntime();
            const sealed = completedFiles();
            await shutdownRuntime(); // repeated calls are harmless
            let callAfterShutdown = 'succeeded';
            try {
                callFunctionSync(runtime, 'answer', {});
            } catch {
                callAfterShutdown = 'failed';
            }
            console.log(JSON.stringify({ before, sealed, after: completedFiles(), callAfterShutdown }));
            process.exit(0);
        `], {
            cwd: process.cwd(),
            encoding: 'utf8',
            timeout: 30_000,
            env,
        });
        expect(child.stderr).not.toContain('Error');
        expect(child.status).toBe(0);
        const { before, sealed, after, callAfterShutdown } = JSON.parse(child.stdout.trim().split('\n').at(-1)!);
        expect(sealed).toBeGreaterThan(before);
        expect(after).toBe(sealed);
        expect(callAfterShutdown).toBe('failed');
    } finally {
        rmSync(home, { recursive: true, force: true });
    }
});
