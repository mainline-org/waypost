'use strict';
const $ = selector => document.querySelector(selector);
let csrf = '', spaces = [], actors = [], sid = '', tab = 'inbox', cursor = null, loadEpoch = 0, authEpoch = 0, pageSize = 20, operator = false, primaryState = {revision: 0, primary_assistant: null};
const ot = key => window.waypostOnboardingText(key);
// Each tab names itself and says what it holds. One table, so the wording
// cannot drift between the sidebar, the page header and the empty state.
const sections = {
  inbox:       {title: '信箱',     note: '助手发给你的报告和消息。'},
  agents:      {title: '助手',     note: '接入一个助手，让它把进展送到这里。'},
  collections: {title: '资料',     note: '资料保留来源和版本，按授权共享。'},
  messages:    {title: '全部消息', note: '当前空间的所有消息，包括助手之间的交接。'},
  feed:        {title: '活动记录', note: '投递、资料修订与回执。助手内部的行为，需要它主动报告。'},
  privacy:     {title: '隐私规则', note: '按助手选择脱敏或阻止。本地规则不向外部模型发送原文。'},
  tokens:      {title: '连接凭据', note: '每份连接的权限、有效期和撤销记录。'},
  submissions: {title: '提交状态', note: '按请求 ID 找回原来的提交结果。'},
  receipts:    {title: '处理回执', note: '客户端报告的收取与处理阶段。'},
  access:      {title: ot('accessTitle'), note: ot('accessHelp')},
};
function el(tag, text, cls) {const node = document.createElement(tag); if (text !== undefined) node.textContent = text; if (cls) node.className = cls; return node;}
function button(text, action, cls = 'secondary') {const node = el('button', text, cls); node.type = 'button'; node.addEventListener('click', () => busy(node, action)); return node;}
function actionButton(key, text, action, cls = 'secondary') {const node = button(text, action, cls); node.dataset.action = key; return node;}
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
function showLogin() {authEpoch++; loadEpoch++; primaryState = {revision: 0, primary_assistant: null}; operator = false; csrf = ''; spaces = []; actors = []; sid = ''; tab = 'inbox'; finishCreation(); $('#detail').close(); $('#detail-title').textContent = ''; $('#app').hidden = true; $('#login').hidden = false; $('#content').replaceChildren(); $('#detail-body').replaceChildren();}
async function entered(session) {authEpoch++; primaryState = {revision: 0, primary_assistant: null}; spaces = []; actors = []; sid = ''; $('#space').replaceChildren(); $('#content').replaceChildren(); $('#add-agent').hidden = true; pageSize = Math.max(1, Math.min(20, session.limits?.max_page_size || 20)); operator = session.operator; $('#service-management').hidden = !operator; $('#operator-label').hidden = !operator; $('#invite-user').hidden = !operator; $('#access-requests-tab').hidden = !operator; csrf = session.csrf_token; $('#new-space').hidden = !session.operator; $('#owner-credential').hidden = !session.operator; $('#login').hidden = true; $('#app').hidden = false; await loadSpaces();}
async function registry(kind, space_id) {
  let cursor = null; const items = [];
  do {const result = await api('/admin/v0/registry', 'POST', {kind, ...(space_id ? {space_id} : {}), cursor, limit: pageSize}); items.push(...result.items); cursor = result.has_more ? result.next_cursor : null;} while (cursor);
  return items;
}
async function loadSpaces() {
  spaces = await registry('spaces');
  const select = $('#space'); select.replaceChildren();
  for (const space of spaces) {const opt = el('option', space.display_name); opt.value = space.space_id; select.append(opt);}
  if (!spaces.some(s => s.space_id === sid)) sid = spaces[0]?.space_id || '';
  select.value = sid;
  select.disabled = spaces.length < 2;
  await load();
}
async function load(append = false) {
  const epoch = ++loadEpoch, spaceId = sid, currentTab = tab;
  $('#title').textContent = sections[tab].title;
  document.querySelectorAll('nav button').forEach(b => b.classList.toggle('active', b.dataset.tab === tab));
  $('#add-agent').hidden = !['agents', 'inbox'].includes(tab) || !sid;
  $('#subtitle').textContent = sections[tab].note;
  if (!append) {$('#content').replaceChildren(); cursor = null; $('#more').hidden = true;}
  if (tab === 'access') {const result = await api('/admin/v0/access-requests', 'POST', {limit: pageSize, cursor}); if (epoch !== loadEpoch) return; cursor = result.next_cursor; renderAccessRequests(result.items); $('#more').hidden = !result.has_more; return;}
  if (!sid) {$('#content').append(el('div', '还没有空间。新建一个，开始接入助手。', 'empty')); return;}
  const loadedActors = await registry('actors', spaceId);
  if (epoch !== loadEpoch) return;
  actors = loadedActors;
  const config = await api(`/admin/v0/spaces/${spaceId}/primary-assistant`);
  if (epoch !== loadEpoch) return;
  primaryState = config;
  if (tab === 'agents') {renderAgents(); return;}
  if (tab === 'privacy') {await renderPrivacy(); return;}
  const result = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind: currentTab, limit: pageSize, cursor});
  if (epoch !== loadEpoch) return;
  cursor = result.next_cursor;
  if (!result.items.length && !append) {
    const empty = el('div', undefined, 'empty');
    const primary = primaryState.primary_assistant;
    empty.append(el('h2', tab === 'inbox' ? (primary ? ot('waitingPrimary') : '等助手的第一封信。') : '这里还没有记录。'), el('p', tab === 'inbox' ? (primary ? `${primary.display_name} · ${ot('waitingPrimaryHelp')}` : '接入助手后，请它向默认收件人发一条消息。') : '开始使用后，对应内容会保存在这里。'));
    if (tab === 'inbox') empty.append(primary ? button(ot('viewAssistants'), async () => {tab = 'agents'; await load();}, '') : button('接入助手', () => openOnboarding(), ''));
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
  const guide = el('div', undefined, 'role-guide');
  guide.append(el('h2', primaryState.primary_assistant ? `${ot('primary')}: ${primaryState.primary_assistant.display_name}` : ot('primaryStart')), el('p', ot('roleHelp')), el('p', ot('rolePermissions'), 'fine'));
  $('#content').append(guide);
  const grid = el('div', undefined, 'grid'), other = el('div', undefined, 'grid');
  for (const actor of actors) {
    const card = el('article', undefined, 'card agent');
    const avatar = el('div', actor.display_name.slice(0, 1), 'avatar'); card.append(avatar, el('h2', actor.display_name));
    card.append(el('span', actor.disabled ? '接入已暂停' : actor.kind === 'agent' ? (primaryState.primary_assistant?.actor_id === actor.actor_id ? ot('primary') : ot('connected')) : actor.kind === 'user' ? '用户入口' : '客户端', `pill${actor.disabled ? ' disabled' : ''}`));
    card.append(el('p', actor.kind === 'agent' && primaryState.primary_assistant?.actor_id === actor.actor_id ? ot('primaryCardHelp') : ot('connectedCardHelp'), 'fine'));
    const actions = el('div', undefined, 'row');
    const spaceId = sid, epoch = loadEpoch;
    actions.append(button('最近交付', async () => {
      const result = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind: 'messages', actor_id: actor.actor_id, limit: pageSize});
      if (spaceId !== sid || epoch !== loadEpoch) return;
      const list = el('section');
      if (result.items.length) renderMessages(result.items, list); else list.append(el('p', '还没有收到它的消息。在助手的运行环境里填好连接，再请它发第一封信。'));
      openDetail(`${actor.display_name} · 最近交付`, list);
    }));
    if (!actor.disabled && actor.kind === 'agent') {actions.append(actionButton('manage-connections', '连接与权限', () => manageConnections(actor))); if (primaryState.primary_assistant?.actor_id !== actor.actor_id) actions.append(actionButton('choose-primary', ot('choosePrimary'), () => choosePrimary(actor))); else actions.append(actionButton('configure-primary', ot('configurePrimary'), () => manageConnections(actor, 'primary')));}
    if (!actor.disabled && actor.kind === 'agent') actions.append(button('暂停接入', async () => {
      if (!confirm(`禁用 ${actor.display_name} 后，其凭据将失效。`)) return;
      await api(`/admin/v0/spaces/${sid}/actors/${actor.actor_id}/disable`, 'POST', {}); await load();
    }));
    const advanced = el('details', undefined, 'technical'); advanced.append(el('summary', '身份与详细活动'), el('code', actor.actor_id), button('查看活动记录', async () => {
      const result = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind: 'feed', actor_id: actor.actor_id, limit: pageSize});
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
      } else showDetail(sections[tab].title, item);
    }));
    if (tab === 'tokens' && !item.revoked_at) action.append(button('撤销', async () => {await api(`/admin/v0/spaces/${sid}/tokens/${item.token_id}`, 'DELETE', {}); await load();}));
    row.append(action); table.append(row);
  }
  wrap.append(table); $('#content').append(wrap);
}
async function renderPrivacy() {
  const config = await api(`/admin/v0/spaces/${sid}/privacy`);
  const form = el('form', undefined, 'card privacy-form');
  form.append(el('h2', `通信隐私 · revision ${config.revision}`), el('p', '规则对选定助手的发信或收信执行，不改写已发布的历史。来源资料按原授权保存。'));
  const checks = el('div', undefined, 'checks');
  function check(text, checked) {const label = el('label'), input = el('input'); input.type = 'checkbox'; input.checked = checked; label.append(input, document.createTextNode(text)); checks.append(label); return input;}
  const enabled = check('启用隐私过滤', config.enabled), all = check('所有助手', config.all_agents), binary = check('允许无法检查的二进制附件', config.allow_uninspected_attachments);
  form.append(checks);
  const modeLabel = el('label', '发现敏感内容时'), mode = el('select');
  for (const [value, text] of [['redact', '自动脱敏后发布'], ['block', '阻止发布']]) {const option = el('option', text); option.value = value; mode.append(option);} mode.value = config.mode; modeLabel.append(mode); form.append(modeLabel);
  const targets = el('div', undefined, 'checks'); const targetInputs = [];
  form.append(el('h2', '需要保护的助手'));
  for (const actor of actors.filter(a => a.kind === 'agent')) {const label = el('label', actor.display_name), input = el('input'); input.type = 'checkbox'; input.checked = config.actor_ids.includes(actor.actor_id); label.prepend(input); targets.append(label); targetInputs.push([actor.actor_id, input]);} form.append(targets);
  const detectors = el('div', undefined, 'checks'), detectorInputs = [];
  for (const [kind, text] of [['credential', '服务凭据 / API key'], ['email', '邮箱'], ['phone', '手机号'], ['identity_number', '18 位身份号码']]) {const label = el('label', text), input = el('input'); input.type = 'checkbox'; input.checked = config.rules.some(r => r.kind === kind); label.prepend(input); detectors.append(label); detectorInputs.push([kind, input]);}
  form.append(el('h2', '本地检测规则'), detectors);
  const customInputs = [];
  for (const [kind, text] of [['literal', '敏感词：每行一个'], ['regex', '自定义正则：每行一个'], ['metadata_key', '敏感 metadata 字段名：每行一个']]) {const label = el('label', text), input = el('textarea'); input.value = config.rules.filter(r => r.kind === kind).map(r => r.value).join('\n'); label.append(input); form.append(label); customInputs.push([kind, input]);}
  form.append(el('p', '规则只检查 UTF-8 文本，认不出所有自然语言里的隐私，也不检查图片、PDF 和二进制内容。未允许的附件类型会阻止消息发布。', 'fine'));
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
  if (!sid) throw new Error(ot('selectSpace'));
  const task = pendingCreation();
  if (task) {
    if (task.type !== 'agent') throw new Error('请恢复尚未完成的创建或签发。');
    return createAgent(task.name);
  }
  const form = el('form'), label = el('label', '给助手取个名字'), input = el('input');
  input.required = true; input.maxLength = 100; input.placeholder = '例如：Muse 或 Instinct'; label.append(input);
  const recipientLabel = el('label', ot('recipientLabel')), recipient = el('select');
  const ownOption = el('option', ot('ownerRecipient')); ownOption.value = ''; recipient.append(ownOption);
  if (primaryState.primary_assistant && !primaryState.primary_assistant.disabled) {const option = el('option', primaryState.primary_assistant.display_name); option.value = primaryState.primary_assistant.actor_id; recipient.append(option); recipient.value = option.value;}
  recipientLabel.append(recipient);
  const submit = el('button', '生成连接信息'); submit.type = 'submit';
  form.append(label, recipientLabel, el('p', '这份连接只有发信能力。收信、附件和资料权限，可以在接入后按需授权。'), submit);
  form.addEventListener('submit', event => {event.preventDefault(); if (input.value.trim()) busy(submit, () => createAgent(input.value.trim(), recipient.value || null));});
  openDetail('接入助手', form); input.focus();
}
async function issueConnection(task) {
  const epoch = authEpoch;
  const root = `/admin/v0/spaces/${task.sid}`;
  let credential;
  while (!credential?.token) {
    if (task.revoke_token_id) {await api(root + `/tokens/${task.revoke_token_id}`, 'DELETE', {}); if (epoch !== authEpoch) return; delete task.revoke_token_id; task.token_request_id = crypto.randomUUID(); saveCreation(task);}
    if (!task.token_request_id) {task.token_request_id = task.prefix + '-token'; saveCreation(task);}
    credential = await api(root + '/tokens', 'POST', {request_id: task.token_request_id, actor_id: task.actor.actor_id, consumer_id: task.consumer.consumer_id, scopes: task.scopes, resource_grants: task.grants || {}, ...(task.expires_at ? {expires_at: task.expires_at} : {}), ...(task.default_recipient_id ? {default_recipient_id: task.default_recipient_id} : {})});
    if (epoch !== authEpoch) return;
    if (!credential.token) {task.revoke_token_id = credential.token_id; saveCreation(task);}
  }
  const info = {mcp_url: `${location.origin}/mcp`, headers: {Authorization: `Bearer ${credential.token}`}, actor_id: task.actor.actor_id, consumer_id: task.consumer.consumer_id, default_recipient_id: task.default_recipient_id || spaces.find(s => s.space_id === task.sid)?.default_actor_id, ...(task.collection ? {collection_id: task.collection.collection_id} : {})};
  const body = el('div');
  body.append(el('p', '把这份连接信息存进助手的运行环境。原始凭据只展示一次。'), el('pre', JSON.stringify(info, null, 2)), el('h3', '这份连接允许'), el('p', task.scopes.map(s => scopeNames[s] || s).join('；')), el('p', task.expires_at ? `到期：${date(task.expires_at)}` : '持续有效，可在连接管理中随时撤销。'), el('p', '接好后，请助手向默认收件人发送第一封信。回到信箱，确认消息已收到。'));
  openDetail('连接信息', body); sid = task.sid; finishCreation(); await loadSpaces();
}
async function createAgent(name, defaultRecipient = null) {
  let task = pendingCreation();
  if (task && task.type !== 'agent') throw new Error('先恢复尚未完成的创建或签发。');
  if (!task) {task = {type: 'agent', name, sid, default_recipient_id: defaultRecipient, prefix: crypto.randomUUID()}; saveCreation(task);}
  const root = `/admin/v0/spaces/${task.sid}`;
  if (!task.actor) {task.actor = await api(root + '/actors', 'POST', {request_id: task.prefix + '-actor', display_name: task.name, kind: 'agent'}); saveCreation(task);}
  if (!task.consumer) {task.consumer = await api(root + '/consumers', 'POST', {request_id: task.prefix + '-consumer', actor_id: task.actor.actor_id, view: 'inbox', filter: {}}); saveCreation(task);}
  task.scopes = ['message.send']; task.grants = task.default_recipient_id ? {recipient_ids: [task.default_recipient_id]} : {}; saveCreation(task);
  await issueConnection(task);
}
async function manageConnections(actor, preset = null) {
  const spaceId = sid, epoch = loadEpoch, root = `/admin/v0/spaces/${spaceId}`;
  const [tokens, consumers, collections] = await Promise.all([api(root + '/inspect', 'POST', {kind: 'tokens', actor_id: actor.actor_id, limit: pageSize}), registry('consumers', spaceId), inspectItems('collections', spaceId)]);
  if (spaceId !== sid || epoch !== loadEpoch) return;
  const body = el('div');
  body.append(el('p', '每份连接有各自的权限。签发新的，请更新助手的配置；缩小权限时，也撤销旧的。'));
  for (const token of tokens.items) {
    const expired = token.expires_at && new Date(token.expires_at).getTime() <= Date.now();
    const row = el('div', undefined, 'card');
    row.append(el('p', token.revoked_at ? '已撤销' : expired ? '已到期' : '有效连接'), el('p', token.scopes.map(s => scopeNames[s] || s).join('；')), el('p', token.expires_at ? `到期：${date(token.expires_at)}` : '持续有效'), el('code', token.token_id));
    const ownerId = spaces.find(s => s.space_id === spaceId)?.default_actor_id;
    row.append(el('p', `${ot('recipientLabel')}: ${token.default_recipient_id && token.default_recipient_id !== ownerId ? actorName(token.default_recipient_id) : ot('ownerRecipient')}`));
    if (!token.revoked_at) row.append(button('撤销这份连接', async () => {if (!confirm('撤销后，这份连接立即失效。继续？')) return; await api(root + `/tokens/${token.token_id}`, 'DELETE', {}); if (spaceId === sid) await manageConnections(actor);}));
    body.append(row);
  }
  if (tokens.has_more) body.append(el('p', `这里只显示最近 ${pageSize} 份连接，完整记录见「管理 → 连接凭据」。`));
  const form = el('form', undefined, 'connection-form'); form.append(el('h3', '签发新连接'));
  const consumerLabel = el('label', '使用哪个客户端身份'), consumer = el('select');
  for (const c of consumers.filter(c => c.actor_id === actor.actor_id && !c.disabled)) {const option = el('option', c.consumer_id); option.value = c.consumer_id; consumer.append(option);}
  consumer.required = true; consumerLabel.append(consumer); form.append(consumerLabel);
  const scopeInputs = [], scopes = el('fieldset'), choices = el('div', undefined, 'checks'); scopes.append(el('legend', '允许做什么'));
  for (const [scope, name] of Object.entries(scopeNames)) {const label = el('label', name), check = el('input'); check.type = 'checkbox'; check.checked = scope === 'message.send' || (preset === 'primary' && ['message.read', 'receipt.write'].includes(scope)); label.prepend(check); choices.append(label); scopeInputs.push([scope, check]);} scopes.append(choices); form.append(scopes);
  function resourceChoices(title, resources, key) {
    if (!resources.length) return [];
    const field = el('fieldset'), list = el('div', undefined, 'checks'), inputs = []; field.append(el('legend', title));
    for (const r of resources) {const label = el('label', r.display_name), check = el('input'); check.type = 'checkbox'; label.prepend(check); list.append(label); inputs.push([r[key], check]);}
    field.append(list); form.append(field); return inputs;
  }
  const recipients = resourceChoices('额外收件人（未选时只向你发信）', actors.filter(a => a.kind === 'agent' && !a.disabled && a.actor_id !== actor.actor_id), 'actor_id');
  const defaultLabel = el('label', ot('recipientLabel')), defaultRecipient = el('select');
  const ownerOption = el('option', ot('ownerRecipient')); ownerOption.value = ''; defaultRecipient.append(ownerOption);
  for (const [id, check] of recipients) {const option = el('option', actorName(id)); option.value = id; defaultRecipient.append(option);}
  defaultRecipient.addEventListener('change', () => {const entry = recipients.find(([id]) => id === defaultRecipient.value); if (entry) entry[1].checked = true;});
  defaultLabel.append(defaultRecipient); form.append(defaultLabel);

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
      if (defaultRecipient.value && !recipientIds.includes(defaultRecipient.value)) throw new Error('默认收件人必须在授权范围内。');
      if (recipientIds.length && !selected.includes('message.send')) throw new Error('选择额外收件人需要发信能力。');
      if (read.some(([, i]) => i.checked) && !selected.includes('memory.read')) throw new Error('选择可读集合需要资料读取能力。');
      if (write.some(([, i]) => i.checked) && !selected.includes('memory.write')) throw new Error('选择可写集合需要资料更新能力。');
      task = {type: 'connection', name: actor.display_name, sid: spaceId, actor, consumer: {consumer_id: consumer.value}, prefix: crypto.randomUUID(), scopes: selected, grants: {recipient_ids: recipientIds.length ? [spaces.find(s => s.space_id === spaceId).default_actor_id, ...recipientIds] : [], memory_read_collection_ids: read.filter(([, i]) => i.checked).map(([id]) => id), memory_write_collection_ids: write.filter(([, i]) => i.checked).map(([id]) => id)}, default_recipient_id: defaultRecipient.value || null, create_collection: own.checked, expires_at: Number(ttl.value) ? new Date(Date.now() + Number(ttl.value) * 1000).toISOString() : null}; saveCreation(task);
    }
    if (task.create_collection && !task.collection) {task.collection = await api(root + '/collections', 'POST', {request_id: task.prefix + '-collection', source_actor_id: actor.actor_id, display_name: actor.display_name + ' · 资料', scope: 'personal/default'}); saveCreation(task);}
    await issueConnection(task);
  });});
  body.append(form); openDetail(`${actor.display_name} · 连接与权限`, body);
}
async function inspectItems(kind, spaceId) {
  const items = []; let next = null;
  do {const page = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind, limit: pageSize, cursor: next}); items.push(...page.items); next = page.has_more ? page.next_cursor : null;} while (next);
  return items;
}
$('#add-agent').addEventListener('click', () => busy($('#add-agent'), () => openOnboarding()));
startOnboardingSession();

