"""Diagnostic-only generated variants. No compiler/runtime switch is shipped.
Build after check.py build --workloads richards --backends v2.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import statistics
import subprocess
import check as common

ROOT, WS, OUT, EVIDENCE = common.ROOT, common.WS, common.OUT, common.EVIDENCE
ENV = dict(common.ENV)

def build():
    baseline=OUT/'floor-baseline.rs'
    if not baseline.exists(): baseline.write_text((OUT/'v2-build/src/main.rs').read_text())
    original=baseline.read_text()
    # The preserved diagnostic source predates recording-only error PCs.
    # These off-level variants never build error evidence.
    if 'pcs:' not in original:
        original=original.replace('rt::Source{file:', 'rt::Source{pcs:&[],kinds:&[],file:')
    assert '1161046400' not in original # result must be computed, not embedded
    functions={int(i): hidden=='true' for i,hidden in re.findall(r'word:&O(\d+),policy:rt::telemetry::Policy::NONE,hidden:(true|false)',original)}
    assert functions and 'WorkerTask' in original
    # Retain the last unoptimized full-medium executable as variant c.
    if not (OUT/'richards-c').exists():
        assert 'fn add_ok(' not in (WS/'crates/bex_v2_runtime/src/telemetry.rs').read_text(), 'Preserve the control at commit 347d928e04 before building optimized runtime code'
        shutil.copyfile(OUT/'richards-v2',OUT/'richards-c')
        shutil.copymode(OUT/'richards-v2',OUT/'richards-c')
    directory=OUT/'floor-build'
    directory.mkdir(parents=True,exist_ok=True)
    members=[]
    for mode in ['a','b']:
        for clock in ['quanta','ecv']:
            source=original
            def wrapper(m):
                ident=int(m[1]);args=m[2]
                body=f'let entered=diag::read();let result=f{ident}_body({args});let exited=diag::read();'
                body+=f'if cfg!(feature="floor-aggregate"){{diag::add({ident},entered,exited);}}'
                return body+'result'
            source,n=re.subn(r'let frame=rt::telemetry::enter_observed\(telemetry_site\);let mut result=f(\d+)_body\(([^;]*)\);frame.finish\(&mut result\);result',wrapper,source)
            assert n>0,n
            source=re.sub(r'(static O(\d+):std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new\()0(\);)',lambda m:m[1]+str(int(not functions.get(int(m[2]),True)))+m[3],source)
            # Empty runtime tables leave the diagnostic observation words alone;
            # BAML_TELEMETRY=off suppresses all normal paths/records/aggregation.
            source=re.sub(r'TELEMETRY_FUNCTIONS,TELEMETRY_SITES,Some\(\d+\)', '&[],&[],None',source)
            source=source.replace('fn main()->std::process::ExitCode{','fn main()->std::process::ExitCode{let _diagnostic=diag::init();')
            source+= '\n'+(EVIDENCE/'floor_support.rs').read_text().replace('FUNCTION_SLOTS',str(max(functions)+1))
            name=f'floor-{mode}-{clock}'
            members.append(name)
            project=directory/name
            (project/'src').mkdir(parents=True,exist_ok=True)
            (project/'src/main.rs').write_text(source)
            manifest=(OUT/'v2-build/Cargo.toml').read_text().replace('[workspace]', '').replace('name = "baml_rust_program"',f'name = "{name}"')
            defaults=(['floor-aggregate'] if mode=='b' else [])+(['floor-ecv'] if clock=='ecv' else [])
            manifest+='\n[features]\nfloor-aggregate=[]\nfloor-ecv=[]\ndefault='+json.dumps(defaults)+'\n'
            manifest+='\n[dependencies.btel_clock]\npath='+json.dumps(str(WS/'crates/btel_clock'))+'\n[dependencies.libc]\nversion="0.2"\n'
            (project/'Cargo.toml').write_text(manifest)
    (directory/'Cargo.toml').write_text('[workspace]\nresolver="3"\nmembers='+json.dumps(members)+'\n[profile.release]\nopt-level=3\nlto="fat"\ncodegen-units=1\ndebug=1\n')
    shutil.copyfile(WS/'Cargo.lock',directory/'Cargo.lock')
    with (OUT/'floor-build.log').open('w') as log:
        subprocess.run(['cargo','build','--release','--workspace','--bins','-j','4','--manifest-path',directory/'Cargo.toml','--target-dir',OUT/'v2-build/target'],cwd=WS,env=ENV,check=True,stdout=log,stderr=log)
    for mode in ['a','b']:
        for clock in ['quanta','ecv']:
            shutil.copyfile(OUT/f'v2-build/target/release/floor-{mode}-{clock}',OUT/f'richards-{mode}-{clock}')
            shutil.copymode(OUT/f'v2-build/target/release/floor-{mode}-{clock}',OUT/f'richards-{mode}-{clock}')
            print('built',mode,clock,flush=True)

def bench(cpu):
    reader=OUT/'reader-target/release/v2-telemetry-reader'
    variants={'off':('richards-c','off'),'c':('richards-c','medium')}
    for mode in ['a','b']:
        for clock in ['quanta','ecv']:
            variants[mode+'-'+clock]=(f'richards-{mode}-{clock}','off')
    if (OUT/'richards-optimized').exists():
        variants['optimized']=('richards-optimized','medium')
        variants['optimized-off']=('richards-optimized','off')
    expected=0
    for line in (OUT/'run-richards-medium-0-v2/normalized.txt').read_text().splitlines():
        count,payload=line.split(' ',1);record=json.loads(payload)
        if record[0]=='aggregate.entries' and record[-1]=='count' and record[1]['node'][0]['callee'].startswith('user.'):
            expected+=int(count)
    assert expected>0
    rows=[]
    for round in range(5):
        order=list(variants);order=order[round:]+order[:round]
        for name in order:
            binary,level=variants[name]
            env=dict(ENV,BAML_TELEMETRY=level,BAML_GC_STRESS='0',BAML_HOME=str(OUT/f'floor-home-{name}-{round}'))
            before=os.getloadavg()
            result=subprocess.run(['taskset','-c',cpu,OUT/binary,'--jobs','20','--warmup','2'],env=env,cwd=ROOT,check=True,capture_output=True,text=True)
            data=json.loads(result.stdout);assert str(data['result'])=='1161046400';assert data['jobs']==20
            if name.startswith('b-'):
                assert int(re.search(r'diagnostic samples=(\d+)',result.stderr)[1])==expected*22,(name,result.stderr,expected)
            clocks=json.loads(subprocess.run([reader,'--clocks',env['BAML_HOME']],check=True,capture_output=True,text=True).stdout) if level!='off' else []
            row=dict(clocks=clocks,variant=name,round=round+1,cpu=cpu,load_before=before,load_after=os.getloadavg(),stderr=result.stderr,**data)
            rows.append(row);print(json.dumps(row),flush=True)
            (EVIDENCE/'floor-raw.json').write_text(json.dumps(rows,indent=2)+'\n')
    medians={name:statistics.median(r['elapsed_ns']/r['jobs']/1e6 for r in rows if r['variant']==name) for name in variants}
    summary=dict(ms_per_job=medians,over_off={k:v/medians['off'] for k,v in medians.items()},binaries={k:hashlib.sha256((OUT/b).read_bytes()).hexdigest() for k,(b,_) in variants.items()})
    (EVIDENCE/'floor-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    print(json.dumps(summary),flush=True)

def clock_probe(cpu):
    project=OUT/'clock-cost'
    (project/'src').mkdir(parents=True,exist_ok=True)
    shutil.copyfile(EVIDENCE/'clock_cost.rs',project/'src/main.rs')
    manifest='[package]\nname="v2-clock-cost"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[profile.release]\nlto="fat"\ncodegen-units=1\n[dependencies]\nlibc="0.2"\n'
    manifest+=''.join(name+'={path='+json.dumps(str(WS/'crates'/name))+'}\n' for name in ['btel_clock','btel_types'])
    (project/'Cargo.toml').write_text(manifest)
    shutil.copyfile(WS/'Cargo.lock',project/'Cargo.lock')
    with (OUT/'clock-build.log').open('w') as log:
        subprocess.run(['cargo','build','--release','--manifest-path',project/'Cargo.toml','--target-dir',OUT/'v2-build/target'],cwd=WS,env=ENV,check=True,stdout=log,stderr=log)
    before=os.getloadavg()
    result=subprocess.run(['taskset','-c',cpu,OUT/'v2-build/target/release/v2-clock-cost'],env=ENV,check=True,capture_output=True,text=True)
    (EVIDENCE/'clock-cost.jsonl').write_text(result.stdout)
    (EVIDENCE/'clock-source.txt').write_text(result.stderr)
    rows=[json.loads(line) for line in result.stdout.splitlines()]
    summary=dict(cpu=cpu,load_before=before,load_after=os.getloadavg(),backwards=sum(r['backwards'] for r in rows),
        median_ns_per_read={source:statistics.median(r['ns_per_read'] for r in rows if r['source']==source) for source in {r['source'] for r in rows}},
        median_cpu_ns_per_read={source:statistics.median(r['cpu_ns_per_read'] for r in rows if r['source']==source) for source in {r['source'] for r in rows}})
    assert summary['backwards']==0
    (EVIDENCE/'clock-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    print(json.dumps(summary),flush=True)

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('action',choices=['build','bench','clock']);p.add_argument('--cpu',default='12');a=p.parse_args()
    if a.action=='build':build()
    elif a.action=='clock':clock_probe(a.cpu)
    else:bench(a.cpu)
