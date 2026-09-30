#!/usr/bin/env python3
"""Tabulate bench.py --json-out reports side by side (markdown)."""
import argparse
import json
from pathlib import Path

ROWS = [
    ('streams x partials/stream', lambda r: f"{r['run']['streams']} x {r['run']['partials_per_stream']:.0f}"),
    ('accuracy vs seed', lambda r: f"{r['run']['accuracy']:.1%}"),
    ('wall s / cpu s', lambda r: f"{r['run']['wall_s']:.1f} / {r['run']['cpu_s']:.1f}"),
    ('first partial p50 ms', lambda r: f"{r['first_partial_ms']['p50']:.0f}"),
    ('partial gap p50 / p99 / max ms', lambda r: '{p50:.2f} / {p99:.1f} / {max:.0f}'.format(**r['partial_gap_ms'])),
    ('BAML GC cycles (per stream)', lambda r: f"{r['baml_gc']['cycles']} ({r['baml_gc']['cycles_per_stream']:.1f})"),
    ('BAML GC reasons', lambda r: ', '.join(f'{k} {v}' for k, v in r['baml_gc']['cycles_by_reason'].items() if v)),
    ('BAML GC pause total / max ms', lambda r: (
        f"{r['baml_gc']['pause_s'] * 1e3:.0f} / {r['baml_gc']['max_pause_s'] * 1e3:.1f}"
        if 'pause_s' in r['baml_gc'] else 'n/a (no gc_profiling)')),
    ('BAML slots reclaimed / promoted', lambda r: f"{r['baml_gc']['collected_count']:,} / {r['baml_gc']['promoted_to_gen2']:,}"),
    ('BAML peak runtime objects', lambda r: f"{r['baml_gc']['peak_runtime_objects']:,}"),
    ('BAML handles after (leak check)', lambda r: str(r['baml_gc']['active_handles_after_collect'])),
    ('Python GC gen0/1/2 collections', lambda r: '/'.join(str(v) for v in r['python_gc']['collections'].values())),
    ('Python GC pause total / max ms', lambda r: f"{r['python_gc']['pause_total_ms']:.1f} / {r['python_gc']['pause_max_ms']:.2f}"),
    ('Python objects retained', lambda r: str(r['python_gc']['objects_retained_after_collect'])),
    ('RSS before / peak / after MB', lambda r: '{rss_before:.0f} / {rss_peak:.0f} / {rss_after:.0f}'.format(**r['memory_mb'])),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('reports', nargs='+', type=Path)
    args = parser.parse_args()
    reports = [json.loads(p.read_text()) for p in args.reports]
    names = [f"{r['config']['provider']} {r['config']['api']} c{r['config']['concurrency']}" for r in reports]
    print('| metric | ' + ' | '.join(names) + ' |')
    print('|---|' + '---|' * len(names))
    for label, fmt in ROWS:
        print(f'| {label} | ' + ' | '.join(fmt(r) for r in reports) + ' |')


if __name__ == '__main__':
    main()
