"""Actual baml feedback transport and journal, using a local PostHog sink only."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import unittest
from unittest.mock import patch
import uuid

spec=importlib.util.spec_from_file_location('tasks',Path(__file__).with_name('tasks.py'))
tasks=importlib.util.module_from_spec(spec);spec.loader.exec_module(tasks)
CLI=Path(os.environ.get('BAML_CLI',str(Path.home()/'.atb2/target/debug/baml-cli')))

class FeedbackTransportTests(unittest.TestCase):
    @unittest.skipUnless(CLI.exists(),'canary CLI required')
    def test_cli_sends_feedback_and_journal_links_to_posthog_id(self):
        received=[]
        class Handler(BaseHTTPRequestHandler):
            def log_message(self,*_):pass
            def do_POST(self):
                received.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))))
                self.send_response(200);self.end_headers();self.wfile.write(b'{"status":1}')
        with tempfile.TemporaryDirectory() as tmp,ThreadingHTTPServer(('127.0.0.1',0),Handler) as server:
            thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
            sid=str(uuid.uuid4());root=Path(tmp).resolve();home=root/sid/'home';home.mkdir(parents=True)
            env={'PATH':os.environ['PATH'],'HOME':str(home),'BAML_POSTHOG_HOST':f'http://127.0.0.1:{server.server_port}', 'BAML_AGENT_SKILL_CHECK':'off','BAML_TELEMETRY_DISABLED':'1'}
            try:
                result=subprocess.run([str(CLI),'feedback','--anonymous','--title','Offline regression fixture','--description','Transport test against loopback only.'],cwd=home,env=env,capture_output=True,text=True,timeout=30)
            finally:server.shutdown();thread.join()
            self.assertEqual(result.returncode,0,result.stdout+result.stderr)
            with patch.object(tasks,'SESSIONS',root):reports=tasks.cli_feedback(sid)
            self.assertEqual(len(reports),1);self.assertEqual(reports[0]['status'],'anonymous')
            event=next(event for payload in received for event in payload.get('batch',[payload]) if event.get('event')=='baml_feedback')
            self.assertEqual(reports[0]['id'],'PH-'+event['properties']['report_id'])
            self.assertEqual(reports[0]['title'],'Offline regression fixture')

    def test_feedback_and_session_readers_refuse_symlink_escape(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp).resolve();sid=str(uuid.uuid4());home=root/sid/'home';home.mkdir(parents=True)
            secret=root/'private';secret.mkdir();(secret/'feedback.json').write_text('{"reports":[]}')
            (home/'.baml').symlink_to(secret)
            with patch.object(tasks,'SESSIONS',root),self.assertRaises(OSError):tasks.cli_feedback(sid)

if __name__=='__main__':unittest.main()
