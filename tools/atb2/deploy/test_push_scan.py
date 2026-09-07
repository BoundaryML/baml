"""Outgoing history is checked before credentials can be used for a push."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

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

if __name__=='__main__': unittest.main()
