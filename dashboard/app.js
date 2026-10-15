'use strict';
const $ = selector => document.querySelector(selector);
let csrf = '', spaces = [], actors = [], sid = '', tab = 'inbox', cursor = null, loadEpoch = 0, authEpoch = 0, pageSize = 20, operator = false, primaryState = {revision: 0, primary_assistant: null};
let reportHistory = [];
let inboxHead = null, updateTimer = null, checkingUpdates = false;
function stopUpdates() {clearTimeout(updateTimer); updateTimer = null; $('#new-messages').hidden = true; $('#sync-status').hidden = true;}
function scheduleUpdates() {clearTimeout(updateTimer); updateTimer = setTimeout(async () => {await checkForUpdates(); if (!$('#app').hidden) scheduleUpdates();}, 15000);}
async function checkForUpdates() {
  if (checkingUpdates || $('#app').hidden || tab !== 'inbox' || !sid || document.visibilityState === 'hidden') return;
  const spaceId = sid, epoch = loadEpoch;
  checkingUpdates = true;
  try {
    const result = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind:'inbox',limit:1});
    if (spaceId !== sid || epoch !== loadEpoch || tab !== 'inbox') return;
    $('#sync-status').hidden = true;
    if (result.items[0]?.message_id && result.items[0].message_id !== inboxHead) $('#new-messages').hidden = false;
  } catch {
    if (spaceId === sid && epoch === loadEpoch && !$('#app').hidden) {$('#sync-status').textContent = t('err.syncFailed'); $('#sync-status').hidden = false;}
  } finally {checkingUpdates = false;}
}
const t = (key, params) => window.WaypostI18n.t(key, params);
// Each tab names itself and says what it holds. One table, so the wording
// cannot drift between the sidebar, the page header and the empty state.
const sections = {
  inbox:       {title: t('tab.inbox.title'),     note: t('tab.inbox.note')},
  agents:      {title: t('tab.agents.title'),     note: t('tab.agents.note')},
  collections: {title: t('tab.collections.title'),     note: t('tab.collections.note')},
  messages:    {title: t('tab.messages.title'), note: t('tab.messages.note')},
  feed:        {title: t('tab.feed.title'), note: t('tab.feed.note')},
  privacy:     {title: t('tab.privacy.title'), note: t('tab.privacy.note')},
  tokens:      {title: t('tab.tokens.title'), note: t('tab.tokens.note')},
  submissions: {title: t('tab.submissions.title'), note: t('tab.submissions.note')},
  receipts:    {title: t('tab.receipts.title'), note: t('tab.receipts.note')},
  access:      {title: t('accessTitle'), note: t('accessHelp')},
};
function el(tag, text, cls) {const node = document.createElement(tag); if (text !== undefined) node.textContent = text; if (cls) node.className = cls; return node;}
function button(text, action, cls = 'secondary') {const node = el('button', text, cls); node.type = 'button'; node.addEventListener('click', () => busy(node, action)); return node;}
function actionButton(key, text, action, cls = 'secondary') {const node = button(text, action, cls); node.dataset.action = key; return node;}
function notice(message) {$('#notice').textContent = message; $('#notice').hidden = !message; $('#login-error').textContent = message; $('#detail-error').textContent = message; $('#detail-error').hidden = !message;}
async function busy(node, action) {node.disabled = true; try {notice(''); await action();} catch (error) {notice(error.message);} finally {node.disabled = node.getAttribute('aria-disabled') === 'true';}}
async function api(path, method = 'GET', body) {
  const generation = authEpoch;
  const response = await fetch(path, {method, credentials: 'same-origin', headers: {'content-type': 'application/json', 'x-waypost-csrf': csrf}, body: body === undefined ? undefined : JSON.stringify(body)});
  const result = await response.json();
  if (generation !== authEpoch) throw new Error(t('err.session'));
  if (!response.ok || !result.ok) {if (response.status === 401) showLogin(); throw Object.assign(new Error(result.error?.message || `HTTP ${response.status}`), {code:result.error?.code,status:response.status,details:result.error?.details});}
  return result.data;
}
async function readBytes(path) {
  const generation = authEpoch;
  const response = await fetch(path, {credentials:'same-origin'});
  if (generation !== authEpoch) throw new Error(t('err.sessionShort'));
  if (!response.ok) {if (response.status === 401) showLogin(); throw new Error(t('err.readBlob'));}
  const bytes = new Uint8Array(await response.arrayBuffer());
  if (generation !== authEpoch) throw new Error(t('err.sessionShort'));
  return bytes;
}
function showLogin() {authEpoch++; loadEpoch++; stopUpdates(); reportHistory = []; inboxHead = null; primaryState = {revision: 0, primary_assistant: null}; operator = false; csrf = ''; spaces = []; actors = []; sid = ''; tab = 'inbox'; finishCreation(); $('#detail').close(); $('#detail-title').textContent = ''; $('#app').hidden = true; $('#login').hidden = false; $('#content').replaceChildren(); $('#detail-body').replaceChildren();}
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
  if (!append) {$('#content').replaceChildren(); cursor = null; $('#more').hidden = true; $('#new-messages').hidden = true;}
  if (tab === 'access') {const result = await api('/admin/v0/access-requests', 'POST', {limit: pageSize, cursor}); if (epoch !== loadEpoch) return; cursor = result.next_cursor; renderAccessRequests(result.items); $('#more').hidden = !result.has_more; return;}
  if (!sid) {$('#content').append(el('div', t('empty.noSpace'), 'empty')); return;}
  const loadedActors = await registry('actors', spaceId);
  if (epoch !== loadEpoch) return;
  actors = loadedActors;
  const config = await api(`/admin/v0/spaces/${spaceId}/primary-assistant`);
  if (epoch !== loadEpoch) return;
  primaryState = config;
  if (tab === 'agents') {
    const history = await api(`/admin/v0/spaces/${spaceId}/connection-status`);
    if (epoch !== loadEpoch) return;
    reportHistory = history.actors || [];
    renderAgents(); return;
  }
  if (tab === 'privacy') {await renderPrivacy(); return;}
  const result = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind: currentTab, limit: pageSize, cursor});
  if (epoch !== loadEpoch) return;
  if (currentTab === 'inbox' && !append) {inboxHead = result.items[0]?.message_id || null; scheduleUpdates();}
  cursor = result.next_cursor;
  if (!result.items.length && !append) {
    const empty = el('div', undefined, 'empty');
    const primary = primaryState.primary_assistant;
    empty.append(el('h2', tab === 'inbox' ? (primary ? t(primary.disabled ? 'primaryStopped' : 'waitingPrimary') : t('empty.inboxH')) : t('empty.otherH')), el('p', tab === 'inbox' ? (primary ? `${primary.display_name} · ${t(primary.disabled ? 'primaryStoppedHelp' : 'waitingPrimaryHelp')}` : t('empty.inboxP')) : t('empty.otherP')));
    if (tab === 'inbox') empty.append(primary ? button(t('viewAssistants'), async () => {tab = 'agents'; await load();}, '') : button(t('ob.onboardTitle'), () => openOnboarding(), ''));
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
    const attachments = el('div', undefined, 'message-attachments'); attachments.append(el('h3', t('msg.attachments')));
    for (const id of message.attachment_ids) attachments.append(button(t('msg.viewAttachment'), async () => {
      const content = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind: 'blob_content', resource_id: id});
      if (spaceId === sid && epoch === loadEpoch) showDetail(t('msg.attachmentContent'), content.text === null ? content : content.text);
    }));
    body.append(attachments);
  }
  const details = el('details', undefined, 'technical'); details.append(el('summary', t('msg.source')), el('pre', JSON.stringify({message_id: message.message_id, kind: message.kind, source: message.source, metadata: message.metadata, attachment_ids: message.attachment_ids}, null, 2))); body.append(details);
  openDetail(message.subject, body);
}
function renderAgents() {
  const guide = el('div', undefined, 'role-guide');
  const primary = primaryState.primary_assistant;
  guide.append(el('h2', primary ? `${t(primary.disabled ? 'primaryStopped' : 'primary')}: ${primary.display_name}` : t('primaryStart')), el('p', primary?.disabled ? t('primaryStoppedHelp') : t('roleHelp')), el('p', t('rolePermissions'), 'fine'));
  $('#content').append(guide);
  const grid = el('div', undefined, 'grid'), other = el('div', undefined, 'grid');
  for (const actor of actors) {
    const card = el('article', undefined, 'card agent');
    const avatar = el('div', actor.display_name.slice(0, 1), 'avatar'); card.append(avatar, el('h2', actor.display_name));
    card.append(el('span', actor.disabled ? t('agent.suspended') : actor.kind === 'agent' ? (primaryState.primary_assistant?.actor_id === actor.actor_id ? t('primary') : t('connected')) : actor.kind === 'user' ? t('agent.user') : t('agent.client'), `pill${actor.disabled ? ' disabled' : ''}`));
    card.append(el('p', actor.disabled ? t('disabledCardHelp') : actor.kind === 'agent' && primaryState.primary_assistant?.actor_id === actor.actor_id ? t('primaryCardHelp') : t('connectedCardHelp'), 'fine'));
    if (actor.kind === 'agent') {
      const history = reportHistory.find(item => item.actor_id === actor.actor_id);
      const status = el('p', history?.report_count ? t('agent.deliveryHistory', {count: history.report_count, date: date(history.last_report_at)}) : t('agent.waitingFirst'), 'connection-status');
      status.dataset.connectionStatus = actor.actor_id; card.append(status);
      if (history?.report_count) card.append(el('p', t('agent.historyNote'), 'fine'));
    }
    const actions = el('div', undefined, 'row');
    const spaceId = sid, epoch = loadEpoch;
    actions.append(button(t('agent.recentDeliveries'), async () => {
      const result = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind: 'messages', actor_id: actor.actor_id, limit: pageSize});
      if (spaceId !== sid || epoch !== loadEpoch) return;
      const list = el('section');
      if (result.items.length) renderMessages(result.items, list); else list.append(el('p', t('agent.noRecent')));
      openDetail(t('agent.recentTitle', {name: actor.display_name}), list);
    }));
    if (!actor.disabled && actor.kind === 'agent') {actions.append(actionButton('manage-connections', t('agent.connections'), () => manageConnections(actor))); if (primaryState.primary_assistant?.actor_id !== actor.actor_id) actions.append(actionButton('choose-primary', t('choosePrimary'), () => choosePrimary(actor))); else actions.append(actionButton('configure-primary', t('configurePrimary'), () => manageConnections(actor, 'primary')));}
    if (!actor.disabled && actor.kind === 'agent') actions.append(button(t('agent.suspend'), async () => {
      if (!confirm(t('agent.confirmSuspend', {name: actor.display_name}))) return;
      await api(`/admin/v0/spaces/${spaceId}/actors/${actor.actor_id}/disable`, 'POST', {}); if (spaceId === sid) await load();
    }));
    const advanced = el('details', undefined, 'technical'); advanced.append(el('summary', t('agent.identity')), el('code', actor.actor_id), button(t('agent.viewActivity'), async () => {
      const result = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind: 'feed', actor_id: actor.actor_id, limit: pageSize});
      if (spaceId === sid && epoch === loadEpoch) showDetail(t('agent.activityTitle', {name: actor.display_name}), result.items);
    }));
    card.append(actions, advanced); (actor.kind === 'agent' ? grid : other).append(card);
  }
  if (!grid.children.length) grid.append(el('div', t('agent.noAgents'), 'empty'));
  $('#content').append(grid);
  if (other.children.length) {const details = el('details', undefined, 'technical'); details.append(el('summary', t('agent.userClients')), other); $('#content').append(details);}
}
function renderTable(items) {
  const spaceId = sid, epoch = loadEpoch;
  const wrap = el('div', undefined, 'table-wrap'), table = el('table'), head = el('tr');
  const columns = {
    messages: [['subject', t('col.subject')], ['kind', t('col.kind')], ['producer_id', t('col.from')], ['committed_at', t('col.time')]],
    feed: [['item_type', t('col.behavior')], ['producer_id', t('col.party')], ['resource_version', t('col.version')], ['committed_at', t('col.time')]],
    collections: [['display_name', t('col.collection')], ['scope', t('col.scope')], ['source_actor_id', t('col.source')], ['current_revision', t('col.current')]],
    tokens: [['token_id', t('col.credId')], ['actor_id', t('col.party')], ['consumer_id', 'Consumer'], ['revoked_at', t('col.revokedAt')]],
    submissions: [['operation', t('col.op')], ['state', t('col.state')], ['producer_id', t('col.party')], ['request_id', t('col.reqId')]],
    receipts: [['consumer_id', 'Consumer'], ['subject_item_id', t('col.record')], ['stage', t('col.stage')], ['version', t('col.version')]],
  }[tab];
  for (const [, label] of columns) head.append(el('th', label)); head.append(el('th', t('col.check'))); table.append(head);
  for (const item of items) {
    const row = el('tr');
    for (const [field] of columns) {
      let value = item[field];
      if (field.endsWith('_at')) value = date(value);
      else if (['producer_id', 'source_actor_id', 'actor_id'].includes(field)) value = actorName(value);
      else if (field === 'stage') value = value === 2 ? 'processed' : 'received';
      row.append(el('td', value === null || value === undefined ? '—' : String(value)));
    }
    const action = el('td'); action.append(button(t('col.details'), async () => {
      if (tab === 'collections') {
        const generation = authEpoch;
        await window.WaypostMemory.open({spaceId,collection:item,sourceName:actorName(item.source_actor_id),api,readBytes,el,button,openDetail,run:busy,active:()=>spaceId===sid&&generation===authEpoch});
        return;
      } else showDetail(sections[tab].title, item);
    }));
    if (tab === 'tokens' && !item.revoked_at) action.append(button(t('token.revoke'), async () => {await api(`/admin/v0/spaces/${sid}/tokens/${item.token_id}`, 'DELETE', {}); await load();}));
    row.append(action); table.append(row);
  }
  wrap.append(table); $('#content').append(wrap);
}
async function renderPrivacy() {
  const config = await api(`/admin/v0/spaces/${sid}/privacy`);
  const form = el('form', undefined, 'card privacy-form');
  form.append(el('h2', t('priv.title', {rev: config.revision})), el('p', t('priv.note')));
  const checks = el('div', undefined, 'checks');
  function check(text, checked) {const label = el('label'), input = el('input'); input.type = 'checkbox'; input.checked = checked; label.append(input, document.createTextNode(text)); checks.append(label); return input;}
  const enabled = check(t('priv.enable'), config.enabled), all = check(t('priv.all'), config.all_agents), binary = check(t('priv.binary'), config.allow_uninspected_attachments);
  form.append(checks);
  const modeLabel = el('label', t('priv.onDetect')), mode = el('select');
  for (const [value, text] of [['redact', t('priv.redact')], ['block', t('priv.block')]]) {const option = el('option', text); option.value = value; mode.append(option);} mode.value = config.mode; modeLabel.append(mode); form.append(modeLabel);
  const targets = el('div', undefined, 'checks'); const targetInputs = [];
  form.append(el('h2', t('priv.agents')));
  for (const actor of actors.filter(a => a.kind === 'agent')) {const label = el('label', actor.display_name), input = el('input'); input.type = 'checkbox'; input.checked = config.actor_ids.includes(actor.actor_id); label.prepend(input); targets.append(label); targetInputs.push([actor.actor_id, input]);} form.append(targets);
  const detectors = el('div', undefined, 'checks'), detectorInputs = [];
  for (const [kind, text] of [['credential', t('priv.detCred')], ['email', t('priv.detEmail')], ['phone', t('priv.detPhone')], ['identity_number', t('priv.detId')]]) {const label = el('label', text), input = el('input'); input.type = 'checkbox'; input.checked = config.rules.some(r => r.kind === kind); label.prepend(input); detectors.append(label); detectorInputs.push([kind, input]);}
  form.append(el('h2', t('priv.rules')), detectors);
  const customInputs = [];
  for (const [kind, text] of [['literal', t('priv.customLiteral')], ['regex', t('priv.customRegex')], ['metadata_key', t('priv.customMeta')]]) {const label = el('label', text), input = el('textarea'); input.value = config.rules.filter(r => r.kind === kind).map(r => r.value).join('\n'); label.append(input); form.append(label); customInputs.push([kind, input]);}
  form.append(el('p', t('priv.limits'), 'fine'));
  const save = el('button', t('priv.save')); save.type = 'submit'; form.append(save);
  form.addEventListener('submit', event => {event.preventDefault(); busy(save, async () => {
    const rules = detectorInputs.filter(([, i]) => i.checked).map(([kind]) => ({kind}));
    for (const [kind, input] of customInputs) for (const value of input.value.split('\n').filter(v => v.length)) rules.push({kind, value});
    await api(`/admin/v0/spaces/${sid}/privacy`, 'PUT', {expected_revision: config.revision, enabled: enabled.checked, mode: mode.value, all_agents: all.checked, actor_ids: targetInputs.filter(([, i]) => i.checked).map(([id]) => id), allow_uninspected_attachments: binary.checked, rules}); await load();
  });});
  $('#content').append(form);
  const preview = el('div', undefined, 'card'), input = el('textarea'); input.placeholder = t('priv.previewPh'); input.setAttribute('aria-label', t('priv.previewAria'));
  preview.append(el('h2', t('priv.previewH')), input, button(t('priv.previewBtn'), async () => {showDetail(t('priv.previewTitle'), await api(`/admin/v0/spaces/${sid}/privacy/preview`, 'POST', {text: input.value}));}), button(t('priv.auditBtn'), async () => {showDetail(t('priv.auditTitle'), await api(`/admin/v0/spaces/${sid}/privacy/audit`));})); $('#content').append(preview);
}
$('#login-form').addEventListener('submit', event => {event.preventDefault(); const submit = event.currentTarget.querySelector('button'); busy(submit, async () => {const session = await api('/admin/session', 'POST', {admin_secret: $('#secret').value}); $('#secret').value = ''; await entered(session);});});
$('#space').addEventListener('change', () => {sid = $('#space').value; $('#detail').close(); $('#detail-body').replaceChildren(); busy($('#refresh'), () => load());});
$('#refresh').addEventListener('click', () => busy($('#refresh'), () => load()));
$('#new-messages').addEventListener('click', () => busy($('#new-messages'), () => load()));
document.addEventListener('visibilitychange', () => {if (document.visibilityState !== 'hidden') checkForUpdates();});
window.addEventListener('focus', () => checkForUpdates());
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
  if (task && task.type !== 'space') throw new Error(t('ob.resumeSpace'));
  if (!task) {const name = prompt(t('ob.spaceName')); if (!name?.trim()) return; task = {type: 'space', name, prefix: crypto.randomUUID()}; saveCreation(task);}
  const space = await api('/admin/v0/spaces', 'POST', {request_id: task.prefix, display_name: task.name});
  sid = space.space_id; finishCreation(); await loadSpaces();
}));
const scopeNames = {'message.send': t('scope.send'), 'message.read': t('scope.read'), 'receipt.write': t('scope.receipt'), 'blob.write': t('scope.blob'), 'memory.write': t('scope.memWrite'), 'memory.read': t('scope.memRead')};
function openOnboarding() {
  if (!sid) throw new Error(t('selectSpace'));
  const task = pendingCreation();
  if (task) {
    if (task.type !== 'agent') throw new Error(t('ob.resume'));
    return createAgent(task.name);
  }
  const form = el('form'), label = el('label', t('ob.nameLabel')), input = el('input');
  input.required = true; input.maxLength = 100; input.placeholder = t('ob.namePh'); label.append(input);
  const recipientLabel = el('label', t('recipientLabel')), recipient = el('select');
  const ownOption = el('option', t('ownerRecipient')); ownOption.value = ''; recipient.append(ownOption);
  if (primaryState.primary_assistant && !primaryState.primary_assistant.disabled) {const option = el('option', primaryState.primary_assistant.display_name); option.value = primaryState.primary_assistant.actor_id; recipient.append(option); recipient.value = option.value;}
  recipientLabel.append(recipient);
  const submit = el('button', t('ob.generate')); submit.type = 'submit';
  form.append(label, recipientLabel, el('p', t('ob.onboardNoteShort')), submit);
  form.addEventListener('submit', event => {event.preventDefault(); if (input.value.trim()) busy(submit, () => createAgent(input.value.trim(), recipient.value || null));});
  openDetail(t('ob.onboardTitle'), form); input.focus();
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
  body.append(el('p', t('ob.saveNote')), el('pre', JSON.stringify(info, null, 2)), el('h3', t('ob.allows')), el('p', task.scopes.map(s => scopeNames[s] || s).join('；')), el('p', task.expires_at ? t('ob.expiresAt', {date: date(task.expires_at)}) : t('ob.forever')), el('p', t('ob.onboarded')));
  openDetail(t('ob.connTitle'), body); sid = task.sid; finishCreation(); await loadSpaces();
}
async function createAgent(name, defaultRecipient = null) {
  let task = pendingCreation();
  if (task && task.type !== 'agent') throw new Error(t('ob.resumeAgent'));
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
  body.append(el('p', t('ob.connNote')));
  for (const token of tokens.items) {
    const expired = token.expires_at && new Date(token.expires_at).getTime() <= Date.now();
    const row = el('div', undefined, 'card');
    row.append(el('p', token.revoked_at ? t('token.revoked') : expired ? t('token.expired') : t('token.valid')), el('p', token.scopes.map(s => scopeNames[s] || s).join('；')), el('p', token.expires_at ? t('token.expiresAt', {date: date(token.expires_at)}) : t('token.forever')), el('code', token.token_id));
    const ownerId = spaces.find(s => s.space_id === spaceId)?.default_actor_id;
    row.append(el('p', `${t('recipientLabel')}: ${token.default_recipient_id && token.default_recipient_id !== ownerId ? actorName(token.default_recipient_id) : t('ownerRecipient')}`));
    if (!token.revoked_at) row.append(button(t('token.revokeBtn'), async () => {if (!confirm(t('token.confirmRevoke'))) return; await api(root + `/tokens/${token.token_id}`, 'DELETE', {}); if (spaceId === sid) await manageConnections(actor);}));
    body.append(row);
  }
  if (tokens.has_more) body.append(el('p', t('ob.onlySome', {n: pageSize})));
  const form = el('form', undefined, 'connection-form'); form.append(el('h3', t('ob.issueNew')));
  const consumerLabel = el('label', t('ob.consumer')), consumer = el('select');
  for (const c of consumers.filter(c => c.actor_id === actor.actor_id && !c.disabled)) {const option = el('option', c.consumer_id); option.value = c.consumer_id; consumer.append(option);}
  consumer.required = true; consumerLabel.append(consumer); form.append(consumerLabel);
  const scopeInputs = [], scopes = el('fieldset'), choices = el('div', undefined, 'checks'); scopes.append(el('legend', t('ob.allow')));
  for (const [scope, name] of Object.entries(scopeNames)) {const label = el('label', name), check = el('input'); check.type = 'checkbox'; check.checked = scope === 'message.send' || (preset === 'primary' && ['message.read', 'receipt.write'].includes(scope)); label.prepend(check); choices.append(label); scopeInputs.push([scope, check]);} scopes.append(choices); form.append(scopes);
  function resourceChoices(title, resources, key) {
    if (!resources.length) return [];
    const field = el('fieldset'), list = el('div', undefined, 'checks'), inputs = []; field.append(el('legend', title));
    for (const r of resources) {const label = el('label', r.display_name), check = el('input'); check.type = 'checkbox'; label.prepend(check); list.append(label); inputs.push([r[key], check]);}
    field.append(list); form.append(field); return inputs;
  }
  const recipients = resourceChoices(t('ob.recipients'), actors.filter(a => a.kind === 'agent' && !a.disabled && a.actor_id !== actor.actor_id), 'actor_id');
  const defaultLabel = el('label', t('recipientLabel')), defaultRecipient = el('select');
  const ownerOption = el('option', t('ownerRecipient')); ownerOption.value = ''; defaultRecipient.append(ownerOption);
  for (const [id, check] of recipients) {const option = el('option', actorName(id)); option.value = id; defaultRecipient.append(option);}
  defaultRecipient.addEventListener('change', () => {const entry = recipients.find(([id]) => id === defaultRecipient.value); if (entry) entry[1].checked = true;});
  defaultLabel.append(defaultRecipient); form.append(defaultLabel);

  const read = resourceChoices(t('ob.read'), collections.filter(c => c.source_actor_id !== actor.actor_id), 'collection_id');
  const write = resourceChoices(t('ob.write'), collections.filter(c => c.source_actor_id !== actor.actor_id), 'collection_id');
  const ownLabel = el('label', t('ob.own')), own = el('input'); own.type = 'checkbox'; ownLabel.prepend(own); form.append(ownLabel, el('p', t('ob.ownNote')));
  const ttlLabel = el('label', t('ob.ttl')), ttl = el('select');
  for (const [value, text] of [['0', t('ob.ttlForever')], ['3600', t('ob.ttl1h')], ['86400', t('ob.ttl1d')], ['604800', t('ob.ttl7d')]]) {const option = el('option', text); option.value = value; ttl.append(option);} ttlLabel.append(ttl); form.append(ttlLabel);
  const submit = el('button', pendingCreation()?.type === 'connection' ? t('ob.resumeIssue') : t('ob.issue')); submit.type = 'submit'; form.append(submit);
  form.addEventListener('submit', event => {event.preventDefault(); busy(submit, async () => {
    let task = pendingCreation();
    if (task && (task.type !== 'connection' || task.actor.actor_id !== actor.actor_id || task.sid !== spaceId)) throw new Error(t('ob.resumeOthers'));
    if (!task) {
      if (sid !== spaceId || epoch !== loadEpoch) throw new Error(t('ob.stale'));
      const selected = scopeInputs.filter(([, i]) => i.checked).map(([scope]) => scope);
      if (!selected.length) throw new Error(t('ob.pickScope'));
      if (own.checked && !selected.some(s => s.startsWith('memory.'))) throw new Error(t('ob.needMemory'));
      const recipientIds = recipients.filter(([, i]) => i.checked).map(([id]) => id);
      if (defaultRecipient.value && !recipientIds.includes(defaultRecipient.value)) throw new Error(t('ob.defaultScope'));
      if (recipientIds.length && !selected.includes('message.send')) throw new Error(t('ob.needSend'));
      if (read.some(([, i]) => i.checked) && !selected.includes('memory.read')) throw new Error(t('ob.needRead'));
      if (write.some(([, i]) => i.checked) && !selected.includes('memory.write')) throw new Error(t('ob.needWrite'));
      task = {type: 'connection', name: actor.display_name, sid: spaceId, actor, consumer: {consumer_id: consumer.value}, prefix: crypto.randomUUID(), scopes: selected, grants: {recipient_ids: recipientIds.length ? [spaces.find(s => s.space_id === spaceId).default_actor_id, ...recipientIds] : [], memory_read_collection_ids: read.filter(([, i]) => i.checked).map(([id]) => id), memory_write_collection_ids: write.filter(([, i]) => i.checked).map(([id]) => id)}, default_recipient_id: defaultRecipient.value || null, create_collection: own.checked, expires_at: Number(ttl.value) ? new Date(Date.now() + Number(ttl.value) * 1000).toISOString() : null}; saveCreation(task);
    }
    if (task.create_collection && !task.collection) {task.collection = await api(root + '/collections', 'POST', {request_id: task.prefix + '-collection', source_actor_id: actor.actor_id, display_name: actor.display_name + t('ob.collSuffix'), scope: 'personal/default'}); saveCreation(task);}
    await issueConnection(task);
  });});
  body.append(form); openDetail(t('ob.connFor', {name: actor.display_name}), body);
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
  if (!sid) throw new Error(t('ob.needSpace'));
  const path = `/admin/v0/spaces/${sid}/owner-credential`;
  const current = await api(path);
  if (epoch !== authEpoch || spaceId !== sid) return;
  const form = el('form'), name = spaces.find(s => s.space_id === spaceId)?.display_name || spaceId;
  form.append(el('h3', name), el('p', t('ownerPurpose')), el('p', current.active ? t('ownerResetEffect') : current.version ? t('ownerInactiveEffect') : t('ownerFirstEffect')));
  const submit = el('button', current.active ? t('ownerReset') : current.version ? t('ownerRestore') : t('ownerCreate')); submit.type = 'submit'; form.append(submit);
  form.addEventListener('submit', event => {event.preventDefault(); busy(submit, async () => {
    if (epoch !== authEpoch || spaceId !== sid) return;
    const result = await api(path, 'POST', {expected_version: current.version});
    if (epoch !== authEpoch || spaceId !== sid) return;
    openDetail(t('ownerResult'), el('p', name), el('p', t('ownerSave')), el('pre', result.credential), el('p', `${t('loginAddress')}: ${location.origin}/dashboard`));
  });});
  openDetail(t('ownerTitle'), form);
}));

