"""Shirt claims and sandboxed BAML play runs, using the shared session worker."""
import json
import os
from pathlib import Path
import re
import time
from urllib.parse import urlsplit

WORKTREES = Path('/data/worktrees')

def run_link(run_id):
    value=os.environ.get('ATB2_UI_URL','')
    url=urlsplit(value)
    if url.scheme=='https' and url.netloc and not url.username and not url.password and url.path in ('','/') and not url.query and not url.fragment:
        return value.rstrip('/')+'/runs/'+str(run_id)
    return 'Run #'+str(run_id)+' (website URL is not configured)'


def shirt(store, turn):
    who=turn['team']+':'+turn['requested_by']
    code=store.send('rpc/claim_promo',body={'who':who},method='POST')
    if code is None:return 'No shirt codes are available right now. Please ask the team.'
    if not isinstance(code,str) or not code or len(code)>200:raise ValueError('invalid promo code')
    channel=store.slack('conversations.open',{'users':turn['requested_by']}).get('channel',{}).get('id')
    if not isinstance(channel,str) or not re.fullmatch(r'D[A-Z0-9]{1,40}',channel):raise ValueError('invalid DM channel')
    escaped=code.replace('&','&amp;').replace('<','&lt;').replace('>','&gt;')
    store.slack('chat.postMessage',{'channel':channel,'text':'Your shirt promo code: '+escaped,'unfurl_links':False,'unfurl_media':False})
    return 'I sent your shirt promo code in a Slack direct message.'


def visible_transcript(path):
    """Keep text/tool turns only; omit initialization, auth metadata and thinking."""
    turns=[];budget=400_000
    with Path(path).open() as stream:
        for line in stream:
            if len(line)>1_000_000:continue
            try:event=json.loads(line)
            except ValueError:continue
            if event.get('type') not in ('assistant','user'):continue
            message=event.get('message',{});content=message.get('content',[])
            if isinstance(content,str):content=[{'type':'text','text':content}]
            if not isinstance(content,list):continue
            blocks=[]
            for block in content:
                if not isinstance(block,dict):continue
                kind=block.get('type')
                if kind=='text':item={'type':'text','text':str(block.get('text',''))[:20000]}
                elif kind=='tool_use':item={'type':'tool_use','name':str(block.get('name',''))[:100],'text':json.dumps(block.get('input',{}))[:20000]}
                elif kind=='tool_result':item={'type':'tool_result','text':json.dumps(block.get('content',''))[:20000]}
                else:continue
                budget-=len(json.dumps(item))
                if budget<0:return turns
                blocks.append(item)
            if blocks:turns.append({'role':event['type'],'content':blocks})
    return turns


def play_report(text):
    value=json.loads(text)
    if not isinstance(value,dict) or not isinstance(value.get('summary'),str) or not value['summary'].strip() or not isinstance(value.get('worked'),bool):raise ValueError('invalid play result')
    feedback=value.get('feedback',[])
    if not isinstance(feedback,list) or len(feedback)>3:raise ValueError('invalid play feedback')
    for item in feedback:
        if not isinstance(item,dict) or not isinstance(item.get('title'),str) or not item['title'].strip() or not isinstance(item.get('description'),str):raise ValueError('invalid play feedback')
    return {'summary':value['summary'][:8000],'worked':value['worked'],'feedback':feedback}


def play(store, session, turn, agent, bind):
    existing=store.send('runs',{'turn_id':'eq.'+turn['id'],'dataset':'eq.'+store.dataset,'limit':'1'})
    if existing:
        row=existing[0]
        if row.get('play_status')=='completed':return (row.get('report') or {}).get('summary','Run completed.')+'\n'+run_link(row['id'])
        return 'The previous attempt was interrupted. Mention me again to start a new attempt.\n'+run_link(row['id'])
    # This project never receives Git credentials or a push operation.
    folder=WORKTREES/('play-'+session['id'])
    if folder.resolve()!=folder:raise ValueError('unsafe play workspace')
    # Only initialize a new directory. Existing files are agent-controlled and
    # may be symlinks; the controller must never follow them while writing.
    try:folder.mkdir()
    except FileExistsError:
        if not folder.is_dir():raise ValueError('invalid play workspace')
    else:
        source=folder/'baml_src';source.mkdir()
        (folder/'baml.toml').write_text('[package]\nname = "bammy_play"\n')
        (source/'main.baml').write_text('// Scratch BAML task.\n')
    bind(store,session,str(folder))
    store.update_session(session,{'kind':'play'})
    marker=Path('/data/target/.baml-cli-rev')
    revision=marker.read_text().strip() if marker.is_file() else ''
    revision=revision if re.fullmatch('[0-9a-f]{40}',revision) else None
    row=store.send('runs',body={'issue_id':None,'kind':'play','mode':'Live','dataset':store.dataset,
        'session_id':session['id'],'turn_id':turn['id'],'prompt':turn['prompt'],'play_status':'running','canary_sha':revision},method='POST')[0]
    started=time.monotonic()
    try:
        prompt='''Try this task in the scratch BAML project at /workspace. Use /data/target/debug/baml-cli, with --agent-skill-check off, and its describe command to verify APIs. Implement and run appropriate checks/tests. The toolchain is the runner's pinned canary build; do not claim unverified execution. You may edit this scratch project and run commands, but never push, deploy, access other projects, or fetch credentials. Missing model credentials are a limitation, not a compiler bug. File feedback only for reproducible BAML defects. Return only JSON {"summary":"what was tried and verified","worked":true,"feedback":[{"title":"defect","description":"reproduction and evidence"}]}. At most 3 feedback items; use an empty list when there is no verified defect.
Task: '''+turn['prompt']
        text,transcript,final=agent(session,prompt,tools='Read,Glob,Grep,Write,Edit,Bash')
        report=play_report(text);ids=[]
        for index,item in enumerate(report['feedback']):
            feedback=dict(turn['feedback']);feedback.pop('slack_event_id',None)
            identity=('GH-' if store.dataset=='eval' else 'PLAY-')+turn['id']+'-'+str(index)
            feedback.update(id=identity,title=item['title'][:120],body=item['description'][:8000],issue_ids=[],files={},dataset=store.dataset)
            store.send('feedback',{'on_conflict':'id'},feedback,'POST','resolution=ignore-duplicates,return=representation');ids.append(identity)
        usage=final.get('usage',{})
        counts=[usage.get(k) for k in ('input_tokens','output_tokens')] if isinstance(usage,dict) else []
        tokens=sum(counts) if counts and all(type(n) is int and n>=0 for n in counts) else None
        store.send('runs',{'id':'eq.'+str(row['id']),'dataset':'eq.'+store.dataset},
            {'play_status':'completed','report':report,'transcript':visible_transcript(transcript),
             'tokens':tokens,'feedback_ids':ids,'turns':final.get('num_turns',0),'seconds':int(time.monotonic()-started)},'PATCH')
        store.update_session(session,{'last_summary':report['summary'],'feedback_ids':list(dict.fromkeys(session.get('feedback_ids',[])+ids))})
        return report['summary']+'\n'+str(len(ids))+' feedback report(s) filed.\n'+run_link(row['id'])
    except Exception:
        store.send('runs',{'id':'eq.'+str(row['id']),'dataset':'eq.'+store.dataset},
            {'play_status':'failed','reason':'Play run did not complete; no push was authorized.','seconds':int(time.monotonic()-started)},'PATCH')
        raise