$('#owner-credential').addEventListener('click', () => busy($('#owner-credential'), async () => {
  const epoch = authEpoch, spaceId = sid;
  if (!sid) throw new Error('先创建或选择一个用户空间。');
  const path = `/admin/v0/spaces/${sid}/owner-credential`;
  const current = await api(path);
  if (epoch !== authEpoch || spaceId !== sid) return;
  const form = el('form'), name = spaces.find(s => s.space_id === spaceId)?.display_name || spaceId;
  form.append(el('h3', name), el('p', ot('ownerPurpose')), el('p', current.active ? ot('ownerResetEffect') : current.version ? ot('ownerInactiveEffect') : ot('ownerFirstEffect')));
  const submit = el('button', current.active ? ot('ownerReset') : current.version ? ot('ownerRestore') : ot('ownerCreate')); submit.type = 'submit'; form.append(submit);
  form.addEventListener('submit', event => {event.preventDefault(); busy(submit, async () => {
    if (epoch !== authEpoch || spaceId !== sid) return;
    const result = await api(path, 'POST', {expected_version: current.version});
    if (epoch !== authEpoch || spaceId !== sid) return;
    openDetail(ot('ownerResult'), el('p', name), el('p', ot('ownerSave')), el('pre', result.credential), el('p', `${ot('loginAddress')}: ${location.origin}/dashboard`));
  });});
  openDetail(ot('ownerTitle'), form);
}));

