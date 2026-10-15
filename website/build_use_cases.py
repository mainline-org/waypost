#!/usr/bin/env python3
"""Render public, illustrative use cases with the site's existing navigation.

Bilingual output: English ships inline in the markup (the no-JS / fallback
language); Chinese lives in website/assets/locales/zh.json and is applied at
runtime by assets/i18n.js. Display strings come from the locale files so they
stay the single source of truth; article bodies render from docs/use-cases/:
`<slug>.md` (Chinese, stored as case.<slug>.bodyHtml in zh.json) and
`<slug>.en.md` (English, inlined).
"""
from pathlib import Path
from html import escape
import json
from markdown_it import MarkdownIt

ROOT = Path(__file__).resolve().parents[1]
SITE = ROOT / 'website'
LOCALES = SITE / 'assets' / 'locales'
CASES = [
    ('restaurant', '01', 1),
    ('flight-monitoring', '02', 2),
    ('main-assistant', '03', 3),
    ('shared-materials', '04', 4),
    ('resume', '05', 5),
]

en = json.loads((LOCALES / 'en.json').read_text())
zh = json.loads((LOCALES / 'zh.json').read_text())

page = (SITE / 'index.html').read_text()
header = page[page.index('<a class="skip-link"'):page.index('<main id="main"')]
header = header.replace('href="#how"', 'href="/guide/"')
header = header.replace('href="#hosting"', 'href="/#hosting"')
header = header.replace(
    '<a href="/use-cases/" data-i18n="common.navCases">',
    '<a href="/use-cases/" aria-current="page" data-i18n="common.navCases">',
)
footer = page[page.index('<footer class="site-footer"'):page.index('</body>')]


def shell(title_key, route, desc_key, body, data_page):
    title = en[title_key]
    desc = en[desc_key]
    return (
        '<!doctype html><html lang="en" data-page="' + data_page + '"><head>'
        '<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">'
        '<title>' + escape(title) + '</title>'
        '<meta name="description" content="' + escape(desc, quote=True) + '">'
        '<meta name="theme-color" content="#f5f5f7">'
        '<link rel="canonical" href="https://waypost.hika.fyi' + route + '">'
        '<link rel="alternate" hreflang="en" href="https://waypost.hika.fyi' + route + '">'
        '<link rel="alternate" hreflang="zh" href="https://waypost.hika.fyi' + route + '?lang=zh">'
        '<link rel="alternate" hreflang="x-default" href="https://waypost.hika.fyi' + route + '">'
        '<link rel="icon" href="/assets/mark.svg" type="image/svg+xml">'
        '<link rel="stylesheet" href="/assets/site.css">'
        '<script src="/assets/i18n.js" defer></script>'
        '</head><body>' + header + body + footer + '</body></html>'
    )


md = MarkdownIt('js-default', {'html': False, 'linkify': False})
for slug, number, card_no in CASES:
    for key in (f'case.{slug}.title', f'case.{slug}.lede', f'case.{slug}.flow',
                f'case.{slug}.eyebrowHtml', f'case.{slug}.bodyHtml'):
        assert key in en, key
        assert key in zh, key
    source = ROOT / 'docs/use-cases' / (slug + '.md')
    source_en = ROOT / 'docs/use-cases' / (slug + '.en.md')
    zh_body = md.render(source.read_text().split('\n', 1)[1])
    assert zh_body.strip(), slug
    content = md.render(source_en.read_text().split('\n', 1)[1])
    body = (
        '<main id="main" class="container case-detail">'
        f'<a class="back-link" href="/use-cases/" data-i18n="common.backCases">{escape(en["common.backCases"])}</a>'
        f'<p class="eyebrow" data-i18n-html="case.{slug}.eyebrowHtml">{en[f"case.{slug}.eyebrowHtml"]}</p>'
        f'<h1 data-i18n="case.{slug}.title">{escape(en[f"case.{slug}.title"])}</h1>'
        f'<p class="case-lede" data-i18n="case.{slug}.lede">{escape(en[f"case.{slug}.lede"])}</p>'
        f'<div class="case-flow" data-i18n="case.{slug}.flow">{escape(en[f"case.{slug}.flow"])}</div>'
        f'<article class="case-body" data-i18n-html="case.{slug}.bodyHtml">{content}</article>'
        '<div class="case-next">'
        f'<h2 data-i18n="common.caseNextTitle">{escape(en["common.caseNextTitle"])}</h2>'
        f'<p data-i18n="common.caseNextBody">{escape(en["common.caseNextBody"])}</p>'
        f'<a class="button primary" href="/guide/" data-i18n="common.caseNextCta">{escape(en["common.caseNextCta"])}</a>'
        '</div></main>'
    )
    target = SITE / 'use-cases' / slug
    target.mkdir(parents=True, exist_ok=True)
    (target / 'index.html').write_text(
        shell(f'meta.case-{slug}.title', f'/use-cases/{slug}/', f'meta.case-{slug}.desc', body, f'case-{slug}')
    )
    # Bodies are build artifacts of the Markdown sources; keep the locale
    # blobs in sync so zh<->en switching restores the exact same markup.
    en[f'case.{slug}.bodyHtml'] = content
    zh[f'case.{slug}.bodyHtml'] = zh_body


