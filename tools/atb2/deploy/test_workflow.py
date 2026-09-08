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
    def run_expression(self, expression, respond, reject_feedback=False, push_exit_code=None):
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
            if reject_feedback:
                # Substitute only the model response; exercise the actual pipeline/store path.
                path = root / 'baml_src/create_issue.baml'
                code = path.read_text()
                start = code.index('function assess_feedback(')
                end = code.index('\n}', start) + 2
                code = code[:start] + 'function assess_feedback(fb: Feedback) -> FeedbackAssessment { FeedbackAssessment { actionable: false, reason: "No concrete behavior" } }' + code[end:]
                code = code.replace("assess_feedback@parse(", "baml.json.from_string<FeedbackAssessment>(")
                path.write_text(code)
            if push_exit_code is not None:
                path = root / 'baml_src/handle_issue.baml'
                code = path.read_text()
                start = code.index('function push_branch(')
                end = code.index('\n}', start) + 2
                code = code[:start] + f'function push_branch(sb: Sandbox, expected_head: string? = null) -> null {{ check_push_result({push_exit_code}) }}' + code[end:]
                path.write_text(code)
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

    def test_denied_push_writes_an_outcome_and_returns_to_the_worker(self):
        expression = '''
            let branch = "fixture-push";
            let sb = Sandbox { branch: branch, worktree: atb2_home() + "/worktrees/fixture",
                run_dir: run_dir_for(branch), reused_branch: true, base_head: null, source_head: "" };
            baml.fs.mkdir(sb.run_dir, baml.fs.MkdirOptions { recursive: true });
            baml.fs.mkdir(sb.worktree, baml.fs.MkdirOptions { recursive: true });
            baml.fs.write(sb.worktree + "/preserved", "fix");
            let review = Review { branch: branch, feedback: "fix", expected_head: null, proposal_id: null };
            assert.equal(push_review_fix(sb, review, baml.time.Instant.now(), sample_report(), null), false);
            let result = baml.json.from_string<HandleOutcome>(baml.fs.read(sb.run_dir + "/outcome.json"));
            assert.contains(result.reason ?? "", "GitHub rejected the push credential");
            assert.is_true(result.report != null);
            assert.equal(round_outcome(branch, 3), "agent_stopped");
            assert.equal(baml.fs.read(sb.worktree + "/preserved"), "fix");
            check_push_result(0);
        '''
        calls = self.run_expression(expression, lambda *_: (200, []), push_exit_code=77)
        self.assertEqual(calls, [])

    def test_vague_feedback_is_terminal_without_an_issue_or_notification(self):
        terminal = []
        def respond(method, table, query, body):
            if method == 'POST' and table == 'events':
                terminal.extend(body)
                return 200, [{'id': 1}]
            if method == 'GET' and table == 'feedback':
                return 200, [{'id': 'FB-vague'}]
            if method == 'GET' and table == 'events':
                return 200, [{'id': 1}] if terminal else []
            return 200, []
        expression = 'let fb = Feedback { id: "FB-vague", title: "sum shit is broken", body: "", source: FeedbackSource.BamlFeedback, author: Author { email: null, github: null, device_id: "fixture" }, toolchain: null, files: {}, comments: [], issue_ids: [] }; assert.equal(triage_feedback(fb), null); assert.equal(untriaged_feedback().length(), 0);'
        calls = self.run_expression(expression, respond, reject_feedback=True)
        self.assertEqual(len(terminal), 1)
        self.assertEqual(terminal[0]['kind'], 'no_issue')
        self.assertEqual(terminal[0]['payload'], {'reason': 'needs_details'})
        self.assertIsNone(terminal[0]['issue_id'])
        self.assertIsNone(terminal[0]['slack_ts'])
        self.assertEqual([(method, table) for method, table, _, _ in calls if method != 'GET'], [('POST', 'events')])

    def test_issue_and_direct_requests_reuse_one_existing_babysitter(self):
        calls = self.run_expression('request_merge("https://github.com/BoundaryML/baml/pull/1")',
                                   lambda method, table, query, body: (200, [{'id': 7}]))
        self.assertEqual(len(calls), 2)
        self.assertEqual(calls[1][0:2], ('GET', 'babysit_proposals'))
        self.assertEqual(calls[0][0:2], ('GET', 'babysit_requests'))
        self.assertIn('awaiting_approval', calls[0][2]['or'][0])

    def test_retired_approval_does_not_block_a_fresh_babysit_request(self):
        def respond(method, table, query, body):
            if table == 'babysit_requests':
                return 200, [{'id':7, 'status':'done', 'result':{'kind':'awaiting_approval'}}]
            return 200, []
        calls = self.run_expression('assert.equal(active_babysit_request("https://github.com/BoundaryML/baml/pull/1"), null)', respond)
        self.assertTrue(all(method == 'GET' for method, _, _, _ in calls))

    def test_created_fix_cannot_wake_worker_before_outcome_and_cleanup(self):
        def respond(method, table, query, body):
            if method == 'POST' and table == 'babysit_requests': return 200, [{'id':7}]
            return 200, []
        expression = 'handoff_issue_pr(issue_in(Subsystem.Compiler, "owner"), "https://github.com/BoundaryML/baml/pull/1", "fixture plan", SlackRef { channel: "C1", ts: "1.2" })'
        calls = self.run_expression(expression, respond)
        writes = [(table,body) for method,table,query,body in calls if method == 'POST']
        self.assertEqual([table for table,body in writes], ['issues'])
        self.assertEqual(writes[0][1][0]['status']['pr'], 'https://github.com/BoundaryML/baml/pull/1')
        self.assertFalse(any(table == "babysit_requests" for _, table, _, _ in calls))

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

    def test_terminal_approved_request_is_retired_not_requeued(self):
        def respond(method, table, query, body):
            if method == 'GET' and table == 'babysit_proposals': return 200, [{'request_id': 7}]
            if method == 'GET' and table == 'babysit_requests': return 200, [{'result': {'kind': 'merged'}}]
            return 200, []
        calls = self.run_expression('requeue_approved_babysits()', respond)
        writes = [(table, body) for method, table, query, body in calls if method == 'PATCH']
        self.assertEqual(writes, [('babysit_proposals', {'status': 'stale'})])

    def test_recovery_preserves_fair_queue_timestamp(self):
        def respond(method, table, query, body):
            if method == 'GET' and table == 'babysit_proposals': return 200, [{'request_id': 7}]
            if method == 'GET' and table == 'babysit_requests': return 200, [{'result': {'kind': 'awaiting_approval'}}]
            return 200, []
        calls = self.run_expression('requeue_approved_babysits()', respond)
        writes = [body for method, table, query, body in calls if method == 'PATCH']
        self.assertEqual(len(writes), 1)
        self.assertEqual(writes[0]['status'], 'queued')
        self.assertNotIn('finished_at', writes[0])


if __name__ == '__main__': unittest.main()
