"""Audit authored docs pages, their links, and local images/stylesheets.

Usage: python3 scripts/audit-site.py --base-url http://localhost:3099 --output /tmp/audit.json
Use --scope all to include historical references from search-index.json.
Uses one worker by default so a public-site check does not create a load test.
"""

import argparse
import concurrent.futures
import gzip
import json
import time
import urllib.error
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET
from collections import Counter
from html.parser import HTMLParser


class Page(HTMLParser):
    def __init__(self):
        super().__init__()
        self.ids = []
        self.links = set()
        self.assets = set()
        self.h1 = 0

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if attrs.get('id'):
            self.ids.append(attrs['id'])
        if tag == 'h1':
            self.h1 += 1
        if tag == 'a' and attrs.get('href'):
            self.links.add(attrs['href'])
        if tag == 'img' and attrs.get('src'):
            self.assets.add(attrs['src'])
        if tag == 'link' and attrs.get('rel') == 'stylesheet':
            self.assets.add(attrs['href'])


def request(url):
    req = urllib.request.Request(url, headers={
        'User-Agent': 'BAML-documentation-audit', 'Accept-Encoding': 'gzip',
    })
    with urllib.request.urlopen(req, timeout=45) as response:
        raw = response.read()
        if response.headers.get('Content-Encoding') == 'gzip':
            raw = gzip.decompress(raw)
        return response.status, response.url, response.headers.get_content_type(), raw


def inspect(url):
    attempts = []
    for attempt in range(3):
        try:
            status, final, content_type, raw = request(url)
            page = Page()
            if content_type == 'text/html':
                page.feed(raw.decode('utf-8'))
            result = dict(url=url, status=status, final=final, content_type=content_type,
                          ids=page.ids, links=sorted(page.links), assets=sorted(page.assets), h1=page.h1)
            if attempts:
                result['retried_errors'] = attempts
            return result
        except (urllib.error.URLError, TimeoutError, OSError) as error:
            attempts.append(str(error))
            # Retrying missing pages cannot repair a routing bug.
            if isinstance(error, urllib.error.HTTPError) and error.code < 500:
                break
            if attempt < 2:
                time.sleep(attempt + 1)
    return dict(url=url, error=attempts[-1], retried_errors=attempts[:-1])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base-url', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--scope', choices=('authored', 'all'), default='authored')
    parser.add_argument('--workers', type=int, choices=range(1, 5), default=1)
    args = parser.parse_args()
    base = args.base_url.rstrip('/')
    origin = urllib.parse.urlparse(base).netloc
    sitemap = ET.fromstring(request(base + '/sitemap.xml')[3])
    paths = {urllib.parse.urlparse(node.text).path or '/' for node in sitemap.iter()
             if node.tag.endswith('}loc')}
    if args.scope == 'all':
        search = json.loads(request(base + '/search-index.json')[3])
        paths.update(urllib.parse.urlparse(entry['href']).path or '/' for entry in search['entries'])
    else:
        paths = {path for path in paths if not path.startswith(('/baml/packages', '/cli/v'))}
    pages = {}
    print(f'Auditing {len(paths)} pages', flush=True)
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
        for i, result in enumerate(pool.map(inspect, (base + path for path in sorted(paths)))):
            pages[urllib.parse.unquote(urllib.parse.urlparse(result['url']).path) or '/'] = result
            if (i + 1) % 100 == 0:
                print(f'Fetched {i + 1}/{len(paths)}', flush=True)

        def local_target(page, href):
            target = urllib.parse.urlparse(urllib.parse.urljoin(page['url'], href))
            if target.netloc != origin or target.scheme not in ('http', 'https'):
                return None
            return urllib.parse.unquote(target.path) or '/', urllib.parse.unquote(target.fragment)

        extras = set()
        for page in pages.values():
            for href in page.get('assets', []) + page.get('links', []):
                target = local_target(page, href)
                if target and target[0] not in pages:
                    extras.add(target[0])
        assets = {}
        for result in pool.map(inspect, (base + urllib.parse.quote(path, safe='/$') for path in sorted(extras))):
            assets[urllib.parse.unquote(urllib.parse.urlparse(result['url']).path)] = result

    all_targets = {**pages, **assets}
    failures = []
    for path, page in all_targets.items():
        issues = []
        if 'error' in page:
            issues.append(page['error'])
        else:
            if path in pages and page['h1'] != 1:
                issues.append(f'Expected one h1, found {page["h1"]}')
            duplicates = [key for key, count in Counter(page['ids']).items() if count > 1]
            if duplicates:
                issues.append(f'Duplicate IDs: {duplicates}')
            # Linked reference pages are destinations, not another recursive audit.
            for href in (page['links'] + page['assets'] if path in pages else []):
                target = local_target(page, href)
                if not target:
                    continue
                destination = all_targets.get(target[0])
                if destination is None or 'error' in destination:
                    issues.append(f'Broken local link: {href}')
                elif target[1] and destination['content_type'] == 'text/html' and target[1] not in destination['ids']:
                    issues.append(f'Missing fragment: {href}')
        if page.get('retried_errors'):
            issues.append(f'Transient failures: {page["retried_errors"]}')
        page['issues'] = sorted(set(issues))
        if issues:
            failures.append(dict(path=path, issues=page['issues']))
    report = dict(base_url=base, scope=args.scope, pages=len(pages), extra_targets=len(assets),
                  failures=failures, results=list(all_targets.values()))
    with open(args.output, 'w') as output:
        json.dump(report, output, indent=2)
    print(f'{len(pages)} pages, {len(assets)} extra targets, {len(failures)} failures. Report: {args.output}')
    return 1 if failures else 0


if __name__ == '__main__':
    raise SystemExit(main())
