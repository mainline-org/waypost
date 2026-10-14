'use strict';

// Public, local examples only. These controls never call the hosted service.
(() => {
  const controls = [...document.querySelectorAll('[data-preview]')];
  if (!controls.length) return;
  let primary = 'Muse', file = 'flight', revision = '2';
  const sources = {
    preferences: {name: 'travel-preferences.md', actor: '你确认', versions: {
      '2': '这次旅行每人的机票预算：S$600。\n一位不吃花生，预订餐厅前说明忌口。\n实际购票和修改预约，都先交给你确认。',
      '1': '这次旅行每人的机票预算：S$600。\n忌口尚未确认，订餐厅前需要再问。\n实际购票和修改预约，都先交给你确认。'
    }},
    flight: {name: 'flight-options.md', actor: 'Grok Bot', versions: {
      '2': '去程：11 月 6 日，09:40 起飞，14:35 落地，S$540。\n回程：11 月 13 日，S$598。预算 S$600。\n还没有购票，回程日期需要你确认。',
      '1': '去程：11 月 6 日，09:40 起飞，14:35 落地，S$560。\n回程：11 月 13 日，S$625，超过预算。\n这是上一次查价记录，尚未购票。'
    }},
    notes: {name: 'trip-notes.md', actor: 'Today', versions: {
      '2': '周五 14:35 落地，先去酒店。\n19:00，Burnt Ends，两人位已确认。\n一位不吃花生；餐厅允许改到周六 20:00。',
      '1': '周五下午抵达。\n晚餐预约正在确认，先留出 19:00。\n待补充：忌口与改期条件。'
    }},
    questions: {name: 'open-questions.md', actor: 'Today', versions: {
      '2': '请确认回程是否选 11 月 13 日。\n当前最低 S$598，在 S$600 预算内。\n确认以后，再让助手处理购票。',
      '1': '待确认餐厅与回程日期。\n回程 S$625，超过预算；继续观察价格。'
    }}
  };
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
    const target = document.getElementById('demo-agent-detail');
    const title = document.createElement('h3'), body = document.createElement('p');
    const copy = {
      primary: [`${primary} · 主助手`, '收取接入助手的结果，可靠保存后确认收取，再整理一份报告发给你。需要明确的收信、回执和发信权限；角色选择本身不授予权限。'],
      Instinct: ['Instinct · 接入助手', `把餐厅预约确认交给 ${primary}。这份连接只允许发信，不能读取其他助手的信箱。`],
      'Grok Bot': ['Grok Bot · 接入助手', `在自己的运行环境里定期查价，把值得决定的选项交给 ${primary}。本例另行授权了资料上传，尚未购票。`],
      Today: ['Today · 接入助手', `整理行程笔记，交给 ${primary} 接着使用。本例单独授权上传和资料更新；主助手只读获准的资料。`]
    };
    title.textContent = copy[name][0]; body.textContent = copy[name][1]; target.replaceChildren(title, body);
    for (const button of document.querySelectorAll('[data-demo-agent]')) button.setAttribute('aria-pressed', String(button.dataset.demoAgent === name));
  }
  for (const button of document.querySelectorAll('[data-demo-agent]')) button.addEventListener('click', () => describeAgent(button.dataset.demoAgent));
  function showFile() {
    const source = sources[file];
    document.getElementById('demo-document-name').textContent = source.name;
    document.getElementById('demo-document-source').textContent = `来源：${source.actor} · 保留来源与版本，读取需要明确授权`;
    document.getElementById('demo-document-body').textContent = source.versions[revision];
    for (const button of document.querySelectorAll('[data-demo-file]')) button.setAttribute('aria-pressed', String(button.dataset.demoFile === file));
    for (const button of document.querySelectorAll('[data-demo-revision]')) button.setAttribute('aria-pressed', String(button.dataset.demoRevision === revision));
  }
  for (const button of document.querySelectorAll('[data-demo-file]')) button.addEventListener('click', () => {file = button.dataset.demoFile; showFile();});
  for (const link of document.querySelectorAll('[data-show-file]')) link.addEventListener('click', () => {
    select('library'); file = link.dataset.showFile; revision = '2'; showFile();
  });
  for (const button of document.querySelectorAll('[data-demo-revision]')) button.addEventListener('click', () => {revision = button.dataset.demoRevision; showFile();});
  function renderRecipient() {
    const selected = document.querySelector('[data-demo].selected')?.dataset.demo || 'message';
    showScenario(selected);
    for (const name of ['sender', 'recipient', 'body', 'status', 'receipt', 'caption']) {
      const node = document.getElementById(`demo-${name}`);
      node.textContent = node.textContent.replaceAll('Muse', primary);
    }
    document.getElementById('demo-announcement').textContent = document.getElementById('demo-subject').textContent + ' ' + document.getElementById('demo-caption').textContent;
  }
  document.getElementById('demo-primary').addEventListener('change', event => {
    primary = event.target.value;
    document.getElementById('demo-primary-name').textContent = primary;
    document.getElementById('demo-primary-note').textContent = `正在演示把新连接的结果交给「${primary}」。真实服务仍需单独签发收信与回执权限，已有连接和已送达内容不会改写。`;
    renderRecipient(); describeAgent('primary'); showFile();
  });
  for (const button of document.querySelectorAll('[data-demo]')) {
    button.addEventListener('click', renderRecipient);
    button.addEventListener('keydown', event => {if (['ArrowLeft', 'ArrowRight'].includes(event.key)) renderRecipient();});
  }
  document.getElementById('demo-file').addEventListener('click', () => {select('library'); file = 'flight'; revision = '2'; showFile();});
})();
