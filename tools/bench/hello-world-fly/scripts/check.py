#!/usr/bin/env python3
"""Validate the active Fly matrix without deploying or generating traffic (Python 3.11+)."""
import ast
import json
from pathlib import Path
import tomllib

root = Path(__file__).resolve().parents[1]
manifest = json.loads((root / 'manifest.json').read_text())
targets = json.loads((root / 'load-generator/targets.json').read_text())
dashboard = json.loads((root / 'grafana/dashboard-spec.json').read_text())
expected = {'node-baseline', 'node-baml', 'python-baseline', 'python-baml', 'baml-debian', 'baml-debian-telemetry'}
variants = manifest['variants']
assert len(variants) == len(expected)
assert {v['variant'] for v in variants} == expected
assert targets == {v['variant']: v['url'] for v in variants}
assert {v['app']: v['instance'] for v in dashboard['vms']} == {v['app']: v['machine_id'] for v in variants}
assert dashboard['layout']['rows'] == 6 and dashboard['layout']['panels'] == 24
for variant in variants:
    source = root / variant['source']
    config = tomllib.loads((source / 'fly.toml').read_text())
    assert config['app'] == variant['app']
    assert config['vm'] == [{'memory': '1gb', 'cpu_kind': 'shared', 'cpus': 1}]
    assert config['env']['BAML_PROFILE'] == ('1' if variant['variant'] == 'baml-debian-telemetry' else '0')
    assert config['env']['BAML_TELEMETRY_DISABLED'] == '1'
    assert config['http_service']['auto_stop_machines'] == 'off'
for name in ['baml.toml', 'baml_src/main.baml', 'install-baml.sh']:
    assert (root / 'baml-debian' / name).read_bytes() == (root / 'baml-debian-telemetry' / name).read_bytes()
baseline = (root / 'baml-debian/Dockerfile').read_text()
telemetry = (root / 'baml-debian-telemetry/Dockerfile').read_text()
assert telemetry.count('BAML_PROFILE=1') == 1
assert telemetry.replace('BAML_PROFILE=1', 'BAML_PROFILE=0') == baseline
for directory in [root / 'scripts', root / 'load-generator', *(root / name for name in expected)]:
    for path in directory.glob('*.py'):
        ast.parse(path.read_text(), filename=str(path))
print('PASS: six variants, load/dashboard mappings, machine settings, telemetry isolation, and Python syntax')
