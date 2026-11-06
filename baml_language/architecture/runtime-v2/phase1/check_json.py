"""Exact CLI bytecode/v2 JSON error and value differential, normal and GC stress."""
import json
import os
from pathlib import Path
import subprocess

root = Path.cwd()
base = Path('baml_language/architecture/runtime-v2/phase1')
cli = ['target/debug/baml-cli', '--agent-skill-check', 'off', '--no-progress', '--color', 'never']
env = dict(os.environ, BAML_CLI_ALLOW_DIRECT='1', CARGO_TARGET_DIR=str(root/'target'), BAML_TELEMETRY='off')
schema = (base/'json-error.baml').read_text().split('function main')[0]
cases = [
    ('unknown', '{"tool":"browse"}'),
    ('missing', '{"tool":"calc","op":"add","a":1}'),
    ('truncated', '{"tool":'),
    ('wrong-int', '{"tool":"calc","op":"add","a":"x","b":1}'),
    ('float', '{"tool":"calc","op":"add","a":1.0,"b":1}'),
    ('range', '{"tool":"calc","op":"add","a":4611686018427387904,"b":1}'),
    ('literal', '{"tool":"calc","op":"divide","a":1,"b":1}'),
    ('negative-zero', '{"tool":"calc","op":"add","a":-0,"b":1}'),
    ('duplicate', '{"tool":"browse","tool":"search","query":"a"}'),
    ('syntax-first', '{"tool":"calc","op":"bad",'),
]
rows=[]
for name, text in cases:
    source = schema + 'function main() -> int {\n  let action = baml.json.decode<Action>(' + json.dumps(text) + ');\n  match(action) { Calc => action.a, Search => action.limit ?? 10, Finish => 0 }\n}\n'
    path=root/'target/phase1/json-probe.baml'
    path.write_text(source)
    command=cli+['pack','--rust','--runtime','v2','--rust-workspace',str(root/'baml_language'),'--rust-build-dir',str(root/'target/phase1/maps-v2-build'),'--file',str(path),'main','-o','target/phase1/json-probe-v2']
    packed=subprocess.run(command,env=env,capture_output=True,text=True)
    (root/'target/phase1/m3-json-probe-pack.log').write_text(packed.stdout+packed.stderr)
    assert packed.returncode==0,packed.stderr
    for stress in ['0','1']:
        results={}
        for backend,cmd in [('v2',['target/phase1/json-probe-v2']),('bytecode',cli+['run','--file',str(path),'main'])]:
            r=subprocess.run(cmd,env=dict(env,BAML_GC_STRESS=stress),capture_output=True,text=True)
            results[backend]=dict(status=r.returncode,stdout=r.stdout,stderr=r.stderr.replace('warning: code is unformatted; run `baml fmt`\n',''))
        rows.append(dict(case=name,source=source,stress=stress,**results))
        (base/'json-correctness.json').write_text(json.dumps(rows,indent=2)+'\n')
        assert results['v2']==results['bytecode'],rows[-1]
    print(name+': normal/stress parity',flush=True)
