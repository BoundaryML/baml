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
programs=[]
for name, text in cases:
    source = schema + 'function main() -> int {\n  let action = baml.json.decode<Action>(' + json.dumps(text) + ');\n  match(action) { Calc => action.a, Search => action.limit ?? 10, Finish => 0 }\n}\n'
    programs.append((name,source))
extra = [
 ('extra-field', '{"tool":"search","query":"q","ignored":{"nested":[true,null,1]}}'),
 ('escaped-key', '{"t\\u006fol":"search","query":"q"}'),
 ('explicit-null', '{"tool":"search","query":"q","limit":null}'),
 ('wrong-string', '{"tool":"search","query":123}'),
 ('wrong-optional', '{"tool":"search","query":"q","limit":"10"}'),
 ('bool-no-coercion', '{"tool":"calc","op":"add","a":true,"b":1}'),
 ('duplicate-invalid-first', '{"tool":"calc","op":"add","a":"bad","a":3,"b":1}'),
 ('duplicate-invalid-last', '{"tool":"calc","op":"add","a":3,"a":"bad","b":1}'),
 ('int-min', '{"tool":"calc","op":"add","a":-4611686018427387904,"b":1}'),
 ('int-max', '{"tool":"calc","op":"add","a":4611686018427387903,"b":1}'),
 ('non-object', '[1,2]'),
 ('malformed-extra', '{"tool":"search","query":"q","extra":[1,]}'),
]
for name,text in extra:
    programs.append((name,schema+'function main() -> int { let v = baml.json.decode<Action>('+json.dumps(text)+'); match(v) { Calc => v.a, Search => v.limit ?? 10, Finish => 0 } }'))
ambiguous = 'class A { x: int? } class B { x: int?, y: int? } '
for name,text in [('tie','{"x":1}'),('rank','{"x":1,"y":2}'),('rank-failed','{"x":1,"y":"bad"}')]:
    programs.append((name,ambiguous+'function main() -> int { let v = baml.json.decode<A | B>('+json.dumps(text)+'); match(v){ A => 1, B => 2 } }'))
programs.extend([
 ('multiple-types', r'class A { x: int } class B { y: int } function main() -> int { let a = baml.json.decode<A>("{\"x\":3}"); let b = baml.json.decode<B>("{\"y\":4}"); a.x + b.y }'),
 ('alias-and-declared', r'class A { x: int @alias("n") } function main() -> int { let a = baml.json.decode<A>("{\"n\":3}", naming=baml.json.Naming.Aliased); let b = baml.json.decode<A>("{\"x\":4}", naming=baml.json.Naming.Declared); a.x + b.x }'),
 ('alias-missing', r'class A { x: int @alias("n") } function main() -> int { let a = baml.json.decode<A>("{\"x\":3}", naming=baml.json.Naming.Aliased); a.x }'),
 ('skip-fallback', r'class A { x: int @skip, s: string @skip, op: "a" | "b" @skip, n: int? @skip } function main() -> int { let a = baml.json.decode<A>("{\"x\":99}", naming=baml.json.Naming.Aliased); a.x + a.s.length() + (a.n ?? 7) }'),
])
programs.append(('layouts', (base/'json-stream-layouts.baml').read_text()))
for name,text in [('escaped-value',r'{"s":"a\nb\uD83E\uDD80"}'),('bad-surrogate',r'{"s":"\ud800"}')]:
    programs.append((name,'class A { s: string } function main() -> int { let a = baml.json.decode<A>('+json.dumps(text)+'); a.s.length() }'))
programs.extend([
    ('declared-skip-required','class A { x: int @skip } function main() -> int { let a = baml.json.decode<A>("{}"); a.x }'),
    ('declared-alias-missing',r'class A { x: int @alias("n") } function main() -> int { let a = baml.json.decode<A>(`{"n":3}`); a.x }'),
])
programs = programs[25:] + programs[:25]
selection=os.environ.get('PHASE1_JSON_CASES')
if selection:
    requested=set(selection.split(','))
    assert requested <= {name for name,_ in programs}, requested
    programs=[item for item in programs if item[0] in requested]
rows=[]
for name,source in programs:
    path=root/'target/phase1/json-probe.baml'
    path.write_text(source)
    command=cli+['pack','--rust','--runtime','v2','--rust-workspace',str(root/'baml_language'),'--rust-build-dir',str(root/'target/phase1/maps-v2-build'),'--file',str(path),'main','-o','target/phase1/json-probe-v2']
    packed=subprocess.run(command,env=env,capture_output=True,text=True)
    (root/'target/phase1/json-stream-probe-pack.log').write_text(packed.stdout+packed.stderr)
    assert packed.returncode==0,packed.stderr
    for stress in ['0','1']:
        results={}
        for backend,cmd in [('v2',['target/phase1/json-probe-v2']),('bytecode',cli+['run','--file',str(path),'main'])]:
            r=subprocess.run(cmd,env=dict(env,BAML_GC_STRESS=stress),capture_output=True,text=True)
            results[backend]=dict(status=r.returncode,stdout=r.stdout,stderr=r.stderr.replace('warning: code is unformatted; run `baml fmt`\n',''))
        rows.append(dict(case=name,source=source,stress=stress,**results))
        (base/os.environ.get('PHASE1_JSON_OUTPUT','json-stream-correctness.json')).write_text(json.dumps(rows,indent=2)+'\n')
        assert results['v2']==results['bytecode'],rows[-1]
    print(name+': normal/stress parity',flush=True)
