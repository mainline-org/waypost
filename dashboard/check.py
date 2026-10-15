#!/usr/bin/env python3
"""Validate dashboard i18n: locale parity, key references, inline copy,
and that no hardcoded Chinese remains in markup or app logic."""
from pathlib import Path
from html.parser import HTMLParser
from html import unescape
import json
import re
import sys

base = Path(__file__).resolve().parent
errors = []

en = json.loads((base / 'locales/en.json').read_text())
zh = json.loads((base / 'locales/zh.json').read_text())
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

param = re.compile(r'\{([a-z_]+)\}')
for key in en:
    if set(param.findall(en[key])) != set(param.findall(zh[key])):
        errors.append('placeholder mismatch for %s' % key)


class Scan(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=False)
        self.plain = []
        self.aria = []
        self.scripts = []
        self.switchers = 0
        self.title = None
        self._title = False
        self._cur = None

    def handle_starttag(self, tag, attrs):
        d = dict(attrs)
        if tag == 'script' and d.get('src'):
            self.scripts.append(d['src'])
        if tag == 'button' and d.get('data-lang'):
            self.switchers += 1
        if tag == 'title':
            self._title = True
            self.title = ''
        if 'data-i18n-aria' in d:
            self.aria.append(d['data-i18n-aria'])
        if 'data-i18n' in d and self._cur is None:
            self._cur = [d['data-i18n'], [], tag, 1, False]

    def handle_endtag(self, tag):
        if tag == 'title':
            self._title = False
        if self._cur is not None:
            if tag == self._cur[2] and self._cur[3] == 1:
                self.plain.append((self._cur[0], ''.join(self._cur[1]), self._cur[4]))
                self._cur = None
            else:
                self._cur[3] -= 1

    def handle_data(self, data):
        if self._title:
            self.title += data
        if self._cur is not None:
            self._cur[1].append(data)

    def handle_entityref(self, name):
        self.handle_data('&%s;' % name)


html = (base / 'index.html').read_text()
scan = Scan()
scan.feed(html)
for script in ('/dashboard/i18n.js', '/dashboard/app.js'):
    if script not in scan.scripts:
        errors.append('index.html missing %s' % script)
if scan.switchers < 4:
    errors.append('index.html should have login + sidebar switchers (4 data-lang buttons)')
if scan.title is None or unescape(scan.title).strip() != en['meta.title']:
    errors.append('<title> differs from en.json meta.title')
for key in scan.aria:
    if key not in en:
        errors.append('unknown aria key %s' % key)
for key, value, nested in scan.plain:
    if key not in en:
        errors.append('unknown i18n key %s' % key)
    elif nested:
        errors.append('data-i18n %s wraps nested markup' % key)
    elif unescape(value).strip() != en[key].strip():
        errors.append('inline text for %s differs from en.json' % key)

# Visible copy must all be keyed: strip tags/scripts, then look for CJK.
# Language names inside the switcher (中文) stay literal on purpose.
text = re.sub(r'<script.*?</script>', '', html, flags=re.S)
text = re.sub(r'<div class="lang-switch".*?</div>', '', text, flags=re.S)
text = re.sub(r'<[^>]+>', '', text)
if cjk.search(unescape(text)):
    errors.append('index.html has unkeyed Chinese copy: %r' % cjk.search(unescape(text)).group(0))

for js_name in ('app.js', 'memory.js'):
    js = (base / js_name).read_text()
    if cjk.search(js):
        errors.append('%s has hardcoded Chinese: %r…' % (
            js_name, js[cjk.search(js).start():cjk.search(js).start() + 12]))
    for key in sorted(set(re.findall(r"""\bt\(\s*['"]([^'"]+)['"]""", js))):
        if key not in en:
            errors.append('%s t() unknown key %s' % (js_name, key))
    for key, params in re.findall(r"""\bt\(\s*['"]([^'"]+)['"]\s*,\s*\{([^}]*)\}""", js):
        names = set(re.findall(r'(\w+)\s*:', params))
        for lang_name, table in (('en', en), ('zh', zh)):
            want = set(param.findall(table.get(key, '')))
            if key in table and names != want:
                errors.append('%s t(%s) params %s != %s %s' % (
                    js_name, key, sorted(names), lang_name, sorted(want)))

css = (base / 'style.css').read_text()
if '.lang-switch' not in css:
    errors.append('style.css missing .lang-switch')

print(json.dumps({'locale_keys': len(en), 'errors': errors}, ensure_ascii=False))
if errors:
    sys.exit(1)
