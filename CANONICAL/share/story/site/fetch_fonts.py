#!/usr/bin/env python3
"""One-time fetch of the optional web fonts, embedded as data URIs.

Downloads the *latin subset* woff2 files Google Fonts serves (already small)
and writes fonts.css next to this script. build.py inlines fonts.css into the
page; the fonts then work offline and under the artifact CSP. Re-run only to
change the font set — fonts.css is committed so normal builds never need
network access.
"""
import base64, json, pathlib, re, urllib.request

HERE = pathlib.Path(__file__).resolve().parent
UA = ('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 '
      '(KHTML, like Gecko) Chrome/126.0 Safari/537.36')  # woff2-capable UA

FAMILIES = [
    ('Inter', 'https://fonts.googleapis.com/css2?family=Inter:ital,wght@0,400;0,700;1,400&display=swap'),
    ('JetBrains Mono', 'https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400&display=swap'),
    ('Fira Code', 'https://fonts.googleapis.com/css2?family=Fira+Code:wght@400&display=swap'),
]

def fetch(url):
    req = urllib.request.Request(url, headers={'User-Agent': UA})
    with urllib.request.urlopen(req, timeout=30) as r:
        return r.read()

FACE_RE = re.compile(
    r'/\*\s*latin\s*\*/\s*@font-face\s*\{(.*?)\}', re.DOTALL)
PROP_RE = re.compile(r'(font-family|font-style|font-weight):\s*([^;]+);')
URL_RE = re.compile(r'src:\s*url\((https://[^)]+\.woff2)\)')

out = []
total = 0
for name, css_url in FAMILIES:
    css = fetch(css_url).decode()
    faces = FACE_RE.findall(css)
    if not faces:
        raise SystemExit(f'no latin faces found for {name}')
    for body in faces:
        props = dict(PROP_RE.findall(body))
        m = URL_RE.search(body)
        woff = fetch(m.group(1))
        total += len(woff)
        b64 = base64.b64encode(woff).decode()
        out.append(
            '@font-face{font-family:%s;font-style:%s;font-weight:%s;'
            'font-display:swap;src:url(data:font/woff2;base64,%s) format("woff2")}'
            % (props['font-family'], props.get('font-style', 'normal'),
               props.get('font-weight', '400'), b64))
    print(f'{name}: {len(faces)} face(s)')

(HERE / 'fonts.css').write_text('\n'.join(out) + '\n')
print(f'wrote fonts.css ({total/1024:.0f} KB of woff2, '
      f'{(HERE / "fonts.css").stat().st_size/1024:.0f} KB css)')
