"""Direct v2 telemetry acceptance. Run from the designated worktree with uv python.
The normalizer is imported from rust_diff/telemetry.rs, never reimplemented.
"""
import argparse
import difflib
import hashlib
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[4]
WS = ROOT / 'baml_language'
OUT = ROOT / 'target/telemetry'
EVIDENCE = Path(__file__).resolve().parent
WORKLOADS = {'trees': ('memory/binary-trees-14', '3222190'), 'richards': ('programs/richards-50', '1161046400'), 'agent': ('programs/agent-loop-300', '592308425')}
ENV = dict(os.environ, BAML_CLI_ALLOW_DIRECT='1', CARGO_TARGET_DIR=str(ROOT/'target'))

def run(cmd, **kw):
    return subprocess.run(list(map(str, cmd)), cwd=ROOT, env=kw.pop('env',ENV), check=True, **kw)

def inline(v):
    if isinstance(v,dict): return '{'+', '.join(k+' = '+inline(x) for k,x in v.items())+'}'
    return json.dumps(v)

def reader():
    directory = OUT/'reader'
    (directory/'src').mkdir(parents=True,exist_ok=True)
    manifest=tomllib.loads((WS/'Cargo.toml').read_text())
    deps={n:manifest['workspace']['dependencies'][n] for n in ['prost','serde','serde_json','sha2','tempfile']}
    for name, features in [('serde',['derive']),('serde_json',['arbitrary_precision','preserve_order'])]:
        if isinstance(deps[name],str): deps[name]={'version':deps[name]}
        deps[name]['features']=features
    for name in ['btel_reader','btel_recorder','btel_snapshot']:
        deps[name]={'path':str(WS/'crates'/name)}
    (directory/'Cargo.toml').write_text('[package]\nname="v2-telemetry-reader"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\n'+''.join(n+' = '+inline(v)+'\n' for n,v in deps.items()))
    with (directory/'Cargo.toml').open('a') as f:
        f.write('[build-dependencies]\n'+''.join(n+' = '+inline(manifest['workspace']['dependencies'][n])+'\n' for n in ['prost-build','protoc-bin-vendored']))
    proto=WS/'crates/btel_recorder/proto'
    (directory/'build.rs').write_text('fn main(){let mut c=prost_build::Config::new();c.protoc_executable(protoc_bin_vendored::protoc_bin_path().unwrap());c.type_attribute(".","#[derive(serde::Serialize)]");c.compile_protos(&['+json.dumps(str(proto/'recording.proto'))+'],&['+json.dumps(str(proto))+']).unwrap();}')
    source=(WS/'crates/baml_tests/rust_diff/telemetry.rs').read_text()
    (directory/'src/telemetry.rs').write_text(source)
    (directory/'src/known_telemetry_divergences.rs').write_text((WS/'crates/baml_tests/rust_diff/known_telemetry_divergences.rs').read_text())
    (directory/'src/main.rs').write_text('mod telemetry; mod known_telemetry_divergences; fn main(){let a:Vec<_>=std::env::args_os().collect();let trace=telemetry::read(std::path::Path::new(&a[1]));std::fs::write(&a[2],&trace).unwrap();if let Some(path)=a.get(3){std::fs::write(path,known_telemetry_divergences::without_gc(&trace,"v2")).unwrap();}}')
    shutil.copyfile(WS/'Cargo.lock',directory/'Cargo.lock')
    run(['cargo','build','--release','--manifest-path',directory/'Cargo.toml','--target-dir',OUT/'reader-target'],stdout=subprocess.DEVNULL)
    return OUT/'reader-target/release/v2-telemetry-reader'