async function choosePrimary(actor) {
  const spaceId = sid, revision = primaryState.revision;
  const form = el('div');
  form.append(el('p', t('primaryIntro')), el('p', actor.display_name), actionButton('confirm-primary', t('confirmPrimary'), async () => {
    await api(`/admin/v0/spaces/${spaceId}/primary-assistant`, 'PUT', {expected_revision: revision, actor_id: actor.actor_id});
    if (spaceId !== sid) return;
    await load();
    await manageConnections(actor, 'primary');
  }, ''));
  openDetail(t('choosePrimary'), form);
}

function renderAccessRequests(items) {
  if (!items.length) {$('#content').append(el('div', t('access.empty'), 'empty')); return;}
  for (const item of items) {
    const card = el('article', undefined, 'card');
    card.append(el('h2', item.email), el('p', date(item.created_at)), actionButton('approve-access', item.status === 'issued' ? t('issued') : t('approve'), () => openInvitation(item)));
    $('#content').append(card);
  }
}

function openInvitation(access = null) {
  const pending = pendingCreation();
  if (pending) {if (pending.type !== 'invite') throw new Error(t('ob.resumeOther')); return createInvitation(pending.name);}
  const form = el('form'), label = el('label', t('inviteName')), input = el('input'), submit = el('button', t('inviteCreate'));
  input.required = true; input.value = t('ob.personalSpace'); label.append(input); submit.type = 'submit';
  form.append(el('p', t('invitePurpose')), label, el('p', t('invitePrivate')), submit);
  form.addEventListener('submit', event => {event.preventDefault(); if (input.value.trim()) busy(submit, () => createInvitation(input.value.trim(), access));});
  openDetail(t('inviteTitle'), form);
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
  openDetail(t('inviteTitle'), el('p', t('invitePrivate')), el('pre', result.invite_url), el('p', date(result.expires_at)), button(t('revokeInvite'), async () => {await api(`/admin/v0/spaces/${task.sid}/invitations/${result.invitation_id}`, 'DELETE', {}); if (epoch !== authEpoch) return; openDetail(t('inviteTitle'), el('p', t('inviteRevoked')));}));
  if (tab === 'access') await load();
}

$('#invite-user').addEventListener('click', () => busy($('#invite-user'), () => openInvitation()));
$('#request-access').addEventListener('click', () => {
  const form = el('form'), label = el('label', t('email')), input = el('input'), submit = el('button', t('requestButton'));
  input.type = 'email'; input.required = true; input.autocomplete = 'email'; label.append(input); submit.type = 'submit';
  const requestId = crypto.randomUUID();
  form.append(el('p', t('requestHelp')), label, submit);
  form.addEventListener('submit', event => {event.preventDefault(); busy(submit, async () => {
    await api('/admin/access-request', 'POST', {request_id: requestId, email: input.value});
    openDetail(t('requestTitle'), el('p', t('requestAccepted')));
  });});
  openDetail(t('requestTitle'), form);
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
  openDetail(t('inviteCredential'), el('p', t('inviteJoined')), el('pre', session.credential));
  try {await loading;} catch (error) {if (epoch === authEpoch) notice(error.message);}
}
