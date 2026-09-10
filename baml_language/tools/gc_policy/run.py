#!/usr/bin/env python3
"""Run paired GC experiments in fresh processes; keep every result and cycle."""
import argparse
import hashlib
import json
import os
import platform
from pathlib import Path
import random
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--candidate', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--change', default='Candidate implementation supplied by the caller.')
    parser.add_argument('--case', action='append', help='Run only named cases (repeatable)')
    parser.add_argument('--extended', action='store_true', help='Include long-run, cache and idle cases')
    parser.add_argument('--allow-legacy-binaries', action='store_true',
                        help='Explicitly allow binaries without build.py provenance')
    parser.add_argument('--allow-unprofiled', action='store_true', help='Timing-only feature-off comparison')
    parser.add_argument('--timeout', type=float, default=300)
    parser.add_argument('--baseline-policy', help='Override baseline policy (same workload)')
    parser.add_argument('--candidate-policy', help='Override candidate policy (same workload)')
    parser.add_argument('--budget-mib', type=int, help='Shared experimental headroom floor')
    parser.add_argument('--cache-n', type=int, help='Override permanent cache object count')
    parser.add_argument('--calls', type=int, help='Shared call count override')
    parser.add_argument('--baseline-setting', action='append', default=[], metavar='KEY=VALUE')
    parser.add_argument('--candidate-setting', action='append', default=[], metavar='KEY=VALUE')
    args = parser.parse_args()
    workspace = Path(__file__).resolve().parents[2]
    out = args.output.resolve()
    if args.repeats < 1 or args.timeout <= 0:
        parser.error('repeats and timeout must be positive')
    if any(value is not None and value <= 0 for value in [args.budget_mib, args.cache_n, args.calls]):
        parser.error('budget-mib, cache-n and calls must be positive')
    policies = {name: getattr(args, f'{name}_policy') for name in ['baseline', 'candidate']}
    runtime_keys = {'GC_RUNTIME_POLICY', 'GC_YOUNG_MIB', 'GC_FULL_MIB', 'GC_LIVE_MULTIPLIER', 'GC_LIVE_BASIS', 'GC_FIRST_CHUNK', 'GC_MAX_CHUNK', 'GC_POLL'}
    overrides = {}
    for name in ['baseline', 'candidate']:
        settings = {}
        for item in getattr(args, f'{name}_setting'):
            key, separator, value = item.partition('=')
            if not separator or key not in runtime_keys or key in settings:
                parser.error(f'Invalid/duplicate runtime setting: {item}')
            settings[key] = value
        overrides[name] = settings
    variants = {name: path.resolve() for name, path in
                [('baseline', args.baseline), ('candidate', args.candidate)]}
    provenance = {}
    for name, path in variants.items():
        manifest_path = path.parent / 'build-manifest.json'
        if manifest_path.exists():
            data = json.loads(manifest_path.read_text())
            if data['binary_sha256'] != hashlib.sha256(path.read_bytes()).hexdigest():
                parser.error(f'{name}: binary does not match its build manifest')
            provenance[name] = data
        elif not args.allow_legacy_binaries:
            parser.error(f'{name}: use build.py, or explicitly allow legacy binaries')
    cases = [
        ('compute', 'current', dict(GC_WORKLOAD='compute', GC_CALLS=128, GC_N=65536, GC_WARMUP=4)),
        ('scalar', 'full32', dict(GC_WORKLOAD='scalar', GC_CALLS=24000, GC_WARMUP=512)),
        ('tiny', 'full32', dict(GC_WORKLOAD='tiny', GC_CALLS=24000, GC_WARMUP=512)),
        ('churn', 'full32', dict(GC_WORKLOAD='churn')),
        ('retained', 'full32', dict(GC_WORKLOAD='retained')),
        ('retained_minor', 'fixed32', dict(GC_WORKLOAD='retained')),
        ('large_live_fixed', 'full32', dict(GC_WORKLOAD='retained', GC_CALLS=768, GC_N=4096, GC_RETAIN=384)),
        ('large_live_scaled', 'full_live', dict(GC_WORKLOAD='retained', GC_CALLS=768, GC_N=4096, GC_RETAIN=384)),
        ('burst', 'full32', dict(GC_WORKLOAD='burst', GC_CALLS=1536)),
        ('payload', 'full32', dict(GC_WORKLOAD='payload')),
        ('long_call', 'full32', dict(GC_WORKLOAD='churn', GC_CALLS=4, GC_N=1048576, GC_WARMUP=1)),
        ('concurrent', 'full32', dict(GC_WORKERS=8, GC_CALLS=100, GC_N=512)),
    ]
    extended = [
        ('continuous_100k', 'full32', dict(GC_WORKLOAD='tiny', GC_CALLS=100000, GC_WARMUP=512)),
        ('cache', 'full_live', dict(GC_WORKLOAD='cache', GC_CALLS=1024, GC_N=4096, GC_CACHE_N=262144)),
        ('burst_idle', 'full32', dict(GC_WORKLOAD='burst_idle', GC_CALLS=512, GC_N=2048, GC_IDLE_MS=1000)),
        ('payload_sparse', 'full_live', dict(GC_WORKLOAD='payload', GC_CALLS=128,
                                           GC_N=4194304, GC_RETAIN=4, GC_WARMUP=4)),
    ]
    if args.extended or args.case:
        cases += extended
    if args.case:
        unknown = set(args.case) - {c[0] for c in cases}
        if unknown:
            parser.error(f'Unknown cases: {sorted(unknown)}')
        cases = [c for c in cases if c[0] in args.case]
    if (any(policies.values()) or any(overrides.values()) or args.budget_mib is not None) and any(c[0] == 'concurrent' for c in cases):
        parser.error('Concurrent periodic GC does not use policy/budget overrides; select other cases')
    if args.cache_n is not None and not any(c[0] == 'cache' for c in cases):
        parser.error('cache-n requires the cache case')
    for case, _, settings in cases:
        if args.calls is not None:
            settings['GC_CALLS'] = args.calls
        if args.budget_mib is not None:
            settings['GC_BUDGET_MIB'] = args.budget_mib
        if case == 'cache' and args.cache_n is not None:
            settings['GC_CACHE_N'] = args.cache_n
    out.mkdir(parents=True, exist_ok=False)
    shutil.copy2(__file__, out / 'runner.py')
    for name in provenance:
        shutil.copy2(variants[name].parent / 'build-manifest.json', out / f'{name}-build.json')
    manifest = dict(
        runner_git_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=workspace, text=True).strip(),
        runner_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        platform=platform.platform(), libc=platform.libc_ver(),
        allocator_environment={k: os.environ[k] for k in
                               ['GLIBC_TUNABLES', 'MALLOC_CONF', 'MALLOC_ARENA_MAX',
                                'MALLOC_TRIM_THRESHOLD_', 'MALLOC_MMAP_THRESHOLD_',
                                'DYLD_INSERT_LIBRARIES', 'LD_PRELOAD'] if k in os.environ},
        binaries={name: dict(path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest())
                  for name, path in variants.items()},
        seed=4202, repeats=args.repeats, cases=cases, change=args.change,
        build_provenance={name: f'{name}-build.json' if name in provenance else None for name in variants},
        allow_unprofiled=args.allow_unprofiled,
        policy_overrides=policies,
        runtime_overrides=overrides,
    )
    (out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    # Keep baseline/candidate adjacent, randomize their order inside each pair,
    # and shuffle pairs. This limits bias from drift over a long matrix.
    rng = random.Random(4202)
    pairs = [(repeat, case) for repeat in range(args.repeats) for case in cases]
    rng.shuffle(pairs)
    with (out / 'results.jsonl').open('w') as stream:
        for pair_index, (repeat, (case, policy, settings)) in enumerate(pairs):
            order = list(variants)
            rng.shuffle(order)
            for variant in order:
                name = f'{repeat}-{case}-{variant}'
                test = 'profile_concurrent_gc' if case == 'concurrent' else 'compare_gc_policy'
                # Ambient experiment knobs must not silently change a matrix.
                env = {k: v for k, v in os.environ.items() if not k.startswith('GC_')}
                env.update({k: str(v) for k, v in settings.items()})
                env.update(overrides[variant])
                requested_policy = policies[variant] or policy
                env.update(GC_POLICY=requested_policy, GC_TRACE=str(out / f'{name}.cycles.json'))
                try:
                    run = subprocess.run([str(variants[variant]), '--ignored', '--nocapture', '--exact', test],
                                         cwd=workspace, env=env, text=True, capture_output=True, timeout=args.timeout)
                except subprocess.TimeoutExpired as error:
                    # TimeoutExpired can carry bytes even with text=True.
                    decode = lambda value: value.decode(errors='replace') if isinstance(value, bytes) else (value or '')
                    (out / f'{name}.log').write_text(decode(error.stdout) + decode(error.stderr) + '\nTIMED OUT\n')
                    raise RuntimeError(f'{name} timed out; see its log') from error
                (out / f'{name}.log').write_text(run.stdout + run.stderr)
                if run.returncode:
                    raise RuntimeError(f'{name} failed: see {out / (name + ".log")}')
                prefix = 'GC_CONCURRENT_RESULT ' if case == 'concurrent' else 'GC_POLICY_RESULT '
                lines = [line[len(prefix):] for line in run.stdout.splitlines() if line.startswith(prefix)]
                if len(lines) != 1:
                    raise RuntimeError(f'{name}: missing/ambiguous result')
                result = json.loads(lines[0])
                if case != 'concurrent':
                    expected_runtime = overrides[variant].get('GC_RUNTIME_POLICY', 'off')
                    if result.get('runtime_settings', {}).get('name', 'off') != expected_runtime:
                        raise RuntimeError(f'{name}: runtime policy unsupported or not applied')
                    config_fields = {
                        'GC_YOUNG_MIB': ('young_budget', 1048576),
                        'GC_FULL_MIB': ('full_budget_floor', 1048576),
                        'GC_LIVE_MULTIPLIER': ('live_multiplier', 1),
                        'GC_FIRST_CHUNK': ('first_chunk', 1),
                        'GC_MAX_CHUNK': ('max_chunk', 1),
                        'GC_POLL': ('poll_interval', 1),
                    }
                    for key, (field, scale) in config_fields.items():
                        if key in overrides[variant] and result.get('runtime_settings', {}).get(field) != int(overrides[variant][key]) * scale:
                            raise RuntimeError(f'{name}: runtime setting {key} unsupported or not applied')
                    if 'GC_LIVE_BASIS' in overrides[variant] and result.get('runtime_settings', {}).get('live_basis') != overrides[variant]['GC_LIVE_BASIS']:
                        raise RuntimeError(f'{name}: runtime live basis unsupported or not applied')
                    if result['policy'] != requested_policy:
                        raise RuntimeError(f'{name}: policy override was not applied')
                    if args.budget_mib is not None and result.get('budget_floor_bytes') != args.budget_mib * 1048576:
                        raise RuntimeError(f'{name}: budget override unsupported or not applied')
                cycles = json.loads((out / f'{name}.cycles.json').read_text())
                for cycle in cycles:
                    if cycle['profile'] is None and not args.allow_unprofiled:
                        raise RuntimeError('Build both binaries with --features gc_profiling')
                result.update(case=case, variant=variant, repeat=repeat, cycles_file=f'{name}.cycles.json')
                stream.write(json.dumps(result) + '\n')
                stream.flush()
                print(f'{pair_index+1}/{len(pairs)} {name}: {result["elapsed_seconds"]:.3f}s, '
                      f'{len(cycles)} recorded collections', flush=True)


if __name__ == '__main__':
    main()
