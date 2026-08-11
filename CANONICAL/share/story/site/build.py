#!/usr/bin/env python3
"""Build the Project Studio story site from CANONICAL/share/story/*.md.

Usage:
  build.py            full build -> studio-story.html (fails if any figure
                      anchor is missing, so a broken figure cannot ship silently)
  build.py --check    figure-placement check only: renders docs in memory,
                      prints PLACED/MISSING per figure, writes nothing,
                      skips mermaid. Safe to run concurrently.
"""
import os, re, html, json, pathlib, shutil, subprocess, sys, tempfile
import markdown

CHECK_ONLY = '--check' in sys.argv

# ---------------------------------------------------------------------------
# Mermaid: pre-rendered at build time via mermaid-cli (mmdc) into static
# light/dark SVGs, so diagrams work everywhere (file://, localhost, artifact)
# with no runtime renderer, no visibility races. Set MMDC=/path/to/mmdc or
# have `mmdc` on PATH (npm i -g @mermaid-js/mermaid-cli). Without it, blocks
# fall back to <pre class="mermaid"> (renders only in the artifact viewer).
# ---------------------------------------------------------------------------

MMDC = None if CHECK_ONLY else (os.environ.get('MMDC') or shutil.which('mmdc'))

def _mmdc_render(source, theme):
    with tempfile.TemporaryDirectory() as td:
        src = pathlib.Path(td) / 'd.mmd'
        out = pathlib.Path(td) / 'd.svg'
        src.write_text(source)
        subprocess.run([MMDC, '-i', str(src), '-o', str(out), '-t', theme,
                        '-b', 'transparent', '--quiet'],
                       check=True, capture_output=True)
        return out.read_text()

_mmd_counter = [0]

def render_mermaid(source):
    """Return HTML for one mermaid block: static themed SVGs, or a runtime
    fallback <pre> when mmdc is unavailable."""
    if not MMDC:
        return f'<pre class="mermaid" data-src="{html.escape(source, quote=True)}">{html.escape(source)}</pre>'
    _mmd_counter[0] += 1
    n = _mmd_counter[0]
    parts = []
    for theme, cls in (('default', 'mmd-light'), ('dark', 'mmd-dark')):
        svg = _mmdc_render(source, theme)
        # mmdc emits id="my-svg" with id-scoped styles; make ids unique so
        # multiple embedded diagrams don't collide.
        m = re.search(r'<svg[^>]*\bid="([^"]+)"', svg)
        if m:
            svg = svg.replace(m.group(1), f'mmd{n}{cls[-5:]}')
        parts.append(f'<div class="{cls}">{svg}</div>')
    return '<div class="mmd">' + ''.join(parts) + '</div>'

SRC = pathlib.Path(__file__).resolve().parent.parent   # CANONICAL/share/story/
OUT = pathlib.Path(__file__).resolve().parent / 'studio-story.html'

DOCS = [
    ('00-start-here.md',              'Start here'),
    ('01-why-not-otel.md',            'The problem with collecting data'),
    ('02-what-is-running.md',         'What is actually running'),
    ('03-count-everything.md',        'Count everything'),
    ('04-keep-the-interesting-ones.md','Keep the interesting ones'),
    ('05-values.md',                  'Values: inputs, outputs, errors'),
    ('06-is-the-data-trustworthy.md', 'Is the data trustworthy?'),
    ('07-which-code-was-this.md',     'Which code was this?'),
    ('08-leaving-the-laptop.md',      'From laptop to cloud'),
    ('09-the-catalog.md',             'Table schemas'),
    ('10-how-do-i.md',                'How do I build …?'),
    ('11-the-agent-skill.md',         'The agent skill'),
]
DOC_IDS = [f[:2] for f, _ in DOCS]

def doc_hash(fname):
    return 'doc-' + fname[:2]

CHIP = {
    'built': '<span class="chip c-built" title="On this branch today; numbers are implementation defaults, not frozen contracts">built</span>',
    'v1':    '<span class="chip c-v1" title="Committed target with a delivery gate; designed, not built">v1</span>',
    'open':  '<span class="chip c-open" title="Decision not yet made">open</span>',
}