async function choosePrimary(actor) {
  const spaceId = sid, revision = primaryState.revision;
  const form = el('div');
  form.append(el('p', ot('primaryIntro')), el('p', actor.display_name), actionButton('confirm-primary', ot('confirmPrimary'), async () => {
    await api(`/admin/v0/spaces/${spaceId}/primary-assistant`, 'PUT', {expected_revision: revision, actor_id: actor.actor_id});
    if (spaceId !== sid) return;
    await load();
    await manageConnections(actor, 'primary');
  }, ''));
  openDetail(ot('choosePrimary'), form);
}

function renderAccessRequests(items) {
  if (!items.length) {$('#content').append(el('div', '还没有邀请请求。', 'empty')); return;}
  for (const item of items) {
    const card = el('article', undefined, 'card');
    card.append(el('h2', item.email), el('p', date(item.created_at)), actionButton('approve-access', item.status === 'issued' ? ot('issued') : ot('approve'), () => openInvitation(item)));
    $('#content').append(card);
  }
}

function openInvitation(access = null) {
  const pending = pendingCreation();
  if (pending) {if (pending.type !== 'invite') throw new Error('请先恢复其他尚未完成的创建。'); return createInvitation(pending.name);}
  const form = el('form'), label = el('label', ot('inviteName')), input = el('input'), submit = el('button', ot('inviteCreate'));
  input.required = true; input.value = '个人空间'; label.append(input); submit.type = 'submit';
  form.append(el('p', ot('invitePurpose')), label, el('p', ot('invitePrivate')), submit);
  form.addEventListener('submit', event => {event.preventDefault(); if (input.value.trim()) busy(submit, () => createInvitation(input.value.trim(), access));});
  openDetail(ot('inviteTitle'), form);
}

