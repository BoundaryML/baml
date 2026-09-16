"""Release identity and containment must be verified, not inferred at merge."""
import importlib.util
from pathlib import Path
import unittest
spec = importlib.util.spec_from_file_location('releases',Path(__file__).with_name('reconcile-releases.py'))
r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)
class ReleaseTests(unittest.TestCase):
    def test_catalog_keeps_stable_and_nightly_and_excludes_drafts_and_other_products(self):
        rows=[{'tag_name':tag,'draft':draft,'published_at':at} for tag,draft,at in [
            ('baml-language-0.18.0',False,'2026-09-06T00:00:00Z'),('baml-language-0.18.1-nightly.20260906.a',False,'2026-09-07T00:00:00Z'),
            ('baml-language-0.19.0-nightly.20260906',False,'2026-09-07T00:00:00Z'),
            ('baml-language-0.19.0',True,'2026-09-08T00:00:00Z'),('0.226.0',False,'2026-09-08T00:00:00Z')]]
        self.assertEqual(r.published_releases(lambda _:rows),[('2026-09-06T00:00:00Z','0.18.0','baml-language-0.18.0'),
            ('2026-09-07T00:00:00Z','0.18.1-nightly.20260906.a','baml-language-0.18.1-nightly.20260906.a')])
    def test_nightly_containing_the_merge_is_advertised_before_the_stable(self):
        releases=[('2026-09-03','0.18.1-nightly.20260903.a','nightly'),('2026-09-10','0.19.0','stable')]
        self.assertEqual(r.first_fixed_version('a'*40,'2026-09-02',releases,lambda tag,sha:True,
            lambda v:{'version':v,'channel':'nightly' if 'nightly' in v else 'canary','artifacts':{'fixture':{}}}),'0.18.1-nightly.20260903.a')
    def test_manifest_on_an_unknown_channel_is_not_trusted(self):
        with self.assertRaises(ValueError):
            r.first_fixed_version('a'*40,None,[('2026-09-03','0.19.0','tag')],lambda *args:True,lambda v:{'version':v,'channel':'beta','artifacts':{'x':{}}})
    def test_first_containing_published_version_is_selected(self):
        releases=[('2026-09-01','0.18.0','old'),('2026-09-03','0.19.0','first'),('2026-09-04','0.20.0','later')]
        self.assertEqual(r.first_fixed_version('a'*40,'2026-09-02',releases,lambda tag,sha:tag!='old',
            lambda v:{'version':v,'channel':'canary','artifacts':{'fixture':{}}}),'0.19.0')
    def test_merge_without_published_containing_release_is_not_fixed(self):
        self.assertIsNone(r.first_fixed_version('a'*40,'2026-09-02',[('2026-09-03','0.19.0','tag')],lambda *args:False,lambda _:self.fail('must not load unrelated manifest')))
    def test_missing_manifest_never_substitutes_a_later_version(self):
        with self.assertRaises(ValueError):
            r.first_fixed_version('a'*40,None,[('2026-09-03','0.19.0','tag')],lambda *args:True,lambda _:{})
    def test_partial_release_catalog_fails_closed(self):
        with self.assertRaises(ValueError): r.published_releases(lambda _:[{}]*100)
if __name__=='__main__': unittest.main()
