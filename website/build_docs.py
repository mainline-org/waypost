#!/usr/bin/env python3
"""Build static protocol pages from a fixed, public-document allowlist."""
from pathlib import Path
from html import escape
from urllib.parse import urlsplit, urlunsplit, quote
import re
import shutil
from markdown_it import MarkdownIt

ROOT = Path(__file__).resolve().parents[1]
SITE = ROOT / 'website'
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

def header():
    return '''<a class="skip-link" href="#main">跳到主要内容</a><header class="site-header"><div class="container header-inner"><a class="brand" href="/" aria-label="Waypost 首页"><img src="/assets/mark.svg" alt="" width="34" height="34"><span>waypost<span class="brand-dot">.</span></span></a><nav class="main-nav" aria-label="主要导航"><a href="/#how">开始使用</a><a href="/rfc/" aria-current="page">RFC</a><a href="/privacy/">数据与隐私</a></nav><a class="header-action" href="/guide/">开始使用 <span aria-hidden="true">↗</span></a></div></header>'''

def footer():
    return '''<footer class="site-footer"><div class="container footer-inner"><div><a class="brand" href="/"><img src="/assets/mark.svg" alt="" width="28" height="28"><span>waypost.</span></a><p>助手的进展，都送到你这里。</p></div><nav aria-label="页脚导航"><a href="/guide/">使用指南</a><a href="/rfc/">RFC</a><a href="/privacy/">数据与隐私</a><a href="https://github.com/mainline-org/waypost">GitHub ↗</a></nav><span class="footer-license">OPEN SOURCE · APACHE 2.0</span></div></footer>'''

def shell(title, slug, body):
    return f'''<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{escape(title)} — Waypost RFC</title><meta name="description" content="{escape(title)}。Waypost 协议与实现文档，保留原文状态，包含章节目录和可下载源码。"><meta name="theme-color" content="#f5f5f7"><link rel="canonical" href="https://waypost.hika.fyi/rfc/{slug}"><link rel="icon" href="/assets/mark.svg" type="image/svg+xml"><link rel="stylesheet" href="/assets/site.css"></head><body>{header()}{body}{footer()}</body></html>'''

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
    body = f'''<main id="main" class="container guide-layout rfc-layout"><aside class="guide-toc rfc-toc"><p>{escape(category)}</p><a class="back-link" href="/rfc/">← 所有 RFC 与实现文档</a><nav class="rfc-toc-nav" aria-label="文档章节目录">{links}</nav></aside><article class="guide-article rfc-article"><p class="eyebrow section-label">WAYPOST / {escape(category)}</p><h1>{escape(title)}</h1><div class="rfc-source-links"><a href="/rfc/source/{source_name}" download>下载 Markdown 原文 ↗</a><a href="/rfc/validation/">查看实现验证 ↗</a></div><p class="notice-box">正文由仓库文档生成，保留原始规范状态和示例。RFC 草案及验证计划与当前实现可能有差异；已验证范围见实现记录，加密与用户隔离升级见 <a href="/rfc/security/">安全说明</a>。</p><div class="rfc-body">{content}</div><a class="back-link rfc-bottom-link" href="/rfc/">← 返回 RFC 索引</a></article></main>'''
    out = SITE / 'rfc' / slug
    out.mkdir(parents=True, exist_ok=True)
    (out / 'index.html').write_text(shell(label, slug + '/', body))

cards = []
notes = {
    '0001': ('Draft / implementation baseline', '身份与授权、消息投递、独立收取、回执、资料封存、记忆版本与失败语义。'),
    '0002': ('Draft / implementation baseline', '管理 Dashboard、内容检查、登录与会话，以及管理行为的权限和信息边界。'),
    '0003': ('Implementation baseline', '按参与 Agent 选择策略，在发布前检查文字、资料和 metadata，并保留安全审计。'),
    'properties': ('配套验证计划', '空间隔离、幂等、发布恢复、独立消费者、CAS 与可控故障的验证契约。'),
    'validation': ('实现记录', '实际运行的测试、原生与 Cloudflare 验收，以及仍未验证的生产边界。'),
    'security': ('实现说明', '用户管理隔离、服务端加密、密钥配置、旧数据迁移与安全清理。'),
}
for _, slug, label, category in DOCS[:CORE_DOC_COUNT]:
    status, description = notes[slug]
    cards.append(f'<a class="rfc-entry" href="/rfc/{slug}/"><span class="mono">{escape(category)}</span><div><h2>{escape(label)}</h2><p>{escape(description)}</p><span class="rfc-status">{escape(status)}</span></div><span class="rfc-entry-arrow" aria-hidden="true">↗</span></a>')
index = '''<main id="main" class="container rfc-index"><p class="eyebrow section-label">PROTOCOL &amp; IMPLEMENTATION</p><h1>理解一段交接，<br>从它的约定开始。</h1><p class="rfc-index-lede">RFC 定义 Waypost 的公开行为：目的、契约、状态转换、失败语义与验证性质。<br>这里保留完整正文，也分别标注规范草案和实际实现记录。</p><div class="rfc-index-list">''' + ''.join(cards) + '''</div><div class="rfc-reading-note"><h2>规范怎样演进</h2><p>草案是实现基线，不等于全部能力已上线。改变可观察行为或权限含义时，应同时说明版本和迁移方式；实现状态通过性质测试和实际验收确认。</p><p>网站从仓库文档生成，保持原文内容和状态。图示保留 Mermaid 源码，可下载原始 Markdown；实现差异以验证与安全说明为准。</p></div></main>'''
(SITE / 'rfc/index.html').write_text(shell('RFC 与实现文档', '', index))
print({'rendered_documents': len(DOCS), 'raw_sources': len(DOCS), 'index': '/rfc/'})
