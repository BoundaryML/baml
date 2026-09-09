"""Shirt claims and sandboxed BAML play runs, using the shared session worker."""
import json
import io
import contextlib
import os
from pathlib import Path
import re
import time
import uuid
import importlib.util
from urllib.parse import urlsplit

WORKTREES = Path('/data/worktrees')
SESSIONS = Path('/data/agent-sessions')

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


def visible_transcript(path, stream=None):
    """Keep text/tool turns only; omit initialization, auth metadata and thinking."""
    turns=[]
    with (contextlib.nullcontext(stream) if stream is not None else Path(path).open()) as stream:
        for line in stream:
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
                if kind=='text':item={'type':'text','text':str(block.get('text',''))}
                elif kind=='tool_use':item={'type':'tool_use','name':str(block.get('name',''))[:100],'text':json.dumps(block.get('input',{}),indent=2)}
                elif kind=='tool_result':
                    content=block.get('content','')
                    item={'type':'tool_result','text':content if isinstance(content,str) else json.dumps(content,indent=2)}
                else:continue
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


def latest_cli():
    spec=importlib.util.spec_from_file_location('cli_cache_service',Path(__file__).with_name('cli-cache-service.py'))
    service=importlib.util.module_from_spec(spec);spec.loader.exec_module(service)
    return service.request('canary')


def session_transcript(session, fallback):
    # Open each agent-controlled path component without following links.
    identity=str(uuid.UUID(session['id']));conversation=str(uuid.UUID(session['claude_session_id']))
    fd=os.open(SESSIONS/identity/'home',os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
    try:
        for part in ['.claude','projects','-workspace']:
            next_fd=os.open(part,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW,dir_fd=fd)
            os.close(fd);fd=next_fd
        journal=os.open(conversation+'.jsonl',os.O_RDONLY|os.O_NOFOLLOW,dir_fd=fd)
        with os.fdopen(journal) as stream:return visible_transcript(None,stream)
    except FileNotFoundError:return visible_transcript(fallback)
    finally:os.close(fd)


def update_play_session(store, session, transcript):
    rows=store.send('runs',{'session_id':'eq.'+session['id'],'kind':'eq.play','dataset':'eq.'+store.dataset,'order':'created_at.desc','limit':'1'})
    if not rows:return
    row=rows[0];reports=cli_feedback(session['id'])
    report=dict(row.get('report') or {});report['feedback']=reports
    store.send('runs',{'id':'eq.'+str(row['id']),'dataset':'eq.'+store.dataset},
        {'transcript':session_transcript(session,transcript),'feedback_ids':[r['id'] for r in reports],'report':report},'PATCH')


def cli_feedback(session_id):
    """Read only the agent's feedback journal, never follow agent-controlled symlinks."""
    if str(uuid.UUID(session_id))!=session_id:raise ValueError('invalid session')
    root=SESSIONS/session_id/'home'
    descriptors=[]
    try:
        fd=os.open(root,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW);descriptors.append(fd)
        fd=os.open('.baml',os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW,dir_fd=fd);descriptors.append(fd)
        fd=os.open('feedback.json',os.O_RDONLY|os.O_NOFOLLOW,dir_fd=fd)
        with os.fdopen(fd) as stream:
            data=stream.read(10_000_001)
        if len(data)>10_000_000:raise ValueError('feedback journal too large')
        rows=json.loads(data).get('reports',[]);result=[]
        for row in rows:
            identity=str(uuid.UUID(row['event_uuid']))
            result.append({'id':'PH-'+identity,'title':str(row.get('title','')),
                'description':str(row.get('description') or ''),'status':str(row.get('status','open')),
                'created_at':row.get('created_at')})
        return result
    except FileNotFoundError:return []
    finally:
        for fd in reversed(descriptors):os.close(fd)


def play(store, session, turn, agent, bind):
    if store.dataset != 'live':raise ValueError('CLI feedback play runs require the live dataset')
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
    row=store.send('runs',body={'issue_id':None,'kind':'play','mode':'Live','dataset':store.dataset,
        'session_id':session['id'],'turn_id':turn['id'],'play_status':'running'},method='POST')[0]
    started=time.monotonic()
    ids=[];reports=[];transcript=None;stage="privacy"
    query={'id':'eq.'+str(row['id']),'dataset':'eq.'+store.dataset}
    try:
        store.ensure_private_run(row['id'])
        store.send('runs',query,{'prompt':turn['prompt']},'PATCH')
        stage='toolchain'
        cli=latest_cli()
        version=Path(cli).parent.parent.name;revision=Path(cli).parent.name
        before={r['id'] for r in cli_feedback(session['id'])}
        store.send('runs',query,{'canary_sha':revision,'report':{'version':version,'summary':'Running with latest canary'}},'PATCH')
        def progress(path):
            nonlocal transcript,ids,reports
            transcript=path
            reports=cli_feedback(session['id'])
            ids=[r['id'] for r in reports]
            store.send('runs',query,{'transcript':session_transcript(session,path),'feedback_ids':ids,
                'report':{'version':version,'feedback':reports,'summary':'Running'}},'PATCH')
        prompt=f"""Try this task in /workspace using the latest canary CLI at {cli} (version {version}, revision {revision}).
Use its describe/help commands to verify APIs and run checks/tests. Do not use the runner's older /data/target CLI.
Report every verified defect with `{cli} feedback --anonymous --title ... --description ...` (attach repro files with --files).
Use the real feedback command, not a JSON suggestion or a direct database write. Preserve its local report journal in ~/.baml.
Do not file speculative defects. Missing model credentials are a limitation, not a compiler bug.
Never push, deploy, fetch credentials, or access other projects. Return JSON {{"summary":"what was tried and verified","worked":true,"feedback":[]}}.
Task: """+turn['prompt']
        stage='agent'
        text,transcript,final=agent(session,prompt,tools='Read,Glob,Grep,Write,Edit,Bash',on_transcript=progress)
        progress(transcript)
        report=play_report(text);report.update(version=version,feedback=reports)
        usage=final.get('usage',{})
        counts=[usage.get(k) for k in ('input_tokens','output_tokens')] if isinstance(usage,dict) else []
        tokens=sum(counts) if counts and all(type(n) is int and n>=0 for n in counts) else None
        store.send('runs',{'id':'eq.'+str(row['id']),'dataset':'eq.'+store.dataset},
            {'play_status':'completed','report':report,'transcript':session_transcript(session,transcript),
             'tokens':tokens,'feedback_ids':ids,'turns':final.get('num_turns',0),'seconds':int(time.monotonic()-started)},'PATCH')
        store.update_session(session,{'last_summary':report['summary'],'feedback_ids':list(dict.fromkeys(session.get('feedback_ids',[])+ids))})
        return report['summary']+'\n'+str(len(set(ids)-before))+' feedback report(s) filed.\n'+run_link(row['id'])
    except Exception:
        reason={'privacy':'Play storage privacy check failed. Configure the anonymous key and hide play runs from public readers.',
                'toolchain':'The latest canary CLI could not be resolved or built.',
                'agent':'Play run did not complete; see the saved transcript.'}[stage]
        store.send('runs',{'id':'eq.'+str(row['id']),'dataset':'eq.'+store.dataset},
            {'play_status':'failed','reason':reason,'feedback_ids':ids,'seconds':int(time.monotonic()-started)},'PATCH')
        raise
