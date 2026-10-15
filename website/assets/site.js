'use strict';

/*
 * Homepage demo + copy buttons.
 *
 * Copy stays bilingual in this file (no fetch race): the initial paint comes
 * from the markup (English) or assets/i18n.js (Chinese via data-i18n), and
 * tab switches render from SCENARIOS below. SCENARIOS must stay in sync with
 * the demo.* keys in assets/locales/*.json — website/check.py enforces it.
 * Widgets follow language changes through the `waypost:lang` event.
 */
var SCENARIOS = {
  en: {
    message: {
      sender: 'Instinct', avatar: 'I', recipient: 'To Muse',
      subject: 'Burnt Ends, Nov 6, 7:00pm, table for two.',
      body: 'Booking confirmed: Burnt Ends, Nov 6, 7:00pm, table for two — can move to Sat 8:00pm. One guest avoids peanuts. Please fold this into the trip plan, Muse.',
      file: '', tag: '',
      status: "Delivered to Muse's inbox", receipt: 'Muse picks it up later', step: 'Message delivery sample',
      caption: 'Instinct sends the booking confirmation, Waypost keeps it, Muse organizes next.'
    },
    files: {
      sender: 'Grok Bot', avatar: 'G', recipient: 'To Muse',
      subject: 'S$540 out, S$598 back — both within budget.',
      body: 'Outbound Nov 6, departs 09:40, lands 14:35 local. Cheapest return is Nov 13 at S$598 against a S$600 budget. Nothing ticketed yet — Muse, please remind them to decide.',
      file: 'flight-options.md', tag: 'Price-check sample',
      status: 'Price results delivered', receipt: 'Muse reads with permission', step: 'Flight-watch sample',
      caption: 'Assistants do the checking; Waypost keeps the results; Muse lines up what needs deciding.'
    },
    summary: {
      sender: 'Muse', avatar: 'M', recipient: 'To you',
      subject: 'Restaurant booked; the return flight is yours to call.',
      body: "Burnt Ends, Fri 7:00pm for two, confirmed — movable to Saturday. Outbound 09:40 picked; three return candidates, cheapest Nov 13 at S$598 against a S$600 budget, $2 apart. Pick the return date first and I'll finish the full itinerary.",
      file: '', tag: '',
      status: 'Summary delivered to you', receipt: 'Read when you like', step: 'Summary report sample',
      caption: "When Muse collects and processes is up to its runtime."
    },
    copy: 'Copy this prompt ↗',
    copied: 'Copied ✓',
    copyFallback: 'Select the text to copy'
  },
  zh: {
    message: {
      sender: 'Instinct', avatar: 'I', recipient: '发给 Muse',
      subject: 'Burnt Ends，11 月 6 日 19:00，两人位。',
      body: '预约已确认：Burnt Ends，11 月 6 日 19:00，两人位，可以改到周六 20:00。备注一位不吃花生。请 Muse 放进这次出行安排。',
      file: '', tag: '',
      status: '已投递到 Muse 的信箱', receipt: 'Muse 稍后收取', step: '消息已投递',
      caption: 'Instinct 发送预约确认，Waypost 保存，Muse 接着整理。'
    },
    files: {
      sender: 'Grok Bot', avatar: 'G', recipient: '发给 Muse',
      subject: '去程 S$540，回程 S$598，都还在预算内。',
      body: '去程 11 月 6 日 09:40 起飞，当地 14:35 落地。回程最便宜的是 11 月 13 日，S$598，预算 S$600。还没有购票，请 Muse 提醒你决定。',
      file: 'flight-options.md', tag: '查价示例',
      status: '查价结果已投递', receipt: 'Muse 按授权读取', step: '机票监测示例',
      caption: '查价由助手执行，Waypost 留下结果；Muse 整理待决定的选项。'
    },
    summary: {
      sender: 'Muse', avatar: 'M', recipient: '发给你',
      subject: '餐厅订好了，回程等你定。',
      body: 'Burnt Ends 周五 19:00 两人位已确认，可改到周六。去程 09:40 起飞已经选定；回程有三个候选，最便宜的 11 月 13 日 S$598，预算 S$600，差 2 美元。请先定回程日期，我再整理完整行程。',
      file: '', tag: '',
      status: '汇总报告已投递给你', receipt: '有空再读', step: '汇总报告示例',
      caption: 'Muse 何时收信和处理，由它的运行环境安排。'
    },
    copy: '复制这段话 ↗',
    copied: '已复制 ✓',
    copyFallback: '请选中文字复制'
  }
};

