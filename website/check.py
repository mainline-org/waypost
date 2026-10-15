#!/usr/bin/env python3
"""Validate static page references, heading IDs, public document snapshots,
and i18n consistency (locale parity, key references, inline copy)."""
from pathlib import Path
from html.parser import HTMLParser
from html import unescape
from urllib.parse import urlparse, unquote
import json
import re
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


VOID = {'meta', 'link', 'img', 'br', 'hr', 'input', 'source', 'wbr'}


class I18NScan(HTMLParser):
    """Collect data-i18n (plain text), data-i18n-html (raw inner HTML),
    data-i18n-aria keys, the <html> data-page value, and script sources."""

    def __init__(self):
        super().__init__(convert_charrefs=False)
        self.plain = []
        self.rich = []
        self.aria = []
        self.scripts = []
        self.data_page = None
        self.has_switcher = False
        self._cur = None
        self._rich = None

    def handle_starttag(self, tag, attrs):
        d = dict(attrs)
        if tag == 'html':
            self.data_page = d.get('data-page')
        if tag == 'script' and d.get('src'):
            self.scripts.append(d['src'])
        if tag == 'button' and d.get('data-lang'):
            self.has_switcher = True
        raw = self.get_starttag_text()
        if self._rich is not None:
            self._rich[3].append(raw)
            if tag not in VOID:
                self._rich[2] += 1
        elif self._cur is not None:
            self._cur.append(('tag', tag))
        if 'data-i18n-aria' in d:
            self.aria.append(d['data-i18n-aria'])
        if self._rich is None and self._cur is None:
            if 'data-i18n-html' in d:
                self._rich = [d['data-i18n-html'], tag, 1, []]
            elif 'data-i18n' in d:
                self._cur = [d['data-i18n'], [], tag, 1, False]

    def handle_endtag(self, tag):
        if self._rich is not None:
            if tag == self._rich[1] and self._rich[2] == 1:
                self.rich.append((self._rich[0], ''.join(self._rich[3])))
                self._rich = None
            else:
                self._rich[2] -= 1
                self._rich[3].append('</%s>' % tag)
        elif self._cur is not None:
            if tag == self._cur[2] and self._cur[3] == 1:
                self.plain.append((self._cur[0], ''.join(self._cur[1]), self._cur[4]))
                self._cur = None
            else:
                self._cur[3] -= 1

    def handle_data(self, data):
        if self._rich is not None:
            self._rich[3].append(data)
        elif self._cur is not None:
            self._cur[1].append(data)

    def handle_entityref(self, name):
        self.handle_data('&%s;' % name)

    def handle_charref(self, name):
        self.handle_data('&#%s;' % name)


def norm_ws(s):
    return re.sub(r'\s+', ' ', s).strip()


en = json.loads((base / 'assets/locales/en.json').read_text())
zh = json.loads((base / 'assets/locales/zh.json').read_text())

errors = []
if set(en) != set(zh):
    errors.append('locale key mismatch: only-en=%s only-zh=%s' % (
        sorted(set(en) - set(zh)), sorted(set(zh) - set(en))))
cjk = re.compile(r'[\u4e00-\u9fff]')
for key, value in en.items():
    if cjk.search(value):
        errors.append('en.json %s contains CJK text' % key)
for key, value in list(en.items()) + list(zh.items()):
    if not value.strip():
        errors.append('locale %s is empty' % key)

