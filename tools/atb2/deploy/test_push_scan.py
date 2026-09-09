"""Outgoing history is checked before credentials can be used for a push."""
import contextlib
import io
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch, MagicMock

spec = importlib.util.spec_from_file_location('scan', Path(__file__).with_name('push-scan.py'))
scan = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scan)

class ScanTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.git('init', 'work')
        self.git('-C', 'work', 'config', 'user.email', 'fixture@example.invalid')
        self.git('-C', 'work', 'config', 'user.name', 'fixture')
        self.commit('file', 'base')
        self.base = self.git('-C','work','rev-parse','HEAD',stdout=subprocess.PIPE).stdout.decode().strip()
    def git(self, *args, **kwargs):
        return subprocess.run(['/usr/bin/git',*args],cwd=self.root,check=True,stderr=subprocess.PIPE,**kwargs)
    def commit(self, name, content):
        p=self.root/'work'/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(content)
        self.git('-C','work','add','.')
        self.git('-C','work','commit','-qm','fixture')
    def run_scan(self):
        self.git('clone','--bare','work','trusted.git')
        commit=self.git('-C','work','rev-parse','HEAD',stdout=subprocess.PIPE).stdout.decode().strip()
        scan.scan(self.git,self.base,commit,self.root)
    def test_intermediate_secret_cannot_be_hidden_by_later_removal(self):
        self.commit('file', 'ghp_' + 'x' * 36)
        self.commit('file', 'clean again')
        with self.assertRaises(ValueError): self.run_scan()
    def test_unpinned_workflow_is_refused(self):
        self.commit('.github/workflows/check.yml','steps:\n  - uses: actions/checkout@main\n')
        with self.assertRaises(ValueError): self.run_scan()
    def test_scanner_failure_blocks_otherwise_clean_commit(self):
        self.commit('file','fix')
        real = subprocess.run
        def run(args, **kwargs):
            if args[0] == '/usr/local/bin/infisical':
                self.assertNotIn('GH_TOKEN',kwargs['env'])
                raise subprocess.CalledProcessError(1,args)
            return real(args,**kwargs)
        with patch.object(subprocess,'run',side_effect=run):
            with self.assertRaises(subprocess.CalledProcessError): self.run_scan()

class PushCredentialTests(unittest.TestCase):
    def test_only_final_push_receives_the_credential(self):
        spec = importlib.util.spec_from_file_location('push', Path(__file__).with_name('push.py'))
        push = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(push)
        calls = []
        def run(command, **kwargs):
            calls.append((command, kwargs['env']))
            return subprocess.CompletedProcess(command, 0, b'a'*40+b'\n', b'')
        with patch.object(push.sys, 'argv', ['push.py', '/data/worktrees/fixture', 'fixture', '', 'b'*40]), patch.dict(push.os.environ, {'GH_TOKEN':'credential-fixture'}), patch.object(push.sandbox, 'workspace', return_value=(Path('/data/worktrees/fixture'), False)), patch.object(push.sandbox, 'command', side_effect=lambda cwd, args: ['sandbox', *args]), patch.object(push.subprocess, 'run', side_effect=run), patch.object(push.push_scan, 'scan'), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(push.main(), 0)
        self.assertGreater(len(calls), 2)
        for command, env in calls[:-1]:
            self.assertNotIn('GH_TOKEN', env)
        self.assertIn('push', calls[-1][0])
        self.assertEqual(calls[-1][1]['GH_TOKEN'], 'credential-fixture')

