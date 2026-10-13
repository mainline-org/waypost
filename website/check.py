#!/usr/bin/env python3
"""Validate static page references, heading IDs, and public document snapshots."""
from pathlib import Path
from html.parser import HTMLParser
from urllib.parse import urlparse, unquote
import json
import sys

base = Path(__file__).resolve().parent
root = base.parent
class Scan(HTMLParser):
    def __init__(self):
        super().__init__(); self.refs = []; self.ids = []; self.h1 = 0
    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if 'id' in attrs: self.ids.append(attrs['id'])
        if tag == 'h1': self.h1 += 1
        for key in ('href', 'src'):
            if key in attrs: self.refs.append(attrs[key])
pages = [p for p in base.rglob('*.html') if '.vercel' not in p.parts]
cache = {}; errors = []
for path in pages:
    scan = Scan(); scan.feed(path.read_text()); cache[path.resolve()] = scan
    if scan.h1 != 1: errors.append(f'{path}: expected one h1')
    if len(scan.ids) != len(set(scan.ids)): errors.append(f'{path}: duplicate IDs')
for path, scan in cache.items():
    for ref in scan.refs:
        url = urlparse(ref)
        if url.scheme or url.netloc: continue
        target = (base / url.path.lstrip('/') if url.path.startswith('/') else path.parent / url.path) if url.path else path
        if target.is_dir(): target /= 'index.html'
        if not target.exists(): errors.append(f'{path}: missing {ref}')
        elif url.fragment and target.suffix == '.html':
            if unquote(url.fragment) not in cache[target.resolve()].ids:
                errors.append(f'{path}: missing anchor {ref}')
json.loads((base / 'vercel.json').read_text())
for source in (base / 'rfc/source').glob('*.md'):
    original = (root / 'docs/rfcs' / source.name.removeprefix('rfcs-')) if source.name.startswith('rfcs-') else next((p for p in [root / source.name, *list((root / 'docs').rglob(source.name))] if p.is_file()), None)
    if original is None or source.read_bytes() != original.read_bytes(): errors.append(f'{source}: differs from source')
urls = []
for page in sorted(pages):
    route = str(page.parent.relative_to(base)).replace('.', '').strip('/')
    urls.append('https://waypost.hika.fyi/' + route + ('/' if route else ''))
(base / 'sitemap.xml').write_text('<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">' + ''.join('<url><loc>' + url + '</loc></url>' for url in urls) + '</urlset>\n')
print(json.dumps({'pages': len(pages), 'raw_documents': len(list((base / 'rfc/source').glob('*.md'))), 'errors': errors, 'readme_lines': len((root / 'README.md').read_text().splitlines())}, ensure_ascii=False))
if errors: sys.exit(1)
