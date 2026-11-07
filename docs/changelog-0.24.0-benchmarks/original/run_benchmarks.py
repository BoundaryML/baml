import json, os, pathlib, statistics, subprocess, sys
results={'023':[], '024':[]}
for trial in range(5):
    order=['023','024'] if trial%2==0 else ['024','023']
    for ver in order:
        root=pathlib.Path(f'/tmp/baml-{ver}-sdk-examples')
        env=dict(os.environ, BOUNDARY_API_KEY='local', BAML_HOME=str(root/'bench-home'), BAML_TELEMETRY='off', BAML_BRIDGE_PATH=str(root/'libbaml_cffi-aarch64-apple-darwin.dylib'), PYTHONPATH=str(root/'python'))
        proc=subprocess.run([str(root/'venv/bin/python'), '/tmp/baml-024-sdk-examples/bench.py', ver],env=env,capture_output=True,text=True,check=True)
        row=json.loads(proc.stdout.strip())
        results[ver].append(row)
        print(f'trial {trial+1} {ver}',flush=True)
        if proc.stderr: print(proc.stderr,file=sys.stderr)
pathlib.Path('/tmp/baml-024-sdk-examples/benchmark-results.json').write_text(json.dumps(results,indent=2))
for name in results['023'][0]:
    key='ms' if 'ms' in results['023'][0][name] else 'us_per_call'
    a=[r[name][key] for r in results['023']]; b=[r[name][key] for r in results['024']]
    meda=statistics.median(a); medb=statistics.median(b)
    print(name, key, 'lower',round(meda,3), 'upper',round(medb,3), 'ratio',round(medb/meda,3), 'ranges', [round(min(a),3),round(max(a),3)],[round(min(b),3),round(max(b),3)], flush=True)
