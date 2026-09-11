"""Private, incremental transcripts for both Claude launch paths. No store credentials."""
import contextlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import uuid

ROOT = Path('/data/agent-traces')
ID = re.compile(r'[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}\Z')
CHUNK = 12000


def atomic(path, value):
    temporary = path.with_suffix('.tmp')
    fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'w') as f: json.dump(value, f)
    os.replace(temporary, path)


class Trace:
    def __init__(self, stage, prompt='', workspace=''):
        ROOT.mkdir(mode=0o700, parents=True, exist_ok=True)
        self.id = str(uuid.uuid4()); self.folder = ROOT/self.id
        self.folder.mkdir(mode=0o700)
        self.meta = {'id':self.id, 'stage':stage, 'status':'running', 'started_at':time.time(),
                     'updated_at':time.time(), 'pid':os.getpid(), 'dataset':os.environ.get('ATB2_DATASET','live'),
                     'proposal_id':os.environ.get('ATB2_TRACE_PROPOSAL_ID'),
                     'issue_ids':sorted(set(re.findall(r'ISSUE-[A-Za-z0-9_-]+', prompt)))[:100],
                     'pr_urls':sorted(set(re.findall(r'https://github.com/BoundaryML/baml/pull/[1-9][0-9]*',prompt)))[:20],
                     'feedback_ids':sorted(set(re.findall(r'(?:PH|GHI|SL)-[A-Za-z0-9_-]+',prompt)))[:100],
                     'workspace':Path(workspace).name, 'reason':None}
        self.output = (self.folder/'events.jsonl').open('w', buffering=1)
        os.chmod(self.folder/'events.jsonl', 0o600)
        self.final = None
        atomic(self.folder/'meta.json', self.meta)
        if prompt: self.block('user', 'text', prompt)

    def block(self, role, kind, text, name=None):
        # Bound each record so a single large tool output cannot exceed the HTTP response limit.
        for offset in range(0, max(len(text),1), CHUNK):
            self.output.write(json.dumps({'role':role,'type':kind,'text':text[offset:offset+CHUNK],
                                         'name':name,'continuation':offset>0})+'\n')

    def event(self, line):
        try: event = json.loads(line)
        except (ValueError,UnicodeDecodeError): return
        if not isinstance(event,dict): return
        if event.get('type') == 'result':
            self.final = event
            self.meta['reason'] = event.get('subtype') if event.get('is_error') else None
        if event.get('type') not in ('assistant','user'): return
        message = event.get('message',{})
        if not isinstance(message,dict): return
        content = message.get('content',[])
        if isinstance(content,str): content=[{'type':'text','text':content}]
        if not isinstance(content,list): return
        for b in content:
            if not isinstance(b,dict): continue
            kind=b.get('type')
            if kind=='text': self.block(event['type'],kind,str(b.get('text','')))
            elif kind=='tool_use': self.block(event['type'],kind,json.dumps(b.get('input',{}),indent=2),str(b.get('name',''))[:100])
            elif kind=='tool_result':
                value=b.get('content','')
                self.block(event['type'],kind,value if isinstance(value,str) else json.dumps(value,indent=2))
        self.meta['updated_at']=time.time(); atomic(self.folder/'meta.json',self.meta)

    def close(self, code):
        self.meta.update(status='completed' if code==0 and self.final and not self.final.get('is_error') else 'failed',updated_at=time.time())
        if self.meta['status']=='failed' and not self.meta['reason']:self.meta['reason']='agent_interrupted_or_no_result'
        atomic(self.folder/'meta.json',self.meta);self.output.close()


def capture(command, env, output, stage, prompt='', workspace='', input_bytes=None, stderr=None):
    trace=Trace(stage,prompt,workspace); code=1
    try:
        with subprocess.Popen(command,env=env,stdout=subprocess.PIPE,stderr=stderr,
                              stdin=subprocess.PIPE if input_bytes is not None else None) as child:
            # Prompts can exceed pipe capacity. Feed concurrently while draining output.
            import threading
            def feed():
                try: child.stdin.write(input_bytes)
                except BrokenPipeError: pass
                finally: child.stdin.close()
            writer=None
            if input_bytes is not None:
                writer=threading.Thread(target=feed);writer.start()
            for line in child.stdout:
                trace.event(line)
                output.write(line);output.flush()
            code=child.wait()
            if writer:writer.join()
        return code
    finally:trace.close(code)


def read_file(folder, name):
    # Every component below ROOT is opened without following agent-controlled links.
    parent=os.open(ROOT,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
    try:
        child=os.open(folder,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW,dir_fd=parent)
        try:return os.open(name,os.O_RDONLY|os.O_NOFOLLOW,dir_fd=child)
        finally:os.close(child)
    finally:os.close(parent)


def metadata(identity):
    if not ID.fullmatch(identity):raise ValueError('invalid trace ID')
    with os.fdopen(read_file(identity,'meta.json')) as f:value=json.load(f)
    # Process death must not leave a permanently running entry.
    if value['status']=='running':
        try:os.kill(value['pid'],0)
        except ProcessLookupError:value.update(status='interrupted',reason='agent_process_ended')
    value.pop('pid',None)
    return value


def read(request):
    dataset=request.get('dataset','live')
    if dataset not in ('live','eval'):raise ValueError('invalid dataset')
    identity=request.get('id','')
    if not identity:
        result=[]
        if ROOT.exists():
            for p in ROOT.iterdir():
                if not ID.fullmatch(p.name):continue
                try:m=metadata(p.name)
                except (OSError,ValueError,KeyError):continue
                if request.get('pr') and request['pr'] not in m.get('pr_urls',[]):continue
                if m.get('dataset')==dataset and (not request.get('issue_id') or request['issue_id'] in m.get('issue_ids',[]) or bool(set(request.get('feedback_ids',[])) & set(m.get('feedback_ids',[])))):result.append(m)
        return sorted(result,key=lambda m:m['started_at'],reverse=True)[:100]
    meta=metadata(identity)
    if meta.get('dataset')!=dataset:raise ValueError('wrong dataset')
    offset=request.get('offset',0)
    if type(offset) is not int or offset<0:raise ValueError('invalid offset')
    events=[]
    with os.fdopen(read_file(identity,'events.jsonl'),'rb') as f:
        size=os.fstat(f.fileno()).st_size
        if offset>size:raise ValueError('invalid offset')
        if offset:
            f.seek(offset-1)
            if f.read(1)!=b'\n':raise ValueError('offset must be a record boundary')
        f.seek(offset)
        while len(events)<20:
            position=f.tell();line=f.readline(200000)
            if not line or not line.endswith(b'\n'):f.seek(position);break
            events.append(json.loads(line))
        return {'meta':meta,'events':events,'offset':f.tell(),'more':len(events)==20 and f.tell()<size}


def main():
    if sys.argv[1:]==['read']:
        try:print(json.dumps(read(json.load(sys.stdin))))
        except (OSError,ValueError,KeyError):print(json.dumps({'error':'Transcript unavailable'}))
        return 0
    args=sys.argv[1:]
    if '-p' not in args and '--print' not in args:os.execve('/usr/local/bin/claude',['claude',*args],dict(os.environ))
    prompt=args[-1] if args and not args[-1].startswith('-') else ''
    stage='gauge' if '--model' in args and args[args.index('--model')+1]=='opus' else 'triage'
    return capture(['/usr/local/bin/claude',*args],dict(os.environ),sys.stdout.buffer,stage,prompt,os.getcwd())


if __name__=='__main__':sys.exit(main())
