'use strict';

// Public, local examples only. These controls never call the hosted service.
//
// Copy lives in D2 below and must stay in sync with the matching d2.* keys
// in assets/locales/*.json — website/check.py enforces it. Static panel
// markup ships English with data-i18n hooks; dynamic parts render from D2
// for the current language and re-render on `waypost:lang`.
(() => {
  const controls = [...document.querySelectorAll('[data-preview]')];
  if (!controls.length) return;

  const D2 = {
    en: {
      optCustom: 'Your everyday assistant',
      optLaixin: 'Laixin (planned)',
      laixinName: 'Laixin',
      laixinPlan: 'This page only demonstrates the role; Laixin integration is still planned.',
      primaryAvatar: 'P',
      primaryNoteDefault: 'Choosing a role never opens collect permissions on its own. Connecting for real still needs separately confirmed collect, receipt and send permissions; existing connections stay untouched.',
      primaryNoteTpl: 'Showing new-connection results going to “{primary}”. The real service still needs separately issued collect and receipt permissions; existing connections and delivered content stay untouched.',
      primaryNoteTplLaixin: 'Showing new-connection results going to “{primary}”. Laixin integration is still planned. The real service still needs separately issued collect and receipt permissions; existing connections and delivered content stay untouched.',
      descPrimaryTitle: '{primary} · Primary',
      descPrimaryBody: 'Collects connected assistants\' results, saves them reliably before confirming collect, then organizes one report for you. Needs explicit collect, receipt and send permissions; choosing the role itself grants nothing.',
      descInstinctTitle: 'Instinct · Connected',
      descInstinctBody: 'Hands the restaurant confirmation to {primary}. This connection can only send, never read other assistants\' inboxes.',
      descGrokTitle: 'Grok Bot · Connected',
      descGrokBody: 'Checks prices on a schedule in its own runtime, handing options worth deciding to {primary}. This sample separately authorized library upload; nothing ticketed.',
      descTodayTitle: 'Today · Connected',
      descTodayBody: 'Organizes trip notes for {primary} to keep using. This sample separately authorized upload and library updates; the primary only reads authorized libraries.',
      docSource: 'Source: {actor} · sources and versions kept; reading needs explicit permission',
      prefsActor: 'You',
      filePrefsSmall: 'Your confirmed picks · budget & dietary',
      docPrefsV2: 'This trip\'s per-person flight budget: S$600.\nOne guest avoids peanuts — note dietary needs before booking restaurants.\nReal ticketing and booking changes both wait for your confirmation.',
      docPrefsV1: 'This trip\'s per-person flight budget: S$600.\nDietary needs unconfirmed — ask again before booking restaurants.\nReal ticketing and booking changes both wait for your confirmation.',
      docFlightV2: 'Outbound: Nov 6, departs 09:40, lands 14:35, S$540.\nReturn: Nov 13, S$598. Budget S$600.\nNothing ticketed yet; the return date needs your call.',
      docFlightV1: 'Outbound: Nov 6, departs 09:40, lands 14:35, S$560.\nReturn: Nov 13, S$625, over budget.\nThe previous check; nothing ticketed.',
      docNotesV2: 'Landing Fri 14:35, hotel first.\n19:00, Burnt Ends, table for two confirmed.\nOne guest avoids peanuts; the restaurant allows moving to Sat 20:00.',
      docNotesV1: 'Arriving Friday afternoon.\nDinner booking being confirmed; holding 19:00 for now.\nStill to add: dietary notes and rebooking terms.',
      docQuestionsV2: 'Please confirm the return on Nov 13.\nCurrent lowest S$598, inside the S$600 budget.\nOnce confirmed, let an assistant handle ticketing.',
      docQuestionsV1: 'Still to confirm: restaurant and return date.\nReturn at S$625 is over budget; keep watching prices.'
    },
    zh: {
      optCustom: '你常用的助手',
      optLaixin: '来信（计划接入）',
      laixinName: '来信',
      laixinPlan: '本页仅演示角色，来信产品接入仍在规划。',
      primaryAvatar: '主',
      primaryNoteDefault: '选择角色不会自动开放收信权限。实际接入时，需要另外确认收信、回执和发信权限；已有连接不会被改写。',
      primaryNoteTpl: '正在演示把新连接的结果交给「{primary}」。真实服务仍需单独签发收信与回执权限，已有连接和已送达内容不会改写。',
      primaryNoteTplLaixin: '正在演示把新连接的结果交给「{primary}」。来信接入仍在规划。真实服务仍需单独签发收信与回执权限，已有连接和已送达内容不会改写。',
      descPrimaryTitle: '{primary} · 主助手',
      descPrimaryBody: '收取接入助手的结果，可靠保存后确认收取，再整理一份报告发给你。需要明确的收信、回执和发信权限；角色选择本身不授予权限。',
      descInstinctTitle: 'Instinct · 接入助手',
      descInstinctBody: '把餐厅预约确认交给 {primary}。这份连接只允许发信，不能读取其他助手的信箱。',
      descGrokTitle: 'Grok Bot · 接入助手',
      descGrokBody: '在自己的运行环境里定期查价，把值得决定的选项交给 {primary}。本例另行授权了资料上传，尚未购票。',
      descTodayTitle: 'Today · 接入助手',
      descTodayBody: '整理行程笔记，交给 {primary} 接着使用。本例单独授权上传和资料更新；主助手只读获准的资料。',
      docSource: '来源：{actor} · 保留来源与版本，读取需要明确授权',
      prefsActor: '你确认',
      filePrefsSmall: '你确认的偏好 · 预算与忌口',
      docPrefsV2: '这次旅行每人的机票预算：S$600。\n一位不吃花生，预订餐厅前说明忌口。\n实际购票和修改预约，都先交给你确认。',
      docPrefsV1: '这次旅行每人的机票预算：S$600。\n忌口尚未确认，订餐厅前需要再问。\n实际购票和修改预约，都先交给你确认。',
      docFlightV2: '去程：11 月 6 日，09:40 起飞，14:35 落地，S$540。\n回程：11 月 13 日，S$598。预算 S$600。\n还没有购票，回程日期需要你确认。',
      docFlightV1: '去程：11 月 6 日，09:40 起飞，14:35 落地，S$560。\n回程：11 月 13 日，S$625，超过预算。\n这是上一次查价记录，尚未购票。',
      docNotesV2: '周五 14:35 落地，先去酒店。\n19:00，Burnt Ends，两人位已确认。\n一位不吃花生；餐厅允许改到周六 20:00。',
      docNotesV1: '周五下午抵达。\n晚餐预约正在确认，先留出 19:00。\n待补充：忌口与改期条件。',
      docQuestionsV2: '请确认回程是否选 11 月 13 日。\n当前最低 S$598，在 S$600 预算内。\n确认以后，再让助手处理购票。',
      docQuestionsV1: '待确认餐厅与回程日期。\n回程 S$625，超过预算；继续观察价格。'
    }
  };

  function lang() {
    const tag = document.documentElement.lang || 'en';
    return tag.toLowerCase().indexOf('zh') === 0 ? 'zh' : 'en';
  }
  function strings() { return D2[lang()] || D2.en; }
  function fill(template, params) {
    let out = template;
    for (const name in params) out = out.split('{' + name + '}').join(params[name]);
    return out;
  }

  let primary = 'usual', file = 'flight', revision = '2', agent = '';
  const sources = {
    preferences: { name: 'travel-preferences.md', actorKey: 'prefsActor', v2: 'docPrefsV2', v1: 'docPrefsV1' },
    flight: { name: 'flight-options.md', actor: 'Grok Bot', v2: 'docFlightV2', v1: 'docFlightV1' },
    notes: { name: 'trip-notes.md', actor: 'Today', v2: 'docNotesV2', v1: 'docNotesV1' },
    questions: { name: 'open-questions.md', actor: 'Today', v2: 'docQuestionsV2', v1: 'docQuestionsV1' }
  };
  function primaryName() {
    if (primary === 'laixin') return strings().laixinName;
    if (primary === 'usual') return strings().optCustom;
    return 'Muse';
  }

  function select(view) {
    for (const control of controls) {
      const active = control.dataset.preview === view;
      control.setAttribute('aria-selected', String(active));
      control.tabIndex = active ? 0 : -1;
      document.getElementById(control.getAttribute('aria-controls')).hidden = !active;
    }
  }
  for (const control of controls) {
    control.addEventListener('click', () => select(control.dataset.preview));
    control.addEventListener('keydown', event => {
      if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End'].includes(event.key)) return;
      event.preventDefault();
      const direction = ['ArrowRight', 'ArrowDown'].includes(event.key) ? 1 : -1;
      const index = event.key === 'Home' ? 0 : event.key === 'End' ? controls.length - 1 : (controls.indexOf(control) + direction + controls.length) % controls.length;
      select(controls[index].dataset.preview); controls[index].focus();
    });
  }
  function describeAgent(name) {
    agent = name;
    const copy = {
      primary: ['descPrimaryTitle', 'descPrimaryBody'],
      Instinct: ['descInstinctTitle', 'descInstinctBody'],
      'Grok Bot': ['descGrokTitle', 'descGrokBody'],
      Today: ['descTodayTitle', 'descTodayBody']
    }[name];
    if (!copy) return;
    const target = document.getElementById('demo-agent-detail');
    const title = document.createElement('h3'), body = document.createElement('p');
    title.textContent = fill(strings()[copy[0]], { primary: primaryName() });
    body.textContent = fill(strings()[copy[1]], { primary: primaryName() });
    if (primary === 'laixin') body.textContent += ' ' + strings().laixinPlan;
    target.replaceChildren(title, body);
    for (const button of document.querySelectorAll('[data-demo-agent]')) button.setAttribute('aria-pressed', String(button.dataset.demoAgent === name));
  }
  for (const button of document.querySelectorAll('[data-demo-agent]')) button.addEventListener('click', () => describeAgent(button.dataset.demoAgent));
  function showFile() {
    const source = sources[file];
    const actor = source.actorKey ? strings()[source.actorKey] : source.actor;
    document.getElementById('demo-document-name').textContent = source.name;
    document.getElementById('demo-document-source').textContent = fill(strings().docSource, { actor: actor });
    document.getElementById('demo-document-body').textContent = strings()[source[revision]];
    for (const button of document.querySelectorAll('[data-demo-file]')) button.setAttribute('aria-pressed', String(button.dataset.demoFile === file));
    for (const button of document.querySelectorAll('[data-demo-revision]')) button.setAttribute('aria-pressed', String(button.dataset.demoRevision === revision));
  }
  for (const button of document.querySelectorAll('[data-demo-file]')) button.addEventListener('click', () => { file = button.dataset.demoFile; showFile(); });
  for (const link of document.querySelectorAll('[data-show-file]')) link.addEventListener('click', () => {
    select('library'); file = link.dataset.showFile; revision = '2'; showFile();
  });
  for (const button of document.querySelectorAll('[data-demo-revision]')) button.addEventListener('click', () => { revision = button.dataset.demoRevision; showFile(); });
  function renderPrimaryNote() {
    const node = document.getElementById('demo-primary-note');
    if (!node) return;
    if (primary === 'laixin') {
      node.textContent = fill(strings().primaryNoteTplLaixin, { primary: primaryName() });
    } else if (primary === 'usual') {
      node.textContent = fill(strings().primaryNoteTpl, { primary: primaryName() });
    } else {
      node.textContent = strings().primaryNoteDefault;
    }
  }
  function renderPrimaryAvatar() {
    const node = document.getElementById('demo-primary-avatar');
    if (node) node.textContent = strings().primaryAvatar;
  }
  function renderRecipient() {
    if (typeof showScenario !== 'function') return;
    const selected = document.querySelector('[data-demo].selected')?.dataset.demo || 'message';
    showScenario(selected);
    for (const name of ['sender', 'recipient', 'body', 'status', 'receipt', 'caption']) {
      const node = document.getElementById(`demo-${name}`);
      node.textContent = node.textContent.split('Muse').join(primaryName());
    }
    document.getElementById('demo-announcement').textContent = document.getElementById('demo-subject').textContent + ' ' + document.getElementById('demo-caption').textContent;
  }
  document.getElementById('demo-primary').addEventListener('change', event => {
    primary = event.target.value;
    document.getElementById('demo-primary-name').textContent = primaryName();
    renderPrimaryAvatar();
    renderPrimaryNote();
    renderRecipient(); if (agent) describeAgent(agent); showFile();
  });
  for (const button of document.querySelectorAll('[data-demo]')) {
    button.addEventListener('click', renderRecipient);
    button.addEventListener('keydown', event => { if (['ArrowLeft', 'ArrowRight'].includes(event.key)) renderRecipient(); });
  }
  document.getElementById('demo-file').addEventListener('click', () => { select('library'); file = 'flight'; revision = '2'; showFile(); });

  document.addEventListener('waypost:lang', () => {
    renderPrimaryNote();
    renderPrimaryAvatar();
    document.getElementById('demo-primary-name').textContent = primaryName();
    const selectNode = document.getElementById('demo-primary');
    if (selectNode) {
      for (const option of selectNode.options) {
        if (option.value === 'usual') option.textContent = strings().optCustom;
        if (option.value === 'laixin') option.textContent = strings().optLaixin;
      }
    }
    if (agent) describeAgent(agent);
    showFile();
    renderRecipient();
  });
  renderRecipient();
})();
