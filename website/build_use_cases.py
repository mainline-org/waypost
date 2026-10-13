#!/usr/bin/env python3
"""Render public, illustrative use cases with the site's existing navigation."""
from pathlib import Path
from html import escape
from markdown_it import MarkdownIt

ROOT = Path(__file__).resolve().parents[1]
SITE = ROOT / 'website'
CASES = [
    ('restaurant', '日常安排', '餐厅订好后，确认信息交给 Muse', 'Instinct 确认周五晚的两人位，Waypost 保存消息，Muse 把预约放进出行安排。', 'Instinct → Waypost → Muse', '01'),
    ('flight-monitoring', '日常安排', '定期看机票，留下值得决定的选项', 'Grok Bot 按你设定的周期查价，把预算内的航班交给 Muse 继续整理。', 'Grok Bot → Waypost → Muse', '02'),
    ('main-assistant', '助手协作', '几个助手做事，Muse 给你一份完整报告', 'Today、Instinct、Grok Bot 各自交付，由 Muse 整理完成项与待决定事项。', '其他助手 → Waypost → Muse → 你', '03'),
    ('shared-materials', '资料交接', '原始资料交出去，当时的版本留下来', '行程笔记与参考文件按组保存。Muse 获准读取，也能回到历史版本核对。', '资料来源 → Waypost 版本 → Muse', '04'),
    ('resume', '断线恢复', '中途断开，交接仍然接得上', '发信方找回原来的提交，Muse 从保存的进度继续收取与处理。', '恢复提交 → Waypost → 恢复收取', '05'),
]
page = (SITE / 'index.html').read_text()
header = page[page.index('<a class="skip-link"'):page.index('<main id="main"')]
header = header.replace('href="#how"', 'href="/guide/"')
header = header.replace('href="/use-cases/">使用场景', 'href="/use-cases/" aria-current="page">使用场景')
footer = page[page.index('<footer class="site-footer"'):page.index('</body>')]

def shell(title, route, description, body):
    return f'''<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{escape(title)} — Waypost</title><meta name="description" content="{escape(description, quote=True)}"><meta name="theme-color" content="#f5f5f7"><link rel="canonical" href="https://waypost.hika.fyi{route}"><link rel="icon" href="/assets/mark.svg" type="image/svg+xml"><link rel="stylesheet" href="/assets/site.css"></head><body>{header}{body}{footer}</body></html>'''

md = MarkdownIt('js-default', {'html': False, 'linkify': False})
for slug, category, title, description, flow, number in CASES:
    source = ROOT / 'docs/use-cases' / (slug + '.md')
    text = source.read_text().split('\n', 1)[1]
    content = md.render(text)
    body = f'''<main id="main" class="container case-detail"><a class="back-link" href="/use-cases/">← 所有使用场景</a><p class="eyebrow">{escape(category)} · 配置示例</p><h1>{escape(title)}</h1><p class="case-lede">{escape(description)}</p><div class="case-flow">{escape(flow)}</div><article class="case-body">{content}</article><div class="case-next"><h2>从一封信开始配置</h2><p>使用我们提供的 Cloudflare 托管服务，或自己运行 Waypost。接入与权限步骤见使用指南。</p><a class="button primary" href="/guide/">查看接入步骤</a></div></main>'''
    target = SITE / 'use-cases' / slug
    target.mkdir(parents=True, exist_ok=True)
    (target / 'index.html').write_text(shell(title, '/use-cases/' + slug + '/', description, body))

def card(case):
    slug, category, title, description, flow, number = case
    return f'''<a class="case-card" href="/use-cases/{slug}/"><div class="case-card-top"><span class="case-category">{escape(category)}</span><span class="case-number">{number}</span></div><h3>{escape(title)}</h3><p>{escape(description)}</p><div class="case-card-flow">{escape(flow)}</div><span class="case-read">看看怎样交接 <span aria-hidden="true">→</span></span></a>'''

body = '''<main id="main" class="container cases-index"><p class="eyebrow">WAYPOST / 使用场景</p><h1>各自做事，<br>结果接得上。</h1><p class="case-lede">餐厅预约、机票查价、行程笔记。<br>其他助手把结果交给 Waypost，由 Muse 接着处理。</p><p class="cases-note">下面是配置示例，介绍怎样用 Waypost 交接结果。外部任务由助手完成，接入需要对应运行环境支持 HTTP MCP 或 HTTP API。</p><section aria-labelledby="featured-cases"><h2 id="featured-cases">先看这三个场景</h2><div class="case-grid">''' + ''.join(card(c) for c in CASES[:3]) + '''</div></section><section aria-labelledby="more-cases"><h2 id="more-cases">资料与恢复，也一起保留</h2><div class="case-grid case-grid-two">''' + ''.join(card(c) for c in CASES[3:]) + '''</div></section><div class="case-next"><h2>选一位主助手，接入其他助手。</h2><p>先让 Muse 收到一条消息，再按需开放资料和协作。</p><a class="button primary" href="/guide/#main-assistant">开始配置</a></div></main>'''
target = SITE / 'use-cases'
target.mkdir(parents=True, exist_ok=True)
(target / 'index.html').write_text(shell('使用场景', '/use-cases/', '看看餐厅预约、机票查价和资料交接怎样经过 Waypost，由 Muse 汇总给你。', body))
print({'use_cases': len(CASES), 'index': '/use-cases/'})
