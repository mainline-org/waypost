'use strict';
const $ = selector => document.querySelector(selector);
let csrf = '', spaces = [], actors = [], sid = '', tab = 'inbox', cursor = null, loadEpoch = 0, authEpoch = 0;
const titles = {inbox: '信箱', agents: '助手', feed: '活动记录', messages: '全部消息', collections: '资料', privacy: '隐私规则', tokens: '连接凭据', submissions: '提交状态', receipts: '处理回执'};
function el(tag, text, cls) {const node = document.createElement(tag); if (text !== undefined) node.textContent = text; if (cls) node.className = cls; return node;}
function button(text, action, cls = 'secondary') {const node = el('button', text, cls); node.type = 'button'; node.addEventListener('click', () => busy(node, action)); return node;}
function notice(message) {$('#notice').textContent = message; $('#notice').hidden = !message; $('#login-error').textContent = message; $('#detail-error').textContent = message; $('#detail-error').hidden = !message;}
async function busy(node, action) {node.disabled = true; try {notice(''); await action();} catch (error) {notice(error.message);} finally {node.disabled = false;}}
async function api(path, method = 'GET', body) {
  const generation = authEpoch;
  const response = await fetch(path, {method, credentials: 'same-origin', headers: {'content-type': 'application/json', 'x-waypost-csrf': csrf}, body: body === undefined ? undefined : JSON.stringify(body)});
  const result = await response.json();
  if (generation !== authEpoch) throw new Error('登录状态已变化，请重新登录后继续。');
  if (!response.ok || !result.ok) {if (response.status === 401) showLogin(); throw new Error(result.error?.message || `HTTP ${response.status}`);}
  return result.data;
}
function showLogin() {authEpoch++; loadEpoch++; csrf = ''; spaces = []; actors = []; sid = ''; tab = 'inbox'; finishCreation(); $('#detail').close(); $('#detail-title').textContent = ''; $('#app').hidden = true; $('#login').hidden = false; $('#content').replaceChildren(); $('#detail-body').replaceChildren();}
async function entered(session) {csrf = session.csrf_token; $('#new-space').hidden = !session.operator; $('#owner-credential').hidden = !session.operator; $('#login').hidden = true; $('#app').hidden = false; await loadSpaces();}
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
  const epoch = ++loadEpoch, spaceId = sid, currentTab = tab;
  $('#title').textContent = titles[tab];
  document.querySelectorAll('nav button').forEach(b => b.classList.toggle('active', b.dataset.tab === tab));
  $('#add-agent').hidden = !['agents', 'inbox'].includes(tab) || !sid;
  $('#subtitle').textContent = tab === 'inbox' ? '助手发给你的报告和消息。' : tab === 'agents' ? '接入一个助手，让它把进展送到这里。' : tab === 'collections' ? '资料保留来源和版本，按授权共享。' : tab === 'feed' ? '投递、资料修订与回执；助手内部行为需主动报告。' : tab === 'privacy' ? '按助手选择脱敏或阻止，本地规则不向外部模型发送原文。' : tab === 'messages' ? '当前空间的所有消息，包括助手之间的交接。' : '查看详细记录，管理和恢复连接。';
  if (!append) {$('#content').replaceChildren(); cursor = null; $('#more').hidden = true;}
  if (!sid) {$('#content').append(el('div', '还没有工作空间。新建一个空间开始接入 Agent。', 'empty')); return;}
  const loadedActors = await registry('actors', spaceId);
  if (epoch !== loadEpoch) return;
  actors = loadedActors;
  if (tab === 'agents') {renderAgents(); return;}
  if (tab === 'privacy') {await renderPrivacy(); return;}
  const result = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind: currentTab, limit: 20, cursor});
  if (epoch !== loadEpoch) return;
  cursor = result.next_cursor;
  if (!result.items.length && !append) {
    const empty = el('div', undefined, 'empty');
    empty.append(el('h2', tab === 'inbox' ? '等助手的第一封信。' : '这里还没有记录。'), el('p', tab === 'inbox' ? '接入助手后，请它向默认收件人发一条消息。' : '开始使用后，对应内容会保存在这里。'));
    if (tab === 'inbox') empty.append(button('接入助手', () => openOnboarding(), ''));
    $('#content').append(empty);
  } else if (result.items.length) {
    if (['inbox', 'messages'].includes(tab)) renderMessages(result.items); else renderTable(result.items);
  }
  $('#more').hidden = !result.has_more;
}
function actorName(id) {return actors.find(a => a.actor_id === id)?.display_name || id || '—';}
function date(value) {return value ? new Date(value).toLocaleString() : '—';}
function openDetail(title, ...nodes) {$('#detail-title').textContent = title; $('#detail-error').textContent = ''; $('#detail-error').hidden = true; $('#detail-body').replaceChildren(...nodes); if (!$('#detail').open) $('#detail').showModal();}
function showDetail(title, value) {openDetail(title, el('pre', typeof value === 'string' ? value : JSON.stringify(value, null, 2)));}
function renderMessages(items, target = $('#content')) {
  for (const item of items) {
    const row = button('', () => readMessage(item), 'message-row'), copy = el('span', undefined, 'message-copy');
    copy.append(el('strong', item.subject), el('small', actorName(item.producer_id)));
    row.append(el('span', actorName(item.producer_id).slice(0, 1), 'avatar'), copy, el('span', date(item.committed_at), 'message-time'));
    target.append(row);
  }
}
async function readMessage(item) {
  const spaceId = sid, epoch = loadEpoch;
  const message = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind: 'message', resource_id: item.message_id});
  if (spaceId !== sid || epoch !== loadEpoch) return;
  const body = el('div');
  body.append(el('p', `${actorName(message.producer_id)} · ${date(message.committed_at)}`, 'message-meta'), el('div', message.body.text, 'message-text'));
  if (message.body.options) body.append(el('pre', JSON.stringify(message.body.options, null, 2)));
  if (message.attachment_ids?.length) {
    const attachments = el('div', undefined, 'message-attachments'); attachments.append(el('h3', '附件'));
    for (const id of message.attachment_ids) attachments.append(button('查看附件', async () => {
      const content = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind: 'blob_content', resource_id: id});
      if (spaceId === sid && epoch === loadEpoch) showDetail('附件内容', content.text === null ? content : content.text);
    }));
    body.append(attachments);
  }
  const details = el('details', undefined, 'technical'); details.append(el('summary', '来源与详细记录'), el('pre', JSON.stringify({message_id: message.message_id, kind: message.kind, source: message.source, metadata: message.metadata, attachment_ids: message.attachment_ids}, null, 2))); body.append(details);
  openDetail(message.subject, body);
}
function renderAgents() {
  const grid = el('div', undefined, 'grid'), other = el('div', undefined, 'grid');
  for (const actor of actors) {
    const card = el('article', undefined, 'card agent');
    const avatar = el('div', actor.display_name.slice(0, 1), 'avatar'); card.append(avatar, el('h2', actor.display_name));
    card.append(el('span', actor.disabled ? '接入已暂停' : actor.kind === 'agent' ? '身份已注册' : actor.kind === 'user' ? '用户入口' : '客户端', `pill${actor.disabled ? ' disabled' : ''}`));
    card.append(el('p', '收到真实来信，才能确认宿主已完成连接。', 'muted'));
    const actions = el('div', undefined, 'row');
    const spaceId = sid, epoch = loadEpoch;
    actions.append(button('最近来信', async () => {
      const result = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind: 'inbox', actor_id: actor.actor_id, limit: 20});
      if (spaceId !== sid || epoch !== loadEpoch) return;
      const list = el('section');
      if (result.items.length) renderMessages(result.items, list); else list.append(el('p', '尚未收到它发给你的消息。请在助手的宿主中配置连接，再发送第一封信。'));
      openDetail(`${actor.display_name} · 最近来信`, list);
    }));
    if (!actor.disabled && actor.kind === 'agent') actions.append(button('连接与权限', () => manageConnections(actor)));
    if (!actor.disabled && actor.kind === 'agent') actions.append(button('暂停接入', async () => {
      if (!confirm(`禁用 ${actor.display_name} 后，其凭据将失效。`)) return;
      await api(`/admin/v0/spaces/${sid}/actors/${actor.actor_id}/disable`, 'POST', {}); await load();
    }));
    const advanced = el('details', undefined, 'technical'); advanced.append(el('summary', '身份与详细活动'), el('code', actor.actor_id), button('查看活动记录', async () => {
      const result = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind: 'feed', actor_id: actor.actor_id, limit: 20});
      if (spaceId === sid && epoch === loadEpoch) showDetail(`${actor.display_name} · 活动记录`, result.items);
    }));
    card.append(actions, advanced); (actor.kind === 'agent' ? grid : other).append(card);
  }
  if (!grid.children.length) grid.append(el('div', '还没有助手。点击「接入助手」，从第一封信开始。', 'empty'));
  $('#content').append(grid);
  if (other.children.length) {const details = el('details', undefined, 'technical'); details.append(el('summary', '用户与客户端身份'), other); $('#content').append(details);}
}
function renderTable(items) {
  const spaceId = sid, epoch = loadEpoch;
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
    const action = el('td'); action.append(button('详情', async () => {
      if (tab === 'collections') {
        const history = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind: 'memory_revision', resource_id: item.collection_id});
        if (spaceId !== sid || epoch !== loadEpoch) return;
        $('#detail-title').textContent = item.display_name;
        const body = $('#detail-body'); body.replaceChildren(el('p', `来源：${actorName(item.source_actor_id)} · 范围：${item.scope}`));
        for (const revision of history.revisions) body.append(button(`revision ${revision.revision} · ${revision.active} 条内容`, async () => {
          const state = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind: 'memory', resource_id: item.collection_id, revision: revision.revision});
          if (spaceId !== sid || epoch !== loadEpoch) return;
          body.replaceChildren(el('p', `revision ${state.revision}${state.has_more ? ' · 当前显示前 100 项' : ''}`));
          for (const entry of state.items) {
            body.append(el('p', `${entry.path}${entry.deleted_in_revision ? ' · 已删除' : ''}`), button('查看资料', async () => {
              const content = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind: 'blob_content', resource_id: entry.blob_id});
              if (spaceId === sid && epoch === loadEpoch) showDetail(entry.path, content.text === null ? content : content.text);
            }));
          }
        }));
        if (!history.revisions.length) body.append(el('p', 'revision 0 · 空集合'));
        if (!$('#detail').open) $('#detail').showModal();
      } else showDetail(titles[tab], item);
    }));
    if (tab === 'tokens' && !item.revoked_at) action.append(button('撤销', async () => {await api(`/admin/v0/spaces/${sid}/tokens/${item.token_id}`, 'DELETE', {}); await load();}));
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
$('#space').addEventListener('change', () => {sid = $('#space').value; $('#detail').close(); $('#detail-body').replaceChildren(); busy($('#refresh'), () => load());});
$('#refresh').addEventListener('click', () => busy($('#refresh'), () => load()));
$('#more').addEventListener('click', () => busy($('#more'), () => load(true)));
$('#logout').addEventListener('click', () => busy($('#logout'), async () => {await api('/admin/session', 'DELETE', {}); finishCreation(); showLogin();}));
$('#close-detail').addEventListener('click', () => {$('#detail').close(); $('#detail-body').replaceChildren();});
$('#detail').addEventListener('close', () => $('#detail-body').replaceChildren());
document.querySelectorAll('nav button').forEach(node => node.addEventListener('click', () => busy(node, async () => {tab = node.dataset.tab; await load();})));
// Persist only operation IDs/stages, never credentials, so lost responses resume one creation.
function pendingCreation() {try {return JSON.parse(sessionStorage.getItem('waypost-pending-creation') || 'null');} catch {return null;}}
function saveCreation(task) {sessionStorage.setItem('waypost-pending-creation', JSON.stringify(task));}
function finishCreation() {sessionStorage.removeItem('waypost-pending-creation');}
$('#new-space').addEventListener('click', () => busy($('#new-space'), async () => {
  let task = pendingCreation();
  if (task && task.type !== 'space') throw new Error('先点击接入助手，恢复尚未完成的接入。');
  if (!task) {const name = prompt('空间名称'); if (!name?.trim()) return; task = {type: 'space', name, prefix: crypto.randomUUID()}; saveCreation(task);}
  const space = await api('/admin/v0/spaces', 'POST', {request_id: task.prefix, display_name: task.name});
  sid = space.space_id; finishCreation(); await loadSpaces();
}));
const scopeNames = {'message.send': '向获准的收件人发信', 'message.read': '收取自己的消息', 'receipt.write': '确认收取和处理', 'blob.write': '上传附件与资料', 'memory.write': '更新获准的资料集合', 'memory.read': '读取获准的资料集合'};
function openOnboarding() {
  const task = pendingCreation();
  if (task) {
    if (task.type !== 'agent') throw new Error('请恢复尚未完成的创建或签发。');
    return createAgent(task.name);
  }
  const form = el('form'), label = el('label', '给助手取个名字'), input = el('input');
  input.required = true; input.maxLength = 100; input.placeholder = '例如：资料助手'; label.append(input);
  const submit = el('button', '生成连接信息'); submit.type = 'submit';
  form.append(label, el('p', '这份连接只允许向你发信。收信、附件和资料权限，可以在接入后按需授权。'), submit);
  form.addEventListener('submit', event => {event.preventDefault(); if (input.value.trim()) busy(submit, () => createAgent(input.value.trim()));});
  openDetail('接入助手', form); input.focus();
}
async function issueConnection(task) {
  const root = `/admin/v0/spaces/${task.sid}`;
  let credential;
  while (!credential?.token) {
    if (task.revoke_token_id) {await api(root + `/tokens/${task.revoke_token_id}`, 'DELETE', {}); delete task.revoke_token_id; task.token_request_id = crypto.randomUUID(); saveCreation(task);}
    if (!task.token_request_id) {task.token_request_id = task.prefix + '-token'; saveCreation(task);}
    credential = await api(root + '/tokens', 'POST', {request_id: task.token_request_id, actor_id: task.actor.actor_id, consumer_id: task.consumer.consumer_id, scopes: task.scopes, resource_grants: task.grants || {}, ...(task.expires_at ? {expires_at: task.expires_at} : {})});
    if (!credential.token) {task.revoke_token_id = credential.token_id; saveCreation(task);}
  }
  const info = {mcp_url: `${location.origin}/mcp`, headers: {Authorization: `Bearer ${credential.token}`}, actor_id: task.actor.actor_id, consumer_id: task.consumer.consumer_id, ...(task.collection ? {collection_id: task.collection.collection_id} : {})};
  const body = el('div');
  body.append(el('p', '将连接信息保存到助手宿主的私密 MCP 配置。原始凭据只展示一次。'), el('pre', JSON.stringify(info, null, 2)), el('h3', '这份连接允许'), el('p', task.scopes.map(s => scopeNames[s] || s).join('；')), el('p', task.expires_at ? `到期：${date(task.expires_at)}` : '持续有效，可在连接管理中随时撤销。'), el('p', '接好后，请助手向默认收件人发送第一封信。回到信箱，确认消息已收到。'));
  openDetail('连接信息', body); sid = task.sid; finishCreation(); await loadSpaces();
}
async function createAgent(name) {
  let task = pendingCreation();
  if (task && task.type !== 'agent') throw new Error('先恢复尚未完成的创建或签发。');
  if (!task) {task = {type: 'agent', name, sid, prefix: crypto.randomUUID()}; saveCreation(task);}
  const root = `/admin/v0/spaces/${task.sid}`;
  if (!task.actor) {task.actor = await api(root + '/actors', 'POST', {request_id: task.prefix + '-actor', display_name: task.name, kind: 'agent'}); saveCreation(task);}
  if (!task.consumer) {task.consumer = await api(root + '/consumers', 'POST', {request_id: task.prefix + '-consumer', actor_id: task.actor.actor_id, view: 'inbox', filter: {}}); saveCreation(task);}
  task.scopes = ['message.send']; task.grants = {}; saveCreation(task);
  await issueConnection(task);
}
async function manageConnections(actor) {
  const spaceId = sid, epoch = loadEpoch, root = `/admin/v0/spaces/${spaceId}`;
  const [tokens, consumers, collections] = await Promise.all([api(root + '/inspect', 'POST', {kind: 'tokens', actor_id: actor.actor_id, limit: 100}), registry('consumers', spaceId), inspectItems('collections', spaceId)]);
  if (spaceId !== sid || epoch !== loadEpoch) return;
  const body = el('div');
  body.append(el('p', '每份有效连接提供各自的权限。签发新连接后，请更新宿主配置；缩小权限时，也要撤销旧连接。'));
  for (const token of tokens.items) {
    const expired = token.expires_at && new Date(token.expires_at).getTime() <= Date.now();
    const row = el('div', undefined, 'card');
    row.append(el('p', token.revoked_at ? '已撤销' : expired ? '已到期' : '有效连接'), el('p', token.scopes.map(s => scopeNames[s] || s).join('；')), el('p', token.expires_at ? `到期：${date(token.expires_at)}` : '持续有效'), el('code', token.token_id));
    if (!token.revoked_at) row.append(button('撤销这份连接', async () => {if (!confirm('撤销后，这份连接立即失效。继续？')) return; await api(root + `/tokens/${token.token_id}`, 'DELETE', {}); if (spaceId === sid) await manageConnections(actor);}));
    body.append(row);
  }
  if (tokens.has_more) body.append(el('p', '这里只显示最近 100 份连接，完整记录见「管理 → 连接凭据」。'));
  const form = el('form', undefined, 'connection-form'); form.append(el('h3', '签发新连接'));
  const consumerLabel = el('label', '使用哪个客户端身份'), consumer = el('select');
  for (const c of consumers.filter(c => c.actor_id === actor.actor_id && !c.disabled)) {const option = el('option', c.consumer_id); option.value = c.consumer_id; consumer.append(option);}
  consumer.required = true; consumerLabel.append(consumer); form.append(consumerLabel);
  const scopeInputs = [], scopes = el('fieldset'), choices = el('div', undefined, 'checks'); scopes.append(el('legend', '允许做什么'));
  for (const [scope, name] of Object.entries(scopeNames)) {const label = el('label', name), check = el('input'); check.type = 'checkbox'; check.checked = scope === 'message.send'; label.prepend(check); choices.append(label); scopeInputs.push([scope, check]);} scopes.append(choices); form.append(scopes);
  function resourceChoices(title, resources, key) {
    if (!resources.length) return [];
    const field = el('fieldset'), list = el('div', undefined, 'checks'), inputs = []; field.append(el('legend', title));
    for (const r of resources) {const label = el('label', r.display_name), check = el('input'); check.type = 'checkbox'; label.prepend(check); list.append(label); inputs.push([r[key], check]);}
    field.append(list); form.append(field); return inputs;
  }
  const recipients = resourceChoices('额外收件人（未选时只向你发信）', actors.filter(a => a.kind === 'agent' && !a.disabled && a.actor_id !== actor.actor_id), 'actor_id');
  const read = resourceChoices('额外可读资料集合', collections.filter(c => c.source_actor_id !== actor.actor_id), 'collection_id');
  const write = resourceChoices('额外可写资料集合', collections.filter(c => c.source_actor_id !== actor.actor_id), 'collection_id');
  const ownLabel = el('label', '为这个助手创建自己的资料集合'), own = el('input'); own.type = 'checkbox'; ownLabel.prepend(own); form.append(ownLabel, el('p', '启用资料读写后，这个助手自己拥有的集合也在权限范围内。额外集合需要同时勾选相应读写能力。'));
  const ttlLabel = el('label', '连接有效期'), ttl = el('select');
  for (const [value, text] of [['0', '持续有效，随时撤销'], ['3600', '1 小时'], ['86400', '1 天'], ['604800', '7 天']]) {const option = el('option', text); option.value = value; ttl.append(option);} ttlLabel.append(ttl); form.append(ttlLabel);
  const submit = el('button', pendingCreation()?.type === 'connection' ? '恢复上次签发' : '签发并显示连接信息'); submit.type = 'submit'; form.append(submit);
  form.addEventListener('submit', event => {event.preventDefault(); busy(submit, async () => {
    let task = pendingCreation();
    if (task && (task.type !== 'connection' || task.actor.actor_id !== actor.actor_id || task.sid !== spaceId)) throw new Error('请先恢复其他尚未完成的创建或签发。');
    if (!task) {
      if (sid !== spaceId || epoch !== loadEpoch) throw new Error('页面已更新，请重新打开连接管理。');
      const selected = scopeInputs.filter(([, i]) => i.checked).map(([scope]) => scope);
      if (!selected.length) throw new Error('至少选择一项能力。');
      if (own.checked && !selected.some(s => s.startsWith('memory.'))) throw new Error('创建资料集合需要选择资料读写能力。');
      const recipientIds = recipients.filter(([, i]) => i.checked).map(([id]) => id);
      if (recipientIds.length && !selected.includes('message.send')) throw new Error('选择额外收件人需要发信能力。');
      if (read.some(([, i]) => i.checked) && !selected.includes('memory.read')) throw new Error('选择可读集合需要资料读取能力。');
      if (write.some(([, i]) => i.checked) && !selected.includes('memory.write')) throw new Error('选择可写集合需要资料更新能力。');
      task = {type: 'connection', name: actor.display_name, sid: spaceId, actor, consumer: {consumer_id: consumer.value}, prefix: crypto.randomUUID(), scopes: selected, grants: {recipient_ids: recipientIds.length ? [spaces.find(s => s.space_id === spaceId).default_actor_id, ...recipientIds] : [], memory_read_collection_ids: read.filter(([, i]) => i.checked).map(([id]) => id), memory_write_collection_ids: write.filter(([, i]) => i.checked).map(([id]) => id)}, create_collection: own.checked, expires_at: Number(ttl.value) ? new Date(Date.now() + Number(ttl.value) * 1000).toISOString() : null}; saveCreation(task);
    }
    if (task.create_collection && !task.collection) {task.collection = await api(root + '/collections', 'POST', {request_id: task.prefix + '-collection', source_actor_id: actor.actor_id, display_name: actor.display_name + ' · 资料', scope: 'personal/default'}); saveCreation(task);}
    await issueConnection(task);
  });});
  body.append(form); openDetail(`${actor.display_name} · 连接与权限`, body);
}
async function inspectItems(kind, spaceId) {
  const items = []; let next = null;
  do {const page = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind, limit: 100, cursor: next}); items.push(...page.items); next = page.has_more ? page.next_cursor : null;} while (next);
  return items;
}
$('#add-agent').addEventListener('click', () => busy($('#add-agent'), () => openOnboarding()));
api('/admin/session').then(entered).catch(() => showLogin());

$('#owner-credential').addEventListener('click', () => busy($('#owner-credential'), async () => {
  if (!sid) throw new Error('先创建或选择一个用户空间。');
  const path = `/admin/v0/spaces/${sid}/owner-credential`;
  const current = await api(path);
  if (current.version && !confirm('重新签发会使旧用户凭据和登录会话失效。继续签发？')) return;
  const result = await api(path, 'POST', {expected_version: current.version});
  showDetail('用户登录凭据 · 只展示一次', {dashboard_url: `${location.origin}/dashboard`, space_id: sid, credential: result.credential, note: '这份凭据只管理当前用户空间。通过私密渠道交给该用户；Agent 使用另外签发的连接凭据。'});
}));