function waypostLang() {
  var tag = document.documentElement.lang || 'en';
  return tag.toLowerCase().indexOf('zh') === 0 ? 'zh' : 'en';
}

var currentDemo = 'message';
var tabs = Array.prototype.slice.call(document.querySelectorAll('[data-demo]'));

function showScenario(key) {
  var state = (SCENARIOS[waypostLang()] || SCENARIOS.en)[key];
  if (!state) return;
  currentDemo = key;

  for (var i = 0; i < tabs.length; i++) {
    var selected = tabs[i].getAttribute('data-demo') === key;
    tabs[i].classList.toggle('selected', selected);
    tabs[i].setAttribute('aria-pressed', String(selected));
  }

  var fields = ['sender', 'avatar', 'recipient', 'subject', 'body', 'status', 'step', 'receipt', 'caption'];
  for (var f = 0; f < fields.length; f++) {
    var node = document.getElementById('demo-' + fields[f]);
    if (node) node.textContent = state[fields[f]];
  }

  var file = document.getElementById('demo-file');
  if (file) {
    file.hidden = !state.file;
    var name = file.querySelector('span');
    var tag = file.querySelector('.file-tag');
    if (name) name.textContent = state.file;
    if (tag) tag.textContent = state.tag;
  }

  var announcement = document.getElementById('demo-announcement');
  if (announcement) announcement.textContent = state.sender + '：' + state.subject + ' ' + state.caption;

  var letter = document.querySelector('.message-card');
  if (letter) {
    letter.classList.remove('changed');
    void letter.offsetWidth;
    letter.classList.add('changed');
  }
}

function resetCopyButtons() {
  var strings = SCENARIOS[waypostLang()] || SCENARIOS.en;
  var buttons = document.querySelectorAll('[data-copy]');
  for (var i = 0; i < buttons.length; i++) {
    buttons[i].textContent = strings.copy;
  }
}

for (var t = 0; t < tabs.length; t++) {
  (function (tab) {
    tab.addEventListener('click', function () { showScenario(tab.getAttribute('data-demo')); });
    tab.addEventListener('keydown', function (event) {
      if (['ArrowLeft', 'ArrowRight'].indexOf(event.key) === -1) return;
      event.preventDefault();
      var index = tabs.indexOf(tab);
      var step = event.key === 'ArrowRight' ? 1 : -1;
      var next = tabs[(index + step + tabs.length) % tabs.length];
      next.focus();
      showScenario(next.getAttribute('data-demo'));
    });
  })(tabs[t]);
}

// Delegated: guide copy buttons are re-created when i18n swaps section HTML.
document.addEventListener('click', function (event) {
  var button = event.target && event.target.closest ? event.target.closest('[data-copy]') : null;
  if (!button) return;
  var strings = SCENARIOS[waypostLang()] || SCENARIOS.en;
  var target = document.getElementById(button.getAttribute('data-copy'));
  if (!target) return;
  var done = function (text) {
    button.textContent = text;
    setTimeout(function () {
      if (document.body.contains(button)) button.textContent = (SCENARIOS[waypostLang()] || SCENARIOS.en).copy;
    }, 1800);
  };
  try {
    var text = target.textContent.trim();
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(text).then(function () {
        done(strings.copied);
      }, function () {
        done(strings.copyFallback);
      });
    } else {
      done(strings.copyFallback);
    }
  } catch (err) {
    done(strings.copyFallback);
  }
});

document.addEventListener('waypost:lang', function () {
  resetCopyButtons();
  if (tabs.length) showScenario(currentDemo);
});
