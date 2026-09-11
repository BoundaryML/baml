#!/usr/bin/env python3
"""Control the dedicated load machine; stop persists across generator reboots."""
import argparse, subprocess, shlex
p = argparse.ArgumentParser()
p.add_argument('action', choices=['status', 'stop', 'start'])
p.add_argument('--seconds', type=int, default=0, help='0 runs continuously (default); otherwise 1..86400')
a = p.parse_args()
if not 0 <= a.seconds <= 86400:
    p.error('seconds must be 0..86400')
code = {
    'status': "from pathlib import Path; p=Path('/results/latest').read_text(); print(Path(p,'status.json').read_text())",
    'stop': "import json,os,signal; from pathlib import Path; Path('/results/disabled').touch(); p=Path('/results/latest').read_text(); s=json.loads(Path(p,'status.json').read_text()); os.kill(s['supervisor_pid'],signal.SIGTERM) if s['status']=='running' else None; print('Continuous load disabled; in-flight requests drain within 10 seconds')",
    'start': f"import json; from pathlib import Path; root=Path('/results'); p=root/'settings.json'; t=root/'settings.tmp'; t.write_text(json.dumps({{'duration_seconds':{a.seconds}}})); t.replace(p); (root/'disabled').unlink(missing_ok=True); print('Load enabled; manager starts one collector if none is already active. Check status after 15 seconds.')",
}[a.action]
subprocess.run(['fly', 'ssh', 'console', '-a', 'baml-hw-0910-load', '-C', 'python -c ' + shlex.quote(code)], check=True)
