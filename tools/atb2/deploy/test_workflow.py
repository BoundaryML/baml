"""Offline queue/lifecycle regression tests using the actual BAML runtime.
A disposable project replaces only the HTTPS transport with a loopback fixture.
No real service credentials, GitHub operations, or agent requests are used.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import unittest
from urllib.parse import urlsplit, parse_qs

SOURCE = Path(__file__).resolve().parents[1]
CLI = Path(os.environ.get('BAML_CLI', str(Path.home() / '.atb2/target/debug/baml-cli')))


@unittest.skipUnless(CLI.is_file(), 'canary baml-cli required')
class WorkflowTests(unittest.TestCase):
    def run_expression(self, expression, respond):
        calls = []
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_): pass
            def do_GET(self): self.handle_request()
            def do_POST(self): self.handle_request()
            def do_PATCH(self): self.handle_request()
            def handle_request(self):
                url = urlsplit(self.path)
                body = self.rfile.read(int(self.headers.get('Content-Length', '0')))
                call = (self.command, url.path.removeprefix('/rest/v1/'), parse_qs(url.query), json.loads(body) if body else None)
                calls.append(call)
                status, result = respond(*call)
                self.send_response(status); self.send_header('Content-Type', 'application/json'); self.end_headers()
                self.wfile.write(json.dumps(result).encode())
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            shutil.copytree(SOURCE / 'baml_src', root / 'baml_src')
            shutil.copy(SOURCE / 'baml.toml', root / 'baml.toml')
            with ThreadingHTTPServer(('127.0.0.1', 0), Handler) as server:
                thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
                # Production still requires HTTPS. This substitution exists only
                # in this temporary fixture and cannot modify the checked-in app.
                store = root / 'baml_src/store.baml'
                store.write_text(store.read_text().replace('if (!base.starts_with("https://"))', 'if (!base.starts_with("http://127.0.0.1:"))'))
                env = {'PATH': os.environ['PATH'], 'HOME': tmp,
                       'FEEDBACK_SUPABASE_URL': f'http://127.0.0.1:{server.server_port}',
                       'FEEDBACK_SUPABASE_KEY': 'offline-fixture', 'BAML_AGENT_SKILL_CHECK':'off',
                       'BAML_TELEMETRY_DISABLED':'1'}
                try:
                    result = subprocess.run([str(CLI), 'run', '-e', expression], cwd=root, env=env,
                                            capture_output=True, text=True, timeout=60)
                finally:
                    server.shutdown(); thread.join()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return calls

    def test_issue_and_direct_requests_reuse_one_existing_babysitter(self):
        calls = self.run_expression('request_merge("https://github.com/BoundaryML/baml/pull/1")',
                                   lambda method, table, query, body: (200, [{'id': 7}]))
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0][0:2], ('GET', 'babysit_requests'))
        self.assertIn('awaiting_approval', calls[0][2]['or'][0])

    def test_created_fix_is_saved_before_queue_handoff(self):
        def respond(method, table, query, body):
            if method == 'POST' and table == 'babysit_requests': return 200, [{'id':7}]
            return 200, []
        expression = 'handoff_issue_pr(issue_in(Subsystem.Compiler, "owner"), "https://github.com/BoundaryML/baml/pull/1", "fixture plan", SlackRef { channel: "C1", ts: "1.2" })'
        calls = self.run_expression(expression, respond)
        writes = [(table,body) for method,table,query,body in calls if method == 'POST']
        self.assertEqual([table for table,body in writes], ['issues','babysit_requests'])
        self.assertEqual(writes[0][1][0]['status']['pr'], 'https://github.com/BoundaryML/baml/pull/1')
        self.assertEqual(writes[1][1][0]['thread_ts'], '1.2')

    def test_duplicate_claim_is_finished_before_any_agent_runs(self):
        def respond(method, table, query, body):
            if method == 'GET' and table == 'babysit_requests':
                if query.get('status') == ['eq.queued']:
                    return 200, [{'id': 8, 'pr':'https://github.com/BoundaryML/baml/pull/1','status':'queued'}]
                if 'or' in query: return 200, [{'id': 7}]
            if method == 'PATCH' and table == 'babysit_requests': return 200, [{'id': 8}]
            return 200, []
        calls = self.run_expression('recover_and_claim_babysit("fixture", 21600)', respond)
        duplicates = [body for method, table, query, body in calls if method == 'PATCH' and body.get('result', {}).get('kind') == 'duplicate']
        self.assertEqual(duplicates, [{'status':'done','result':{'kind':'duplicate','request_id':7},'finished_at':'now()'}])
        requeues = [query for method, table, query, body in calls if method == 'PATCH' and query.get('result->>kind')]
        self.assertEqual(requeues[0]['result->>kind'], ['in.(green,waiting_on_checks)'])
        claims = [query for method, table, query, body in calls if method == 'GET' and query.get('status') == ['eq.queued']]
        self.assertEqual(claims[0]['order'], ['finished_at.asc.nullsfirst,created_at'])

    def test_recovery_outage_is_caught_by_worker_retry_boundary(self):
        calls = self.run_expression('merge_issue_loop(once = true)', lambda *args: (503, {'error':'fixture outage'}))
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0][1], 'babysit_proposals')

    def test_pr_events_keep_issue_and_thread_association(self):
        def respond(method, table, query, body):
            if table == 'issues': return 200, [{'id':'ISSUE-1'}]
            if method == 'POST': return 200, [{'id':9}]
            return 200, []
        calls = self.run_expression('record_pr_event("https://github.com/BoundaryML/baml/pull/1", "babysit_proposed", { "proposal_id": "p1" }.to_json(), SlackRef { channel: "C1", ts: "1.2" })', respond)
        saved = [body for method, table, query, body in calls if method == 'POST'][0][0]
        self.assertEqual(saved['issue_id'], 'ISSUE-1')
        self.assertEqual(saved['slack_ts'], '1.2')
        self.assertEqual(saved['payload'], {'pr':'https://github.com/BoundaryML/baml/pull/1','proposal_id':'p1'})


if __name__ == '__main__': unittest.main()
