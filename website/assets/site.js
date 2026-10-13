'use strict';

/*
 * Homepage demo. Three fixed hand-offs, swapped in place so the page stays
 * readable without JavaScript: the first scenario is written into the markup.
 */
const scenarios = {
  message: {
    sender: 'Instinct', avatar: 'I', recipient: '发给 Muse',
    subject: 'Burnt Ends，11 月 6 日 19:00，两人位。',
    body: '预约已确认：Burnt Ends，11 月 6 日 19:00，两人位，可以改到周六 20:00。备注一位不吃花生。请 Muse 放进这次出行安排。',
    file: '', tag: '',
    status: '已投递到 Muse 的信箱', receipt: 'Muse 稍后收取', step: '消息投递示例',
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
  }
};

const tabs = [...document.querySelectorAll('[data-demo]')];

function showScenario(key) {
  const state = scenarios[key];
  if (!state) return;

  for (const tab of tabs) {
    const selected = tab.dataset.demo === key;
    tab.classList.toggle('selected', selected);
    tab.setAttribute('aria-pressed', String(selected));
  }

  for (const field of ['sender', 'avatar', 'recipient', 'subject', 'body', 'status', 'step', 'receipt', 'caption']) {
    document.getElementById('demo-' + field).textContent = state[field];
  }

  const file = document.getElementById('demo-file');
  file.hidden = !state.file;
  file.querySelector('span').textContent = state.file;
  file.querySelector('.file-tag').textContent = state.tag;

  document.getElementById('demo-announcement').textContent = state.sender + '：' + state.subject + ' ' + state.caption;

  const letter = document.querySelector('.message-card');
  letter.classList.remove('changed');
  void letter.offsetWidth;
  letter.classList.add('changed');
}

for (const tab of tabs) {
  tab.addEventListener('click', () => showScenario(tab.dataset.demo));
  tab.addEventListener('keydown', (event) => {
    if (!['ArrowLeft', 'ArrowRight'].includes(event.key)) return;
    event.preventDefault();
    const index = tabs.indexOf(tab);
    const step = event.key === 'ArrowRight' ? 1 : -1;
    const next = tabs[(index + step + tabs.length) % tabs.length];
    next.focus();
    showScenario(next.dataset.demo);
  });
}

for (const button of document.querySelectorAll('[data-copy]')) {
  button.addEventListener('click', async () => {
    const target = document.getElementById(button.dataset.copy);
    if (!target) return;
    try {
      await navigator.clipboard.writeText(target.textContent.trim());
      button.textContent = '已复制 ✓';
      setTimeout(() => { button.textContent = '复制这段话 ↗'; }, 1800);
    } catch {
      button.textContent = '请选中文字复制';
    }
  });
}