package main

import (
	"strconv"
	"strings"
	"unicode/utf8"
)

// JValue is the union JObject | JArray | JString | JNumber | JLiteral.
type JValue interface{ jvalue() }

type JObject struct {
	names  []string
	values []JValue
}

type JArray struct {
	values []JValue
}

type JString struct {
	value string
}

type JNumber struct {
	text string
}

type JLiteral struct {
	kind int
}

func (*JObject) jvalue()  {}
func (*JArray) jvalue()   {}
func (*JString) jvalue()  {}
func (*JNumber) jvalue()  {}
func (*JLiteral) jvalue() {}

type ParseError struct {
	message string
	offset  int
}

type Parser struct {
	text string
	cps  []rune
	pos  int
}

func (p *Parser) current() int {
	if p.pos < len(p.cps) {
		return int(p.cps[p.pos])
	}
	return -1
}

// fail throws: parse recovers the *ParseError.
func (p *Parser) fail(message string) int {
	panic(&ParseError{message: message, offset: p.pos})
}

func (p *Parser) skipWs() {
	for {
		c := p.current()
		if c != 32 && c != 9 && c != 10 && c != 13 {
			break
		}
		p.pos++
	}
}

func (p *Parser) readChar(c int) bool {
	if p.current() != c {
		return false
	}
	p.pos++
	return true
}

func (p *Parser) expect(c int, what string) {
	if !p.readChar(c) {
		p.fail("expected " + what)
	}
}

func isDigit(c int) bool { return c >= 48 && c <= 57 }

func (p *Parser) readDigits() {
	if !isDigit(p.current()) {
		p.fail("expected digit")
	}
	for isDigit(p.current()) {
		p.pos++
	}
}

func (p *Parser) readNumber() JValue {
	start := p.pos
	p.readChar(45)
	first := p.current()
	p.readDigits()
	minus := 0
	if p.cps[start] == 45 {
		minus = 1
	}
	if first == 48 && p.pos-start > 1+minus {
		p.fail("leading zero")
	}
	if p.readChar(46) {
		p.readDigits()
	}
	if p.readChar(101) || p.readChar(69) {
		if !p.readChar(43) {
			p.readChar(45)
		}
		p.readDigits()
	}
	return &JNumber{text: p.text[start:p.pos]}
}

func (p *Parser) hexValue() int {
	c := p.current()
	p.pos++
	if c >= 48 && c <= 57 {
		return c - 48
	}
	if c >= 97 && c <= 102 {
		return c - 87
	}
	if c >= 65 && c <= 70 {
		return c - 55
	}
	return p.fail("bad hex digit")
}

func (p *Parser) readString() string {
	p.expect(34, "string")
	parts := []string{}
	start := p.pos
	for {
		c := p.current()
		if c == 34 {
			break
		}
		if c < 0 {
			p.fail("unterminated string")
		}
		if c == 92 {
			parts = append(parts, p.text[start:p.pos])
			p.pos++
			e := p.current()
			p.pos++
			if e == 34 {
				parts = append(parts, "\"")
			} else if e == 92 {
				parts = append(parts, "\\")
			} else if e == 47 {
				parts = append(parts, "/")
			} else if e == 98 {
				parts = append(parts, string(rune(8)))
			} else if e == 102 {
				parts = append(parts, string(rune(12)))
			} else if e == 110 {
				parts = append(parts, "\n")
			} else if e == 114 {
				parts = append(parts, "\r")
			} else if e == 116 {
				parts = append(parts, "\t")
			} else if e == 117 {
				cp := p.hexValue()*4096 + p.hexValue()*256 + p.hexValue()*16 + p.hexValue()
				parts = append(parts, string(rune(cp)))
			} else {
				p.fail("bad escape")
			}
			start = p.pos
		} else {
			p.pos++
		}
	}
	parts = append(parts, p.text[start:p.pos])
	p.pos++
	return strings.Join(parts, "")
}

func (p *Parser) readLiteral(word string, kind int) JValue {
	for _, c := range word {
		p.expect(int(c), word)
	}
	return &JLiteral{kind: kind}
}

