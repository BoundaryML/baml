#!/usr/bin/env python3
"""Check harness source and dashboard invariants without starting workloads."""
import ast
import copy
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main():
    names = {e['name'] for e in json.loads((ROOT / 'experiments.json').read_text())}
    assert names == {'node-baseline', 'node-baml', 'python-baseline', 'python-baml', 'baml-debian'}
    source_dirs = [ROOT / name for name in names] + [ROOT / 'scripts', ROOT / 'container-metrics']
    for directory in source_dirs:
        for path in directory.glob('*.py'):
            ast.parse(path.read_text(), filename=str(path))
        for pattern in ('*.js', '*.cjs'):
            for path in directory.glob(pattern):
                subprocess.run(['node', '--check', str(path)], check=True)
        for path in directory.glob('*.sh'):
            subprocess.run(['sh', '-n', str(path)], check=True)
        for path in directory.glob('*.json'):
            json.loads(path.read_text())
    for name in names:
        assert (ROOT / 'load-generator/targets' / (name + '.txt')).read_text().strip() == f'GET http://{name}:8080/'
    original = json.loads((ROOT / 'grafana/dashboards/hello-world.json').read_text())
    comparison = json.loads((ROOT / 'grafana/dashboards-comparison/hello-world.json').read_text())
    assert original['uid'] == comparison['uid'] == 'baml-local-hello-world'
    assert len(original['panels']) == 23
    assert len(comparison['panels']) == 27
    for dashboard in (original, comparison):
        ids = [p['id'] for p in dashboard['panels']]
        assert len(ids) == len(set(ids)), 'Duplicate panel IDs'
    # Comparison preserves every original panel, shifted down by its new row.
    for old in original['panels']:
        new = copy.deepcopy(next(p for p in comparison['panels'] if p['id'] == old['id']))
        assert new['gridPos']['y'] == old['gridPos']['y'] + 11
        new['gridPos']['y'] = old['gridPos']['y']
        assert new == old, old['title']
    panels = [p for p in comparison['panels'] if p['title'].startswith('Comparison · ')]
    assert len(panels) == 4
    colors = None
    for panel in panels:
        assert len(panel['targets']) == 5
        assert {t['legendFormat'] for t in panel['targets']} == names
        for target in panel['targets']:
            assert f'experiment="{target["legendFormat"]}"' in target['expr']
            assert panel['datasource']['uid'] == 'local-prometheus'
        palette = {o['matcher']['options']: next(p['value']['fixedColor'] for p in o['properties'] if p['id'] == 'color')
                   for o in panel['fieldConfig']['overrides']}
        assert set(palette) == names and len(set(palette.values())) == 5
        if colors is None:
            colors = palette
        assert palette == colors
        for target in panel['targets']:
            expr = target['expr']
            if 'Throughput' in panel['title']:
                assert 'rate(request_seconds_count{' in expr and 'status="200"' in expr
            elif 'latency' in panel['title']:
                assert 'histogram_quantile(0.9,' in expr and 'rate(request_seconds_bucket{' in expr
            elif 'CPU' in panel['title']:
                assert expr.startswith('rate(hello_container_cpu_seconds_total{')
            else:
                assert 'Memory' in panel['title']
                assert expr == f'process_resident_memory_bytes{{experiment="{target["legendFormat"]}"}}'
    print('Source syntax, five variants, all 23 preserved panels, and 20 comparison queries pass.')


if __name__ == '__main__':
    main()
