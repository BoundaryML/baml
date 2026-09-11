import importlib.util
import io
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch, Mock

spec=importlib.util.spec_from_file_location('service',Path(__file__).with_name('cli-cache-service.py'))
service=importlib.util.module_from_spec(spec);spec.loader.exec_module(service)

class LazyCacheTests(unittest.TestCase):
    def test_cache_hit_never_builds_or_fetches(self):
        with patch.object(service,'cached',return_value='/cache/0.18.0/cli'), patch.object(service.subprocess,'Popen') as run:
            self.assertEqual(service.ensure('0.18.0'),'/cache/0.18.0/cli')
            run.assert_not_called()

    def test_miss_builds_without_credentials_and_only_publishes_success(self):
        with tempfile.TemporaryDirectory() as tmp:
            def build(command,**kwargs):
                self.assertIn('--reuid=1001',command)
                self.assertNotIn('GH_TOKEN',kwargs['env'])
                self.assertNotIn('INFISICAL_TOKEN',kwargs['env'])
                self.assertEqual(kwargs['env']['HOME'],'/data/bootstrap/home')
                kwargs['stdout'].write((__import__('json').dumps({'version':'0.18.0','revision':'a'*40})+'\n').encode()+b'executable')
                return Mock(pid=123, wait=Mock(return_value=0))
            with patch.object(service,'ROOT',Path(tmp)/'cache'), patch.object(service,'cached',return_value=None), patch.object(service.subprocess,'Popen',side_effect=build), patch.object(service.os,'killpg'):
                path=service.ensure('0.18.0')
                self.assertEqual(Path(path).read_bytes(),b'executable')
            with patch.object(service,'cached',return_value=None), patch.object(service.subprocess,'Popen',return_value=Mock(pid=123, wait=Mock(return_value=1))), patch.object(service.os,'killpg'), patch.object(service.cache,'publish') as publish:
                with self.assertRaises(subprocess.CalledProcessError):service.ensure('0.19.0')
                publish.assert_not_called()

    def test_timeout_kills_the_build_process_group_without_publication(self):
        child = Mock(pid=123, wait=Mock(side_effect=[subprocess.TimeoutExpired('build', 2400), 0]))
        with patch.object(service, 'cached', return_value=None), patch.object(service.subprocess, 'Popen', return_value=child), patch.object(service.os, 'killpg') as kill, patch.object(service.cache, 'publish') as publish:
            with self.assertRaises(subprocess.TimeoutExpired):
                service.ensure('0.18.0')
            kill.assert_called_once_with(123, service.signal.SIGKILL)
            publish.assert_not_called()

    def test_canary_resolves_current_head_and_reuses_that_exact_revision(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp).resolve();binary=root/'canary'/'0.19.0'/('b'*40)/'baml-cli';binary.parent.mkdir(parents=True);binary.write_bytes(b'fixture');binary.chmod(0o555)
            original=Path.stat
            def stat(path,*args,**kwargs):
                value=original(path,*args,**kwargs)
                if path==binary:return __import__('types').SimpleNamespace(st_uid=0,st_mode=value.st_mode)
                return value
            with patch.object(service,'ROOT',root),patch.object(service.Path,'stat',stat),patch.object(service.subprocess,'run',return_value=Mock(stdout='b'*40+'\trefs/heads/canary\n')) as lookup,patch.object(service.subprocess,'Popen') as build:
                self.assertEqual(service.ensure('canary'),str(binary));build.assert_not_called()
                self.assertEqual(lookup.call_args.kwargs['env']['HOME'],'/nonexistent')

    def test_nightly_resolves_the_manifest_before_reusing_a_cached_build(self):
        response = Mock(status=200, read=Mock(return_value=b'{"version":"0.18.1-nightly.20260910.a"}'))
        connection = Mock(getresponse=Mock(return_value=response))
        with patch.object(service.http.client, 'HTTPSConnection', return_value=connection), patch.object(service, 'cached', return_value='/verified/cli') as lookup:
            self.assertEqual(service.ensure('nightly'), '/verified/cli')
            lookup.assert_called_once_with('0.18.1-nightly.20260910.a')
        connection.close.assert_called_once()

    def test_invalid_nightly_manifest_never_starts_a_build(self):
        for raw in [b'{"version":"../../secret"}', b'{"version":null}', b'{"version":"canary"}']:
            connection = Mock(getresponse=Mock(return_value=Mock(status=200, read=Mock(return_value=raw))))
            with patch.object(service.http.client, 'HTTPSConnection', return_value=connection), patch.object(service.subprocess, 'Popen') as build:
                with self.assertRaises(ValueError): service.ensure('nightly')
                build.assert_not_called()

    def test_untrusted_version_is_not_a_command_or_path(self):
        for v in ['../bad','canary;id','0.18.0\ncommand','--help']:
            with self.assertRaises(ValueError):service.cached(v)

if __name__=='__main__':unittest.main()
