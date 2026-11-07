"""Opt-in producer experiments. Nothing enables these features in shipped packs.
Build with `uv run python .../diagnostics.py build`, then `... run`.
"""
import argparse, hashlib, json, os, re, shutil, statistics, subprocess
from pathlib import Path
import check as c
ROOT,WS,OUT,E=c.ROOT,c.WS,c.OUT,c.EVIDENCE
D=OUT/'diagnostics'
MODES={'diagnostic-reference':0,'raw-cntvct':1,'shared-boundary':2,'cheaper-bookkeeping':3,'best-exact':4,'count-only':5,'sample-16':6,'sample-64':7,'interval-1khz':8}
ENV=dict(c.ENV,CARGO_INCREMENTAL='0')
def run(cmd,**kw):return subprocess.run(list(map(str,cmd)),cwd=ROOT,env=kw.pop('env',ENV),check=True,**kw)
def write_rows(path,rows):
 path.write_text('[\n'+',\n'.join(json.dumps(r,separators=(',',':')) for r in rows)+'\n]\n')
def write_if_changed(path,text):
 if not path.exists() or path.read_text()!=text:path.write_text(text)
def build(names):
 D.mkdir(exist_ok=True)
 members=[]
 for name in names:
  baseline=D/(name+'-current')
  if not baseline.exists():shutil.copy2(OUT/(name+'-v2'),baseline)
  saved=D/(name+'.rs')
  if not saved.exists():
   c.ENV['BAML_V2_DIAGNOSTIC_CODEGEN']='1'
   c.build([name],['v2'])
   saved.write_text((OUT/'v2-build/src/main.rs').read_text())
  source=saved.read_text();assert 'diag_boundary_work();' in source
  for variant,mode in list(MODES.items())+[('audit',0)]:
   project=D/(name+'-'+variant+'-build');(project/'src').mkdir(parents=True,exist_ok=True)
   body=source
   if variant in ('audit','shared-boundary','best-exact'):
    # Only direct generated calls with scalar/local operands. These argument
    # copies cannot allocate, throw, dispatch a virtual call or run BAML work.
    # All other calls retain the compiler's invalidation marker.
    body=re.sub(r'diag_boundary_work\(\);\n(?=l\d+=match \(f\d+\(\d+(?:,\s*(?:l\d+|-?\d+i64|true|false))*\)\))','',body)
   if variant=='audit':
    body=body.replace('let frame=rt::telemetry::enter_observed(telemetry_site);','let frame=rt::telemetry::diagnostics::enter_audit(telemetry_site);').replace('frame.finish(&mut result);','frame.finish_audit(&mut result);')
    barrier='rt::telemetry::diagnostics::invalidate();'
   else:
    body=body.replace('let frame=rt::telemetry::enter_observed(telemetry_site);',f'let frame=rt::telemetry::diagnostics::enter::<{mode},false>(telemetry_site);').replace('frame.finish(&mut result);',f'frame.finish::<{mode},false,_>(&mut result);')
    barrier='rt::telemetry::diagnostics::invalidate();' if mode in (2,4) else ''
   generic='9,true' if variant=='audit' else f'{mode},false'
   body=body.replace('rt::telemetry::evidence::observe_captured(',f'rt::telemetry::evidence::observe_diagnostic::<{generic},_>(')
   body=body.replace('rt::telemetry::observed_captured(',f'rt::telemetry::diagnostics::observed_captured::<{generic},_>(')
   body=body.replace('fn diag_boundary_work() {}',f'fn diag_boundary_work() {{{barrier}}}')
   assert 'enter_observed(telemetry_site)' not in body
   write_if_changed(project/'src/main.rs',body)
   manifest=(OUT/'v2-build/Cargo.toml').read_text().split('[profile.release]')[0]
   manifest=manifest.replace('[dependencies.bex_v2_runtime]','[dependencies.bex_v2_runtime]\nfeatures=["telemetry-diagnostics"]').replace('name = "baml_rust_program"',f'name = "diag-{name}-{variant}"')
   write_if_changed(project/'Cargo.toml',manifest);members.append(project.name)
 (D/'Cargo.toml').write_text('[workspace]\nresolver="3"\nmembers='+json.dumps(members)+'\n[profile.release]\nopt-level=3\nlto="fat"\ncodegen-units=1\ndebug=1\n')
 shutil.copyfile(WS/'Cargo.lock',D/'Cargo.lock')
 with (D/'build.log').open('w') as f:run(['cargo','+1.98.0','build','--release','--workspace','--bins','-j','4','--manifest-path',D/'Cargo.toml','--target-dir',OUT/'v2-build/target'],stdout=f,stderr=f)
 for name in names:
  for variant in [*MODES,'audit']:
   shutil.copy2(OUT/f'v2-build/target/release/diag-{name}-{variant}',D/(name+'-'+variant))