func (p *Parser) readValue() JValue {
	c := p.current()
	if c == 123 {
		p.pos++
		obj := &JObject{names: []string{}, values: []JValue{}}
		p.skipWs()
		if p.readChar(125) {
			return obj
		}
		for {
			p.skipWs()
			obj.names = append(obj.names, p.readString())
			p.skipWs()
			p.expect(58, "':'")
			p.skipWs()
			obj.values = append(obj.values, p.readValue())
			p.skipWs()
			if !p.readChar(44) {
				break
			}
		}
		p.expect(125, "'}'")
		return obj
	}
	if c == 91 {
		p.pos++
		arr := &JArray{values: []JValue{}}
		p.skipWs()
		if p.readChar(93) {
			return arr
		}
		for {
			p.skipWs()
			arr.values = append(arr.values, p.readValue())
			p.skipWs()
			if !p.readChar(44) {
				break
			}
		}
		p.expect(93, "']'")
		return arr
	}
	if c == 34 {
		return &JString{value: p.readString()}
	}
	if c == 116 {
		return p.readLiteral("true", 1)
	}
	if c == 102 {
		return p.readLiteral("false", 2)
	}
	if c == 110 {
		return p.readLiteral("null", 0)
	}
	if c == 45 || isDigit(c) {
		return p.readNumber()
	}
	p.fail("unexpected character")
	return &JLiteral{kind: 0}
}

// parse returns the value, or the ParseError thrown while parsing.
func parse(text string) (v JValue, err *ParseError) {
	defer func() {
		if r := recover(); r != nil {
			e, ok := r.(*ParseError)
			if !ok {
				panic(r)
			}
			v, err = nil, e
		}
	}()
	p := &Parser{text: text, cps: []rune(text), pos: 0}
	p.skipWs()
	v = p.readValue()
	p.skipWs()
	if p.pos != len(p.cps) {
		p.fail("trailing input")
	}
	return v, nil
}

func walk(v JValue) int {
	switch v := v.(type) {
	case *JObject:
		h := len(v.names) * 7
		for _, n := range v.names {
			h += utf8.RuneCountInString(n)
		}
		for _, x := range v.values {
			h += walk(x)
		}
		return h
	case *JArray:
		h := 3
		for _, x := range v.values {
			h += walk(x)
		}
		return h
	case *JString:
		return 11 + utf8.RuneCountInString(v.value)
	case *JNumber:
		return 13 + utf8.RuneCountInString(v.text)
	case *JLiteral:
		return 17 + v.kind
	}
	panic("unreachable")
}

// makeDoc builds the input: 160 records exercising every JSON form, with
// escapes in strings and numbers in every shape, from a seeded LCG.
func makeDoc(n int) string {
	seed := 5
	records := []string{}
	for i := 0; i < n; i++ {
		seed = seed * 16807 % 2147483647
		tags := []string{}
		for k := 0; k < i%5; k++ {
			tags = append(tags, "\"t"+strconv.Itoa(seed%(k+3))+"\"")
		}
		sign := ""
		if i%3 == 0 {
			sign = "-"
		}
		exp := ""
		if i%4 == 0 {
			exp = "e+" + strconv.Itoa(i%5)
		} else if i%4 == 1 {
			exp = "E-" + strconv.Itoa(i%3)
		}
		parent := "null"
		if i%4 == 0 {
			parent = strconv.Itoa(i / 4)
		}
		active := "false"
		if i%2 == 0 {
			active = "true"
		}
		records = append(records, "{\"id\": "+strconv.Itoa(i)+", \"name\": \"user_"+strconv.Itoa(seed%10007)+"\", \"score\": "+sign+
			strconv.Itoa(seed%1000)+"."+strconv.Itoa(seed%10)+exp+", \"tags\": ["+strings.Join(tags, ", ")+"], \"active\": "+active+
			", \"parent\": "+parent+", \"bio\": \"line "+strconv.Itoa(i)+"\\nwith \\\"quotes\\\", a tab\\tand caf\\u00e9 \\u2603 / slash \\\\ back\""+
			", \"nested\": {\"depth\": [{\"x\": ["+strconv.Itoa(i)+", ["+strconv.Itoa(i+1)+", {\"y\": null}]]}]}}")
	}
	return "{\"records\": [\n  " + strings.Join(records, ",\n  ") + "\n], \"count\": " + strconv.Itoa(n) + ", \"ok\": true}"
}

func Main() int {
	doc := makeDoc(160)
	bad := []string{"{\"a\": tru}", "[1, 2", "{\"a\" 1}", "[01]", "\"unterminated", "{\"a\": -}"}
	total := 0
	for i := 0; i < 15; i++ {
		v, err := parse(doc)
		if err != nil {
			panic(err.message)
		}
		total += walk(v)
		for _, b := range bad {
			r, err := parse(b)
			if err != nil {
				r = &JLiteral{kind: 1000 + err.offset}
			}
			total += walk(r)
		}
	}
	return total
}
