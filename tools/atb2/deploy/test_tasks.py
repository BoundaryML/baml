"""Offline tests of promo delivery and sandboxed play bookkeeping."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

spec=importlib.util.spec_from_file_location('tasks',Path(__file__).with_name('tasks.py'))
t=importlib.util.module_from_spec(spec);spec.loader.exec_module(t)

class Store:
    dataset='live'
    def __init__(self):self.calls=[];self.run=None
    def send(self,table,query=None,body=None,method='GET',prefer=None):
        self.calls.append((table,query,body,method))
        if table=='runs':
            if method=='GET':return [self.run] if self.run else []
            if method=='POST':self.run={'id':1,**body};return [self.run]
            self.run.update(body);return [self.run]
        return []
    def update_session(self,session,values):session.update(values)

class TaskTests(unittest.TestCase):
    def test_shirt_retries_claim_same_identity_without_posting_code_to_thread(self):
        store=Mock();store.send.return_value='fixture-code'
        store.slack.side_effect=[{'channel':{'id':'D1'}},ValueError('DM unavailable'),{'channel':{'id':'D1'}},{'ok':True}]
        turn={'team':'T1','requested_by':'U1'}
        with self.assertRaises(ValueError):t.shirt(store,turn)
        answer=t.shirt(store,turn)
        self.assertNotIn('fixture-code',answer)
        self.assertEqual(store.send.call_args_list[0],store.send.call_args_list[1])
        self.assertEqual(store.slack.call_args.args[1]['channel'],'D1')

    def test_exhausted_codes_do_not_send_a_dm(self):
        store=Mock();store.send.return_value=None
        self.assertIn('No shirt codes',t.shirt(store,{'team':'T1','requested_by':'U1'}))
        store.slack.assert_not_called()

    def test_visible_transcript_excludes_initialization_and_thinking(self):
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)/'turns.jsonl'
            path.write_text('\n'.join(json.dumps(x) for x in [
                {'type':'system','apiKeySource':'private-metadata'},
                {'type':'assistant','message':{'content':[{'type':'thinking','thinking':'hidden'}, {'type':'text','text':'answer'}, {'type':'tool_use','name':'Bash','input':{'command':'baml check'}}]}},
                {'type':'user','message':{'content':[{'type':'tool_result','content':'passed'}]}}
            ]))
            turns=t.visible_transcript(path)
            self.assertEqual(len(turns),2)
            self.assertEqual([b['type'] for b in turns[0]['content']],['text','tool_use'])
            self.assertNotIn('private-metadata',json.dumps(turns));self.assertNotIn('hidden',json.dumps(turns))

    def test_play_writes_shared_feedback_and_never_reexecutes_completed_turn(self):
        with tempfile.TemporaryDirectory() as tmp,patch.object(t,'WORKTREES',Path(tmp).resolve()):
            journal=Path(tmp)/'journal';journal.write_text('')
            store=Store();session={'id':'fixture','feedback_ids':[]}
            turn={'id':'turn','prompt':'try a task','feedback':{'id':'SL-1','slack_event_id':'Ev1','source':'Slack'}}
            agent=Mock(return_value=(json.dumps({'summary':'Reproduced parser bug','worked':False,'feedback':[{'title':'Parser bug','description':'steps'}]}),str(journal),{'usage':{'input_tokens':10,'output_tokens':5},'num_turns':2}))
            bind=Mock()
            result=t.play(store,session,turn,agent,bind)
            self.assertIn('Reproduced parser bug',result)
            self.assertEqual(store.run['tokens'],15);self.assertEqual(store.run['play_status'],'completed')
            feedback=[c[2] for c in store.calls if c[0]=='feedback'][0]
            self.assertEqual(feedback['issue_ids'],[]);self.assertNotIn('slack_event_id',feedback)
            self.assertEqual(session['feedback_ids'],['PLAY-turn-0'])
            t.play(store,session,turn,agent,bind);agent.assert_called_once()
            self.assertEqual(agent.call_args.kwargs['tools'],'Read,Glob,Grep,Write,Edit,Bash')

    def test_interrupted_run_requires_new_mention(self):
        store=Store();store.run={'id':1,'play_status':'running'};agent=Mock()
        self.assertIn('interrupted',t.play(store,{}, {'id':'t'},agent,Mock()))
        agent.assert_not_called()

    def test_controller_does_not_follow_files_left_by_previous_agent(self):
        with tempfile.TemporaryDirectory() as tmp,patch.object(t,'WORKTREES',Path(tmp).resolve()):
            root=Path(tmp).resolve();folder=root/'play-fixture';folder.mkdir()
            victim=root/'controller-file'
            (folder/'baml.toml').symlink_to(victim)
            (folder/'baml_src').symlink_to(root/'missing')
            store=Store();agent=Mock(side_effect=ValueError('offline'))
            with self.assertRaises(ValueError):t.play(store,{'id':'fixture'}, {'id':'t','prompt':'try'},agent,Mock())
            self.assertFalse(victim.exists());self.assertFalse((root/'missing').exists())
            self.assertEqual(store.run['play_status'],'failed')

    def test_invalid_model_result_is_rejected(self):
        for text in ['{}','{"summary":"ok","worked":"yes"}','{"summary":"ok","worked":true,"feedback":[{}]}']:
            with self.assertRaises(ValueError):t.play_report(text)

if __name__=='__main__':unittest.main()
