#!/usr/bin/env python3
"""Summarize paired results and export collection timelines for trace viewers."""
import argparse
import collections
import csv
import json
from pathlib import Path
import statistics


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    args = parser.parse_args()
    root = args.directory
    manifest = json.loads((root / 'manifest.json').read_text())
    rows = [json.loads(line) for line in (root / 'results.jsonl').read_text().splitlines()]
    expected = len(manifest['cases']) * manifest['repeats'] * 2
    if len(rows) != expected:
        parser.error(f'Matrix incomplete: {len(rows)}/{expected} runs')
    phases = ['prepare', 'trace', 'keepalive', 'fixup', 'reclaim', 'bookkeeping']
    groups = collections.defaultdict(dict)
    expected_keys = {(case[0], repeat) for case in manifest['cases'] for repeat in range(manifest['repeats'])}
    for row in rows:
        key = (row['case'], row['repeat'])
        assert key in expected_keys, f'Unexpected run: {key}'
        assert row['variant'] not in groups[key], key
        groups[key][row['variant']] = row
        cycles = json.loads((root / row['cycles_file']).read_text())
        profiled = [cycle for cycle in cycles if cycle['profile'] is not None]
        sums = collections.defaultdict(float)
        timeline = []
        for cycle in profiled:
            profile = cycle['profile']
            # End timestamps are sampled by the caller immediately after the
            # collection returns. Reconstruct relative spans; do not pretend
            # these are scheduler events or exact cross-thread timestamps.
            start_ms = cycle['at_seconds'] * 1000 - profile['total_ms']
            for phase in phases + ['park_wait', 'root_scan', 'holder_fixup', 'post_gc', 'pause', 'total']:
                sums[phase + '_ms'] += profile[phase + '_ms']
            assert profile['heap_total_ms'] + 0.001 >= sum(profile[p + '_ms'] for p in phases)
            assert profile['total_ms'] + 0.001 >= profile['park_wait_ms'] + profile['pause_ms'] + profile['post_gc_ms']
            pos = start_ms
            durations = [(phase, profile[phase + '_ms'])
                         for phase in ['park_wait', 'root_scan'] + phases]
            # The end snapshot and small instrumentation gaps are included in
            # heap_total but excluded from the disjoint phase laps. Preserve
            # that gap so later reconstructed spans are not shifted earlier.
            durations.append(('heap_unattributed', max(0, profile['heap_total_ms'] -
                                                       sum(profile[p + '_ms'] for p in phases))))
            durations += [(phase, profile[phase + '_ms']) for phase in ['holder_fixup', 'post_gc']]
            for phase, duration in durations:
                timeline.append(dict(name=phase, cat='gc', ph='X', pid=1, tid=1,
                                     ts=pos * 1000, dur=duration * 1000,
                                     args=dict(trigger=cycle['trigger'])))
                pos += duration
            for when, sizes in [(start_ms, profile['before_generations']),
                                (cycle['at_seconds'] * 1000, profile['after_generations'])]:
                if 'slot_bytes' in row:
                    timeline.append(dict(name='Object slot storage (MiB)', ph='C', pid=1, tid=1,
                                         ts=when * 1000,
                                         args={f'gen{i}': count * row['slot_bytes'] / 1048576
                                               for i, count in enumerate(sizes)}))
        row['recorded_phase_totals_ms'] = dict(sums)
        row['recorded_max_pause_ms'] = max((c['profile']['pause_ms'] for c in profiled), default=0.0 if row.get('measured_cycle_reasons') is not None else None)
        row['recorded_max_park_wait_ms'] = max((c['profile']['park_wait_ms'] for c in profiled), default=None)
        row['recorded_pauses_over_ms'] = {
            str(limit): sum(c['profile']['pause_ms'] > limit for c in profiled)
            for limit in [5, 20, 100]
        }
        row['recorded_copied_objects'] = sum(c.get('copied_objects', 0) for c in cycles)
        row['actual_new_objects_in_recorded_cycles'] = sum(c['profile']['actual_new_objects'] for c in profiled)
        row['reserved_gen0_slots_in_recorded_cycles'] = sum(c['profile']['before_generations'][0] for c in profiled)
        (root / row['cycles_file'].replace('.cycles.json', '.trace.json')).write_text(
            json.dumps(dict(traceEvents=timeline, displayTimeUnit='ms')) + '\n')
    by_case = collections.defaultdict(list)
    for (case, repeat), pair in groups.items():
        assert set(pair) == {'baseline', 'candidate'}
        by_case[case].append(pair)
    lines = ['# Paired GC profiling results', '',
             f'{len(rows)} successful runs, {manifest["repeats"]} repetitions per case and binary. '
             'Each pair ran adjacent in randomized order; pairs were also shuffled. '
             'Reported changes are medians of paired ratios, not ratios of unrelated medians.', '',
             'Baseline and candidate use the same workload and warmup. Policy overrides: '
             + json.dumps(manifest.get('policy_overrides', {}), sort_keys=True) + '. '
             + manifest.get('change', 'The candidate optimizes the unhandled-spawn-error scan.') +
             ' Binary hashes and experiment settings are recorded in the manifest.', '',
             '| Case | Baseline s | Candidate s | Speedup (paired range) | Candidate peak slots MiB | Candidate longest recorded pause ms |',
             '|---|---:|---:|---:|---:|---:|']
    for case, pairs in sorted(by_case.items()):
        assert len(pairs) == manifest['repeats']
        med = lambda variant, key: statistics.median(p[variant][key] for p in pairs)
        ratios = [p['baseline']['elapsed_seconds'] / p['candidate']['elapsed_seconds'] for p in pairs]
        slots = '-' if case == 'concurrent' else f'{med("candidate", "peak_slot_mib"):.1f}'
        pauses = [p['candidate']['recorded_max_pause_ms'] for p in pairs]
        pause = f'{statistics.median(pauses):.1f}' if all(p is not None for p in pauses) else '-'
        lines.append(f'| {case} | {med("baseline", "elapsed_seconds"):.3f} | '
                     f'{med("candidate", "elapsed_seconds"):.3f} | {statistics.median(ratios):.2f}× '
                     f'({min(ratios):.2f}–{max(ratios):.2f}) | {slots} | {pause} |')
    lines += ['', '## Pause frequency and caller latency', '',
              'Medians across runs. Pause counts cover recorded collections in the timed window (see each run’s cycle_coverage); '
              'validation and warmup GC are excluded. Counts over 5/20/100 ms are nested, not disjoint. '
              'Caller p99 can miss an infrequent long pause, so maxima are reported too. '
              'A shared budget floor is not necessarily equal peak memory: live-scaled policies can grant more headroom.', '',
              '| Case | Variant / policy | Minor / full | GC wall % | Pauses >5 / >20 / >100 ms | Max pause ms | Call p99 / max ms | Peak slots MiB |',
              '|---|---|---:|---:|---:|---:|---:|---:|']
    for case, pairs in sorted(by_case.items()):
        for variant in ['baseline', 'candidate']:
            runs = [p[variant] for p in pairs]
            if case == 'concurrent':
                continue
            med = lambda key: statistics.median(r[key] for r in runs)
            counts = ' / '.join(f'{statistics.median(r["recorded_pauses_over_ms"][str(n)] for r in runs):g}'
                                for n in [5, 20, 100])
            pause = med('recorded_max_pause_ms') if all(r['recorded_max_pause_ms'] is not None for r in runs) else None
            pause_text = f'{pause:.1f}' if pause is not None else '-'
            if pause is None and any(r['minor_count'] + r['major_count'] > 0 for r in runs):
                counts = '-'
            gc_percent = max(0.0, statistics.median(r['gc_ms_total'] / (r['elapsed_seconds'] * 10) for r in runs))
            lines.append(f'| {case} | {variant} / {runs[0]["policy"]} | '
                         f'{med("minor_count"):g} / {med("major_count"):g} | {gc_percent:.1f} | {counts} | '
                         f'{pause_text} | {med("call_ms_p99"):.2f} / {med("call_ms_max"):.1f} | '
                         f'{med("peak_slot_mib"):.1f} |')
    tradeoffs = []
    lines += ['', '## CPU, throughput and resident memory', '',
              'Medians across fresh processes. CPU is user + kernel time. Heap GC CPU excludes engine root handling '
              'and callbacks; remaining CPU is not pure application CPU. Lifetime peak RSS includes compilation and warmup; '
              'sampled RSS can miss peaks. Final cleanup is shown separately so deferring GC is visible.', '',
              '| Case | Variant | Wall s | CPU s | Heap GC CPU s | Peak RSS MiB | End RSS MiB | Final cleanup CPU s |',
              '|---|---|---:|---:|---:|---:|---:|---:|']
    metrics = ['elapsed_seconds', 'calls_per_second', 'process_cpu_seconds', 'gc_heap_cpu_seconds',
               'non_gc_heap_cpu_seconds', 'process_lifetime_peak_rss_mib', 'initial_rss_mib',
               'peak_sampled_rss_mib', 'end_rss_mib', 'after_cleanup_rss_mib', 'cleanup_seconds',
               'cleanup_cpu_seconds', 'elapsed_with_cleanup_seconds', 'cpu_with_cleanup_seconds',
               'recorded_max_pause_ms', 'call_ms_p99', 'call_ms_max', 'minor_count', 'major_count', 'charged_bytes']
    for case, pairs in sorted(by_case.items()):
        for variant in ['baseline', 'candidate']:
            runs = [p[variant] for p in pairs]
            entry = dict(case=case, variant=variant, runtime_settings=runs[0].get('runtime_settings'))
            for key in metrics:
                values = [r.get(key) for r in runs]
                entry[key] = statistics.median(values) if all(v is not None for v in values) else None
            tradeoffs.append(entry)
            cells = [entry[k] for k in ['elapsed_seconds', 'process_cpu_seconds', 'gc_heap_cpu_seconds',
                                      'process_lifetime_peak_rss_mib', 'end_rss_mib', 'cleanup_cpu_seconds']]
            lines.append(f'| {case} | {variant} | ' + ' | '.join('-' if v is None else f'{v:.3f}' for v in cells) + ' |')
    with (root / 'tradeoffs.csv').open('w', newline='') as stream:
        writer = csv.DictWriter(stream, fieldnames=['case', 'variant', 'runtime_settings'] + metrics)
        writer.writeheader()
        writer.writerows(tradeoffs)
    lines += ['', '## Where collection time went', '',
              'Medians of per-run recorded totals. New single-threaded runs capture automatic and explicit cycles; '
              'older runs and concurrent runs may capture only harness-requested collections. '
              'Check cycle_coverage and measured_cycle_reasons in the raw results.', '',
              '| Case | Binary | Trace/copy ms | Error/finalizer scans ms | Pointer fixup ms | Reclaim ms | Wait to park ms |',
              '|---|---|---:|---:|---:|---:|---:|']
    for case, pairs in sorted(by_case.items()):
        for variant in ['baseline', 'candidate']:
            if any(p[variant]['recorded_max_pause_ms'] is None for p in pairs):
                lines.append(f'| {case} | {variant} | - | - | - | - | - |')
                continue
            values = [statistics.median(p[variant]['recorded_phase_totals_ms'].get(k + '_ms', 0)
                                       for p in pairs) for k in ['trace', 'keepalive', 'fixup', 'reclaim', 'park_wait']]
            lines.append(f'| {case} | {variant} | ' + ' | '.join(f'{v:.1f}' for v in values) + ' |')
    lines += ['', '## Reading these measurements', '',
              '- Time and memory must be compared together. `large_live_fixed` and `large_live_scaled` use identical '
              'workloads; full_live grants max(configured floor, surviving slot bytes) between full collections. '
              'This is a policy experiment, not a change to engine defaults.',
              '- Slots include reservation slack. `actual_new_objects_in_recorded_cycles` excludes unused slots; '
              'the original `collected_count` means reclaimed slots and must not be interpreted as dead-object count.',
              '- The `payload` case passes 64 KiB strings and retains 256 results; `payload_sparse` uses 4 MiB '
              'strings and retains four. Payload bytes, compiler memory, '
              'allocator retention and scratch copies are not interchangeable with slot counts. RSS samples are in the raw results; '
              'they are sampled every 32 calls, include startup/compilation, and can miss brief peaks.',
              '- The long-call case allocates roughly 64 MiB of slots within each call. A 32 MiB threshold checked '
              'only between calls cannot constrain that in-call growth. Production scheduling still needs safe allocation checkpoints.',
              '- `park_wait` is time waiting to acquire all permits; different callers may stop at different times. '
              '`pause` is the exclusive-permit window. Post-GC callbacks and finalizers run after it. '
              'All are wall times, not CPU samples.',
              '- Tiny/scalar use 512 warmup calls; most other cases use 32, long-call uses one. Warmup cleanup '
              'and post-run collection/retained-result validation are outside measured time. Retained arrays '
              'are checked by summing every element after a moving collection. Payload lengths and concurrent array sums are checked.',
              '- These are synthetic engine tests, not production SDK benchmarks. Check the build manifests for '
              'profile, features and compiler settings. A few paired repetitions identify large differences; small differences can be noise.',
              '- Instrumentation is feature-gated. Missing phase measurements are shown as unavailable. '
              'Compare equally instrumented builds to attribute collector changes; a feature-on/off comparison measures different overhead.',
              '- Per-run `.trace.json` files can be opened in a Chrome Trace-compatible viewer. Phase start times are '
              'reconstructed from measured durations and caller timestamps. They are diagnostic timelines, not exact scheduling traces.', '']
    (root / 'RESULTS.md').write_text('\n'.join(lines))
    (root / 'summary.json').write_text(json.dumps(rows, indent=2) + '\n')
    print('\n'.join(lines[:lines.index('## Where collection time went')]))


if __name__ == '__main__':
    main()
