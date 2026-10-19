class JObject:
    __slots__ = ('names', 'values')

    def __init__(self):
        self.names = []
        self.values = []

class JArray:
    __slots__ = ('values',)

    def __init__(self):
        self.values = []

class JString:
    __slots__ = ('value',)

    def __init__(self, v):
        self.value = v

class JNumber:
    __slots__ = ('text',)

    def __init__(self, t):
        self.text = t

class JLiteral:
    __slots__ = ('kind',)

    def __init__(self, k):
        self.kind = k

class ParseError(Exception):

    def __init__(self, message, offset):
        self.message = message
        self.offset = offset
ESC = {34: '"', 92: '\\', 47: '/', 98: '\x08', 102: '\x0c', 110: '\n', 114: '\r', 116: '\t'}

class Parser:

    def __init__(self, text):
        self.text = text
        self.cps = [ord(c) for c in text]
        self.pos = 0

    def current(self):
        return self.cps[self.pos] if self.pos < len(self.cps) else -1

    def fail(self, message):
        raise ParseError(message, self.pos)

    def skip_ws(self):
        while self.current() in (32, 9, 10, 13):
            self.pos += 1

    def read_char(self, c):
        if self.current() != c:
            return False
        self.pos += 1
        return True

    def expect(self, c, what):
        if not self.read_char(c):
            self.fail('expected ' + what)

    def read_digits(self):
        if not 48 <= self.current() <= 57:
            self.fail('expected digit')
        while 48 <= self.current() <= 57:
            self.pos += 1

    def read_number(self):
        start = self.pos
        self.read_char(45)
        first = self.current()
        self.read_digits()
        if first == 48 and self.pos - start > 1 + (1 if self.cps[start] == 45 else 0):
            self.fail('leading zero')
        if self.read_char(46):
            self.read_digits()
        if self.read_char(101) or self.read_char(69):
            if not self.read_char(43):
                self.read_char(45)
            self.read_digits()
        return JNumber(self.text[start:self.pos])

    def hex_value(self):
        c = self.current()
        self.pos += 1
        if 48 <= c <= 57:
            return c - 48
        if 97 <= c <= 102:
            return c - 87
        if 65 <= c <= 70:
            return c - 55
        self.fail('bad hex digit')

    def read_string(self):
        self.expect(34, 'string')
        parts = []
        start = self.pos
        while True:
            c = self.current()
            if c == 34:
                break
            if c < 0:
                self.fail('unterminated string')
            if c == 92:
                parts.append(self.text[start:self.pos])
                self.pos += 1
                e = self.current()
                self.pos += 1
                if e in ESC:
                    parts.append(ESC[e])
                elif e == 117:
                    cp = self.hex_value() * 4096 + self.hex_value() * 256 + self.hex_value() * 16 + self.hex_value()
                    parts.append(chr(cp))
                else:
                    self.fail('bad escape')
                start = self.pos
            else:
                self.pos += 1
        parts.append(self.text[start:self.pos])
        self.pos += 1
        return ''.join(parts)

    def read_literal(self, word, kind):
        for ch in word:
            self.expect(ord(ch), word)
        return JLiteral(kind)

    def read_value(self):
        c = self.current()
        if c == 123:
            self.pos += 1
            obj = JObject()
            self.skip_ws()
            if self.read_char(125):
                return obj
            while True:
                self.skip_ws()
                obj.names.append(self.read_string())
                self.skip_ws()
                self.expect(58, "':'")
                self.skip_ws()
                obj.values.append(self.read_value())
                self.skip_ws()
                if not self.read_char(44):
                    break
            self.expect(125, "'}'")
            return obj
        if c == 91:
            self.pos += 1
            arr = JArray()
            self.skip_ws()
            if self.read_char(93):
                return arr
            while True:
                self.skip_ws()
                arr.values.append(self.read_value())
                self.skip_ws()
                if not self.read_char(44):
                    break
            self.expect(93, "']'")
            return arr
        if c == 34:
            return JString(self.read_string())
        if c == 116:
            return self.read_literal('true', 1)
        if c == 102:
            return self.read_literal('false', 2)
        if c == 110:
            return self.read_literal('null', 0)
        if c == 45 or 48 <= c <= 57:
            return self.read_number()
        self.fail('unexpected character')

def parse(text):
    p = Parser(text)
    p.skip_ws()
    v = p.read_value()
    p.skip_ws()
    if p.pos != len(p.cps):
        p.fail('trailing input')
    return v

def walk(v):
    if isinstance(v, JObject):
        return len(v.names) * 7 + sum((len(n) for n in v.names)) + sum((walk(x) for x in v.values))
    if isinstance(v, JArray):
        return 3 + sum((walk(x) for x in v.values))
    if isinstance(v, JString):
        return 11 + len(v.value)
    if isinstance(v, JNumber):
        return 13 + len(v.text)
    return 17 + v.kind

def make_doc(n):
    seed = 5
    records = []
    for i in range(n):
        seed = seed * 16807 % 2147483647
        tags = ['"t%d"' % (seed % (k + 3)) for k in range(i % 5)]
        sign = "-" if i % 3 == 0 else ""
        exp = "e+%d" % (i % 5) if i % 4 == 0 else "E-%d" % (i % 3) if i % 4 == 1 else ""
        parent = str(i // 4) if i % 4 == 0 else "null"
        active = "true" if i % 2 == 0 else "false"
        records.append('{"id": %d, "name": "user_%d", "score": %s%d.%d%s, "tags": [%s], "active": %s, "parent": %s, '
                       '"bio": "line %d\\nwith \\"quotes\\", a tab\\tand caf\\u00e9 \\u2603 / slash \\\\ back", '
                       '"nested": {"depth": [{"x": [%d, [%d, {"y": null}]]}]}}'
                       % (i, seed % 10007, sign, seed % 1000, seed % 10, exp, ", ".join(tags), active, parent, i, i, i + 1))
    return '{"records": [\n  ' + ",\n  ".join(records) + '\n], "count": %d, "ok": true}' % n


def main():
    doc = make_doc(160)
    bad = ['{"a": tru}', '[1, 2', '{"a" 1}', '[01]', '"unterminated', '{"a": -}']
    total = 0
    for _ in range(15):
        total += walk(parse(doc))
        for b in bad:
            try:
                r = parse(b)
            except ParseError as e:
                r = JLiteral(1000 + e.offset)
            total += walk(r)
    return total