def preprocess(text):
    # strip HTML comments (provenance notes for source readers)
    text = re.sub(r'<!--.*?-->', '', text, flags=re.DOTALL)
    # status chips: **[built]** / [built]
    for k, v in CHIP.items():
        text = text.replace(f'**[{k}]**', v).replace(f'[{k}]', v)
    return text

# ---------------------------------------------------------------------------
# Syntax highlighting (build-time; no client JS).
# Token classes: tk-k keyword, tk-s string, tk-c comment, tk-t type/title,
# tk-n number, tk-m meta/attribute, tk-j jinja/interp, tk-o operator, tk-p param.
# BAML keyword/type/literal/builtin lists mirror the repo grammar:
# typescript2/pkg-grammar-hljs/src/baml.js (derived from the real lexer).
# ---------------------------------------------------------------------------

BAML_KEYWORDS = {
    'class','enum','interface','implements','implement','extends','requires',
    'function','client','generator','test','testset','retry_policy',
    'template_string','type_builder','type',
    'if','else','for','while','let','const','in','break','continue','return',
    'throw','match','catch','catch_all','throws','spawn','await','defer',
    'watch','instanceof','is','dynamic','as','with',
}
BAML_LITERALS = {'true','false','null'}
BAML_TYPES = {'int','float','bigint','string','bool','image','audio','map',
              'json','unknown','never','Self'}
BAML_BUILTINS = {'env','root','baml','self','_'}
BAML_DECL_KW = {'class','enum','interface','function','client','type',
                'template_string','retry_policy','generator','testset','test'}
BAML_IDENT = r'\$?[A-Za-z_][A-Za-z0-9_-]*(?:\$[A-Za-z_][A-Za-z0-9_-]*)*'

def esc(s):
    return html.escape(s, quote=False)

def span(cls, s):
    return f'<span class="{cls}">{esc(s)}</span>'

def hl_jinja_inside(body):
    """Highlight {{ }} / {% %} / {# #} markers inside a raw/quoted string body."""
    out, i = [], 0
    pat = re.compile(r'\{\{.*?\}\}|\{%.*?%\}|\{#.*?#\}', re.DOTALL)
    for m in pat.finditer(body):
        out.append(esc(body[i:m.start()]))
        out.append(span('tk-j', m.group(0)))
        i = m.end()
    out.append(esc(body[i:]))
    return ''.join(out)

BAML_TOKENS = [
    ('comment',  re.compile(r'//[^\n]*')),
    ('bcomment', re.compile(r'/\*.*?\*/', re.DOTALL)),
    ('rawstr',   re.compile(r'(#{1,3})".*?"\1', re.DOTALL)),
    ('bytestr',  re.compile(r'\bb"(?:\\.|[^"\\])*"')),
    ('tickstr',  re.compile(r'(`{1,3}).*?\1', re.DOTALL)),
    ('dqstr',    re.compile(r'"(?:\\.|[^"\\])*"')),
    ('attr',     re.compile(r'@@?' + r'\$?[A-Za-z_][A-Za-z0-9_-]*(?:\.[A-Za-z_][A-Za-z0-9_-]*)*')),
    ('number',   re.compile(r'\b\d+n\b|\b\d+(?:\.\d+)?[eE][+-]?\d+\b|\b\d+\.\d+\b|\b\d+\b')),
    ('arrow',    re.compile(r'->|=>')),
    ('word',     re.compile(BAML_IDENT)),
]

def hl_baml(code):
    out, i, n = [], 0, len(code)
    prev_word = None
    while i < n:
        for kind, pat in BAML_TOKENS:
            m = pat.match(code, i)
            if not m:
                continue
            s = m.group(0)
            if kind in ('comment', 'bcomment'):
                out.append(span('tk-c', s))
            elif kind in ('rawstr', 'dqstr'):
                q = 1 if kind == 'dqstr' else len(m.group(1)) + 1
                head, body, tail = s[:q], s[q:-q], s[-q:]
                out.append(f'<span class="tk-s">{esc(head)}{hl_jinja_inside(body)}{esc(tail)}</span>')
            elif kind in ('bytestr', 'tickstr'):
                out.append(span('tk-s', s))
            elif kind == 'attr':
                out.append(span('tk-m', s))
            elif kind == 'number':
                out.append(span('tk-n', s))
            elif kind == 'arrow':
                out.append(span('tk-o', s))
            else:  # word
                if prev_word in BAML_DECL_KW and s not in BAML_KEYWORDS:
                    out.append(span('tk-t', s))
                elif s in BAML_KEYWORDS:
                    out.append(span('tk-k', s))
                elif s in BAML_TYPES:
                    out.append(span('tk-t', s))
                elif s in BAML_LITERALS:
                    out.append(span('tk-n', s))
                elif s in BAML_BUILTINS:
                    out.append(span('tk-p', s))
                else:
                    out.append(esc(s))
                prev_word = s
            if kind != 'word':
                prev_word = None
            i = m.end()
            break
        else:
            if not code[i].isspace():
                prev_word = prev_word if code[i] in '<>' else None
            out.append(esc(code[i]))
            i += 1
    return ''.join(out)

