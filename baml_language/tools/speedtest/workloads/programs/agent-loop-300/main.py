import json

class AgentFailed(Exception):

    def __init__(self, retries):
        self.retries = retries

def is_int(v):
    return type(v) is int

def decode(raw):
    try:
        d = json.loads(raw)
    except ValueError:
        return None
    if not isinstance(d, dict):
        return None
    t = d.get('tool')
    if t == 'search' and isinstance(d.get('query'), str) and (d.get('limit') is None or is_int(d['limit'])):
        return d
    if t == 'calc' and d.get('op') in ('add', 'sub', 'mul') and is_int(d.get('a')) and is_int(d.get('b')):
        return d
    if t == 'finish' and isinstance(d.get('answer'), str) and (d.get('confidence') in ('low', 'medium', 'high')):
        return d
    return None

def fake_search(query, limit):
    h = 0
    for ch in query:
        h = (h * 31 + ord(ch)) % 1000003
    return h % (limit * 100)

def run_conversation(turns):
    st = {'scratch': {}, 'history': [], 'steps': 0, 'retries': 0, 'answer': None}
    for raw in turns:
        a = decode(raw)
        if a is None:
            st['retries'] += 1
            st['history'].append('retry')
            if st['retries'] > 2:
                raise AgentFailed(st['retries'])
            continue
        st['steps'] += 1
        t = a['tool']
        if t == 'search':
            lim = a.get('limit')
            v = fake_search(a['query'], 10 if lim is None else lim)
            st['scratch'][a['query']] = v
            st['history'].append('search:' + a['query'] + '=' + str(v))
        elif t == 'calc':
            op = a['op']
            v = a['a'] + a['b'] if op == 'add' else a['a'] - a['b'] if op == 'sub' else a['a'] * a['b']
            st['scratch']['calc' + str(st['steps'])] = v
            st['history'].append('calc:' + op + '=' + str(v))
        else:
            st['answer'] = a['answer'] + '/' + a['confidence']
            st['history'].append('finish')
            break
    return st

class Rng:
    def __init__(self, seed):
        self.seed = seed

    def draw(self, n):
        self.seed = self.seed * 16807 % 2147483647
        return self.seed % n


WORDS = ["weather", "paris", "capital", "population", "rust", "python", "llm", "agent", "tokens", "latency"]


def conversations(count):
    rng = Rng(17)
    convs = []
    for _ in range(count):
        turns = []
        for _ in range(2 + rng.draw(6)):
            kind = rng.draw(10)
            if kind < 4:
                q = [WORDS[rng.draw(10)] for _ in range(1 + rng.draw(3))]
                r = '{"tool": "search", "query": "' + " ".join(q) + '"'
                if rng.draw(2) == 1:
                    r += ', "limit": ' + str(1 + rng.draw(9))
                turns.append(r + "}")
            elif kind < 7:
                op = ["add", "sub", "mul"][rng.draw(3)]
                a = rng.draw(1000)
                b = rng.draw(1000)
                turns.append('{"tool": "calc", "op": "%s", "a": %d, "b": %d}' % (op, a, b))
            elif kind == 7:
                turns.append('{"tool": "search", "query": "broken"}'[: 5 + rng.draw(10)])
            elif kind == 8:
                turns.append('{"tool": "browse", "url": "http://x"}')
            else:
                turns.append('{"tool": "calc", "op": "add", "a": %d}' % rng.draw(10))
        answer = rng.draw(100000)
        turns.append('{"tool": "finish", "answer": "answer %d", "confidence": "%s"}' % (answer, ["low", "medium", "high"][rng.draw(3)]))
        convs.append(turns)
    return convs


def main():
    convs = conversations(300)
    check = 0
    for _ in range(3):
        for turns in convs:
            try:
                st = run_conversation(turns)
                score = st['steps'] * 1000 + st['retries'] * 100 + len(st['answer'] or '') + len(st['history']) * 3 + sum(st['scratch'].values()) % 9973
            except AgentFailed as e:
                score = 7 + e.retries
            check = (check * 31 + score) % 1000000007
    return check
