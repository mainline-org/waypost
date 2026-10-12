'use strict';
const $ = selector => document.querySelector(selector);
let csrf = '', spaces = [], actors = [], sid = '', tab = 'agents', cursor = null;
const titles = {agents: 'Agents', feed: '活动记录', messages: '消息', collections: '记忆', privacy: '隐私策略', tokens: '连接凭据', submissions: '提交状态', receipts: '独立回执'};
function el(tag, text, cls) {const node = document.createElement(tag); if (text !== undefined) node.textContent = text; if (cls) node.className = cls; return node;}
function button(text, action, cls = 'secondary') {const node = el('button', text, cls); node.type = 'button'; node.addEventListener('click', () => busy(node, action)); return node;}
function notice(message) {$('#notice').textContent = message; $('#notice').hidden = !message; $('#login-error').textContent = message;}
async function busy(node, action) {node.disabled = true; try {notice(''); await action();} catch (error) {notice(error.message);} finally {node.disabled = false;}}
async function api(path, method = 'GET', body) {
  const response = await fetch(path, {method, credentials: 'same-origin', headers: {'content-type': 'application/json', 'x-waypost-csrf': csrf}, body: body === undefined ? undefined : JSON.stringify(body)});
  const result = await response.json();
  if (!response.ok || !result.ok) {if (response.status === 401) showLogin(); throw new Error(result.error?.message || `HTTP ${response.status}`);}
  return result.data;
}
function showLogin() {csrf = ''; $('#app').hidden = true; $('#login').hidden = false; $('#content').replaceChildren(); $('#detail-body').replaceChildren();}
async function entered(session) {csrf = session.csrf_token; $('#login').hidden = true; $('#app').hidden = false; await loadSpaces();}
async function registry(kind, space_id) {
  let cursor = null; const items = [];
  do {const result = await api('/admin/v0/registry', 'POST', {kind, ...(space_id ? {space_id} : {}), cursor, limit: 20}); items.push(...result.items); cursor = result.has_more ? result.next_cursor : null;} while (cursor);
  return items;
}
async function loadSpaces() {
  spaces = await registry('spaces');
  const select = $('#space'); select.replaceChildren();
  for (const space of spaces) {const opt = el('option', space.display_name); opt.value = space.space_id; select.append(opt);}
  if (!spaces.some(s => s.space_id === sid)) sid = spaces[0]?.space_id || '';
  select.value = sid;
  await load();
}
async function load(append = false) {
  $('#title').textContent = titles[tab];
  document.querySelectorAll('nav button').forEach(b => b.classList.toggle('active', b.dataset.tab === tab));
  $('#add-agent').hidden = tab !== 'agents';
  $('#subtitle').textContent = tab === 'feed' ? '这些是 Waypost 记录的投递、记忆修订与回执。内部工具行为需由 Agent 主动报告。' : tab === 'privacy' ? '按参与 Agent 选择脱敏或阻止；本地规则不向外部模型发送原文。' : '持久化记录可以检查、重试和恢复。';
  if (!append) {$('#content').replaceChildren(); cursor = null; $('#more').hidden = true;}
  if (!sid) {$('#content').append(el('div', '还没有工作空间。新建一个空间开始接入 Agent。', 'empty')); return;}
  actors = await registry('actors', sid);
  if (tab === 'agents') {renderAgents(); return;}
  if (tab === 'privacy') {await renderPrivacy(); return;}
  const result = await api(`/admin/v0/spaces/${sid}/inspect`, 'POST', {kind: tab, limit: 20, cursor});
  cursor = result.next_cursor;
  if (!result.items.length && !append) $('#content').append(el('div', '还没有记录。Agent 接入后，这里会显示对应内容。', 'empty'));
  else if (result.items.length) renderTable(result.items);
  $('#more').hidden = !result.has_more;
}
function actorName(id) {return actors.find(a => a.actor_id === id)?.display_name || id || '—';}
function date(value) {return value ? new Date(value).toLocaleString() : '—';}
function showDetail(title, value) {$('#detail-title').textContent = title; $('#detail-body').textContent = typeof value === 'string' ? value : JSON.stringify(value, null, 2); if (!$('#detail').open) $('#detail').showModal();}
function renderAgents() {
  const grid = el('div', undefined, 'grid');
  for (const actor of actors) {
    const card = el('article', undefined, 'card agent');
    const avatar = el('div', actor.display_name.slice(0, 1), 'avatar'); card.append(avatar, el('h2', actor.display_name));
    card.append(el('span', actor.disabled ? '已禁用' : actor.kind === 'agent' ? '已注册 Agent' : actor.kind === 'user' ? '用户入口' : '客户端', `pill${actor.disabled ? ' disabled' : ''}`));
    card.append(el('code', actor.actor_id));
    card.append(el('p', '注册状态不代表运行在线。查看服务实际记录的活动。', 'muted'));
    const actions = el('div', undefined, 'row');
    actions.append(button('查看活动', async () => {
      const result = await api(`/admin/v0/spaces/${sid}/inspect`, 'POST', {kind: 'feed', actor_id: actor.actor_id, limit: 20});
      showDetail(`${actor.display_name} · 最近的活动`, result.items);
    }));
    if (!actor.disabled && actor.kind === 'agent') actions.append(button('禁用', async () => {
      if (!confirm(`禁用 ${actor.display_name} 后，其凭据将失效。`)) return;
      await api(`/admin/v0/spaces/${sid}/actors/${actor.actor_id}/disable`, 'POST', {}); await load();
    }));
    card.append(actions); grid.append(card);
  }
  $('#content').append(grid, el('p', '身份与 Consumer 独立。每个 Consumer 保存自己的收取位置和处理回执。', 'muted'));
}
function renderTable(items) {
  const wrap = el('div', undefined, 'table-wrap'), table = el('table'), head = el('tr');
  const columns = {
    messages: [['subject', '主题'], ['kind', '类型'], ['producer_id', '发信人'], ['committed_at', '时间']],
    feed: [['item_type', '行为'], ['producer_id', '参与方'], ['resource_version', '版本'], ['committed_at', '时间']],
    collections: [['display_name', '集合'], ['scope', '范围'], ['source_actor_id', '来源'], ['current_revision', '当前版本']],
    tokens: [['token_id', '凭据 ID'], ['actor_id', '参与方'], ['consumer_id', 'Consumer'], ['revoked_at', '撤销时间']],
    submissions: [['operation', '操作'], ['state', '状态'], ['producer_id', '参与方'], ['request_id', '请求 ID']],
    receipts: [['consumer_id', 'Consumer'], ['subject_item_id', '记录'], ['stage', '阶段'], ['version', '版本']],
  }[tab];
  for (const [, label] of columns) head.append(el('th', label)); head.append(el('th', '检查')); table.append(head);
  for (const item of items) {
    const row = el('tr');
    for (const [field] of columns) {
      let value = item[field];
      if (field.endsWith('_at')) value = date(value);
      else if (['producer_id', 'source_actor_id', 'actor_id'].includes(field)) value = actorName(value);
      else if (field === 'stage') value = value === 2 ? 'processed' : 'received';
      row.append(el('td', value === null || value === undefined ? '—' : String(value)));
    }
    const action = el('td'); action.append(button(tab === 'messages' ? '查看正文' : '详情', async () => {
      if (tab === 'messages') {
        const message = await api(`/admin/v0/spaces/${sid}/inspect`, 'POST', {kind: 'message', resource_id: item.message_id});
        showDetail(message.subject, `${actorName(message.producer_id)} · ${message.kind}\n\n${message.body.text}\n\n${JSON.stringify({options: message.body.options, source: message.source, metadata: message.metadata, attachment_ids: message.attachment_ids}, null, 2)}`);
      } else if (tab === 'collections') {
        const history = await api(`/admin/v0/spaces/${sid}/inspect`, 'POST', {kind: 'memory_revision', resource_id: item.collection_id});
        $('#detail-title').textContent = item.display_name;
        const body = $('#detail-body'); body.replaceChildren(el('p', `来源：${actorName(item.source_actor_id)} · 范围：${item.scope}`));
        for (const revision of history.revisions) body.append(button(`revision ${revision.revision} · ${revision.active} 条内容`, async () => {
          const state = await api(`/admin/v0/spaces/${sid}/inspect`, 'POST', {kind: 'memory', resource_id: item.collection_id, revision: revision.revision});
          body.replaceChildren(el('p', `revision ${state.revision}${state.has_more ? ' · 当前显示前 100 项' : ''}`));
          for (const entry of state.items) {
            body.append(el('p', `${entry.path}${entry.deleted_in_revision ? ' · 已删除' : ''}`), button('查看资料', async () => {
              const content = await api(`/admin/v0/spaces/${sid}/inspect`, 'POST', {kind: 'blob_content', resource_id: entry.blob_id});
              showDetail(entry.path, content.text === null ? content : content.text);
            }));
          }
        }));
        if (!history.revisions.length) body.append(el('p', 'revision 0 · 空集合'));
        if (!$('#detail').open) $('#detail').showModal();
      } else showDetail(titles[tab], item);
    }));
    if (tab === 'tokens' && !item.revoked_at) action.append(button('撤销', async () => {await api(`/admin/v0/tokens/${item.token_id}`, 'DELETE', {}); await load();}));
    row.append(action); table.append(row);
  }
  wrap.append(table); $('#content').append(wrap);
}
async function renderPrivacy() {
  const config = await api(`/admin/v0/spaces/${sid}/privacy`);
  const form = el('form', undefined, 'card privacy-form');
  form.append(el('h2', `通信隐私 · revision ${config.revision}`), el('p', '规则针对选定 Agent 的发信或收信执行；不改写已发布历史。来源记忆按原授权保存。'));
  const checks = el('div', undefined, 'checks');
  function check(text, checked) {const label = el('label'), input = el('input'); input.type = 'checkbox'; input.checked = checked; label.append(input, document.createTextNode(text)); checks.append(label); return input;}
  const enabled = check('启用隐私过滤', config.enabled), all = check('所有 Agent', config.all_agents), binary = check('允许无法检查的二进制附件', config.allow_uninspected_attachments);
  form.append(checks);
  const modeLabel = el('label', '发现敏感内容时'), mode = el('select');
  for (const [value, text] of [['redact', '自动脱敏后发布'], ['block', '阻止发布']]) {const option = el('option', text); option.value = value; mode.append(option);} mode.value = config.mode; modeLabel.append(mode); form.append(modeLabel);
  const targets = el('div', undefined, 'checks'); const targetInputs = [];
  form.append(el('h2', '需要保护的 Agent'));
  for (const actor of actors.filter(a => a.kind === 'agent')) {const label = el('label', actor.display_name), input = el('input'); input.type = 'checkbox'; input.checked = config.actor_ids.includes(actor.actor_id); label.prepend(input); targets.append(label); targetInputs.push([actor.actor_id, input]);} form.append(targets);
  const detectors = el('div', undefined, 'checks'), detectorInputs = [];
  for (const [kind, text] of [['credential', '服务凭据 / API key'], ['email', '邮箱'], ['phone', '手机号'], ['identity_number', '18 位身份号码']]) {const label = el('label', text), input = el('input'); input.type = 'checkbox'; input.checked = config.rules.some(r => r.kind === kind); label.prepend(input); detectors.append(label); detectorInputs.push([kind, input]);}
  form.append(el('h2', '本地检测规则'), detectors);
  const customInputs = [];
  for (const [kind, text] of [['literal', '敏感词：每行一个'], ['regex', '自定义正则：每行一个'], ['metadata_key', '敏感 metadata 字段名：每行一个']]) {const label = el('label', text), input = el('textarea'); input.value = config.rules.filter(r => r.kind === kind).map(r => r.value).join('\n'); label.append(input); form.append(label); customInputs.push([kind, input]);}
  form.append(el('p', '规则只检查 UTF-8 文本，无法识别所有自然语言隐私，也不检查图片、PDF 或二进制内容。未允许的附件类型会阻止消息发布。', 'muted'));
  const save = el('button', '保存策略'); save.type = 'submit'; form.append(save);
  form.addEventListener('submit', event => {event.preventDefault(); busy(save, async () => {
    const rules = detectorInputs.filter(([, i]) => i.checked).map(([kind]) => ({kind}));
    for (const [kind, input] of customInputs) for (const value of input.value.split('\n').filter(v => v.length)) rules.push({kind, value});
    await api(`/admin/v0/spaces/${sid}/privacy`, 'PUT', {expected_revision: config.revision, enabled: enabled.checked, mode: mode.value, all_agents: all.checked, actor_ids: targetInputs.filter(([, i]) => i.checked).map(([id]) => id), allow_uninspected_attachments: binary.checked, rules}); await load();
  });});
  $('#content').append(form);
  const preview = el('div', undefined, 'card'), input = el('textarea'); input.placeholder = '输入要检查的文本（使用已保存的规则）'; input.setAttribute('aria-label', '隐私检测预览文本');
  preview.append(el('h2', '规则预览'), input, button('预览', async () => {showDetail('隐私预览', await api(`/admin/v0/spaces/${sid}/privacy/preview`, 'POST', {text: input.value}));}), button('查看脱敏审计', async () => {showDetail('审计 · 不保存原匹配值', await api(`/admin/v0/spaces/${sid}/privacy/audit`));})); $('#content').append(preview);
}
$('#login-form').addEventListener('submit', event => {event.preventDefault(); const submit = event.currentTarget.querySelector('button'); busy(submit, async () => {const session = await api('/admin/session', 'POST', {admin_secret: $('#secret').value}); $('#secret').value = ''; await entered(session);});});
$('#space').addEventListener('change', () => {sid = $('#space').value; busy($('#refresh'), () => load());});
$('#refresh').addEventListener('click', () => busy($('#refresh'), () => load()));
$('#more').addEventListener('click', () => busy($('#more'), () => load(true)));
$('#logout').addEventListener('click', () => busy($('#logout'), async () => {await api('/admin/session', 'DELETE', {}); finishCreation(); showLogin();}));
$('#close-detail').addEventListener('click', () => {$('#detail').close(); $('#detail-body').replaceChildren();});
document.querySelectorAll('nav button').forEach(node => node.addEventListener('click', () => busy(node, async () => {tab = node.dataset.tab; await load();})));
// Persist only operation IDs/stages, never credentials, so lost responses resume one creation.
function pendingCreation() {try {return JSON.parse(sessionStorage.getItem('waypost-pending-creation') || 'null');} catch {return null;}}
function saveCreation(task) {sessionStorage.setItem('waypost-pending-creation', JSON.stringify(task));}
function finishCreation() {sessionStorage.removeItem('waypost-pending-creation');}
$('#new-space').addEventListener('click', () => busy($('#new-space'), async () => {
  let task = pendingCreation();
  if (task && task.type !== 'space') throw new Error('先点击接入 Agent，恢复尚未完成的接入。');
  if (!task) {const name = prompt('空间名称'); if (!name?.trim()) return; task = {type: 'space', name, prefix: crypto.randomUUID()}; saveCreation(task);}
  const space = await api('/admin/v0/spaces', 'POST', {request_id: task.prefix, display_name: task.name});
  sid = space.space_id; finishCreation(); await loadSpaces();
}));
$('#add-agent').addEventListener('click', () => busy($('#add-agent'), async () => {
  let task = pendingCreation();
  if (task && task.type !== 'agent') throw new Error('先点击新建空间，恢复尚未完成的创建。');
  if (!task) {const name = prompt('Agent 名称，例如 Muse 或 Instict'); if (!name?.trim()) return; task = {type: 'agent', name, sid, prefix: crypto.randomUUID()}; saveCreation(task);}
  const root = `/admin/v0/spaces/${task.sid}`;
  if (!task.actor) {task.actor = await api(root + '/actors', 'POST', {request_id: task.prefix + '-actor', display_name: task.name, kind: 'agent'}); saveCreation(task);}
  if (!task.consumer) {task.consumer = await api(root + '/consumers', 'POST', {request_id: task.prefix + '-consumer', actor_id: task.actor.actor_id, view: 'inbox', filter: {}}); saveCreation(task);}
  if (!task.collection) {task.collection = await api(root + '/collections', 'POST', {request_id: task.prefix + '-collection', source_actor_id: task.actor.actor_id, display_name: task.name + ' · 记忆', scope: 'personal/default'}); saveCreation(task);}
  let credential;
  while (!credential?.token) {
    if (task.revoke_token_id) {await api(`/admin/v0/tokens/${task.revoke_token_id}`, 'DELETE', {}); delete task.revoke_token_id; task.token_request_id = crypto.randomUUID(); saveCreation(task);}
    if (!task.token_request_id) {task.token_request_id = task.prefix + '-token'; saveCreation(task);}
    credential = await api(root + '/tokens', 'POST', {request_id: task.token_request_id, actor_id: task.actor.actor_id, consumer_id: task.consumer.consumer_id, scopes: ['message.send', 'message.read', 'receipt.write', 'blob.write', 'memory.write', 'memory.read'], resource_grants: {}});
    if (!credential.token) {task.revoke_token_id = credential.token_id; saveCreation(task);}
  }
  showDetail('连接信息 · 原始凭据仅展示一次', {mcp_url: `${location.origin}/mcp`, headers: {Authorization: `Bearer ${credential.token}`}, actor_id: task.actor.actor_id, consumer_id: task.consumer.consumer_id, collection_id: task.collection.collection_id, note: '保存到助手的私密 MCP 配置。默认投递给空间的用户入口；需要向其他 Agent 发信时，由所有者签发包含 recipient_ids 的新凭据。'});
  sid = task.sid; finishCreation(); await loadSpaces();
}));
api('/admin/session').then(entered).catch(() => showLogin());
