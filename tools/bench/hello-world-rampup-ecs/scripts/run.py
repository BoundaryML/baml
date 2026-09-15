#!/usr/bin/env python3
"""Explicitly deploy or inspect one named ECS experiment; no implicit AWS writes."""
import argparse
import datetime
import json
import os
import re
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--region', default=os.environ.get('AWS_REGION', 'us-east-1'))
    p.add_argument('--aws-profile')
    sub = p.add_subparsers(dest='command', required=True)
    up = sub.add_parser('up')
    up.add_argument('--name', required=True, help='Unique run/stack name; use a fresh name for a fresh experiment')
    up.add_argument('--images', type=Path, required=True)
    up.add_argument('--profile', type=Path, required=True, help='Load profile JSON')
    up.add_argument('--app-count', type=int, choices=[0, 1], default=1)
    up.add_argument('--load-count', type=int, choices=[0, 1], default=1)
    up.add_argument('--target-cell', choices=['python-only-arm64', 'python-only-x64', 'python-baml-arm64', 'python-baml-x64',
                                              'node-only-arm64', 'node-only-x64', 'node-baml-arm64', 'node-baml-x64',
                                              'baml-only-arm64', 'baml-only-x64'])
    up.add_argument('--local-load-cidr', help='Expose a single target to this IPv4 /32; requires --target-cell and --load-count 0')
    status = sub.add_parser('status')
    status.add_argument('--name', required=True)
    a = p.parse_args()
    if not re.fullmatch(r'[a-z][a-z0-9-]{1,30}[a-z0-9]', a.name):
        p.error('Run name must be 3..32 lowercase letters, digits, or hyphens, starting with a letter')
    aws = ['aws', '--region', a.region]
    if a.aws_profile:
        aws += ['--profile', a.aws_profile]

    def fetch(*args):
        return json.loads(subprocess.check_output(aws + list(args) + ['--output', 'json']))

    if a.command == 'up':
        if a.local_load_cidr and (not a.target_cell or a.load_count != 0 or not re.fullmatch(r'(?:[0-9]{1,3}\.){3}[0-9]{1,3}/32', a.local_load_cidr)):
            p.error('--local-load-cidr requires a valid IPv4 /32, --target-cell, and --load-count 0')
        output = ROOT / 'artifacts' / a.name / 'outputs.json'
        output.parent.mkdir(parents=True, exist_ok=True)
        config = {'run': a.name, 'region': a.region, 'profile': json.loads(a.profile.read_text()),
                  'images': json.loads(a.images.read_text()), 'app_count': a.app_count, 'load_count': a.load_count,
                  'target_cell': a.target_cell, 'local_load_cidr': a.local_load_cidr}
        config_path = output.parent / 'deployment-config.json'
        if config_path.exists():
            stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
            (output.parent / f'deployment-config-before-{stamp}.json').write_bytes(config_path.read_bytes())
        config_path.write_text(json.dumps(config, indent=2) + '\n')
        env = dict(os.environ, AWS_REGION=a.region, AWS_DEFAULT_REGION=a.region)
        if a.aws_profile:
            env['AWS_PROFILE'] = a.aws_profile
        subprocess.run(['npx', 'cdk', 'deploy', a.name, '--require-approval', 'never',
            '--outputs-file', str(output), '--output', str(output.parent / 'cdk.out'), '-c', f'run={a.name}',
            '-c', f'images={a.images.resolve()}', '-c', f'profile={a.profile.resolve()}',
            '-c', f'appCount={a.app_count}', '-c', f'loadCount={a.load_count}',
            *(['-c', f'targetCell={a.target_cell}'] if a.target_cell else []),
            *(['-c', f'localLoadCidr={a.local_load_cidr}'] if a.local_load_cidr else [])], cwd=ROOT, env=env, check=True)
        print(f'Run {a.name}: CloudWatch dashboard {a.name}. Inspect status and task-events before calling this a pass.')
    else:
        stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
        output = ROOT / 'artifacts' / a.name / stamp
        output.mkdir(parents=True, exist_ok=True)
        records = {'stack': fetch('cloudformation', 'describe-stacks', '--stack-name', a.name),
                   'resources': fetch('cloudformation', 'describe-stack-resources', '--stack-name', a.name),
                   'cluster': fetch('ecs', 'describe-clusters', '--clusters', a.name),
                   'instances': fetch('ec2', 'describe-instances', '--filters', f'Name=tag:RunName,Values={a.name}')}
        services = fetch('ecs', 'list-services', '--cluster', a.name)['serviceArns']
        records['services'] = []
        for i in range(0, len(services), 10):
            records['services'].append(fetch('ecs', 'describe-services', '--cluster', a.name, '--services', *services[i:i+10]))
        records['tasks'] = []
        for state in ('RUNNING', 'STOPPED'):
            arns = fetch('ecs', 'list-tasks', '--cluster', a.name, '--desired-status', state)['taskArns']
            for i in range(0, len(arns), 100):
                records['tasks'].append(fetch('ecs', 'describe-tasks', '--cluster', a.name, '--tasks', *arns[i:i+100]))
        for key, value in records.items():
            (output / (key + '.json')).write_text(json.dumps(value, indent=2) + '\n')
        print(output)
        for batch in records['services']:
            for service in batch['services']:
                print(service['serviceName'], 'desired', service['desiredCount'], 'running', service['runningCount'], 'pending', service['pendingCount'])


if __name__ == '__main__':
    main()
