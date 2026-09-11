"""Offline versioned CLI publication and cache-integrity checks."""
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('cache_cli', Path(__file__).with_name('cache-cli.py'))
cache = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cache)


class CacheTests(unittest.TestCase):
    def test_versions_and_revisions_are_retained_and_indexed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / 'cache'
            first = cache.publish(root, 'baml 0.18.0', 'a'*40, io.BytesIO(b'first'))
            second = cache.publish(root, '0.18.0', 'b'*40, io.BytesIO(b'second'))
            third = cache.publish(root, 'baml-cli 0.19.0-beta.1', 'c'*40, io.BytesIO(b'third'))
            self.assertEqual(first.read_bytes(), b'first')
            self.assertNotEqual(first, second)
            self.assertEqual(third.stat().st_mode & 0o777, 0o555)
            self.assertEqual(len(json.loads((root / 'index.json').read_text())), 3)
            cache.publish(root, '0.18.0', 'a'*40, io.BytesIO(b'first'))
            with self.assertRaises(ValueError):
                cache.publish(root, '0.18.0', 'a'*40, io.BytesIO(b'poison'))
            self.assertEqual(first.read_bytes(), b'first')

    def test_invalid_keys_empty_artifacts_and_symlinks_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / 'cache'
            for version, revision in [('../escape','a'*40), ('0.18.0','../escape')]:
                with self.assertRaises(ValueError):
                    cache.publish(root, version, revision, io.BytesIO(b'cli'))
            with self.assertRaises(ValueError):
                cache.publish(root, '0.18.0', 'a'*40, io.BytesIO(b''))
            (root/'0.19.0').symlink_to(Path(tmp))
            with self.assertRaises(ValueError):
                cache.publish(root, '0.19.0', 'b'*40, io.BytesIO(b'cli'))

if __name__ == '__main__': unittest.main()
