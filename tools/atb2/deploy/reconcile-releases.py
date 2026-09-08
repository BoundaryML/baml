"""Resolve merged feedback issues to published stable toolchains, once per hour.

Only controller code runs here. The Git index is private and contains no agent
checkout/configuration. A published GitHub tag AND immutable package manifest
are required before a version is advertised to reporters.
"""
import fcntl
import http.client
import json
import os
from pathlib import Path
import re
import subprocess
import time
from urllib.parse import urlencode, urlsplit

REPO = 'BoundaryML/baml'
VERSION = re.compile(r'baml-language-((?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*))\Z')
SHA = re.compile(r'[0-9a-f]{40}\Z')
CURSOR = re.compile(r'[A-Za-z0-9_.:-]{1,80}\Z')


def request(host, path, token=None, method='GET', body=None, apikey=None):
    headers = {'Accept': 'application/json', 'User-Agent': 'atb2-release-index'}
    if token: headers['Authorization'] = 'Bearer ' + token
    if apikey: headers['apikey'] = apikey
    if body is not None:
        body = json.dumps(body); headers['Content-Type'] = 'application/json'
        headers['Prefer'] = 'return=representation'
    conn = http.client.HTTPSConnection(host, timeout=15)
    try:
        conn.request(method, path, body, headers)
        response = conn.getresponse()
        raw = response.read(4_000_001)
        if not 200 <= response.status < 300 or len(raw) > 4_000_000:
            raise ValueError('release/store request failed')
        return json.loads(raw) if raw else None
    finally: conn.close()


def published_releases(api):
    releases = []
    for page in range(1, 101):
        rows = api(f'/repos/{REPO}/releases?per_page=100&page={page}')
        for row in rows:
            match = VERSION.fullmatch(row.get('tag_name', ''))
            if match and not row.get('draft') and not row.get('prerelease') and row.get('published_at'):
                releases.append((row['published_at'], match[1], row['tag_name']))
        if len(rows) < 100: return sorted(releases)
    raise ValueError('release catalog incomplete')


def first_fixed_version(merge_sha, merged_at, releases, contains, manifest):
    for published_at, version, tag in releases:
        if merged_at and published_at < merged_at: continue
        if not contains(tag, merge_sha): continue
        data = manifest(version)
        if data.get('version') == version and data.get('channel') == 'canary' and data.get('artifacts'):
            return version
        # Never substitute a newer release when the earliest containing release
        # cannot be verified. A later hourly run retries publication failures.
        raise ValueError('release manifest could not be verified')
    return None


