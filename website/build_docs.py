#!/usr/bin/env python3
"""Build static protocol pages from a fixed, public-document allowlist."""
from pathlib import Path
from html import escape
from urllib.parse import urlsplit, urlunsplit, quote
import re
import json
import shutil
from markdown_it import MarkdownIt

ROOT = Path(__file__).resolve().parents[1]
SITE = ROOT / 'website'
LOCALES = SITE / 'assets' / 'locales'
en = json.loads((LOCALES / 'en.json').read_text())
zh = json.loads((LOCALES / 'zh.json').read_text())
DOCS = [
    ('docs/rfcs/0001-async-mailbox-and-memory-sync.md', '0001', '异步信箱与记忆交换', 'RFC-0001'),
    ('docs/rfcs/0002-operator-dashboard.md', '0002', '管理 Dashboard 与内容检查', 'RFC-0002'),
    ('docs/rfcs/0003-agent-privacy-policies.md', '0003', 'Agent 消息隐私策略', 'RFC-0003'),
    ('docs/rfcs/0001-properties.md', 'properties', '性质与验证计划', 'RFC-0001 配套'),
    ('docs/validation.md', 'validation', '实现验证记录', 'IMPLEMENTATION'),
    ('docs/security.md', 'security', '安全与数据管理', 'IMPLEMENTATION'),
]
CORE_DOC_COUNT = len(DOCS)
DOCS.extend([
    ('CONTEXT.md', 'glossary', '领域术语', 'REFERENCE'),
    ('docs/design/0001-rust-data-model.md', 'data-model', 'Rust 数据模型', 'REFERENCE'),
    ('docs/design/0002-property-test-contracts.md', 'test-contracts', '测试契约', 'REFERENCE'),
    ('docs/research/0001-protocol-precedents.md', 'protocol-research', '协议研究依据', 'REFERENCE'),
    ('docs/production.md', 'deployment', 'Cloudflare 生产运行', 'OPERATIONS'),
    ('docs/deployment.md', 'hosting', '运行与部署', 'OPERATIONS'),
    ('docs/demo.md', 'demo', '完整运行示例', 'REFERENCE'),
])
ROUTES = {(ROOT / path).resolve(): '/rfc/' + slug + '/' for path, slug, _, _ in DOCS}
ROUTES[(ROOT / 'docs/rfcs/README.md').resolve()] = '/rfc/'

SWITCHER = '<div class="header-side"><div class="lang-switch" role="group" data-i18n-aria="common.langLabel" aria-label="Language"><button type="button" data-lang="zh" aria-pressed="false">中文</button><button type="button" data-lang="en" aria-pressed="true">EN</button></div><a class="header-action" href="/guide/" data-i18n-html="common.headerStartHtml">Get started <span aria-hidden="true">↗</span></a></div>'

def header():
    return '<a class="skip-link" href="#main" data-i18n="common.skip">Skip to main content</a><header class="site-header"><div class="container header-inner"><a class="brand" href="/" data-i18n-aria="common.brandAria" aria-label="Waypost home"><img src="/assets/mark.svg" alt="" width="34" height="34"><span>waypost<span class="brand-dot">.</span></span></a><nav class="main-nav" data-i18n-aria="common.navLabel" aria-label="Primary"><a href="/use-cases/" data-i18n="common.navCases">Use cases</a><a href="/rfc/" aria-current="page" data-i18n="common.navRfc">RFC</a><a href="/privacy/" data-i18n="common.navPrivacy">Data &amp; privacy</a></nav>' + SWITCHER + '</div></header>'

def footer():
    return '<footer class="site-footer"><div class="container footer-inner"><div><a class="brand" href="/"><img src="/assets/mark.svg" alt="" width="28" height="28"><span>waypost.</span></a><p data-i18n="common.footTag">Every assistant\'s progress, delivered to you.</p></div><nav data-i18n-aria="common.footLabel" aria-label="Footer"><a href="/guide/" data-i18n="common.footGuide">Guide</a><a href="/use-cases/" data-i18n="common.footCases">Use cases</a><span aria-current="page">RFC</span><a href="/privacy/" data-i18n="common.footPrivacy">Data &amp; privacy</a><a href="https://github.com/mainline-org/waypost">GitHub ↗</a></nav><span class="footer-license">OPEN SOURCE · APACHE 2.0</span></div></footer>'