SQL_KEYWORDS = {
    'select','from','where','group','by','order','limit','having','join','on',
    'and','or','not','in','as','distinct','case','when','then','else','end',
    'is','null','desc','asc','between','like','union','all','with','exists',
    'inner','left','right','outer','cross','offset',
}
SQL_FUNCTIONS = {'sum','count','avg','min','max','nullif','coalesce','round'}

SQL_TOKENS = [
    ('comment', re.compile(r'--[^\n]*')),
    ('string',  re.compile(r"'(?:''|[^'])*'")),
    ('qident',  re.compile(r'"(?:[^"])*"')),
    ('param',   re.compile(r':[A-Za-z_][A-Za-z0-9_]*')),
    ('number',  re.compile(r'\b\d+(?:\.\d+)?\b')),
    ('word',    re.compile(r'[A-Za-z_][A-Za-z0-9_]*')),
]

def hl_sql(code):
    out, i, n = [], 0, len(code)
    while i < n:
        for kind, pat in SQL_TOKENS:
            m = pat.match(code, i)
            if not m:
                continue
            s = m.group(0)
            if kind == 'comment':
                out.append(span('tk-c', s))
            elif kind == 'string':
                out.append(span('tk-s', s))
            elif kind == 'qident':
                out.append(span('tk-t', s))
            elif kind == 'param':
                out.append(span('tk-p', s))
            elif kind == 'number':
                out.append(span('tk-n', s))
            else:
                low = s.lower()
                if low in SQL_KEYWORDS:
                    out.append(span('tk-k', s))
                elif low in SQL_FUNCTIONS:
                    out.append(span('tk-t', s))
                else:
                    out.append(esc(s))
            i = m.end()
            break
        else:
            out.append(esc(code[i]))
            i += 1
    return ''.join(out)

HIGHLIGHTERS = {'baml': hl_baml, 'sql': hl_sql}

def extract_fences(text):
    """Pull mermaid + all fenced code out before md conversion; reinsert after."""
    stash = []
    def repl(m):
        lang = (m.group(1) or '').strip()
        body = m.group(2)
        idx = len(stash)
        if lang == 'mermaid':
            stash.append(render_mermaid(body))
        else:
            cls = f' data-lang="{lang}"' if lang else ''
            label = f'<span class="code-lang">{lang}</span>' if lang in ('baml','sql') else ''
            rendered = HIGHLIGHTERS[lang](body) if lang in HIGHLIGHTERS else html.escape(body)
            stash.append(f'<div class="codewrap">{label}<pre class="code"{cls}><code>{rendered}</code></pre></div>')
        return f'\nFENCEPLACEHOLDER{idx}ENDFENCE\n'
    text = re.sub(r'```([^\n]*)\n(.*?)```', repl, text, flags=re.DOTALL)
    return text, stash

def restore_fences(htm, stash):
    def repl(m):
        return stash[int(m.group(1))]
    htm = re.sub(r'<p>FENCEPLACEHOLDER(\d+)ENDFENCE</p>', repl, htm)
    htm = re.sub(r'FENCEPLACEHOLDER(\d+)ENDFENCE', repl, htm)
    return htm

def fix_links(htm):
    # cross-doc links NN-foo.md -> #doc-NN
    htm = re.sub(r'href="(\d\d)-[a-z0-9\-]+\.md"', lambda m: f'href="#doc-{m.group(1)}"', htm)
    # repo-relative links become inert references
    def repo_ref(m):
        return f'<span class="repo-ref" title="in the baml repo">{m.group(2)}&nbsp;<code>{m.group(1)}</code></span>'
    htm = re.sub(r'<a href="((?:\.\./|\.\./\.\./)[^"]+)">([^<]+)</a>', repo_ref, htm)
    return htm