class Reconciler:
    def __init__(self, folder):
        url = urlsplit(os.environ['FEEDBACK_SUPABASE_URL'])
        if url.scheme != 'https' or not url.hostname or url.username or url.password or url.path not in ('','/') or url.query or url.fragment or url.port:
            raise ValueError('invalid store origin')
        self.host = url.hostname
        self.key = os.environ['FEEDBACK_SUPABASE_KEY']
        self.token = os.environ.get('GH_TOKEN') or os.environ.get('ATB_GITHUB_TOKEN')
        self.folder = folder
        self.env = {'PATH':'/usr/bin:/bin', 'HOME':str(folder), 'GIT_CONFIG_GLOBAL':'/dev/null',
                    'GIT_CONFIG_SYSTEM':'/dev/null','GIT_CONFIG_NOSYSTEM':'1','GIT_TERMINAL_PROMPT':'0'}
        self.tags = {}
        self.manifests = {}
        if not (folder/'index.git').exists(): self.git('init','--bare','index.git')
    def git(self, *args):
        return subprocess.run(['/usr/bin/git',*args],cwd=self.folder,env=self.env,
                              stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=120,check=True).stdout.decode().strip()
    def api(self, path): return request('api.github.com',path,self.token)
    def store(self, query, patch=None):
        return request(self.host,'/rest/v1/issues?'+query,self.key,'PATCH' if patch is not None else 'GET',patch,self.key)
    def contains(self, tag, sha):
        if tag not in self.tags:
            self.git('--git-dir=index.git','-c','core.hooksPath=/dev/null','fetch','--no-tags',
                     f'https://github.com/{REPO}.git','refs/tags/'+tag)
            self.tags[tag] = self.git('--git-dir=index.git','rev-parse','FETCH_HEAD^{commit}')
        result = subprocess.run(['/usr/bin/git','--git-dir=index.git','merge-base','--is-ancestor',sha,self.tags[tag]],
                                cwd=self.folder,env=self.env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30)
        # Missing commit means it cannot be in this fetched release's history.
        if result.returncode == 128:
            try: self.git('--git-dir=index.git','cat-file','-e',sha+'^{commit}')
            except subprocess.CalledProcessError: return False
        if result.returncode not in (0,1): raise ValueError('ancestry verification failed')
        return result.returncode == 0
    def manifest(self, version):
        if version not in self.manifests:
            self.manifests[version] = request('pkg.boundaryml.com',f'/manifest/v1/version/{version}.json')
        return self.manifests[version]
    def run(self):
        releases = None
        # The scan resumes from the persisted cursor so a large prefix of
        # still-unresolved rows cannot starve later merged issues forever.
        cursor = self.folder/'scan-cursor'
        try: after = cursor.read_text()
        except OSError: after = ''
        if not CURSOR.fullmatch(after): after = ''
        for _ in range(100):
            query = {'select':'id,status,merge_sha,merged_at','dataset':'eq.live','state':'eq.merged',
                     'fixed_in':'is.null','order':'id','limit':'100'}
            if after: query['id'] = 'gt.' + after
            rows = self.store(urlencode(query))
            for row in rows:
                after = row['id']
                try:
                    sha, merged_at = row.get('merge_sha'), row.get('merged_at')
                    # Backfill pre-feature merged issues from the PR API itself.
                    if not sha:
                        status = row.get('status')
                        match = re.fullmatch(r'https://github.com/BoundaryML/baml/pull/([1-9][0-9]{0,9})',
                                             status.get('pr','') if isinstance(status,dict) else '')
                        if not match: continue
                        pr = self.api(f'/repos/{REPO}/pulls/{match[1]}')
                        sha, merged_at = pr.get('merge_commit_sha'), pr.get('merged_at')
                        if not pr.get('merged') or not merged_at or not SHA.fullmatch(sha or ''): continue
                        self.store(urlencode({'id':'eq.'+row['id'],'dataset':'eq.live','state':'eq.merged'}),
                                   {'merge_sha':sha,'merged_at':merged_at})
                    if not SHA.fullmatch(sha or ''): continue
                    if releases is None: releases = published_releases(self.api)
                    version = first_fixed_version(sha,merged_at,releases,self.contains,self.manifest)
                    if version:
                        self.store(urlencode({'id':'eq.'+row['id'],'dataset':'eq.live','state':'eq.merged','fixed_in':'is.null','merge_sha':'eq.'+sha}),{'fixed_in':version})
                except (ValueError, OSError, subprocess.SubprocessError, KeyError, AttributeError, TypeError):
                    print('atb2: one release resolution could not be verified; will retry',flush=True)
            if len(rows) < 100:
                # A completed scan restarts from the top next hour, retrying
                # every row that is still unresolved.
                cursor.write_text('')
                return
            cursor.write_text(after)
        print('atb2: issue scan window exhausted; next run resumes from the saved cursor',flush=True)


def main():
    folder = Path('/data/release-index')
    folder.mkdir(mode=0o700,exist_ok=True)
    with (folder/'poll.lock').open('a+') as lock:
        try: fcntl.flock(lock,fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError: return
        marker = folder/'last-poll'
        if marker.exists() and time.time() - marker.stat().st_mtime < 3600: return
        Reconciler(folder).run()
        marker.touch(mode=0o600)

if __name__ == '__main__':
    try: main()
    except Exception:
        print('atb2: release resolution unavailable; no unverified versions recorded',flush=True)
        raise SystemExit(1)
