#!/usr/bin/env python3
"""Render public, illustrative use cases with the site's existing navigation."""
from pathlib import Path
from html import escape
from markdown_it import MarkdownIt

ROOT = Path(__file__).resolve().parents[1]
SITE = ROOT / 'website'
CASES = [
    ('restaurant', '日常安排', '餐厅订好后，确认信息交给 Muse', 'Burnt Ends 11 月 6 日 19:00 两人位已订好，Waypost 保存消息，Muse 把忌口和改期条件放进出行安排。', 'Instinct → Waypost → Muse', '01'),
    ('flight-monitoring', '日常安排', '定期看机票，留下值得决定的选项', 'Grok Bot 按你设定的周期查价，把预算内的航班和落地时刻交给 Muse 继续整理。', 'Grok Bot → Waypost → Muse', '02'),
    ('main-assistant', '助手协作', '几个助手做事，主助手给你一份完整报告', 'Today、Instinct、Grok Bot 各自交付，由你主要对话的助手整理出已完成的事和等你决定的事。', '接入助手 → Waypost → 主助手 → 你', '03'),
    ('shared-materials', '资料交接', '原始资料交出去，当时的版本留下来', '行程笔记与参考文件按组保存，改过的价格也留着旧版本。Muse 获准读取，也能回到历史版本核对。', '资料来源 → Waypost 版本 → Muse', '04'),
    ('resume', '断线恢复', '中途断开，交接仍然接得上', '发信方找回去程 S$540、回程 S$598 的那次提交，Muse 从保存的进度继续收取与处理。', '恢复提交 → Waypost → 恢复收取', '05'),
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
    body = f'''<main id="main" class="container case-detail"><a class="back-link" href="/use-cases/">← 所有使用场景</a><p class="eyebrow">{escape(category)} · 配置示例</p><h1>{escape(title)}</h1><p class="case-lede">{escape(description)}</p><div class="case-flow">{escape(flow)}</div><article class="case-body">{content}</article><div class="case-next"><h2>先收到一封信</h2><p>用我们的 Cloudflare 托管服务，或自己运行 Waypost。接入与权限步骤见使用指南。</p><a class="button primary" href="/guide/">查看接入步骤</a></div></main>'''
    target = SITE / 'use-cases' / slug
    target.mkdir(parents=True, exist_ok=True)
    (target / 'index.html').write_text(shell(title, '/use-cases/' + slug + '/', description, body))

def card(case):
    slug, category, title, description, flow, number = case
    return f'''<a class="case-card" href="/use-cases/{slug}/"><div class="case-card-top"><span class="case-category">{escape(category)}</span><span class="case-number">{number}</span></div><h3>{escape(title)}</h3><p>{escape(description)}</p><div class="case-card-flow">{escape(flow)}</div><span class="case-read">看看怎样交接 <span aria-hidden="true">→</span></span></a>'''

body = '''<main id="main" class="container cases-index"><p class="eyebrow">WAYPOST / 使用场景</p><h1>各自做事，<br>结果接得上。</h1><p class="case-lede">餐厅预约、机票查价、行程笔记。<br>接入助手把结果交给 Waypost，由主助手接着处理。</p><p class="cases-note">下面以 Muse 担任主助手举例，也可以换成你现在使用的助手。订餐厅、查机票由助手的运行环境完成；Waypost 负责收信、留存和授权。</p><section aria-labelledby="featured-cases"><h2 id="featured-cases">常见的交接</h2><div class="case-grid">''' + ''.join(card(c) for c in CASES[:3]) + '''</div></section><section aria-labelledby="more-cases"><h2 id="more-cases">资料与恢复</h2><div class="case-grid case-grid-two">''' + ''.join(card(c) for c in CASES[3:]) + '''</div></section><div class="case-next"><h2>先收到一封信，再按需开放协作。</h2><p>选定主助手，让它收取接入助手的结果。</p><a class="button primary" href="/guide/#main-assistant">查看接入步骤</a></div></main>'''
target = SITE / 'use-cases'
target.mkdir(parents=True, exist_ok=True)
(target / 'index.html').write_text(shell('使用场景', '/use-cases/', '看看餐厅预约、机票查价和资料交接怎样经过 Waypost，由 Muse 汇总给你。', body))
print({'use_cases': len(CASES), 'index': '/use-cases/'})