async function createInvitation(name, access = null) {
  const epoch = authEpoch;
  let task = pendingCreation();
  if (!task) {task = {type: 'invite', name, prefix: crypto.randomUUID(), access_request_id: access?.request_id || null, sid: access?.space_id || null}; saveCreation(task);}
  if (!task.sid) {const space = await api('/admin/v0/spaces', 'POST', {request_id: task.prefix + '-space', display_name: task.name}); if (epoch !== authEpoch) return; task.sid = space.space_id; saveCreation(task);}
  let result;
  while (!result?.invite_token) {
    if (task.revoke_invitation_id) {await api(`/admin/v0/spaces/${task.sid}/invitations/${task.revoke_invitation_id}`, 'DELETE', {}); if (epoch !== authEpoch) return; delete task.revoke_invitation_id; task.invite_request_id = crypto.randomUUID(); saveCreation(task);}
    if (!task.invite_request_id) task.invite_request_id = task.prefix + '-invite';
    saveCreation(task);
    result = await api(`/admin/v0/spaces/${task.sid}/invitations`, 'POST', {request_id: task.invite_request_id, ...(task.access_request_id ? {access_request_id: task.access_request_id} : {})});
    if (epoch !== authEpoch) return;
    if (!result.invite_token) {task.revoke_invitation_id = result.invitation_id; saveCreation(task);}
  }
  finishCreation();
  openDetail(ot('inviteTitle'), el('p', ot('invitePrivate')), el('pre', result.invite_url), el('p', date(result.expires_at)), button(ot('revokeInvite'), async () => {await api(`/admin/v0/spaces/${task.sid}/invitations/${result.invitation_id}`, 'DELETE', {}); if (epoch !== authEpoch) return; openDetail(ot('inviteTitle'), el('p', ot('inviteRevoked')));}));
  if (tab === 'access') await load();
}