class PushFailureTests(unittest.TestCase):
    def test_auth_rejection_is_actionable_without_leaking_git_stderr(self):
        spec = importlib.util.spec_from_file_location('push', Path(__file__).with_name('push.py'))
        push = importlib.util.module_from_spec(spec); spec.loader.exec_module(push)
        error = subprocess.CalledProcessError(128, ['git', 'push'], stderr=b'credential-fixture: The requested URL returned error: 403')
        failure = push.push_failure(error)
        self.assertEqual(failure.code, 77)
        self.assertIn('Contents write permission', str(failure))
        self.assertNotIn('credential-fixture', str(failure))

    def test_workflow_permission_rejection_is_specific_and_redacted(self):
        spec = importlib.util.spec_from_file_location('push', Path(__file__).with_name('push.py'))
        push = importlib.util.module_from_spec(spec); spec.loader.exec_module(push)
        for diagnostic in (
            b"refusing to allow a Personal Access Token to create or update workflow `.github/workflows/private.yml` without `workflow` scope",
            b"refusing to allow a GitHub App to create or update workflow `.github/workflows/private.yml` without `workflows` permission",
        ):
            error = subprocess.CalledProcessError(1, ['git', 'push'], stderr=diagnostic + b' credential-fixture')
            failure = push.push_failure(error)
            self.assertEqual(failure.code, 78)
            self.assertIn('Workflows write permission', str(failure))
            self.assertNotIn('credential-fixture', str(failure))
            self.assertNotIn('private.yml', str(failure))

    def test_branch_rejection_is_not_misreported_as_an_auth_failure(self):
        spec = importlib.util.spec_from_file_location('push', Path(__file__).with_name('push.py'))
        push = importlib.util.module_from_spec(spec); spec.loader.exec_module(push)
        error = subprocess.CalledProcessError(1, ['git', 'push'], stderr=b'credential-fixture: stale info')
        failure = push.push_failure(error)
        self.assertEqual(failure.code, 80)
        self.assertIn('branch changed', str(failure))
        self.assertNotIn('credential-fixture', str(failure))

class PushPreflightTests(unittest.TestCase):
    def setUp(self):
        spec = importlib.util.spec_from_file_location('push', Path(__file__).with_name('push.py'))
        self.push = importlib.util.module_from_spec(spec); spec.loader.exec_module(self.push)

    def test_preflight_only_reads_the_fixed_github_endpoint(self):
        response = MagicMock(status=200, headers={'Content-Type': 'application/x-git-receive-pack-advertisement'})
        opener = MagicMock(); opener.open.return_value.__enter__.return_value = response
        with patch.dict(self.push.os.environ, {'ATB2_GITHUB_TOKEN': 'new-fixture', 'GH_TOKEN': 'old-fixture'}, clear=True), patch.object(self.push.urllib.request, 'build_opener', return_value=opener):
            self.assertEqual(self.push.preflight(), 0)
        request = opener.open.call_args.args[0]
        self.assertEqual(request.get_method(), 'GET')
        self.assertEqual(request.full_url, 'https://github.com/BoundaryML/baml.git/info/refs?service=git-receive-pack')
        self.assertIsNone(request.data)
        self.assertEqual(opener.open.call_args.kwargs['timeout'], 20)
        self.assertEqual(request.get_header('Authorization'), 'Basic ' + self.push.base64.b64encode(b'x-access-token:new-fixture').decode())
        self.assertIsNone(self.push.NoRedirect().redirect_request(request, None, 302, '', {}, 'https://example.invalid'))

    def test_denied_access_and_transport_errors_are_safe(self):
        for error, code in (
            (self.push.urllib.error.HTTPError('private-url', 403, 'credential-fixture', {}, None), 77),
            (self.push.urllib.error.HTTPError('private-url', 500, 'credential-fixture', {}, None), 75),
            (self.push.urllib.error.URLError('credential-fixture'), 75),
        ):
            opener = MagicMock(); opener.open.side_effect = error
            with patch.dict(self.push.os.environ, {'ATB2_GITHUB_TOKEN': 'credential-fixture'}, clear=True), patch.object(self.push.urllib.request, 'build_opener', return_value=opener):
                with self.assertRaises(self.push.PushFailure) as raised: self.push.preflight()
            self.assertEqual(raised.exception.code, code)
            self.assertNotIn('credential-fixture', str(raised.exception))
            self.assertNotIn('private-url', str(raised.exception))

    def test_missing_token_never_contacts_github(self):
        with patch.dict(self.push.os.environ, {}, clear=True), patch.object(self.push.urllib.request, 'build_opener') as opener:
            with self.assertRaises(self.push.PushFailure) as raised: self.push.preflight()
        self.assertEqual(raised.exception.code, 77)
        opener.assert_not_called()

    def test_rules_and_unknown_errors_are_distinct(self):
        for diagnostic, code in ((b'GH013 repository rule violations', 79), (b'unknown credential-fixture', 76)):
            failure = self.push.push_failure(subprocess.CalledProcessError(1, [], stderr=diagnostic))
            self.assertEqual(failure.code, code)
            self.assertNotIn('credential-fixture', str(failure))

if __name__=='__main__': unittest.main()
