"""Release identity and containment must be verified, not inferred at merge."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from urllib.parse import parse_qsl
spec = importlib.util.spec_from_file_location('releases',Path(__file__).with_name('reconcile-releases.py'))
r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)


def scan_reconciler(folder, rows, contained_sha):
    """A Reconciler over an in-memory issues table, one verified release, and ancestry by SHA."""
    rec = r.Reconciler.__new__(r.Reconciler)
    rec.folder = folder
    def store(query, patch=None):
        q = dict(parse_qsl(query))
        if patch is not None:
            for row in rows:
                if row['id'] == q['id'][3:]: row['fixed_in'] = patch['fixed_in']
            return []
        return [row for row in rows if row['fixed_in'] is None and row['id'] > q.get('id','gt.')[3:]][:100]
    rec.store = store
    rec.api = lambda _:[{'tag_name':'baml-language-0.19.0','draft':False,'prerelease':False,'published_at':'2026-09-05T00:00:00Z'}]
    rec.contains = lambda tag,sha: sha == contained_sha
    rec.manifest = lambda v:{'version':v,'channel':'canary','artifacts':{'fixture':{}}}
    return rec


def merged_row(id, sha, status={}):
    return {'id':id,'status':status,'merge_sha':sha,'merged_at':'2026-09-01T00:00:00Z','fixed_in':None}
class ReleaseTests(unittest.TestCase):
    def test_catalog_excludes_drafts_nightlies_and_other_products(self):
        rows=[{'tag_name':tag,'draft':draft,'prerelease':pre,'published_at':'2026-09-06T00:00:00Z'} for tag,draft,pre in [
            ('baml-language-0.18.0',False,False),('baml-language-0.19.0-nightly.20260906',False,True),
            ('baml-language-0.19.0',True,False),('0.226.0',False,False)]]
        self.assertEqual(r.published_releases(lambda _:rows),[('2026-09-06T00:00:00Z','0.18.0','baml-language-0.18.0')])
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
    def test_unresolved_prefix_cannot_starve_later_rows(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)
            # 10,050 merged rows; the first 10,000 stay unresolved (not in any release).
            rows = [merged_row(f'I-{i:05d}',('a' if i < 10_000 else 'b')*40) for i in range(10_050)]
            rec = scan_reconciler(folder,rows,'b'*40)
            rec.run()
            self.assertIsNone(rows[10_000]['fixed_in'])
            self.assertEqual((folder/'scan-cursor').read_text(),'I-09999')
            rec.run()
            self.assertEqual(rows[10_000]['fixed_in'],'0.19.0')
            self.assertEqual(rows[10_049]['fixed_in'],'0.19.0')
            self.assertEqual((folder/'scan-cursor').read_text(),'')
    def test_malformed_status_rows_cannot_abort_the_scan(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)
            # A null and a non-object status (both possible in the store) must be
            # skipped, not abort the run and re-poison every later hourly scan.
            rows = [merged_row('I-00000',None,status=None),
                    merged_row('I-00001',None,status='merged'),
                    merged_row('I-00002','b'*40)]
            scan_reconciler(folder,rows,'b'*40).run()
            self.assertIsNone(rows[0]['fixed_in'])
            self.assertIsNone(rows[1]['fixed_in'])
            self.assertEqual(rows[2]['fixed_in'],'0.19.0')
            self.assertEqual((folder/'scan-cursor').read_text(),'')
    def test_corrupt_cursor_falls_back_to_a_full_rescan(self):
        with tempfile.TemporaryDirectory() as tmp:
            folder = Path(tmp)
            (folder/'scan-cursor').write_text('../evil cursor\n')
            rows = [merged_row('I-00000','b'*40)]
            scan_reconciler(folder,rows,'b'*40).run()
            self.assertEqual(rows[0]['fixed_in'],'0.19.0')
            self.assertEqual((folder/'scan-cursor').read_text(),'')
if __name__=='__main__': unittest.main()