def build(names, backends):
    cli=ROOT/'target/debug/baml-cli'
    for name in names:
        workload,_=WORKLOADS[name]
        source=WS/'tools/speedtest/workloads'/workload
        project=OUT/(name+'-project')
        (project/'baml_src').mkdir(parents=True,exist_ok=True)
        shutil.copyfile(source/'main.baml',project/'baml_src/main.baml')
        driver=(WS/'tools/speedtest/src/speedtest/drivers/bench_driver.baml').read_text().replace('    main();','    if (string.from(main()) != result) { baml.sys.panic("result mismatch"); }')
        (project/'baml_src/driver.baml').write_text(driver)
        (project/'baml.toml').write_text('[package]\nname="telemetry_'+name+'"\n[pack.env_var_names]\nBAML_TELEMETRY="BAML_TELEMETRY"\n')
        (OUT/(name+'.js')).write_text((source/'main.js').read_text())
        for backend,entry in [('v2','main'),('oracle','main'),('native','speedtest_driver')]:
            if backend not in backends: continue
            cmd=[cli,'--agent-skill-check','off','--no-progress','--color','never','pack','--rust','--rust-workspace',WS,'--rust-build-dir',OUT/('v2-build' if backend=='v2' else 'native-build'),'--from',project,entry,'-o',OUT/(name+'-'+backend)]
            if backend=='v2': cmd[cmd.index('--rust')+1:cmd.index('--rust')+1]=['--runtime','v2']
            with (OUT/(name+'-'+backend+'-build.log')).open('w') as log: run(cmd,stdout=log,stderr=log)

def check(names,levels):
    normalizer=reader()
    results=[]
    for name in names:
        for stress in ['0','1']:
            for level in levels:
                normalized=[]
                gc_diagnostic=[]
                for backend in ['oracle','v2']:
                    home=OUT/f'run-{name}-{level}-{stress}-{backend}'
                    if home.exists(): shutil.rmtree(home)
                    home.mkdir()
                    env=dict(ENV,BAML_HOME=str(home),BAML_TELEMETRY=level,BAML_GC_STRESS=stress,BAML_COMPILED_BODIES='0' if backend=='oracle' else '1')
                    result=subprocess.run([str(OUT/(name+'-'+backend))],cwd=ROOT,env=env,capture_output=True,text=True,timeout=1200)
                    assert result.returncode==0,(name,level,stress,backend,result.stderr)
                    assert result.stdout.strip()==WORKLOADS[name][1],result.stdout
                    (home/'stderr.txt').write_text(result.stderr)
                    # CONTRACTS.md C5 exempts only successful collector maintenance.
                    # Keep raw equality and its diff alongside accepted parity.
                    run([normalizer,home,home/'normalized.txt',home/'gc-diagnostic.txt'])
                    gc_diagnostic.append((home/'gc-diagnostic.txt').read_text())
                    normalized.append((home/'normalized.txt').read_text())
                strict_parity=normalized[0]==normalized[1]
                parity=gc_diagnostic[0]==gc_diagnostic[1]
                diff_path=None
                if not strict_parity:
                    diff=''.join(difflib.unified_diff(normalized[0].splitlines(True),normalized[1].splitlines(True),fromfile='bytecode',tofile='v2'))
                    diff_path=OUT/f'{name}-{level}-{stress}.diff'
                    diff_path.write_text(diff)
                row=dict(workload=name,level=level,stress=stress,strict_parity=strict_parity,parity=parity,parity_except_existing_gc_records=gc_diagnostic[0]==gc_diagnostic[1],diff=str(diff_path) if diff_path else None)
                results.append(row); print(json.dumps(row),flush=True)
                (EVIDENCE/'parity.json').write_text(json.dumps(results,indent=2)+'\n')
    if any(not row['parity'] for row in results):
        raise AssertionError('BTEL parity failed outside the C5 GC exception; per-case diffs are retained in target/telemetry')

