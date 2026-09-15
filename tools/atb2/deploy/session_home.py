"""Persistent conversation homes, independent from the controller's Claude login."""
import contextlib
import fcntl
import json
import os
from pathlib import Path
import re

ROOT=Path('/data/agent-sessions')
UUID=re.compile(r'[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}\Z')


def metadata(workspace):
    path=ROOT/'workspaces'/Path(workspace).name
    if not path.exists():return None
    data=json.loads(path.read_text())
    if not all(UUID.fullmatch(data.get(k,'')) for k in ('id','claude_session_id')):raise ValueError('invalid session mapping')
    return data


@contextlib.contextmanager
def conversation(workspace, argv):
    data=metadata(workspace)
    if not data:
        yield argv,None
        return
    folder=ROOT/data['id'];home=folder/'home';lock_path=folder/'turn.lock'
    if home.resolve()!=home or not home.is_dir():raise ValueError('unsafe conversation home')
    inherited=os.environ.get('ATB2_SESSION_LOCK_FD')
    with contextlib.ExitStack() as stack:
        if inherited is not None:
            fd=int(inherited);a=os.fstat(fd);b=lock_path.stat()
            if (a.st_dev,a.st_ino)!=(b.st_dev,b.st_ino):raise ValueError('wrong session lock')
            fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB)
        else:
            lock=stack.enter_context(lock_path.open('a+'))
            fcntl.flock(lock,fcntl.LOCK_EX)
        session_id=data['claude_session_id']
        previous=home/'.claude/projects/-workspace'/f'{session_id}.jsonl'
        if previous.is_symlink():raise ValueError('unsafe conversation file')
        args=list(argv)
        # Prompts arrive on stdin, so every argv entry is an option.
        args=[arg for arg in args if arg!='--no-session-persistence']
        # Every invocation still supplies its current explicit tool allowlist,
        # disabled hooks and MCP settings. Resume carries context, not permission.
        args += ['--resume' if previous.is_file() else '--session-id',session_id]
        yield args,home
