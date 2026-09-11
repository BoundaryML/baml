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
    def run_expression(self, expression, respond, reject_feedback=False, push_exit_code=None, fixture_logs=False, fixture_issue=False, ddl_scan_codes=None, fixture_traces=False, fixture_github=False, fixture_intake_failures=False, fixture_auto_plan=False):
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
            if fixture_github:
                path=root/'baml_src/intake.baml';code=path.read_text()
                start=code.index('function github_issue_page(');end=code.index('\n}',start)+2
                replacement='function github_issue_page(page: int) -> json[] { assert.equal(page,1); baml.json.from_string<json[]>(`[{"number":4816,"pull_request":{"url":"fixture"}},{"number":4815,"title":"Full issue","body":"full description with code","user":{"login":"reporter"}}]`) }'
                if isinstance(fixture_github, str): replacement=fixture_github
                path.write_text(code[:start]+replacement+code[end:])
            if fixture_intake_failures:
                for filename,name,signature,error in [
                    ('intake.baml','posthog_feedback','after: string?, resume: string? = null, max_pages: int = 10','PosthogFeedback'),
                    ('slack.baml','slack_feedback','oldest: string?, resume: string? = null','SlackFeedback')]:
                    path=root/'baml_src'/filename;code=path.read_text()
                    start=code.index('function '+name+'(');end=code.index('\n}',start)+2
                    failure='IntakeError' if name=='posthog_feedback' else 'SlackError'
                    path.write_text(code[:start]+f'function {name}({signature}) -> {error} {{ throw {failure} {{ message: "offline outage" }}; }}'+code[end:])
            if fixture_auto_plan:
                path=root/'baml_src/merge_issue.baml';code=path.read_text()
                start=code.index('function make_babysit_plan(');end=code.index('\n}',start)+2
                replacement='function make_babysit_plan(snap: PrSnapshot, brief: string, thread: SlackRef?, proposal_id: string? = null) -> DesignDoc | WorkflowOnly { DesignDoc { suggested_fix: "Fix the assertion", doc: "Use the expected value and rerun tests" } }'
                path.write_text(code[:start]+replacement+code[end:])
            if fixture_traces:
                script = root / 'traces.py'
                trace_root = root / 'traces'
                script.write_text((SOURCE/'deploy/traces.py').read_text().replace("ROOT = Path('/data/agent-traces')", "ROOT = Path(" + repr(str(trace_root)) + ")"))
                folder = trace_root/'11111111-1111-1111-1111-111111111111'; folder.mkdir(parents=True)
                (folder/'meta.json').write_text(json.dumps({'id':folder.name,'stage':'fix','status':'failed','dataset':'live','started_at':1,'reason':'error_max_turns','issue_ids':[],'proposal_id':'proposal-fixture'}))
                (folder/'events.jsonl').write_text(json.dumps({'role':'assistant','type':'text','text':'partial progress'})+'\n')
                path = root/'baml_src/website.baml'
                path.write_text(path.read_text().replace('/usr/local/lib/atb2/traces.py',str(script)))
            if ddl_scan_codes is not None:
                path = root / 'baml_src/handle_issue.baml'
                code = path.read_text()
                replacements = {
                    'scan_branch': 'function scan_branch(sb: Sandbox) -> int { let p = atb2_home() + "/scan-count"; let n = if (baml.fs.exists(p)) { baml.fs.read(p).length() } else { 0 }; baml.fs.write(p, "x".repeat(n+1)); let codes: int[] = ' + json.dumps(ddl_scan_codes) + '; codes.at(n) ?? baml.sys.panic("too many scans") }',
                    'run_agent_fix': 'function run_agent_fix(sb: Sandbox, issue: Issue, plan: string, timeout_s: int) -> FixReport? { assert.equal(timeout_s, 300); assert.contains(plan, "every unpublished commit"); let p = atb2_home() + "/repair-count"; assert.equal(baml.fs.exists(p), false); baml.fs.write(p, "x"); sample_report() }',
                    'checkpoint_uncommitted': 'function checkpoint_uncommitted(sb: Sandbox) -> null { null }',
                }
                for name, replacement in replacements.items():
                    start = code.index('function ' + name + '('); end = code.index('\n}', start) + 2
                    code = code[:start] + replacement + code[end:]
                path.write_text(code)
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
                       'BAML_TELEMETRY_DISABLED':'1','ATB2_SHEPHERDS':'owner:U1','ATB2_UI_RUNNER_SECRET':'offline-website-signing-fixture-key-00000'}
                if fixture_github: env['ATB2_GITHUB_TOKEN']='offline-fixture'
                try:
                    result = subprocess.run([str(CLI), 'run', '-e', expression], cwd=root, env=env,
                                            capture_output=True, text=True, timeout=60)
                finally:
                    server.shutdown(); thread.join()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return calls

    def test_github_intake_stores_complete_issues_skips_prs_and_preserves_existing_links(self):
        stored = {}; cursors = []
        def respond(method,table,query,body):
            if method=='GET' and table=='cursors':return 200,[]
            if method=='POST' and table=='feedback':
                self.assertEqual(query,{'on_conflict':['id']})
                row=body[0]
                if row['id'] in stored:return 200,[]
                stored[row['id']]=row;return 200,[row]
            if method=='POST' and table=='cursors':cursors.append(body);return 200,[]
            if method=='POST' and table=='events':return 200,[{'id':123}]
            return 500,{}
        expression='let first = ingest_github_issues(); assert.equal(first.length(),1); let second=ingest_github_issues(); assert.equal(second.length(),0);'
        self.run_expression(expression,respond,fixture_github=True)
        self.assertEqual(list(stored),['GHI-4815'])
        row=stored['GHI-4815'];self.assertEqual(row['source'],'Github');self.assertEqual(row['author_github'],'reporter')
        self.assertEqual(row['origin']['issue']['body'],'full description with code')
        self.assertIn('https://github.com/BoundaryML/baml/issues/4815',row['body'])
        self.assertEqual(json.loads(cursors[-1][0]['value'])['after'],4816)

    def test_github_reports_reach_triage_when_other_intakes_fail(self):
        stored={};cursors=[]
        def respond(method,table,query,body):
            if method=='GET' and table in ('cursors','events'):return 200,[]
            if method=='GET' and table=='feedback':return 200,list(stored.values())
            if method=='POST' and table=='feedback':
                row=body[0];stored[row['id']]=row;return 200,[row]
            if method=='POST' and table=='cursors':cursors.extend(body);return 200,[]
            if method=='POST' and table=='events':return 200,[{'id':123}]
            return 500,{}
        expression='let fresh=ingest_feedback(); assert.equal(fresh.length(),1); let waiting=untriaged_feedback(); assert.equal(waiting.length(),1); assert.equal(waiting[0].source,FeedbackSource.Github); assert.equal(waiting[0].id,"GHI-4815");'
        self.run_expression(expression,respond,fixture_github=True,fixture_intake_failures=True)
        self.assertEqual([c['name'] for c in cursors],['live:github_issues'])

    def test_github_failed_store_retries_without_advancing_cursor(self):
        stored={};cursors=[];attempts=0
        def respond(method,table,query,body):
            nonlocal attempts
            if method=='GET' and table=='cursors':return 200,[]
            if method=='POST' and table=='feedback':
                attempts+=1
                if attempts==1:return 503,{'error':'temporary fixture outage'}
                row=body[0];stored[row['id']]=row;return 200,[row]
            if method=='POST' and table=='cursors':cursors.extend(body);return 200,[]
            if method=='POST' and table=='events':return 200,[{'id':123}]
            return 500,{}
        self.run_expression('assert.equal(github_intake_tick(),0); assert.equal(github_intake_tick(),1);',respond,fixture_github=True)
        self.assertEqual(attempts,2);self.assertEqual(len(cursors),1)
        self.assertEqual(list(stored),['GHI-4815'])

    def test_github_resumes_at_saved_page_and_keeps_high_water(self):
        cursor={'after':4700,'newest':4900,'page':2};stored={}
        def respond(method,table,query,body):
            nonlocal cursor
            if method=='GET' and table=='cursors':return 200,[{'value':json.dumps(cursor)}]
            if method=='POST' and table=='feedback':
                row=body[0];stored[row['id']]=row;return 200,[row]
            if method=='POST' and table=='cursors':cursor=json.loads(body[0]['value']);return 200,[]
            if method=='POST' and table=='events':return 200,[{'id':123}]
            return 500,{}
        fixture='function github_issue_page(page: int) -> json[] { assert.equal(page,2); baml.json.from_string<json[]>(`[{"number":4801,"title":"Recovered issue","body":"repro","user":{"login":"fixture"}},{"number":4699,"title":"Already scanned","user":{"login":"fixture"}}]`) }'
        self.run_expression('assert.equal(ingest_github_issues().length(),1);',respond,fixture_github=fixture)
        self.assertEqual(list(stored),['GHI-4801'])
        self.assertEqual(cursor,{'after':4900,'newest':4900,'page':1})

    def test_live_transcripts_require_a_signature_and_keep_failed_output(self):
        expression = '''
            baml.fs.mkdir(atb2_home(), baml.fs.MkdirOptions { recursive: true });
            let body = `{ "operation":"traces", "author":"fixture-user", "id":"11111111-1111-1111-1111-111111111111" }`;
            let timestamp = baml.time.Instant.now().to_timestamp_seconds().to_string();
            let req = baml.http.Request { method: "POST", url: "/ui", body: body, headers: {
                "X-ATB2-Timestamp": timestamp,
                "X-ATB2-Signature": "v0=" + hmac_sha256_hex("offline-website-signing-fixture-key-00000", "v0:" + timestamp + ":" + body)
            } };
            let response = website_http(req);
            assert.equal(response.status_code, 200);
            assert.contains(response.text(), "partial progress");
            assert.contains(response.text(), "error_max_turns");
            assert.contains(response.text(), "Exact proposed fix");
            assert.equal(website_http(baml.http.Request { method: "POST", url: "/ui", body: body, headers: {} }).status_code,403);
        '''
        calls=self.run_expression(expression,lambda method,table,query,body:(200,[{'id':'proposal-fixture','summary':'Exact proposed fix','status':'pending'}]) if table=='babysit_proposals' else (500,{}),fixture_traces=True)
        self.assertEqual(len(calls),1)
        self.assertEqual(calls[0][2]['id'],['eq.proposal-fixture'])

    def test_cancellation_patch_is_scoped_and_conditional(self):
        issue = {'id':'ISSUE-fixture','title':'Fixture','status':{'state':'approved'},
                 'shepherd':'owner','slack_ts':'123.4','slack_channel':'C1'}
        def respond(method, table, query, body):
            if method == 'GET': return 200, [issue]
            return 200, []  # Lost race: no notification or second update.
        event = json.dumps({'type':'reaction_added','reaction':'x','user':'U1',
                            'item':{'type':'message','channel':'C1','ts':'123.4'}})
        calls = self.run_expression('accept_cancellation(baml.json.parse(`' + event + '`));', respond)
        self.assertEqual([c[0] for c in calls], ['GET','PATCH'])
        query, body = calls[1][2:]
        self.assertEqual(query['state'], ['not.in.(cancelled,merged,shipped)'])
        self.assertEqual(query['dataset'], ['eq.live'])
        self.assertEqual(query['shepherd'], ['eq.owner'])
        self.assertEqual(query['slack_channel'], ['eq.C1'])
        self.assertEqual(query['slack_ts'], ['eq.123.4'])
        self.assertEqual(body['status']['state'], 'cancelled')
        self.assertEqual(set(body), {'status'})

    def test_lifecycle_saves_do_not_replace_concurrent_comments_or_repros(self):
        expression='let row = baml.json.from_json<map<string,json>>(issue_row(gh_issue(7,Difficulty.Medium))); row.set("id","ISSUE-fixture".to_json()); save_issue(issue_from_row(row.to_json()));'
        calls=self.run_expression(expression,lambda *_:(200,[]))
        self.assertEqual([c[0] for c in calls],['POST','PATCH'])
        self.assertIn('on_conflict',calls[0][2])
        for key in ['comments','repros','feedback_ids','slack_ts','slack_channel']:self.assertNotIn(key,calls[1][3])
        self.assertIn('status',calls[1][3])
        self.assertEqual(calls[1][2]['state'], ['neq.cancelled'])

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
        event=next(body for m,t,_,body in calls if m=='POST' and t=='events' and body[0]['kind']=='feedback_linked')
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
                terminal.extend(row for row in body if row['kind']=='no_issue')
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
        self.assertEqual([(method, table) for method, table, _, _ in calls if method != 'GET'], [('POST', 'events'), ('POST', 'events')])

    def test_duplicate_initial_issue_claim_stops_before_agent_launch(self):
        def respond(method,table,query,body):
            self.assertEqual((method,table),('PATCH','issues'))
            self.assertEqual(query['state'],['eq.open'])
            self.assertEqual(query['status->>automatic_fix'],['eq.true'])
            self.assertEqual(body['status'],{'state':'in_progress','pr':None})
            return 200,[]
        expression='let issue=with_status(with_difficulty(issue_in(Subsystem.Compiler,"owner"),Difficulty.Easy),Open {state:"open",automatic_fix:true}); assert.equal(handle_one(issue,HandleMode.Live),null);'
        calls=self.run_expression(expression,respond)
        self.assertEqual(len(calls),1)


if __name__ == '__main__': unittest.main()