def wrap_tables(htm):
    return htm.replace('<table>', '<div class="tbl"><table>').replace('</table>', '</table></div>')

def mark_callouts(htm):
    # a blockquote opening with **Experimental.** renders as the ochre
    # experimental-callout card (styled in shell.html)
    return re.sub(r'<blockquote>(\s*<p><strong>Experimental\.?</strong>)',
                  r'<blockquote class="callout-exp">\1', htm)

def build_doc(fname, title):
    raw = (SRC / fname).read_text()
    raw = preprocess(raw)
    raw, stash = extract_fences(raw)
    md = markdown.Markdown(extensions=['tables', 'toc', 'sane_lists'], extension_configs={'toc': {'permalink': False}})
    htm = md.convert(raw)
    htm = restore_fences(htm, stash)
    htm = fix_links(htm)
    htm = wrap_tables(htm)
    htm = mark_callouts(htm)
    # section list for sidebar (h2 only)
    secs = [(t['id'], re.sub(r'<[^>]+>', '', t['name'])) for t in md.toc_tokens for t in [t] if t['level'] <= 2] if md.toc_tokens else []
    secs = []
    for tok in md.toc_tokens:
        if tok['level'] == 1:
            for ch in tok['children']:
                if ch['level'] == 2:
                    secs.append((ch['id'], re.sub(r'<[^>]+>', '', ch['name'])))
        elif tok['level'] == 2:
            secs.append((tok['id'], re.sub(r'<[^>]+>', '', tok['name'])))
    return htm, secs

TAPE_ANIMATION = '''
<figure class="tape-fig" id="tape-demo">
  <figcaption>The rolling tape, animated: events stream in on the right and the oldest fall off the left, until a trigger fires and a slice is sealed into a dump.</figcaption>
  <div class="tape-stage" aria-label="Animation of a bounded rolling event tape; a trigger seals a slice into a durable dump">
    <div class="tape-row">
      <div class="tape-cap">bounded memory</div>
      <div class="tape-track"><div class="tape-cells" id="tapeCells"></div>
        <div class="tape-clamp" id="tapeClamp" hidden><span>preserve</span></div>
      </div>
    </div>
    <div class="tape-dumprow">
      <div class="tape-cap">sealed dumps</div>
      <div class="tape-dumps" id="tapeDumps"></div>
    </div>
    <div class="tape-ctl">
      <button type="button" id="tapeBtn" class="btn">Replay</button>
      <span class="tape-note" id="tapeNote"></span>
    </div>
  </div>
</figure>
'''

SPINE_DIAGRAM = '''
<div class="spine" role="img" aria-label="Two layers of truth: the complete layer summarizes every call cheaply; the retained layer keeps exact evidence for the interesting few, selected by policy">
  <div class="spine-src">your program<br><span>any call volume</span></div>
  <div class="spine-arrows">
    <div class="spine-arrow a-complete"></div>
    <div class="spine-arrow a-retained"></div>
  </div>
  <div class="spine-layers">
    <div class="spine-layer l-complete">
      <h3>The complete layer</h3>
      <p>small summaries of <strong>every single call</strong>: cheap, bounded, never sampled</p>
      <p class="spine-q">answers “how much, how often, how slow”</p>
    </div>
    <div class="spine-layer l-retained">
      <h3>The retained layer</h3>
      <p>exact calls, event tape, captured values for <strong>the interesting few</strong>, selected by explicit policy</p>
      <p class="spine-q">answers “show me exactly what happened”</p>
    </div>
  </div>
</div>
'''

def inject_extras(doc_id, htm):
    if doc_id == '04':
        # place the animation right after the "The rolling tape" h2's first paragraph
        m = re.search(r'(<h2 id="the-rolling-tape">.*?</h2>\s*<p>.*?</p>)', htm, flags=re.DOTALL)
        if m:
            htm = htm[:m.end(1)] + TAPE_ANIMATION + htm[m.end(1):]
        else:
            print('WARN: tape anchor not found in 04')
    if doc_id == '00':
        # replace the ASCII spine (first codewrap containing COMPLETE LAYER) with the graphic
        m = re.search(r'<div class="codewrap">(?:(?!</div>).)*?THE COMPLETE LAYER(?:(?!</div>).)*?</pre></div>', htm, flags=re.DOTALL)
        if m:
            htm = htm[:m.start()] + SPINE_DIAGRAM + htm[m.end():]
        else:
            print('WARN: spine ascii not found in 00')
    return htm