def bench(names,cpu):
    environment=dict(cpu=cpu,logical_cpus=os.cpu_count(),affinity=sorted(os.sched_getaffinity(0)),load=os.getloadavg(),
        platform=os.uname()._asdict() if hasattr(os.uname(), '_asdict') else list(os.uname()),
        node=run(['node','--version'],capture_output=True,text=True).stdout.strip(),
        rustc=subprocess.run(['rustc','--version'],cwd=WS,check=True,capture_output=True,text=True).stdout.strip(),
        base_commit=run(['git','rev-parse','HEAD'],capture_output=True,text=True).stdout.strip(),
        binaries={name+'-'+backend:hashlib.sha256((OUT/(name+'-'+backend)).read_bytes()).hexdigest()
                  for name in names for backend in ['v2','oracle','native']})
    (EVIDENCE/'benchmark-environment.json').write_text(json.dumps(environment,indent=2)+'\n')
    rows=[]
    for round in range(5):
        for name in names:
            jobs=3 if name=='trees' else 10
            commands={
                'v2-off':([OUT/(name+'-v2'),'--jobs',jobs,'--warmup',1],'off'),
                'v2-medium':([OUT/(name+'-v2'),'--jobs',jobs,'--warmup',1],'medium'),
                'native-medium':([OUT/(name+'-native'),'--jobs',jobs,'--warmup',1],'medium'),
                'node':(['node',WS/'tools/speedtest/src/speedtest/drivers/driver.js',OUT/(name+'.js'),jobs,1],'off'),
            }
            order=list(commands)
            order=order[round % len(order):]+order[:round % len(order)]
            for backend in order:
                cmd,level=commands[backend]
                env=dict(ENV,BAML_TELEMETRY=level,BAML_GC_STRESS='0',BAML_COMPILED_BODIES='1')
                env['BAML_HOME']=str(OUT/f'bench-{name}-{backend}-{round}')
                before=os.getloadavg()
                result=run(['taskset','-c',cpu,*cmd],env=env,capture_output=True,text=True)
                data=json.loads(result.stdout)
                assert str(data['result'])==WORKLOADS[name][1],data
                assert data['jobs']==jobs,data
                row=dict(workload=name,backend=backend,round=round+1,cpu=cpu,load_before=before,load_after=os.getloadavg(),stderr=result.stderr,**data)
                rows.append(row);print(json.dumps(row),flush=True)
                (EVIDENCE/'benchmark-raw.json').write_text(json.dumps(rows,indent=2)+'\n')
    summary={}
    for name in names:
        medians={b:statistics.median(r['elapsed_ns']/r['jobs']/1e6 for r in rows if r['workload']==name and r['backend']==b) for b in commands}
        summary[name]=dict(ms_per_job=medians,medium_over_off=medians['v2-medium']/medians['v2-off'])
    (EVIDENCE/'benchmark-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    print(json.dumps(summary),flush=True)

def profile(cpu):
    perf = shutil.which('perf') or '/nix/store/i0cd89mpcvd3jb6slpzry4vr23gz4j9k-perf-linux-7.1/bin/perf'
    env = dict(ENV,BAML_TELEMETRY='medium',BAML_GC_STRESS='0',BAML_HOME=str(OUT/'profile-home'))
    command = ['taskset','-c',cpu,perf,'record','-g','--call-graph','dwarf','-o',OUT/'richards-medium.perf.data','--',OUT/'richards-v2','--jobs','300','--warmup','1']
    load_before=os.getloadavg()
    result=run(command,env=env,capture_output=True,text=True)
    data=json.loads(result.stdout)
    assert str(data['result'])==WORKLOADS['richards'][1],data
    (EVIDENCE/'richards-medium-profile-run.json').write_text(json.dumps(dict(command=list(map(str,command)),load_before=load_before,load_after=os.getloadavg(),stderr=result.stderr,**data),indent=2)+'\n')
    profile_report()

def profile_report():
    perf = shutil.which('perf') or '/nix/store/i0cd89mpcvd3jb6slpzry4vr23gz4j9k-perf-linux-7.1/bin/perf'
    report=run([perf,'report','--stdio','--no-children','--call-graph','none','--sort','symbol','--percent-limit','0.5','-i',OUT/'richards-medium.perf.data'],capture_output=True,text=True)
    (EVIDENCE/'richards-medium-profile.txt').write_text('\n'.join(line.rstrip() for line in report.stdout.splitlines())+'\n')
    lines=run([perf,'report','--stdio','--no-children','--call-graph','none','--full-source-path','--sort','srcline','--percent-limit','0.5','-i',OUT/'richards-medium.perf.data'],capture_output=True,text=True)
    (EVIDENCE/'richards-medium-lines.txt').write_text('\n'.join(line.rstrip() for line in lines.stdout.splitlines())+'\n')

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('action',choices=['build','check','bench','reader','profile','profile-report']);p.add_argument('--workloads',default='trees,richards');p.add_argument('--levels',default='off,low,medium');p.add_argument('--cpu',default='12');p.add_argument('--backends',default='v2,oracle,native');a=p.parse_args()
    OUT.mkdir(parents=True,exist_ok=True)
    names=a.workloads.split(',')
    if a.action=='build':build(names,a.backends.split(','))
    elif a.action=='check':check(names,a.levels.split(','))
    elif a.action=='reader':reader()
    elif a.action=='profile':profile(a.cpu)
    elif a.action=='profile-report':profile_report()
    else:bench(names,a.cpu)
