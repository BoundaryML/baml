import { execFile } from 'node:child_process';
import { once } from 'node:events';
import { mkdtempSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';
import { describe, expect, it } from 'vitest';

const runNode = promisify(execFile);
const bridge = new URL('../dist/index.js', import.meta.url).href;
const native = new URL('../dist/native.js', import.meta.url).href;

// A separate process is required: runtime initialization snapshots the environment,
// and changing the working directory/env in a Vitest worker would affect other tests.
const source = `
function Extract(text: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: \`Return the input: \${text}\`
}
`;

describe('source runtime telemetry opt-out (#5013)', () => {
    it.each(['local', 'cloud'])('off prevents %s delivery during initialization, build and parse', async (destination) => {
        const root = mkdtempSync(join(tmpdir(), 'baml-telemetry-off-'));
        const cwd = join(root, 'cwd');
        mkdirSync(cwd);
        const requests: string[] = [];
        const server = createServer((request, response) => {
            requests.push(request.url ?? '');
            response.writeHead(401).end();
        });
        try {
            server.listen(0, '127.0.0.1');
            await once(server, 'listening');
            const address = server.address();
            if (address === null || typeof address === 'string') throw new Error('missing TCP address');
            const env = { ...process.env };
            // Avoid loading a developer's login, native override or telemetry defaults.
            for (const key of Object.keys(env)) {
                if (/^(BAML_|BOUNDARY_|DEV_BAML_|NAPI_RS_)/.test(key)) delete env[key];
            }
            Object.assign(env, {
                HOME: join(root, 'home'),
                USERPROFILE: join(root, 'home'),
                BAML_HOME: join(root, 'baml-home'),
                BAML_TELEMETRY: 'off',
                // An old profile directory override must not revive the old store.
                BAML_PROFILE_DIR: join(root, 'profiles'),
                BAML_PROFILE_PUBLISH_INTERVAL_MS: '0',
                BOUNDARY_API_KEY: destination === 'local' ? 'local' : 'bdry_secret_test',
                BOUNDARY_API_URL: `http://127.0.0.1:${address.port}`,
                BOUNDARY_PROJECT: 'test/test',
                OPENAI_API_KEY: 'unused-test-key',
                DO_NOT_TRACK: '1',
            });
            const { stdout, stderr } = await runNode(process.execPath, ['--input-type=module', '--eval', `
                import assert from 'node:assert/strict';
                import { readdirSync } from 'node:fs';
                import { initializeRuntime, getRuntime, callFunctionSync, callFunction } from ${JSON.stringify(bridge)};
                import { shutdownRuntime } from ${JSON.stringify(native)};

                initializeRuntime('/absolute/source', { 'main.baml': ${JSON.stringify(source)} });
                assert.deepEqual(readdirSync('.'), []);
                // A later env change must not enable tracing on the existing engine.
                process.env.BAML_TELEMETRY = 'high';
                const spec = callFunctionSync(getRuntime(), 'Extract@spec', { text: 'private prompt 5013' }).result();
                assert.match(JSON.stringify(spec.buildRequest()), /private prompt 5013/);
                assert.equal(spec.parse('hello'), 'hello');
                const asyncSpec = (await callFunction(getRuntime(), 'Extract@spec', { text: 'private async prompt 5013' })).result();
                assert.match(JSON.stringify(await asyncSpec.buildRequestAsync()), /private async prompt 5013/);
                assert.equal(await asyncSpec.parseAsync('hello async'), 'hello async');
                await shutdownRuntime();
                console.log('build and parse passed');
            `], { cwd, env, timeout: 20_000 });
            expect(stdout).toContain('build and parse passed');
            expect(stderr).toBe('');
            // Also covers sibling lockfiles, home recordings/CAS, and shutdown writes.
            expect(readdirSync(root)).toEqual(['cwd']);
            expect(readdirSync(cwd)).toEqual([]);
            expect(requests).toEqual([]);
        } finally {
            server.closeAllConnections();
            await new Promise<void>((resolve) => server.close(() => resolve()));
            rmSync(root, { recursive: true, force: true });
        }
    }, 30_000);
});
