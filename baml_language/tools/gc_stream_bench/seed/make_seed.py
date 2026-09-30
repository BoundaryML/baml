#!/usr/bin/env python3
"""Snapshot a GitHub PR with `gh` and render every field of it as plain text.

The JSON is the extraction ground truth; the text is the LLM's input. Rerun to
refresh the snapshot (the PR keeps changing, so the checked-in pair is pinned):

    python3 seed/make_seed.py --pr 5041
"""
import argparse
import json
import subprocess
from pathlib import Path

FIELDS = [
    'number', 'title', 'url', 'state', 'isDraft', 'author', 'baseRefName',
    'headRefName', 'createdAt', 'updatedAt', 'closedAt', 'mergedAt', 'mergedBy',
    'mergeCommit', 'reviewDecision', 'additions', 'deletions', 'changedFiles',
    'labels', 'assignees', 'reviewRequests', 'body', 'commits', 'files',
    'comments', 'reviews',
]


def actor(a):
    if a is None:
        return 'nobody'
    parts = [a.get('name') or '', f"@{a['login']}" if a.get('login') else '']
    text = ' '.join(p for p in parts if p) or a.get('slug') or '(unknown)'
    extras = []
    if 'email' in a:
        extras.append(f"email {a['email'] or 'not public'}")
    if a.get('id'):
        extras.append(f"node id {a['id']}")
    if 'is_bot' in a:
        extras.append('bot' if a['is_bot'] else 'not a bot')
    return f"{text} ({', '.join(extras)})" if extras else text


def when(value):
    return value or 'never'


def yes(value):
    return 'yes' if value else 'no'


def reactions(groups):
    if not groups:
        return 'none'
    return ', '.join(f"{g['content']} x{g['users']['totalCount']}" for g in groups)


def block(text, indent='    '):
    lines = (text or '').splitlines() or ['(empty)']
    return '\n'.join(indent + line if line else '' for line in lines)


def render(pr):
    out = [
        f"Pull request #{pr['number']}: {pr['title']}",
        f"Link: {pr['url']}",
        f"State: {pr['state']}; draft: {yes(pr['isDraft'])}; review decision: {pr['reviewDecision'] or 'none'}",
        f"Opened by {actor(pr['author'])}",
        f"Wants to merge {pr['headRefName']} into {pr['baseRefName']}",
        (f"Created {pr['createdAt']}, last updated {pr['updatedAt']}, "
        f"closed {when(pr['closedAt'])}, merged {when(pr['mergedAt'])}"),
        (f"Merged by: {actor(pr['mergedBy'])}; merge commit: "
        f"{pr['mergeCommit']['oid'] if pr['mergeCommit'] else 'none'}"),
        (f"Size: {pr['additions']} lines added, {pr['deletions']} lines deleted, "
        f"{pr['changedFiles']} files changed"),
        'Labels: ' + (', '.join(
            f"{label['name']} (color #{label['color']}, id {label['id']}"
            f"{', ' + label['description'] if label.get('description') else ''})"
            for label in pr['labels']) or 'none'),
        'Assignees: ' + (', '.join(actor(a) for a in pr['assignees']) or 'none'),
        'Review requested from: ' + (', '.join(actor(r) for r in pr['reviewRequests']) or 'nobody'),
        '',
        'Description:',
        block(pr['body']),
        '',
        f"There are {len(pr['commits'])} commits.",
    ]
    for i, c in enumerate(pr['commits'], 1):
        out += [
            '',
            f"Commit {i}, {c['oid']}: {c['messageHeadline']}",
            f"  authored {c['authoredDate']}, committed {c['committedDate']}",
            '  by ' + '; '.join(actor(a) for a in c['authors']),
            '  full message body:',
            block(c['messageBody']),
        ]
    out += ['', f"The PR touches {len(pr['files'])} files:"]
    for f in pr['files']:
        out.append(f"  {f['path']} was {f['changeType'].lower()} "
                   f"(+{f['additions']} / -{f['deletions']})")
    out += ['', f"There are {len(pr['comments'])} conversation comments."]
    for i, c in enumerate(pr['comments'], 1):
        out += [
            '',
            (f"Comment {i} ({c['id']}) by {actor(c['author'])}, association {c['authorAssociation']}, "
            f"posted {c['createdAt']}"),
            f"  permalink {c['url']}",
            (f"  edited: {yes(c['includesCreatedEdit'])}; minimized: {yes(c['isMinimized'])}"
            f"{' (' + c['minimizedReason'] + ')' if c['minimizedReason'] else ''}; "
            f"written by the viewer: {yes(c['viewerDidAuthor'])}; reactions: {reactions(c['reactionGroups'])}"),
            '  text:',
            block(c['body']),
        ]
    out += ['', f"There are {len(pr['reviews'])} reviews."]
    for i, r in enumerate(pr['reviews'], 1):
        out += [
            '',
            (f"Review {i} ({r['id']}) by {actor(r['author'])}, association {r['authorAssociation']}: "
            f"{r['state']} at {r['submittedAt']} on commit {r['commit']['oid']}"),
            f"  edited: {yes(r['includesCreatedEdit'])}; reactions: {reactions(r['reactionGroups'])}",
            '  text:',
            block(r['body']),
        ]
    return '\n'.join(out) + '\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--pr', type=int, default=5041)
    parser.add_argument('--repo', default='BoundaryML/baml')
    parser.add_argument('--from-json', type=Path, help='Re-render an existing snapshot instead of calling gh')
    args = parser.parse_args()
    here = Path(__file__).resolve().parent
    if args.from_json:
        pr = json.loads(args.from_json.read_text())
    else:
        pr = json.loads(subprocess.check_output(
            ['gh', 'pr', 'view', str(args.pr), '--repo', args.repo, '--json', ','.join(FIELDS)]))
        pr = {k: pr[k] for k in FIELDS}
    stem = f"pr-{pr['number']}"
    (here / f'{stem}.json').write_text(json.dumps(pr, indent=2, ensure_ascii=False) + '\n')
    (here / f'{stem}.txt').write_text(render(pr))


if __name__ == '__main__':
    main()
