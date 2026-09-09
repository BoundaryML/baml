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
    def run_expression(self, expression, respond, reject_feedback=False, push_exit_code=None, fixture_logs=False, fixture_issue=False):
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
                start = code.index('function run_with(')
                end = code.index('\n}', start) + 2
                code = code[:start] + f'function run_with(c: Cmd, env: map<string, string>) -> CmdResult {{ CmdResult {{ cmd: "fixture", exit_code: {push_exit_code}, stdout: "", stderr: "private fixture diagnostic", ok: false }} }}' + code[end:]
                path.write_text(code)
            if fixture_issue:
                path = root / 'baml_src/create_issue.baml'
                code = path.read_text();start=code.index('function create_issue(');end=code.index('\n}',start)+2
                replacement = 'function create_issue(fb: Feedback) -> Issue? { let row = baml.json.from_json<map<string,json>>(issue_row(gh_issue(99, Difficulty.Medium))); row.set("feedback_ids", [fb.id].to_json()); row.set("repros", [Repro { files: { "main.baml": "fixture" }, command: "baml check", setup: null, expectation: ShouldCompile { check: "should_compile" } }].to_json()); issue_from_row(row.to_json()) }'
                path.write_text(code[:start]+replacement+code[end:])
            if fixture_logs:
                path = root / 'baml_src/merge_issue.baml'
                code = path.read_text()
                start = code.index('function gh(')
                end = code.index('\n}', start) + 2
                code = code[:start] + 'function gh(args: string[]) -> CmdResult {\n                    let path = atb2_home() + "/log-fetches";\n                    let old = if (baml.fs.exists(path)) { baml.fs.read(path) } else { "" };\n                    baml.fs.write(path, old + "x");\n                    CmdResult { cmd: "fixture", exit_code: 0, stdout: "unique failure", stderr: "", ok: true }\n                }' + code[end:]
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
                       'BAML_TELEMETRY_DISABLED':'1','ATB2_UI_RUNNER_SECRET':'offline-website-signing-fixture-key-00000'}
                try:
                    result = subprocess.run([str(CLI), 'run', '-e', expression], cwd=root, env=env,
                                            capture_output=True, text=True, timeout=60)
                finally:
                    server.shutdown(); thread.join()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return calls

    def test_checks_in_one_workflow_fetch_and_include_logs_once(self):
        expression = '\n            let logs = failed_logs(PrFeedback { checks: [\n                CheckRow { name: "first", bucket: "fail", link: "https://github.com/BoundaryML/baml/actions/runs/123/job/1" },\n                CheckRow { name: "second", bucket: "fail", link: "https://github.com/BoundaryML/baml/actions/runs/123/job/2" }\n            ], comments: [] });\n            assert.equal(logs.get("first") ?? "", "unique failure");\n            assert.contains(logs.get("second") ?? "", "included above");\n            assert.equal(baml.fs.read(atb2_home() + "/log-fetches"), "x");\n        '
        self.assertEqual(self.run_expression(expression, lambda *_: (500, {}), fixture_logs=True), [])

    def test_lifecycle_saves_do_not_replace_concurrent_comments_or_repros(self):
        expression='let row = baml.json.from_json<map<string,json>>(issue_row(gh_issue(7,Difficulty.Medium))); row.set("id","ISSUE-fixture".to_json()); save_issue(issue_from_row(row.to_json()));'
        calls=self.run_expression(expression,lambda *_:(200,[]))
        self.assertEqual([c[0] for c in calls],['POST','PATCH'])
        self.assertIn('on_conflict',calls[0][2])
        for key in ['comments','repros','feedback_ids','slack_ts','slack_channel']:self.assertNotIn(key,calls[1][3])
        self.assertIn('status',calls[1][3])

    def test_similar_feedback_reuses_issue_and_only_appends_repros_and_report_links(self):
        issue={'id':'ISSUE-original','title':'0.17.0: `throws` clause with an unresolved type panics the compiler (index out of bounds)',
               'version':'0.17.0','subsystem':'Compiler','feedback_ids':['PH-original'],'status':{'state':'awaiting_approval'},'repros':[]}
        def respond(method,table,query,body):
            if method=='GET' and table=='issues':return 200,[issue] if query.get('state')==['eq.awaiting_approval'] else []
            return 200,[{'id':1}]
        expression='let fb = Feedback { id: "PH-new", title: "same defect", body: "repro", source: FeedbackSource.BamlFeedback, author: Author { email: null, github: null, device_id: "fixture" }, toolchain: null, files: {}, comments: [], issue_ids: [] }; let result = triage_feedback(fb); assert.equal(result?.id ?? "", "ISSUE-original");'
        calls=self.run_expression(expression,respond,fixture_issue=True)
        patches=[c for c in calls if c[0]=='PATCH' and c[1]=='issues']
        self.assertEqual(len(patches),1);self.assertEqual(set(patches[0][3]),{'repros','feedback_ids'})
        self.assertEqual(patches[0][3]['feedback_ids'],['PH-original','PH-new']);self.assertEqual(len(patches[0][3]['repros']),1)
        self.assertFalse(any(m=='POST' and t=='issues' for m,t,_,_ in calls))
        event=next(body for m,t,_,body in calls if m=='POST' and t=='events')
        self.assertEqual(event[0]['kind'],'feedback_linked')
        self.assertEqual(event[0]['issue_id'],'ISSUE-original')

    def test_website_authentication_and_comment_compare_and_swap(self):
        def respond(method,table,query,body):
            if method=='GET':return 200,[{'id':'ISSUE-fixture','comments':[]}]
            return 200,[] # A simultaneous comment won the compare-and-swap.
        expression = '''
            let body = `{"operation":"comment","id":"ISSUE-fixture","author":"fixture-user","dataset":"live","body":"Useful context"}`;
            let ts = baml.time.Instant.now().to_timestamp_seconds().to_string();
            let req = baml.http.Request { method:"POST", url:"/ui", body:body, headers:{} };
            assert.equal(website_http(req).status_code,403);
            let signed = baml.http.Request { method:"POST", url:"/ui", body:body, headers:{"X-ATB2-Timestamp":ts,"X-ATB2-Signature":"v0=" + hmac_sha256_hex("offline-website-signing-fixture-key-00000", "v0:"+ts+":"+body)} };
            assert.equal(website_http(signed).status_code,409);
        '''
        calls=self.run_expression(expression,respond)
        self.assertEqual(len(calls),3)
        patch=calls[-1];self.assertEqual(patch[0],'PATCH');self.assertEqual(patch[2]['comments'],['eq.[]'])
        self.assertEqual(patch[3]['comments'][0]['author'],'fixture-user')
        self.assertEqual(set(patch[3]),{'comments'})

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
            assert.contains(result.reason ?? "", "GitHub rejected");
            assert.is_true(result.report != null);
            let status = baml.json.parse(baml.fs.read(sb.run_dir + "/push-status.json"));
            assert.equal(baml.json.path_or<string>(status, ".phase", ""), "finished");
            assert.is_true(baml.json.path_or<int>(status, ".exit_code", 0) != 0);
            assert.is_true(!baml.fs.read(sb.run_dir + "/push-status.json").includes("private fixture diagnostic"));
            assert.equal(round_outcome(branch, 3), "agent_stopped");
            assert.equal(baml.fs.read(sb.worktree + "/preserved"), "fix");
            check_push_result(0);
        '''
        for code in (77, 78):
            with self.subTest(exit_code=code):
                calls = self.run_expression(expression, lambda *_: (200, []), push_exit_code=code)
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
