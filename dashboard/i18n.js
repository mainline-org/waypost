'use strict';

/*
 * Dashboard i18n runtime. Loaded before app.js (defer order is preserved).
 *
 * Same language policy as the marketing site, sharing its localStorage key
 * (`waypost-lang`) so a choice made on either surface carries over:
 * `?lang=zh|en` override, saved choice, browser primary language
 * (zh* -> Chinese), otherwise English. Markup ships English; Chinese is
 * fetched from /dashboard/locales and applied.
 *
 * app.js waits for `WaypostI18n.ready` before the first render, reads
 * strings synchronously through `WaypostI18n.t(key, params)` (`{name}`
 * placeholders), and re-renders the current tab on `waypost:lang`.
 */
(function () {
  var SUPPORTED = ['en', 'zh'];
  var STORAGE_KEY = 'waypost-lang';
  var cache = {};
  var current = 'en';
  var readyResolve = null;
  var ready = new Promise(function (resolve) { readyResolve = resolve; });

  function normalize(tag) {
    tag = String(tag || '').toLowerCase().replace(/_/g, '-');
    if (tag === 'zh' || tag.indexOf('zh-') === 0) return 'zh';
    return 'en';
  }

  function detect() {
    try {
      var q = new URLSearchParams(window.location.search).get('lang');
      if (q && SUPPORTED.indexOf(normalize(q)) !== -1) return normalize(q);
    } catch (err) { /* URL parsing unavailable; continue */ }
    try {
      var saved = window.localStorage.getItem(STORAGE_KEY);
      if (saved && SUPPORTED.indexOf(saved) !== -1) return saved;
    } catch (err) { /* storage unavailable; continue */ }
    try {
      var langs = Array.isArray(navigator.languages) && navigator.languages.length
        ? navigator.languages
        : (navigator.language ? [navigator.language] : []);
      if (langs.length && normalize(langs[0]) === 'zh') return 'zh';
    } catch (err) { /* navigator unavailable; continue */ }
    return 'en';
  }

  function lookup(dict, key) {
    var value = dict ? dict[key] : undefined;
    return typeof value === 'string' ? value : null;
  }

  function t(key, params) {
    var value = lookup(cache[current], key);
    if (value === null) value = lookup(cache.en, key);
    if (value === null) return key;
    if (params) {
      for (var name in params) {
        if (Object.prototype.hasOwnProperty.call(params, name)) {
          value = value.split('{' + name + '}').join(String(params[name]));
        }
      }
    }
    return value;
  }

  function applyStatic() {
    var dict = cache[current] || {};
    document.documentElement.lang = current === 'zh' ? 'zh-CN' : 'en';
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
    var title = lookup(dict, 'meta.title');
    if (title) document.title = title;
    var buttons = document.querySelectorAll('[data-lang]');
    for (var b = 0; b < buttons.length; b++) {
      buttons[b].setAttribute('aria-pressed', String(buttons[b].getAttribute('data-lang') === current));
    }
  }

  function emit() {
    try {
      document.dispatchEvent(new CustomEvent('waypost:lang', { detail: { lang: current } }));
    } catch (err) { /* old engine without CustomEvent */ }
  }

  function fetchDict(lang) {
    if (cache[lang]) return Promise.resolve(cache[lang]);
    return fetch('/dashboard/locales/' + lang + '.json', { credentials: 'same-origin' })
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
      } catch (err) { /* storage unavailable; applies for this visit */ }
    }
    if (options && options.updateQuery !== false) {
      try {
        var url = new URL(window.location.href);
        url.searchParams.set('lang', lang);
        window.history.replaceState(null, '', url.toString());
      } catch (err) { /* history unavailable; ignore */ }
    }
    return fetchDict(lang).then(function () {
      applyStatic();
      emit();
      return lang;
    }).catch(function () {
      document.documentElement.lang = 'en';
      return 'en';
    });
  }

  window.WaypostI18n = {
    get: get,
    set: setLang,
    t: t,
    ready: ready,
  };

  function get() { return current; }

  document.addEventListener('click', function (event) {
    var target = event.target && event.target.closest ? event.target.closest('[data-lang]') : null;
    if (!target) return;
    var lang = target.getAttribute('data-lang');
    if (lang !== current) setLang(lang);
  });

  var initial = detect();
  // Always load the dictionary: dynamic strings render through t(), so even
  // the English default needs en.json. Detection stays transient; only
  // explicit switches persist.
  setLang(initial, { updateQuery: false, persist: false }).then(readyResolve, readyResolve);
})();
