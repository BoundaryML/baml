"""Compare packed agent-loop to bytecode including stdout, stderr and status."""
import json
import os
from pathlib import Path
import subprocess
rows=[]
for stress in ['0','1']:
    results={}
    for backend, command, extra in [('v2',['target/phase1/agent-v2'],{}),('bytecode',['target/phase1/agent-oracle'],{'BAML_COMPILED_BODIES':'0'})]:
        result=subprocess.run(command,env=dict(os.environ,BAML_TELEMETRY='off',BAML_GC_STRESS=stress,**extra),capture_output=True,text=True)
        results[backend]=dict(command=command,status=result.returncode,stdout=result.stdout,stderr=result.stderr)
        assert (result.returncode,result.stdout,result.stderr)==(0,'592308425\n',''),results
    rows.append(dict(stress=stress,**results))
Path('baml_language/architecture/runtime-v2/phase1/agent-correctness.json').write_text(json.dumps(rows,indent=2)+'\n')
print('agent: 592308425; bytecode parity normal/stressed')