pages = [p for p in base.rglob('*.html') if '.vercel' not in p.parts]
cache = {}
for path in pages:
    scan = Scan(); scan.feed(path.read_text()); cache[path.resolve()] = scan
    if scan.h1 != 1: errors.append(f'{path}: expected one h1')
    if len(scan.ids) != len(set(scan.ids)): errors.append(f'{path}: duplicate IDs')

    text = path.read_text()
    iso = I18NScan(); iso.feed(text)
    rel = path.relative_to(base).as_posix()

    if '/assets/i18n.js' not in iso.scripts:
        errors.append(f'{path}: missing /assets/i18n.js')
    if not iso.has_switcher:
        errors.append(f'{path}: missing language switcher')
    if not iso.data_page:
        errors.append(f'{path}: <html> missing data-page')
    elif iso.data_page != 'rfc-doc':
        for suffix in ('title', 'desc'):
            key = 'meta.%s.%s' % (iso.data_page, suffix)
            if key not in en:
                errors.append(f'{path}: missing locale key {key}')
        title_m = re.search(r'<title>(.*?)</title>', text, re.S)
        if title_m and unescape(title_m.group(1)).strip() != en.get('meta.%s.title' % iso.data_page, '').strip():
            errors.append(f'{path}: <title> differs from en.json meta.{iso.data_page}.title')

    for key in iso.aria:
        if key not in en: errors.append(f'{path}: unknown aria key {key}')
    for key, value, nested in iso.plain:
        if key not in en:
            errors.append(f'{path}: unknown i18n key {key}')
        elif nested:
            errors.append(f'{path}: data-i18n {key} wraps nested markup (use data-i18n-html)')
        elif unescape(value).strip() != en[key].strip():
            errors.append(f'{path}: inline text for {key} differs from en.json')
    for key, raw in iso.rich:
        if key not in en:
            errors.append(f'{path}: unknown i18n-html key {key}')
        elif norm_ws(unescape(raw)) != norm_ws(unescape(en[key])):
            errors.append(f'{path}: inline HTML for {key} differs from en.json')
    if iso._rich is not None or iso._cur is not None:
        errors.append(f'{path}: unbalanced data-i18n capture')

    needs_site_js = ('data-demo=' in text) or ('data-copy=' in text)
    if needs_site_js and '/assets/site.js' not in iso.scripts:
        errors.append(f'{path}: interactive copy needs /assets/site.js')

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

# Use-case bodies: English inline output must match docs/use-cases/<slug>.en.md,
# Chinese locale blobs must match docs/use-cases/<slug>.md.
try:
    from markdown_it import MarkdownIt
    mdr = MarkdownIt('js-default', {'html': False, 'linkify': False})
    for detail in (base / 'use-cases').glob('*/index.html'):
        slug = detail.parent.name
        if slug == 'use-cases':
            continue
        body_m = re.search(r'<article class="case-body" data-i18n-html="([^"]+)">(.*?)</article>', detail.read_text(), re.S)
        if not body_m:
            errors.append(f'{detail}: case-body without data-i18n-html')
            continue
        if body_m.group(1) != f'case.{slug}.bodyHtml':
            errors.append(f'{detail}: body key {body_m.group(1)} != case.{slug}.bodyHtml')
        for md_name, expected in (('.en.md', body_m.group(2)), ('.md', zh.get(f'case.{slug}.bodyHtml', ''))):
            src = root / 'docs/use-cases' / (slug + md_name)
            if not src.is_file():
                errors.append(f'{detail}: missing {src}')
                continue
            text = src.read_text()
            rendered = mdr.render(text.split('\n', 1)[1])
            if norm_ws(unescape(rendered)) != norm_ws(unescape(expected)):
                errors.append(f'{detail}: body differs from {src.name}')
            want_title = (en if md_name == '.en.md' else zh)[f'case.{slug}.title']
            if text.split('\n', 1)[0].lstrip('# ').strip() != want_title:
                errors.append(f'{detail}: {src.name} heading differs from case.{slug}.title')
except ImportError:
    errors.append('markdown-it-py not installed; skipping use-case body check')

# Demo scenarios in site.js must match the demo.* locale entries.
site_js = (base / 'assets/site.js').read_text()
for lang_name, table in (('en', en), ('zh', zh)):
    for key, value in table.items():
        if not key.startswith('demo.') or key.endswith(('.file', '.avatar')):
            continue
        if value not in site_js:
            errors.append(f'site.js missing {lang_name} {key}={value[:24]!r}…')

# demo.js embeds its dynamic panel copy; values must match the d2.* entries.
demo_js = (base / 'assets/demo.js').read_text().replace('\\\\n', '\n')
for lang_name, table in (('en', en), ('zh', zh)):
    for key, value in table.items():
        if not (key.startswith('d2.desc') or key.startswith('d2.doc') or key in ('d2.docSource', 'd2.primaryNoteTpl', 'd2.primaryNoteDefault', 'd2.optCustom')):
            continue
        if value not in demo_js:
            errors.append(f'demo.js missing {lang_name} {key}={value[:24]!r}…')

urls = []
for page in sorted(pages):
    route = str(page.parent.relative_to(base)).replace('.', '').strip('/')
    urls.append('https://waypost.hika.fyi/' + route + ('/' if route else ''))
(base / 'sitemap.xml').write_text('<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">' + ''.join('<url><loc>' + url + '</loc></url>' for url in urls) + '</urlset>\n')
print(json.dumps({'pages': len(pages), 'raw_documents': len(list((base / 'rfc/source').glob('*.md'))), 'locale_keys': len(en), 'errors': errors, 'readme_lines': len((root / 'README.md').read_text().splitlines())}, ensure_ascii=False))
if errors: sys.exit(1)