def shell(title, slug, body, data_page, meta_desc_key=None):
    route = '/rfc/' + slug
    desc = en[meta_desc_key] if meta_desc_key else (title + '. Waypost protocol and implementation doc, kept in its original state, with a chapter index and downloadable source. Full text is currently Chinese-only.')
    return f'''<!doctype html><html lang="en" data-page="{data_page}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{escape(title)} — Waypost RFC</title><meta name="description" content="{escape(desc, quote=True)}"><meta name="theme-color" content="#f5f5f7"><link rel="canonical" href="https://waypost.hika.fyi{route}"><link rel="alternate" hreflang="en" href="https://waypost.hika.fyi{route}"><link rel="alternate" hreflang="zh" href="https://waypost.hika.fyi{route}?lang=zh"><link rel="alternate" hreflang="x-default" href="https://waypost.hika.fyi{route}"><link rel="icon" href="/assets/mark.svg" type="image/svg+xml"><link rel="stylesheet" href="/assets/site.css"><script src="/assets/i18n.js" defer></script></head><body>{header()}{body}{footer()}</body></html>'''

def text_of(token):
    return ''.join(t.content for t in (token.children or []) if t.type in ('text', 'code_inline', 'image')) or token.content

def render(path):
    md = MarkdownIt('js-default', {'html': False, 'linkify': False})
    original = path.read_text()
    tokens = md.parse(original)
    if tokens and tokens[0].type == 'heading_open' and tokens[0].tag == 'h1':
        title = text_of(tokens[1])
        tokens = tokens[3:]
    else:
        title = path.stem
    toc = []
    seen = {}
    for i, token in enumerate(tokens):
        if token.type == 'heading_open':
            text = text_of(tokens[i + 1])
            anchor = re.sub(r'[^\w\- ]', '', text.lower()).replace(' ', '-')
            number = seen.get(anchor, 0); seen[anchor] = number + 1
            if number: anchor += '-' + str(number)
            token.attrSet('id', anchor)
            if token.tag == 'h2': toc.append((anchor, text))
        if token.type == 'table_open':
            token.meta['rfc_table'] = True
        for child in token.children or []:
            if child.type != 'link_open': continue
            href = child.attrGet('href') or ''
            parts = urlsplit(href)
            if parts.scheme or parts.netloc or not parts.path: continue
            target = (path.parent / parts.path).resolve()
            if target in ROUTES:
                rewritten = ROUTES[target]
            elif target == (ROOT / 'README.md').resolve():
                rewritten = '/guide/'
            elif target.is_relative_to(ROOT) and (target.is_file() or target.is_dir()):
                relative = target.relative_to(ROOT).as_posix()
                rewritten = 'https://github.com/mainline-org/waypost/' + ('tree' if target.is_dir() else 'blob') + '/main/' + quote(relative)
            else:
                raise ValueError('Unresolved document reference: ' + href)
            child.attrSet('href', rewritten + (('#' + parts.fragment) if parts.fragment else ''))
    # Tables scroll independently on narrow screens; code and raw diagram source are preserved.
    md.add_render_rule('table_open', lambda self, toks, idx, options, env: '<div class="rfc-table"><table>')
    md.add_render_rule('table_close', lambda self, toks, idx, options, env: '</table></div>')
    fence_default = md.renderer.rules['fence']
    def fence(self, toks, idx, options, env):
        code = fence_default(toks, idx, options, env)
        if toks[idx].info.strip() == 'mermaid':
            return '<details class="diagram-source"><summary>图示源码 · Mermaid<span aria-hidden="true">＋</span></summary>' + code + '</details>'
        return code
    md.add_render_rule('fence', fence)
    return title, toc, md.renderer.render(tokens, md.options, {})