def clocks(cpu):
 project=D/'clock-cost';(project/'src').mkdir(parents=True,exist_ok=True)
 # Match raw-cntvct's compiler memory barrier while deliberately omitting ISB.
 source=(E/'clock_cost.rs').read_text().replace('options(nomem,nostack,preserves_flags)','options(nostack,preserves_flags)')
 (project/'src/main.rs').write_text(source)
 manifest='[package]\nname="diagnostic-clock-cost"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[profile.release]\nlto="fat"\ncodegen-units=1\n[dependencies]\nlibc="0.2"\n'
 for crate in ['btel_clock','btel_types']:manifest+=crate+'={path='+json.dumps(str(WS/'crates'/crate))+'}\n'
 (project/'Cargo.toml').write_text(manifest)
 shutil.copyfile(WS/'Cargo.lock',project/'Cargo.lock')
 run(['cargo','+1.98.0','build','--release','--manifest-path',project/'Cargo.toml','--target-dir',OUT/'clock-cost/target'],stdout=subprocess.DEVNULL)
 before=os.getloadavg()
 p=run(['taskset','-c',cpu,OUT/'clock-cost/target/release/diagnostic-clock-cost'],capture_output=True,text=True)
 rows=[json.loads(l) for l in p.stdout.splitlines()]
 result=dict(cpu=cpu,load_before=before,load_after=os.getloadavg(),source=p.stderr,rows=rows,summary={k:statistics.median(r['ns_per_read'] for r in rows if r['source']==k) for k in {r['source'] for r in rows}})
 (E/'diagnostic-clocks.json').write_text(json.dumps(result,indent=2)+'\n')

class ClockFallback(Exception): pass

def durations(reader,home):
 try:return json.loads(run([reader,'--durations',home],capture_output=True,text=True).stdout)
 except subprocess.CalledProcessError as e:
  if 'ARM counter diagnostic requires an ARM clock domain' in e.stderr and 'left: 1' in e.stderr:
   raise ClockFallback(e.stderr) from e
  raise

def recorded_run(cmd,env,reader,home,identity):
 # Select the ARM counter domain, never select by elapsed time. Calibration
 # fallback under host load is recorded and retried, not silently mixed with
 # raw CNTVCT timestamps. Every other recorder/assertion failure stays fatal.
 for attempt in range(5):
  shutil.rmtree(home,ignore_errors=True)
  before=os.getloadavg();p=run(cmd,env=env,capture_output=True,text=True)
  data=json.loads(p.stdout)
  try:return p,data,durations(reader,home),before,os.getloadavg()
  except ClockFallback as error:
   row=dict(identity=identity,attempt=attempt,load=before,data=data,error=str(error))
   with (E/'diagnostic-clock-exclusions.jsonl').open('a') as f:f.write(json.dumps(row)+'\n')
   print('clock calibration fallback; retry',identity,attempt,flush=True)
 raise RuntimeError(('ARM clock calibration failed five times',identity))