# ---------------------------------------------------------------------------
# Build-time figures. Each figure is a pair in site/figures/:
#   <name>.html  the fragment (a <figure class="fig fig-...">, optionally with
#                a <style> scoped to its own class)
#   <name>.json  where it goes:
#     {"doc": "02",
#      "place": {"type": "swap_fence", "key": "unique substring"},
#      "also":  [{"type": "remove_table", "key": "..."}]}          # optional
# Anchor types:
#   swap_fence / swap_table      replace the matched block with the figure
#   remove_fence / remove_table  delete the matched block ("also" cleanups)
#   after_heading                insert after the h2/h3 with id == key
#   after_para                   insert after the first <p> following that heading
# "key" must match exactly one block in the doc's rendered HTML (use "index"
# to disambiguate deliberately). Shared component CSS lives in figures/*.css,
# spliced into the page at /*%%FIGCSS%%*/. The build FAILS if any figure
# cannot be placed, so a moved anchor is caught at build time, not in review.
# The markdown sources keep their ASCII/table fallbacks; like the doc-00
# spine, figures exist only in the rendered site.
# ---------------------------------------------------------------------------

FIGDIR = pathlib.Path(__file__).resolve().parent / 'figures'

def load_figures():
    figs = []
    for j in sorted(FIGDIR.glob('*.json')) if FIGDIR.exists() else []:
        spec = json.loads(j.read_text())
        spec['_name'] = j.stem
        h = j.with_suffix('.html')
        spec['_html'] = h.read_text() if h.exists() else None
        figs.append(spec)
    return figs

def figures_css():
    if not FIGDIR.exists():
        return ''
    return '\n'.join(p.read_text() for p in sorted(FIGDIR.glob('*.css')))

_BLOCK_PATS = {
    'fence': re.compile(r'<div class="codewrap">.*?</pre></div>', re.DOTALL),
    'table': re.compile(r'<div class="tbl"><table>.*?</table></div>', re.DOTALL),
}

class FigureError(Exception):
    pass

def _match_block(htm, kind, key, index):
    hits = [m for m in _BLOCK_PATS[kind].finditer(htm) if key in m.group(0)]
    if not hits:
        raise FigureError(f'no {kind} contains {key!r}')
    if index is None:
        if len(hits) > 1:
            raise FigureError(f'{len(hits)} {kind}s contain {key!r}; add "index"')
        return hits[0]
    if index >= len(hits):
        raise FigureError(f'index {index} out of range: {len(hits)} {kind}s contain {key!r}')
    return hits[index]

def _apply_anchor(htm, anchor, payload):
    typ, key = anchor['type'], anchor['key']
    idx = anchor.get('index')
    if typ in ('swap_fence', 'swap_table', 'remove_fence', 'remove_table'):
        kind = typ.split('_')[1]
        m = _match_block(htm, kind, key, idx)
        repl = payload if typ.startswith('swap') else ''
        return htm[:m.start()] + repl + htm[m.end():]
    if typ in ('after_heading', 'after_para', 'before_para'):
        h = re.search(r'<h[23] id="' + re.escape(key) + r'">.*?</h[23]>', htm, re.DOTALL)
        if not h:
            raise FigureError(f'no h2/h3 with id {key!r}')
        pos = h.end()
        if typ in ('after_para', 'before_para'):
            # "index" selects the Nth paragraph after the heading (0-based,
            # default 0); the search never crosses the next h2/h3.
            # after_para inserts after that paragraph, before_para before it
            # (i.e. after whatever block precedes it, such as a list).
            nxt = re.search(r'<h[23] ', htm[pos:])
            limit = pos + nxt.start() if nxt else len(htm)
            pat = re.compile(r'<p>.*?</p>', re.DOTALL)
            hits = [m for m in pat.finditer(htm, pos) if m.end() <= limit]
            n = idx or 0
            if n >= len(hits):
                raise FigureError(
                    f'only {len(hits)} <p> after heading {key!r}, need index {n}')
            pos = hits[n].end() if typ == 'after_para' else hits[n].start()
        return htm[:pos] + '\n' + payload + '\n' + htm[pos:]
    raise FigureError(f'unknown anchor type {typ!r}')

