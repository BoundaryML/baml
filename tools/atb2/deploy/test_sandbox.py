"""Offline security regression tests. Run in the runner image with user namespaces.
No network access, real credentials, or model requests are needed.
"""
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
import urllib.error
import urllib.request
from unittest.mock import patch

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('sandbox', Path(__file__).with_name('sandbox.py'))
sandbox = importlib.util.module_from_spec(spec); spec.loader.exec_module(sandbox)
spec = importlib.util.spec_from_file_location('push', Path(__file__).with_name('push.py'))
push = importlib.util.module_from_spec(spec); spec.loader.exec_module(push)


class BrokerTests(unittest.TestCase):
    def test_saved_login_uses_private_lock_and_returns_cached_token(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / '.credentials.json'
            path.write_text(json.dumps({'claudeAiOauth': {
                'accessToken': 'private-fixture', 'expiresAt': (time.time() + 3600) * 1000}}))
            with patch.object(sandbox.http.client, 'HTTPSConnection') as connection:
                self.assertEqual(sandbox.CredentialStore(path).token(), 'private-fixture')
                connection.assert_not_called()
            self.assertEqual((path.parent / '.atb2-oauth.lock').stat().st_mode & 0o777, 0o600)

    def test_login_lock_refuses_symlinks(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / '.credentials.json'
            (path.parent / '.atb2-oauth.lock').symlink_to(path.parent / 'other')
            with self.assertRaises(OSError):
                sandbox.CredentialStore(path).token()

    def test_broker_replaces_auth_and_refuses_other_destinations(self):
        seen = []
        class Credentials:
            def token(self): return 'private-fixture'
        class Connection:
            def request(self, method, path, body, headers): seen.append((method,path,headers))
            def getresponse(self):
                r = io.BytesIO(b'{"ok":true}'); r.status = 200
                r.getheader = lambda *_: 'application/json'
                return r
            def close(self): pass
        with sandbox.broker(Credentials()) as broker:
            broker.connect = Connection
            def send(path, token):
                req = urllib.request.Request('http://127.0.0.1:'+str(broker.server_port)+path,
                    data=b'{}', headers={'Authorization':'Bearer '+token})
                try:
                    with urllib.request.urlopen(req) as r: return r.status, r.read()
                except urllib.error.HTTPError as r: return r.code, r.read()
            self.assertEqual(send('/v1/messages', 'wrong')[0],403)
            self.assertEqual(send('/api/oauth/token',broker.client_token)[0],403)
            self.assertEqual(send('/v1/messages',broker.client_token),(200,b'{"ok":true}'))
        self.assertEqual(len(seen),1)
        self.assertEqual(seen[0][2]['Authorization'],'Bearer private-fixture')

    def test_upstream_failure_after_headers_does_not_append_an_http_error(self):
        class Credentials:
            def token(self): return 'private-fixture'
        class Response:
            status = 200
            def getheader(self, *_): return 'text/event-stream'
            def read1(self, _): raise OSError('private-upstream-diagnostic')
        class Connection:
            def request(self, *_): pass
            def getresponse(self): return Response()
            def close(self): pass
        with sandbox.broker(Credentials()) as broker:
            broker.connect = Connection
            req = urllib.request.Request('http://127.0.0.1:'+str(broker.server_port)+'/v1/messages',
                data=b'{}', headers={'Authorization':'Bearer '+broker.client_token})
            with urllib.request.urlopen(req) as response:
                self.assertEqual(response.status, 200)
                self.assertEqual(response.read(), b'')

    def test_broker_errors_never_echo_credentials(self):
        class Credentials:
            def token(self): raise ValueError('private-fixture')
        with sandbox.broker(Credentials()) as broker:
            req=urllib.request.Request('http://127.0.0.1:'+str(broker.server_port)+'/v1/messages',
                data=b'{}',headers={'Authorization':'Bearer '+broker.client_token})
            with self.assertRaises(urllib.error.HTTPError) as error: urllib.request.urlopen(req)
            self.assertNotIn(b'private-fixture',error.exception.read())


@unittest.skipUnless(sys.platform=='linux' and Path('/usr/bin/bwrap').exists(), 'Linux bubblewrap required')
class BoundaryTests(unittest.TestCase):
    def setUp(self):
        self.work = Path(tempfile.mkdtemp(dir='/data/worktrees',prefix='security-'))
        self.addCleanup(lambda: __import__('shutil').rmtree(self.work))
        Path('/data/home/security-fixture').write_text('private-fixture')
        self.addCleanup(lambda: Path('/data/home/security-fixture').unlink())

    def isolated(self,*argv):
        return subprocess.run(sandbox.command(str(self.work),list(argv)),env={'PATH':sandbox.SAFE_PATH},
            capture_output=True,text=True,timeout=30)

    def test_files_processes_and_controller_git_are_not_visible(self):
        (self.work/'escape').symlink_to('/data/home/security-fixture')
        code='''import os,pathlib
assert os.environ['HOME']=='/home/agent'
assert not pathlib.Path('/data/home').exists()
assert not pathlib.Path('/data/repo').exists()
assert not pathlib.Path('escape').exists()
for p in pathlib.Path('/proc').glob('[0-9]*/environ'):
 try: assert b'FAKE_APP_SECRET' not in p.read_bytes()
 except PermissionError: pass
pathlib.Path('allowed-write').write_text('ok')
print('isolated')'''
        with patch.dict(os.environ,{'FAKE_APP_SECRET':'private-fixture'}):
            result=self.isolated('/usr/bin/python3','-c',code)
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual((self.work/'allowed-write').read_text(),'ok')

    def test_project_hook_does_not_run_with_planner_flags(self):
        (self.work/'.claude').mkdir()
        marker=self.work/'hook-ran'
        (self.work/'.claude/settings.json').write_text(json.dumps({'hooks':{'SessionStart':[{'hooks':[
            {'type':'command','command':'touch '+str(marker)}]}]}}))
        result=self.isolated('claude','-p','Return OK','--output-format','stream-json','--verbose',
            '--permission-mode','bypassPermissions','--safe-mode','--setting-sources','',
            '--strict-mcp-config','--mcp-config','{"mcpServers":{}}','--settings','{"disableAllHooks":true}',
            '--no-session-persistence','--tools','Read,Glob,Grep')
        self.assertFalse(marker.exists(),result.stdout+result.stderr)
        self.assertNotIn('unknown option',result.stderr)
        self.assertNotIn('bwrap:',result.stderr)

    def test_claude_uses_broker_without_real_login(self):
        seen = []
        class Credentials:
            def token(self): return 'private-fixture'
        class Connection:
            def request(self, method, path, body, headers):
                seen.append((path, headers))
            def getresponse(self):
                events = [
                    ('message_start', {'type':'message_start','message':{'id':'msg_fixture','type':'message','role':'assistant','content':[], 'model':'claude-sonnet-4-6','stop_reason':None,'stop_sequence':None,'usage':{'input_tokens':1,'output_tokens':0}}}),
                    ('content_block_start', {'type':'content_block_start','index':0,'content_block':{'type':'text','text':''}}),
                    ('content_block_delta', {'type':'content_block_delta','index':0,'delta':{'type':'text_delta','text':'OK'}}),
                    ('content_block_stop', {'type':'content_block_stop','index':0}),
                    ('message_delta', {'type':'message_delta','delta':{'stop_reason':'end_turn','stop_sequence':None},'usage':{'output_tokens':1}}),
                    ('message_stop', {'type':'message_stop'}),
                ]
                raw = ''.join('event: '+name+'\ndata: '+json.dumps(value)+'\n\n' for name,value in events)
                r=io.BytesIO(raw.encode()); r.status=200
                r.getheader=lambda *_:'text/event-stream'
                return r
            def close(self): pass
        with sandbox.broker(Credentials()) as broker:
            broker.connect=Connection
            args=sandbox.command(str(self.work), ['claude','-p','Return OK','--model','claude-sonnet-4-6',
                '--safe-mode','--setting-sources','','--strict-mcp-config','--mcp-config','{"mcpServers":{}}',
                '--settings','{"disableAllHooks":true}','--no-session-persistence','--tools','Read,Glob,Grep'],
                {'ANTHROPIC_BASE_URL':'http://127.0.0.1:'+str(broker.server_port),
                 'CLAUDE_CODE_OAUTH_TOKEN':broker.client_token,'CLAUDE_CODE_SAFE_MODE':'1'})
            result=subprocess.run(args,env={'PATH':sandbox.SAFE_PATH},capture_output=True,text=True,timeout=30)
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        self.assertIn('OK',result.stdout)
        self.assertTrue(seen)
        self.assertTrue(all(h['Authorization']=='Bearer private-fixture' for _,h in seen))
        self.assertNotIn('private-fixture',result.stdout+result.stderr)

    def test_symlinked_git_metadata_is_refused(self):
        (self.work/'.git').symlink_to('/data/repo',target_is_directory=True)
        with self.assertRaises(ValueError): sandbox.workspace(str(self.work))

    def test_linked_worktrees_are_refused(self):
        (self.work/'.git').write_text('gitdir: /data/repo/.git/worktrees/unsafe')
        with self.assertRaises(ValueError): sandbox.workspace(str(self.work))

    def test_shared_cli_is_readable_but_not_writable_from_sandbox(self):
        cache = Path('/data/cli-cache')
        cache.mkdir(exist_ok=True)
        cache.chmod(0o755)
        fixture = cache / 'test-readonly'
        fixture.write_text('cached fixture')
        fixture.chmod(0o444)
        self.addCleanup(fixture.unlink)
        code = """from pathlib import Path
p = Path('/data/cli-cache/test-readonly')
assert p.read_text() == 'cached fixture'
assert not Path('/data/cli-build/request.sock').exists()
try: p.write_text('poison')
except OSError: pass
else: raise AssertionError('shared cache was writable')
"""
        result = self.isolated('/usr/bin/python3', '-c', code)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_workspace_caches_cannot_poison_other_sessions(self):
        with tempfile.TemporaryDirectory(dir='/data/worktrees') as other:
            poison = """import os
from pathlib import Path
cache=Path(os.environ['CARGO_HOME']);cache.mkdir(parents=True,exist_ok=True)
(cache/'config.toml').write_text('[build]\\nrustc = "/data/agent-cache/poison"\\n')
p=Path('/data/agent-cache/poison');p.write_text('#!/bin/sh\\necho POISONED\\n');p.chmod(0o755)
try: Path(os.environ['RUSTUP_HOME'],'settings.toml').write_text('poison')
except OSError: pass
else: raise AssertionError('shared toolchain was writable')
"""
            result=self.isolated('python3','-c',poison)
            self.assertEqual(result.returncode,0,result.stderr)
            result=subprocess.run(sandbox.command(other,['bash','-c',
                'test ! -e "$CARGO_HOME/config.toml" && test ! -e /data/agent-cache/poison && cargo --version && rustc --version']),
                capture_output=True,text=True,timeout=30)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertNotIn('POISONED',result.stdout)

    def test_gate_tools_and_compiled_binary_are_available_in_private_target(self):
        (self.work/'src').mkdir()
        (self.work/'Cargo.toml').write_text('[package]\nname="gate_fixture"\nversion="0.0.0"\nedition="2021"\n')
        (self.work/'src/main.rs').write_text('fn main() { println!("patched-fixture"); }')
        result=self.isolated('bash','-c',
            'cargo nextest --version && cargo insta --version && cargo build --offline && "$CARGO_TARGET_DIR/debug/gate_fixture"')
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertIn('patched-fixture',result.stdout)

    def test_push_ignores_agent_origin_and_credential_helper(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder=Path(tmp)
            def git(cwd,*args):
                return subprocess.check_output(['/usr/bin/git',*args],cwd=cwd,stderr=subprocess.DEVNULL).decode().strip()
            for name in ['intended.git','attacker.git']:git(folder,'init','--bare',name)
            git(self.work,'init','-b','fix');git(self.work,'config','user.name','fixture');git(self.work,'config','user.email','fixture@example.invalid')
            (self.work/'file').write_text('base');git(self.work,'add','.');git(self.work,'commit','-m','base')
            base=git(self.work,'rev-parse','HEAD')
            for name in ['intended.git','attacker.git']:git(self.work,'push',str(folder/name),'fix')
            git(self.work,'remote','add','origin',str(folder/'attacker.git'))
            git(self.work,'config','credential.helper','!touch '+str(folder/'token-helper-ran'))
            git(self.work,'config','url.'+str(folder/'attacker.git')+'.insteadOf',push.REPOSITORY)
            (self.work/'file').write_text('fix');git(self.work,'commit','-am','fix')
            commit=git(self.work,'rev-parse','HEAD')
            real_run=subprocess.run
            def intercept(args,**kwargs):
                if args[0]=='/usr/bin/git' and 'push' in args:
                    self.assertIn(push.REPOSITORY,args)
                    self.assertEqual(args[-1],commit+':refs/heads/fix')
                    # Replace only the network transport in this offline test.
                    args=[str(folder/'intended.git') if a==push.REPOSITORY else
                          'protocol.file.allow=always' if a=='protocol.https.allow=always' else a for a in args]
                return real_run(args,**kwargs)
            with patch.object(sys,'argv',['push.py',str(self.work),'fix',base,base]), patch.dict(os.environ,{'GH_TOKEN':'fixture'}), patch.object(subprocess,'run',side_effect=intercept):
                self.assertEqual(push.main(),0)
            self.assertEqual(git(folder/'intended.git','rev-parse','fix'),commit)
            self.assertEqual(git(folder/'attacker.git','rev-parse','fix'),base)
            self.assertFalse((folder/'token-helper-ran').exists())


if __name__=='__main__': unittest.main()