def stats(rows,ref):
 a={r['key']:r for r in rows};b={r['key']:r for r in ref};assert a.keys()==b.keys(),(a.keys()-b.keys(),b.keys()-a.keys())
 for k in a:
  assert all(a[k][f]==b[k][f] for f in ['count','await','errored','cancelled','panicked']),(k,a[k],b[k])
 errors=[abs(a[k]['ns']-b[k]['ns'])/b[k]['ns'] for k in b if b[k]['ns']]
 return dict(wape=sum(abs(a[k]['ns']-b[k]['ns']) for k in b)/sum(r['ns'] for r in b.values()),median=statistics.median(errors),p95=sorted(errors)[min(len(errors)-1,int(len(errors)*.95))],zero_paths=sum(r['ns']==0 for r in a.values()),paths=len(a))

def bench(names,cpu):
 reader=c.reader();rows=[]
 variants=['off','medium',*MODES,'node']
 environment=dict(cpu=cpu,timer_cpu=13,cpus=os.cpu_count(),load=os.getloadavg(),base_commit=run(['git','rev-parse','HEAD'],capture_output=True,text=True).stdout.strip(),hashes={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for name in names for p in [D/(name+'-current'),*[D/(name+'-'+v) for v in MODES]]})
 (E/'diagnostic-environment.json').write_text(json.dumps(environment,indent=2)+'\n')
 for rnd in range(5):
  for name in names:
   jobs=3 if name=='trees' else 10
   order=variants[rnd:]+variants[:rnd]
   for variant in order:
    home=D/f'home-{name}-{variant}-{rnd}';shutil.rmtree(home,ignore_errors=True)
    if variant=='node':cmd=['node',WS/'tools/speedtest/src/speedtest/drivers/driver.js',OUT/(name+'.js'),jobs,1]
    else:cmd=[D/(name+('-current' if variant in ('off','medium') else '-'+variant)),'--jobs',jobs,'--warmup',1]
    env=dict(ENV,BAML_HOME=str(home),BAML_TELEMETRY='off' if variant in ('off','node') else 'medium',BAML_V2_DIAG_MODE=str(MODES.get(variant,0)),BAML_V2_TIMER_CPU='13',BAML_V2_DIAG_SEED=str(42+rnd),BAML_GC_STRESS='0')
    cmd=['taskset','-c',cpu,*cmd]
    if variant in ('off','node'):
     before=os.getloadavg();p=run(cmd,env=env,capture_output=True,text=True);data=json.loads(p.stdout);after=os.getloadavg();actual=None
    else:
     p,data,actual,before,after=recorded_run(cmd,env,reader,home,dict(workload=name,variant=variant,round=rnd))
    assert str(data['result'])==c.WORKLOADS[name][1];assert data['jobs']==jobs
    row=dict(workload=name,variant=variant,round=rnd,cpu=cpu,load_before=before,load_after=after,stderr=p.stderr,**data)
    if actual is not None:
     row['durations']=actual
     run([reader,home,home/'normalized.txt',home/'gc-filtered.txt'],stdout=subprocess.DEVNULL)
     row['normalized_digest']=hashlib.sha256((home/'gc-filtered.txt').read_bytes()).hexdigest()
    rows.append(row);write_rows(E/'diagnostic-raw.json',rows);print(name,variant,rnd,round(data['elapsed_ns']/jobs/1e6,4),flush=True)
    shutil.rmtree(home,ignore_errors=True)
 summary={}
 for name in names:
  med={v:statistics.median(r['elapsed_ns']/r['jobs']/1e6 for r in rows if r['workload']==name and r['variant']==v) for v in variants}
  summary[name]={v:dict(ms=med[v],off=med[v]/med['off'],node=med[v]/med['node']) for v in variants}
  for v in MODES:
   for i in range(5):
    selected=[r for r in rows if r['workload']==name and r['round']==i and r['variant'] in (v,'medium')]
    assert len({r['normalized_digest'] for r in selected})==1,(name,v,i,'non-duration parity failure')
   errors=[stats(next(r['durations'] for r in rows if r['workload']==name and r['variant']==v and r['round']==i),next(r['durations'] for r in rows if r['workload']==name and r['variant']=='medium' and r['round']==i)) for i in range(5)]
   summary[name][v]['error']={k:statistics.median(r[k] for r in errors) for k in errors[0]}
 (E/'diagnostic-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 path_errors=[]
 for name in names:
  for variant in MODES:
   paths={}
   for rnd in range(5):
    actual=next(r['durations'] for r in rows if r['workload']==name and r['variant']==variant and r['round']==rnd)
    reference={d['key']:d for r in rows if r['workload']==name and r['variant']=='medium' and r['round']==rnd for d in r['durations']}
    for d in actual:
     ref=reference[d['key']]['ns']
     paths.setdefault(d['key'],[]).append(dict(round=rnd,actual_ns=d['ns'],reference_ns=ref,relative_error=(d['ns']-ref)/ref if ref else None))
   path_errors.extend(dict(workload=name,variant=variant,key=key,rounds=values) for key,values in paths.items())
 write_rows(E/'diagnostic-path-errors.json',path_errors)


def audit(names,cpu):
 reader=c.reader();rows=[]
 for rnd in range(5):
  for name in names:
   for v,m in MODES.items():
    home=D/f'audit-{name}-{v}';shutil.rmtree(home,ignore_errors=True)
    env=dict(ENV,BAML_HOME=str(home),BAML_TELEMETRY='medium',BAML_V2_DIAG_MODE=str(m),BAML_V2_DIAG_AUDIT='1',BAML_V2_TIMER_CPU='13',BAML_V2_DIAG_SEED=str(42+rnd))
    p,data,actual,_,_=recorded_run(['taskset','-c',cpu,D/(name+'-audit'),'--jobs','1','--warmup','1'],env,reader,home,dict(audit=True,workload=name,variant=v,round=rnd))
    assert str(data['result'])==c.WORKLOADS[name][1]
    records=[json.loads(l.removeprefix('DIAGNOSTIC ')) for l in p.stderr.splitlines() if l.startswith('DIAGNOSTIC ')]
    assert records
    samples=[r for d in records for r in d['rows']];reference=sum(r['reference'] for r in samples)
    errors=sorted(abs(r['ticks']-r['reference'])/r['reference'] for r in samples if r['reference'])
    row=dict(workload=name,variant=v,round=rnd,load=os.getloadavg(),wape=sum(abs(r['ticks']-r['reference']) for r in samples)/reference,p95=errors[min(len(errors)-1,int(len(errors)*.95))],reads_per_call=sum(r['reads'] for r in samples)/sum(r['count'] for r in samples),zero_paths=sum(r['samples']==0 for r in samples),paths=len(samples),backwards=sum(r['backwards'] for r in samples),records=records)
    # Published output is checked independently: only duration fields may differ.
    assert sum(r['count'] for r in actual)==sum(r['count'] for r in samples),(name,v,'uninstrumented population')
    assert sum(r['ticks'] for r in actual)==sum(r['ticks'] for r in samples),(name,v,'sidecar differs from published duration')
    reference_rows=json.loads((E/'diagnostic-raw.json').read_text())
    # Benchmark recordings contain warmup+jobs; compare separately normalized per-job counts.
    baseline=next(r for r in reference_rows if r['workload']==name and r['variant']=='medium')
    expected=[dict(r,**{k:2*r[k]//(baseline['jobs']+1) for k in ['count','await','errored','cancelled','panicked']}) for r in baseline['durations']]
    stats(actual,expected)
    rows.append(row);write_rows(E/'diagnostic-audit.json',rows);print(name,v,rnd,row['wape'],row['reads_per_call'],flush=True)
    shutil.rmtree(home,ignore_errors=True)
 summary={name:{v:{k:statistics.median(r[k] for r in rows if r['workload']==name and r['variant']==v) for k in ['wape','p95','reads_per_call','zero_paths','paths','backwards']} for v in MODES} for name in names}
 (E/'diagnostic-audit-summary.json').write_text(json.dumps(summary,indent=2)+'\n')

if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('action',choices=['build','bench','audit','run','clocks']);p.add_argument('--workloads',default='richards,trees,agent');p.add_argument('--cpu',default='12');a=p.parse_args();names=a.workloads.split(',')
 if a.action=='build':build(names)
 if a.action in ('bench','run'):bench(names,a.cpu)
 if a.action in ('audit','run'):audit(names,a.cpu)
 if a.action in ('clocks','run'):clocks(a.cpu)
