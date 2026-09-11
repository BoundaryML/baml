import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('traces',Path(__file__).with_name('traces.py'))
traces=importlib.util.module_from_spec(spec);spec.loader.exec_module(traces)

class TraceTests(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
        self.root=Path(self.tmp.name)/'traces'
        p=patch.object(traces,'ROOT',self.root);p.start();self.addCleanup(p.stop)
    def test_live_incremental_output_survives_failure_without_auth_or_thinking(self):
        trace=traces.Trace('fix','investigate ISSUE-fixture')
        trace.event(json.dumps({'type':'system','apiKey':'private-fixture'}))
        trace.event(json.dumps({'type':'assistant','message':{'content':[{'type':'thinking','thinking':'private-fixture'},{'type':'text','text':'working'}]}}))
        first=traces.read({'id':trace.id})
        self.assertEqual(first['meta']['status'],'running')
        self.assertEqual([e['text'] for e in first['events']],['investigate ISSUE-fixture','working'])
        trace.event(json.dumps({'type':'user','message':{'content':[{'type':'tool_result','content':'test failed'}]}}))
        trace.event(json.dumps({'type':'result','is_error':True,'subtype':'error_max_turns'}));trace.close(1)
        second=traces.read({'id':trace.id,'offset':first['offset']})
        self.assertEqual(second['meta']['status'],'failed');self.assertEqual(second['meta']['reason'],'error_max_turns')
        self.assertEqual(second['events'][0]['text'],'test failed')
        self.assertNotIn('private-fixture',json.dumps(first)+json.dumps(second))
        self.assertEqual(len(traces.read({'issue_id':'ISSUE-fixture'})),1)
    def test_large_outputs_are_paginated_without_truncation(self):
        trace=traces.Trace('play');text='😀'*300000
        trace.event(json.dumps({'type':'assistant','message':{'content':[{'type':'text','text':text}]}}));trace.close(0)
        got=[];offset=0
        while True:
            page=traces.read({'id':trace.id,'offset':offset});got.extend(e['text'] for e in page['events']);offset=page['offset']
            if not page['more']:break
        self.assertEqual(''.join(got),text)
    def test_path_traversal_symlinks_and_cross_dataset_are_refused(self):
        trace=traces.Trace('play');trace.close(1)
        for request in [{'id':'../secret'},{'id':trace.id,'offset':1},{'id':trace.id,'offset':True},{'id':trace.id,'dataset':'eval'}]:
            with self.assertRaises(ValueError):traces.read(request)
        p=self.root/trace.id/'events.jsonl';p.unlink();p.symlink_to('/etc/passwd')
        with self.assertRaises(OSError):traces.read({'id':trace.id})
    def test_partial_record_waits_without_requesting_immediate_retry(self):
        trace=traces.Trace('fix');trace.close(1)
        path=self.root/trace.id/'events.jsonl'
        path.write_bytes(b'{"role":"assistant"')
        page=traces.read({'id':trace.id})
        self.assertEqual(page['events'],[]);self.assertEqual(page['offset'],0)
        self.assertFalse(page['more'])
        with path.open('ab') as f:f.write(b'}\n')
        self.assertEqual(len(traces.read({'id':trace.id})['events']),1)
    def test_capture_preserves_child_stream_and_records_terminal_error(self):
        line=json.dumps({'type':'result','is_error':True,'subtype':'error_max_turns'})+'\n'
        output=io.BytesIO()
        code=traces.capture(['/usr/bin/printf','%s',line],{'PATH':'/usr/bin:/bin'},output,'triage')
        self.assertEqual(code,0);self.assertEqual(output.getvalue(),line.encode())
        self.assertEqual(traces.read({})[0]['status'],'failed')

if __name__=='__main__':unittest.main()
