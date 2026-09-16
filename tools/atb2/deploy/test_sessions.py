"""Offline routing, persistence and session-lock regression tests."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch, Mock
import uuid


def module(name, filename):
    spec=importlib.util.spec_from_file_location(name,Path(__file__).with_name(filename))
    value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value
s=module('sessions','sessions.py');h=module('home','session_home.py')

class SessionTests(unittest.TestCase):
    def test_run_privacy_check_uses_the_anonymous_role_and_refuses_visible_rows(self):
        store=object.__new__(s.Store);store.host='example.invalid';store.key='private-fixture'
        with patch.dict(s.os.environ,{'FEEDBACK_SUPABASE_ANON_KEY':'public-fixture'}),patch.object(s,'request',return_value=[]) as request:
            store.ensure_private_run(1)
            self.assertEqual(request.call_args.args[2],'public-fixture')
            self.assertEqual(request.call_args.kwargs['key'],'public-fixture')
            request.return_value=[{'id':1}]
            with self.assertRaises(ValueError):store.ensure_private_run(1)

    def test_followup_uses_existing_session_and_explicit_tasks_route_separately(self):
        self.assertEqual(s.route('why did that test fail?',True),'chat')
        self.assertEqual(s.route('give me a t-shirt',True),'shirt')
        self.assertEqual(s.route('try a classifier',True),'play')
        self.assertEqual(s.route('babysit this PR',False),'babysit')
        self.assertEqual(s.route('the shirt function crashes',False),'infer')
        self.assertEqual(s.route('parser broke',False),'infer')
    def test_same_session_cannot_execute_two_turns_concurrently(self):
        with tempfile.TemporaryDirectory() as tmp,patch.object(s,'ROOT',Path(tmp).resolve()):
            session=str(uuid.uuid4())
            with s.session_lock(session):
                with self.assertRaises(BlockingIOError):
                    with s.session_lock(session,blocking=False):pass
            with s.session_lock(session,blocking=False):pass
    def test_new_and_resumed_invocations_keep_explicit_permissions(self):
        with tempfile.TemporaryDirectory() as tmp,patch.object(h,'ROOT',Path(tmp).resolve()):
            root=Path(tmp).resolve();sid=str(uuid.uuid4());cid=str(uuid.uuid4())
            home=root/sid/'home';home.mkdir(parents=True)
            (root/'workspaces').mkdir();(root/'workspaces'/'checkout').write_text(json.dumps({'id':sid,'claude_session_id':cid}))
            args=['claude','-p','--no-session-persistence','--tools','Read,Glob,Grep']
            with h.conversation('/data/worktrees/checkout',args) as (argv,mount):
                self.assertEqual(argv[-2:],['--session-id',cid]);self.assertEqual(mount,home)
                self.assertNotIn('--no-session-persistence',argv)
                self.assertIn('Read,Glob,Grep',argv)
            journal=home/'.claude/projects/-workspace'/f'{cid}.jsonl';journal.parent.mkdir(parents=True);journal.write_text('{}\n')
            with h.conversation('/data/worktrees/checkout',args) as (argv,_):
                self.assertEqual(argv[-2:],['--resume',cid]);self.assertIn('Read,Glob,Grep',argv)
    def test_malformed_mapping_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp,patch.object(h,'ROOT',Path(tmp).resolve()):
            root=Path(tmp).resolve();(root/'workspaces').mkdir();(root/'workspaces'/'checkout').write_text('{"id":"../home","claude_session_id":"bad"}')
            with self.assertRaises(ValueError):h.metadata('/data/worktrees/checkout')
    def test_new_workspace_cannot_remove_one_still_running_tests(self):
        class Store:
            def update_session(self,session,values):session.update(values)
            def reply(self,*args):raise AssertionError('not a lost session')
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp).resolve();index=root/'workspaces';index.mkdir()
            sid=str(uuid.uuid4());cid=str(uuid.uuid4())
            (root/sid/'home').mkdir(parents=True)
            old=root/'old';new=root/'new';old.mkdir();new.mkdir()
            (index/'old').write_text(json.dumps({'id':sid,'claude_session_id':cid,'active':True}))
            session={'id':sid,'claude_session_id':cid,'workspace':str(old)}
            with patch.object(s,'ROOT',root),patch.object(s,'INDEX',index),patch.object(s,'workspace_path',side_effect=Path):
                s.bind(Store(),session,str(new),active=True)
                self.assertTrue(old.exists())
                (index/'old').write_text(json.dumps({'id':sid,'claude_session_id':cid,'active':False}))
                session['workspace']=str(old)
                s.bind(Store(),session,str(new),active=True)
                self.assertFalse(old.exists())

    def test_failed_classification_does_not_turn_a_retry_into_chat(self):
        session={'workspace':'/data/worktrees/fixture'}
        turn={'prompt':'parser crashes on this program','feedback':{'id':'fixture'}}
        store=Mock();store.update_session.side_effect=lambda row,values:row.update(values)
        # A prepared workspace exists even when classification never succeeded.
        with patch.object(s,'prepare'),patch.object(s,'agent',side_effect=ValueError('offline failure')):
            with self.assertRaises(ValueError):s.dispatch(store,session,turn,s.has_context(session))
        with patch.object(s,'prepare'),patch.object(s,'agent',return_value=('{"kind":"feedback"}','',{})):
            answer=s.dispatch(store,session,turn,s.has_context(session))
        self.assertIn('Logged as feedback',answer)
        self.assertEqual(store.send.call_args.args[0],'feedback')

    def test_explicit_feedback_after_clarification_or_chat_is_filed(self):
        store=Mock();store.update_session.side_effect=lambda row,values:row.update(values)
        with patch.object(s,'agent') as agent:
            answer=s.dispatch(store,{'last_summary':'Earlier question'},
                {'prompt':'report this bug please','feedback':{'id':'fixture'}},True)
        self.assertIn('Logged as feedback',answer)
        agent.assert_not_called()
        self.assertEqual(store.send.call_args.args[0],'feedback')

    def test_failed_classification_requests_clarification(self):
        with patch.object(s,'prepare'),patch.object(s,'agent',return_value=('not json','',{})):
            answer=s.dispatch(None,{}, {'prompt':'ambiguous'},False)
            self.assertIn('Do you want',answer)

if __name__=='__main__':unittest.main()