def save_locale(name, data):
    (LOCALES / f'{name}.json').write_text(json.dumps(data, ensure_ascii=False, indent=2) + '\n')


save_locale('en', en)
save_locale('zh', zh)


def card(case):
    slug, number, card_no = case
    return (
        f'<a class="case-card" href="/use-cases/{slug}/">'
        '<div class="case-card-top">'
        f'<span class="case-category" data-i18n="case.card{card_no}.cat">{escape(en[f"case.card{card_no}.cat"])}</span>'
        f'<span class="case-number">{number}</span></div>'
        f'<h3 data-i18n="case.card{card_no}.title">{escape(en[f"case.card{card_no}.title"])}</h3>'
        f'<p data-i18n="case.card{card_no}.desc">{escape(en[f"case.card{card_no}.desc"])}</p>'
        f'<div class="case-card-flow" data-i18n="case.card{card_no}.flow">{escape(en[f"case.card{card_no}.flow"])}</div>'
        f'<span class="case-read" data-i18n-html="common.caseReadHtml">{en["common.caseReadHtml"]}</span></a>'
    )


body = (
    '<main id="main" class="container cases-index">'
    f'<p class="eyebrow" data-i18n="cases.eyebrow">{escape(en["cases.eyebrow"])}</p>'
    f'<h1 data-i18n-html="cases.titleHtml">{en["cases.titleHtml"]}</h1>'
    f'<p class="case-lede" data-i18n-html="cases.ledeHtml">{en["cases.ledeHtml"]}</p>'
    f'<p class="cases-note" data-i18n="cases.note">{escape(en["cases.note"])}</p>'
    '<section aria-labelledby="featured-cases">'
    f'<h2 id="featured-cases" data-i18n="cases.featH">{escape(en["cases.featH"])}</h2>'
    '<div class="case-grid">' + ''.join(card(c) for c in CASES[:3]) + '</div></section>'
    '<section aria-labelledby="more-cases">'
    f'<h2 id="more-cases" data-i18n="cases.moreH">{escape(en["cases.moreH"])}</h2>'
    '<div class="case-grid case-grid-two">' + ''.join(card(c) for c in CASES[3:]) + '</div></section>'
    '<div class="case-next">'
    f'<h2 data-i18n="cases.nextTitle">{escape(en["cases.nextTitle"])}</h2>'
    f'<p data-i18n="cases.nextBody">{escape(en["cases.nextBody"])}</p>'
    f'<a class="button primary" href="/guide/#main-assistant" data-i18n="cases.nextCta">{escape(en["cases.nextCta"])}</a>'
    '</div></main>'
)
target = SITE / 'use-cases'
target.mkdir(parents=True, exist_ok=True)
(target / 'index.html').write_text(shell('meta.cases.title', '/use-cases/', 'meta.cases.desc', body, 'cases'))
print({'use_cases': len(CASES), 'index': '/use-cases/'})