source_dir = SITE / 'rfc/source'
source_dir.mkdir(parents=True, exist_ok=True)
for source, slug, label, category in DOCS:
    path = ROOT / source
    title, toc, content = render(path)
    source_name = ('rfcs-' if source.startswith('docs/rfcs/') else '') + path.name
    shutil.copyfile(path, source_dir / source_name)
    links = ''.join(f'<a href="#{escape(anchor, quote=True)}">{escape(text)}</a>' for anchor, text in toc)
    body = f'''<main id="main" class="container guide-layout rfc-layout"><aside class="guide-toc rfc-toc"><p>{escape(category)}</p><a class="back-link" href="/rfc/" data-i18n="rfc.backAll">{escape(en["rfc.backAll"])}</a><nav class="rfc-toc-nav" data-i18n-aria="rfc.tocLabel" aria-label="Document sections">{links}</nav></aside><article class="guide-article rfc-article"><p class="eyebrow section-label">WAYPOST / {escape(category)}</p><h1>{escape(title)}</h1><div class="rfc-source-links"><a href="/rfc/source/{source_name}" download data-i18n="rfc.sourceDl">{escape(en["rfc.sourceDl"])}</a><a href="/rfc/validation/" data-i18n="rfc.seeValidation">{escape(en["rfc.seeValidation"])}</a></div><p class="notice-box" data-i18n-html="rfc.noticeHtml">{en["rfc.noticeHtml"]}</p><div class="rfc-body">{content}</div><a class="back-link rfc-bottom-link" href="/rfc/" data-i18n="rfc.bottomBack">{escape(en["rfc.bottomBack"])}</a></article></main>'''
    out = SITE / 'rfc' / slug
    out.mkdir(parents=True, exist_ok=True)
    (out / 'index.html').write_text(shell(label, slug + '/', body, 'rfc-doc'))

cards = []
notes = {
    '0001': ('Draft / implementation baseline', None, 'rfc.card0001Title', 'rfc.card0001Desc'),
    '0002': ('Draft / implementation baseline', None, 'rfc.card0002Title', 'rfc.card0002Desc'),
    '0003': ('Implementation baseline', None, 'rfc.card0003Title', 'rfc.card0003Desc'),
    'properties': ('Validation plan', 'rfc.statusProperties', 'rfc.cardPropertiesTitle', 'rfc.cardPropertiesDesc'),
    'validation': ('Implementation record', 'rfc.statusValidation', 'rfc.cardValidationTitle', 'rfc.cardValidationDesc'),
    'security': ('Implementation notes', 'rfc.statusSecurity', 'rfc.cardSecurityTitle', 'rfc.cardSecurityDesc'),
}
for _, slug, label, category in DOCS[:CORE_DOC_COUNT]:
    status_en, status_key, title_key, desc_key = notes[slug]
    for key in (title_key, desc_key):
        assert key in en and key in zh, key
    if status_key:
        assert status_key in en and status_key in zh, status_key
        status_html = f'<span class="rfc-status" data-i18n="{status_key}">{escape(en[status_key])}</span>'
    else:
        status_html = f'<span class="rfc-status">{escape(status_en)}</span>'
    cards.append(f'<a class="rfc-entry" href="/rfc/{slug}/"><span class="mono">{escape(category)}</span><div><h2 data-i18n="{title_key}">{escape(en[title_key])}</h2><p data-i18n="{desc_key}">{escape(en[desc_key])}</p>{status_html}</div><span class="rfc-entry-arrow" aria-hidden="true">↗</span></a>')
index = f'''<main id="main" class="container rfc-index"><p class="eyebrow section-label" data-i18n="rfc.indexEyebrow">{escape(en["rfc.indexEyebrow"])}</p><h1 data-i18n-html="rfc.indexTitleHtml">{en["rfc.indexTitleHtml"]}</h1><p class="rfc-index-lede" data-i18n-html="rfc.indexLedeHtml">{en["rfc.indexLedeHtml"]}</p><div class="rfc-index-list">''' + ''.join(cards) + f'''</div><div class="rfc-reading-note"><h2 data-i18n="rfc.noteTitle">{escape(en["rfc.noteTitle"])}</h2><div data-i18n-html="rfc.noteBodyHtml">{en["rfc.noteBodyHtml"]}</div></div></main>'''
(SITE / 'rfc/index.html').write_text(shell('RFCs & implementation docs', '', index, 'rfc-index', 'meta.rfc-index.desc'))
print({'rendered_documents': len(DOCS), 'raw_sources': len(DOCS), 'index': '/rfc/'})
