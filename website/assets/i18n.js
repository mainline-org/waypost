'use strict';

/*
 * Waypost i18n runtime.
 *
 * Single URL per page, dictionary-based. Page markup ships English so that
 * no-JavaScript, unknown locales, and failed loads all fall back to English.
 * Chinese is applied when the visitor prefers it:
 *
 *   1. explicit `?lang=zh|en` query override (also shareable),
 *   2. saved choice in localStorage (`waypost-lang`),
 *   3. browser environment (`navigator.languages`, zh* -> Chinese),
 *   4. otherwise English.
 *
 * Translatable nodes use data-i18n (plain text), data-i18n-html (trusted
 * local markup), or data-i18n-aria (aria-label). Document title, meta
 * description and og:locale come from `meta.<page>.*` keys, where <page>
 * is the `data-page` value on <html>. Language buttons carry `data-lang`.
 * Interactive widgets (demo tabs, copy buttons) listen for `waypost:lang`.
 */
(function () {
  var SUPPORTED = ['en', 'zh'];
  var STORAGE_KEY = 'waypost-lang';
  var cache = {};
  var current = 'en';

  function normalize(tag) {
    tag = String(tag || '').toLowerCase().replace(/_/g, '-');
    if (tag === 'zh' || tag.indexOf('zh-') === 0) return 'zh';
    return 'en';
  }

  function detect() {
    try {
      var q = new URLSearchParams(window.location.search).get('lang');
      if (q) {
        var forced = normalize(q);
        if (SUPPORTED.indexOf(forced) !== -1) return forced;
      }
    } catch (err) { /* URL parsing unavailable; continue */ }
    try {
      var saved = window.localStorage.getItem(STORAGE_KEY);
      if (saved && SUPPORTED.indexOf(saved) !== -1) return saved;
    } catch (err) { /* storage unavailable; continue */ }
    var langs = [];
    try {
      if (Array.isArray(navigator.languages) && navigator.languages.length) langs = navigator.languages;
      else if (navigator.language) langs = [navigator.language];
    } catch (err) { /* navigator unavailable; continue */ }
    // Only the primary preference decides; a secondary zh must not
    // override an explicit primary language.
    if (langs.length && normalize(langs[0]) === 'zh') return 'zh';
    try {
      if (normalize(navigator.language) === 'zh' && !langs.length) return 'zh';
    } catch (err) { /* ignore */ }
    return 'en';
  }

  function lookup(dict, key) {
    var value = dict[key];
    return typeof value === 'string' ? value : null;
  }

  function applyMeta(dict) {
    var page = document.documentElement.getAttribute('data-page');
    if (!page) return;
    var title = lookup(dict, 'meta.' + page + '.title');
    var desc = lookup(dict, 'meta.' + page + '.desc');
    if (title) document.title = title;
    if (desc) {
      var metaDesc = document.querySelector('meta[name="description"]');
      if (metaDesc) metaDesc.setAttribute('content', desc);
      var ogDesc = document.querySelector('meta[property="og:description"]');
      if (ogDesc) ogDesc.setAttribute('content', desc);
      var ogTitle = document.querySelector('meta[property="og:title"]');
      if (ogTitle && title) ogTitle.setAttribute('content', title);
    }
    var ogLocale = document.querySelector('meta[property="og:locale"]');
    if (ogLocale) ogLocale.setAttribute('content', current === 'zh' ? 'zh_CN' : 'en_US');
  }

  function applyStrings(dict) {
    var nodes = document.querySelectorAll('[data-i18n]');
    for (var i = 0; i < nodes.length; i++) {
      var text = lookup(dict, nodes[i].getAttribute('data-i18n'));
      if (text !== null) nodes[i].textContent = text;
    }
    var rich = document.querySelectorAll('[data-i18n-html]');
    for (var j = 0; j < rich.length; j++) {
      var html = lookup(dict, rich[j].getAttribute('data-i18n-html'));
      if (html !== null) rich[j].innerHTML = html;
    }
    var labels = document.querySelectorAll('[data-i18n-aria]');
    for (var k = 0; k < labels.length; k++) {
      var label = lookup(dict, labels[k].getAttribute('data-i18n-aria'));
      if (label !== null) labels[k].setAttribute('aria-label', label);
    }
  }

  function syncSwitcher() {
    var buttons = document.querySelectorAll('[data-lang]');
    for (var i = 0; i < buttons.length; i++) {
      buttons[i].setAttribute('aria-pressed', String(buttons[i].getAttribute('data-lang') === current));
    }
  }

  function apply(dict) {
    document.documentElement.lang = current === 'zh' ? 'zh-CN' : 'en';
    applyStrings(dict);
    applyMeta(dict);
    syncSwitcher();
    try {
      document.dispatchEvent(new CustomEvent('waypost:lang', { detail: { lang: current } }));
    } catch (err) { /* old engine without CustomEvent; widgets read documentElement.lang */ }
  }

  function fetchDict(lang) {
    if (cache[lang]) return Promise.resolve(cache[lang]);
    return fetch('/assets/locales/' + lang + '.json', { credentials: 'same-origin' })
      .then(function (res) {
        if (!res.ok) throw new Error('locale ' + lang + ' -> ' + res.status);
        return res.json();
      })
      .then(function (dict) {
        cache[lang] = dict;
        return dict;
      });
  }

  function setLang(lang, options) {
    if (SUPPORTED.indexOf(lang) === -1) lang = 'en';
    current = lang;
    var persist = !options || options.persist !== false;
    if (persist) {
      try {
        window.localStorage.setItem(STORAGE_KEY, lang);
      } catch (err) { /* storage unavailable; language still applies for this visit */ }
    }
    if (options && options.updateQuery !== false) {
      try {
        var url = new URL(window.location.href);
        url.searchParams.set('lang', lang);
        window.history.replaceState(null, '', url.toString());
      } catch (err) { /* history unavailable; ignore */ }
    }
    if (lang === 'en' && cache.en) {
      apply(cache.en);
      return Promise.resolve(lang);
    }
    return fetchDict(lang).then(function (dict) {
      if (lang === 'en') cache.en = dict;
      apply(dict);
      return lang;
    }).catch(function () {
      // Stay on the English markup already in the DOM; still expose the choice.
      document.documentElement.lang = 'en';
      syncSwitcher();
      return 'en';
    });
  }

  function bindSwitcher() {
    document.addEventListener('click', function (event) {
      var target = event.target && event.target.closest ? event.target.closest('[data-lang]') : null;
      if (!target) return;
      var lang = target.getAttribute('data-lang');
      if (lang !== current) setLang(lang);
    });
  }

  // Expose a minimal API for widgets (demo tabs, copy buttons).
  window.WaypostI18n = {
    get: function () { return current; },
    set: setLang,
    lookup: function (key) {
      var active = cache[current];
      if (active && typeof active[key] === 'string') return active[key];
      return null;
    }
  };

  bindSwitcher();
  var initial = detect();
  current = initial;
  if (initial === 'en') {
    // Markup already ships English: only sync chrome, no fetch needed.
    document.documentElement.lang = 'en';
    syncSwitcher();
  } else {
    // Environment detection stays transient so a later browser-language
    // change still takes effect; only explicit switches persist.
    setLang(initial, { updateQuery: false, persist: false });
  }
})();
