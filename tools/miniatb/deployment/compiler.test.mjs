import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

test('image compiler streams a large Claude prompt while draining output', { skip: !fs.existsSync('/opt/baml/bin/baml') }, () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'miniatb-compiler-'));
  try {
    const payload = 'BEGIN-' + 'x'.repeat(1024 * 1024) + '-雪-END';
    fs.writeFileSync(`${dir}/payload`, payload);
    fs.writeFileSync(`${dir}/baml.toml`, '[package]\nname = "probe"\n');
    fs.writeFileSync(`${dir}/probe`, `#!/bin/sh
set -eu
for arg in "$@"; do test "\${#arg}" -lt 65536 || exit 42; done
printf '{"type":"system","padding":"'
dd if=/dev/zero bs=1024 count=256 2>/dev/null | tr '\\000' x
printf '"}\\n'
cat > '${dir}/received'
printf '%s\\n' '{"type":"result","subtype":"success","structured_output":"ok"}'
`, { mode: 0o700 });
    fs.writeFileSync(`${dir}/main.baml`, `
function Probe(payload: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: \`${'${payload} ${ctx.output_format()}'}\`
}
function main() -> void {
    let spec = Probe@spec(baml.fs.read("${dir}/payload"));
    let cl = claude_code.ClaudeCodeClient.new(executable = "${dir}/probe", timeout_ms = 10000);
    cl.invoke(ai.ModelTurnInput {
        prompt: spec.prompt_template, journal: ai.Journal.new(spec),
        toolbox: spec.tools(), output_type: spec.output_type(),
    });
}
`);
    const result = spawnSync('/opt/baml/bin/baml', ['run', '--directory', dir, '--agent-skill-check', 'off', 'main'], { encoding: 'utf8', timeout: 60000 });
    assert.equal(result.status, 0, result.stderr);
    assert.ok(fs.readFileSync(`${dir}/received`, 'utf8').includes(payload));
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});