FIGURES = load_figures()
_fig_report = []   # (name, doc, 'PLACED' | error message)

def apply_figures(doc_id, htm):
    for fig in FIGURES:
        if fig.get('doc') != doc_id:
            continue
        name = fig['_name']
        if fig['_html'] is None:
            _fig_report.append((name, doc_id, 'MISSING: no .html next to .json'))
            continue
        try:
            htm = _apply_anchor(htm, fig['place'], fig['_html'])
            for extra in fig.get('also', []):
                htm = _apply_anchor(htm, extra, '')
            _fig_report.append((name, doc_id, 'PLACED'))
        except FigureError as e:
            _fig_report.append((name, doc_id, f'MISSING: {e}'))
    return htm

def figure_summary():
    bad = [(n, d, s) for n, d, s in _fig_report if s != 'PLACED']
    for n, d, s in _fig_report:
        print(f'  figure {n} (doc {d}): {s}')
    print(f'figures: {len(_fig_report) - len(bad)} placed, {len(bad)} missing')
    return bad

def collapse_terms(htm):
    """Wrap the 'Terms defined here' section in a <details>, collapsed by
    default. The h2 (with its id, so the sidebar link still lands) becomes
    the <summary>; the router opens the details when navigating to it."""
    m = re.search(r'<h2 id="terms-defined-here">.*?</h2>', htm, re.DOTALL)
    if not m:
        return htm
    nxt = re.search(r'<h2 ', htm[m.end():])
    end = m.end() + nxt.start() if nxt else len(htm)
    return (htm[:m.start()]
            + '<details class="terms"><summary>' + htm[m.start():m.end()]
            + '</summary>' + htm[m.end():end] + '</details>'
            + htm[end:])

docs_html = []
nav_items = []
for i, (fname, short) in enumerate(DOCS):
    body, secs = build_doc(fname, short)
    did = fname[:2]
    body = inject_extras(did, body)
    body = apply_figures(did, body)
    body = collapse_terms(body)
    prev_a = f'<a class="pn pn-prev" href="#doc-{DOCS[i-1][0][:2]}">&larr; {DOCS[i-1][1]}</a>' if i > 0 else '<span></span>'
    next_a = f'<a class="pn pn-next" href="#doc-{DOCS[i+1][0][:2]}">{DOCS[i+1][1]} &rarr;</a>' if i < len(DOCS)-1 else '<span></span>'
    docs_html.append(f'<section class="doc" id="doc-{did}" hidden>\n<article>{body}</article>\n<nav class="pagenav">{prev_a}{next_a}</nav>\n</section>')
    sec_html = ''.join(f'<li><a href="#doc-{did}" data-sec="{sid}">{html.escape(name)}</a></li>' for sid, name in secs)
    nav_items.append(
        f'<li class="nav-doc" data-doc="doc-{did}">'
        f'<a class="nav-link" href="#doc-{did}"><span class="nav-num">{did}</span><span>{html.escape(short)}</span></a>'
        f'<ul class="nav-secs">{sec_html}</ul></li>'
    )

missing = figure_summary()
if CHECK_ONLY:
    sys.exit(1 if missing else 0)
if missing:
    sys.exit('BUILD FAILED: figures missing anchors (see report above); nothing written')

page = pathlib.Path(__file__).with_name('shell.html').read_text()
fonts_css_path = pathlib.Path(__file__).with_name('fonts.css')
fonts_css = fonts_css_path.read_text() if fonts_css_path.exists() else '/* fonts.css missing; run fetch_fonts.py */'
page = page.replace('/*%%FONTS%%*/', fonts_css)
page = page.replace('/*%%FIGCSS%%*/', figures_css())
page = page.replace('%%NAV%%', '\n'.join(nav_items)).replace('%%DOCS%%', '\n'.join(docs_html))

OUT.write_text(page)
print('wrote', OUT, f'{OUT.stat().st_size/1024:.0f} KB')