$('#invite-user').addEventListener('click', () => busy($('#invite-user'), () => openInvitation()));
$('#request-access').addEventListener('click', () => {
  const form = el('form'), label = el('label', ot('email')), input = el('input'), submit = el('button', ot('requestButton'));
  input.type = 'email'; input.required = true; input.autocomplete = 'email'; label.append(input); submit.type = 'submit';
  const requestId = crypto.randomUUID();
  form.append(el('p', ot('requestHelp')), label, submit);
  form.addEventListener('submit', event => {event.preventDefault(); busy(submit, async () => {
    await api('/admin/access-request', 'POST', {request_id: requestId, email: input.value});
    openDetail(ot('requestTitle'), el('p', ot('requestAccepted')));
  });});
  openDetail(ot('requestTitle'), form);
});

async function startOnboardingSession() {
  const parameters = new URLSearchParams(location.hash.slice(1)), invitation = parameters.get('invite');
  if (!invitation) {const requesting = parameters.has('request-access'); if (requesting) history.replaceState(null, '', location.pathname + location.search); try {await entered(await api('/admin/session'));} catch {showLogin(); if (requesting) $('#request-access').click();} return;}
  history.replaceState(null, '', location.pathname + location.search);
  let session;
  try {
    session = await api('/admin/invitation', 'POST', {invite_token: invitation});
  } catch (error) {showLogin(); notice(error.message); return;}
  const loading = entered(session), epoch = authEpoch;
  openDetail(ot('inviteCredential'), el('p', ot('inviteJoined')), el('pre', session.credential));
  try {await loading;} catch (error) {if (epoch === authEpoch) notice(error.message);}
}
