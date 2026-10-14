'use strict';
const $ = selector => document.querySelector(selector);
const t = (key, params) => window.WaypostI18n.t(key, params);
let csrf = '', spaces = [], actors = [], sid = '', tab = 'inbox', cursor = null, loadEpoch = 0, authEpoch = 0;
// Each tab names itself and says what it holds. One table, so the wording
// cannot drift between the sidebar, the page header and the empty state.
// Values are locale keys; t() resolves them at render time so a language
// switch re-renders without reloading.
const sections = {
  inbox:       {title: 'tab.inbox.title',     note: 'tab.inbox.note'},
  agents:      {title: 'tab.agents.title',     note: 'tab.agents.note'},
  collections: {title: 'tab.collections.title',     note: 'tab.collections.note'},
  messages:    {title: 'tab.messages.title', note: 'tab.messages.note'},
  feed:        {title: 'tab.feed.title', note: 'tab.feed.note'},
  privacy:     {title: 'tab.privacy.title', note: 'tab.privacy.note'},
  tokens:      {title: 'tab.tokens.title', note: 'tab.tokens.note'},
  submissions: {title: 'tab.submissions.title', note: 'tab.submissions.note'},
  receipts:    {title: 'tab.receipts.title', note: 'tab.receipts.note'},
};
function el(tag, text, cls) {const node = document.createElement(tag); if (text !== undefined) node.textContent = text; if (cls) node.className = cls; return node;}
function button(text, action, cls = 'secondary') {const node = el('button', text, cls); node.type = 'button'; node.addEventListener('click', () => busy(node, action)); return node;}
function notice(message) {$('#notice').textContent = message; $('#notice').hidden = !message; $('#login-error').textContent = message; $('#detail-error').textContent = message; $('#detail-error').hidden = !message;}
async function busy(node, action) {node.disabled = true; try {notice(''); await action();} catch (error) {notice(error.message);} finally {node.disabled = false;}}
async function api(path, method = 'GET', body) {
  const generation = authEpoch;
  const response = await fetch(path, {method, credentials: 'same-origin', headers: {'content-type': 'application/json', 'x-waypost-csrf': csrf}, body: body === undefined ? undefined : JSON.stringify(body)});
  const result = await response.json();
  if (generation !== authEpoch) throw new Error(t('err.session'));
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
  $('#title').textContent = t(sections[tab].title);
  document.querySelectorAll('nav button').forEach(b => b.classList.toggle('active', b.dataset.tab === tab));
  $('#add-agent').hidden = !['agents', 'inbox'].includes(tab) || !sid;
  $('#subtitle').textContent = t(sections[tab].note);
  if (!append) {$('#content').replaceChildren(); cursor = null; $('#more').hidden = true;}
  if (!sid) {$('#content').append(el('div', t('empty.noSpace'), 'empty')); return;}
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
    empty.append(el('h2', tab === 'inbox' ? t('empty.inboxH') : t('empty.otherH')), el('p', tab === 'inbox' ? t('empty.inboxP') : t('empty.otherP')));
    if (tab === 'inbox') empty.append(button(t('ob.onboardTitle'), () => openOnboarding(), ''));
    $('#content').append(empty);
  } else if (result.items.length) {
    if (['inbox', 'messages'].includes(tab)) renderMessages(result.items); else renderTable(result.items);
  }
  $('#more').hidden = !result.has_more;
}
function actorName(id) {return actors.find(a => a.actor_id === id)?.display_name || id || '—';}
function date(value) {return value ? new Date(value).toLocaleString(window.WaypostI18n.get() === 'zh' ? 'zh-CN' : 'en-US') : '—';}
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
  const grid = el('div', undefined, 'grid'), other = el('div', undefined, 'grid');
  for (const actor of actors) {
    const card = el('article', undefined, 'card agent');
    const avatar = el('div', actor.display_name.slice(0, 1), 'avatar'); card.append(avatar, el('h2', actor.display_name));
    card.append(el('span', actor.disabled ? t('agent.paused') : actor.kind === 'agent' ? t('agent.registered') : actor.kind === 'user' ? t('agent.user') : t('agent.client'), `pill${actor.disabled ? ' disabled' : ''}`));
    card.append(el('p', t('agent.fine'), 'fine'));
    const actions = el('div', undefined, 'row');
    const spaceId = sid, epoch = loadEpoch;
    actions.append(button(t('agent.recent'), async () => {
      const result = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind: 'inbox', actor_id: actor.actor_id, limit: 20});
      if (spaceId !== sid || epoch !== loadEpoch) return;
      const list = el('section');
      if (result.items.length) renderMessages(result.items, list); else list.append(el('p', t('agent.noRecent')));
      openDetail(t('agent.recentTitle', {name: actor.display_name}), list);
    }));
    if (!actor.disabled && actor.kind === 'agent') actions.append(button(t('agent.connections'), () => manageConnections(actor)));
    if (!actor.disabled && actor.kind === 'agent') actions.append(button(t('agent.pause'), async () => {
      if (!confirm(t('agent.confirmDisable', {name: actor.display_name}))) return;
      await api(`/admin/v0/spaces/${sid}/actors/${actor.actor_id}/disable`, 'POST', {}); await load();
    }));
    const advanced = el('details', undefined, 'technical'); advanced.append(el('summary', t('agent.identity')), el('code', actor.actor_id), button(t('agent.viewActivity'), async () => {
      const result = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind: 'feed', actor_id: actor.actor_id, limit: 20});
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
    messages: [['subject', 'col.subject'], ['kind', 'col.kind'], ['producer_id', 'col.from'], ['committed_at', 'col.time']],
    feed: [['item_type', 'col.behavior'], ['producer_id', 'col.party'], ['resource_version', 'col.version'], ['committed_at', 'col.time']],
    collections: [['display_name', 'col.collection'], ['scope', 'col.scope'], ['source_actor_id', 'col.source'], ['current_revision', 'col.current']],
    tokens: [['token_id', 'col.credId'], ['actor_id', 'col.party'], ['consumer_id', 'Consumer'], ['revoked_at', 'col.revokedAt']],
    submissions: [['operation', 'col.op'], ['state', 'col.state'], ['producer_id', 'col.party'], ['request_id', 'col.reqId']],
    receipts: [['consumer_id', 'Consumer'], ['subject_item_id', 'col.record'], ['stage', 'col.stage'], ['version', 'col.version']],
  }[tab];
  for (const [, key] of columns) head.append(el('th', t(key))); head.append(el('th', t('col.check'))); table.append(head);
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
        const history = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind: 'memory_revision', resource_id: item.collection_id});
        if (spaceId !== sid || epoch !== loadEpoch) return;
        $('#detail-title').textContent = item.display_name;
        const body = $('#detail-body'); body.replaceChildren(el('p', t('lib.srcScope', {source: actorName(item.source_actor_id), scope: item.scope})));
        for (const revision of history.revisions) body.append(button(t('lib.revBtn', {rev: revision.revision, active: revision.active}), async () => {
          const state = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind: 'memory', resource_id: item.collection_id, revision: revision.revision});
          if (spaceId !== sid || epoch !== loadEpoch) return;
          body.replaceChildren(el('p', t(state.has_more ? 'lib.revStateMore' : 'lib.revState', {rev: state.revision})));
          for (const entry of state.items) {
            body.append(el('p', entry.deleted_in_revision ? t('lib.entryDeleted', {path: entry.path}) : entry.path), button(t('lib.viewBlob'), async () => {
              const content = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind: 'blob_content', resource_id: entry.blob_id});
              if (spaceId === sid && epoch === loadEpoch) showDetail(entry.path, content.text === null ? content : content.text);
            }));
          }
        }));
        if (!history.revisions.length) body.append(el('p', t('lib.revZero')));
        if (!$('#detail').open) $('#detail').showModal();
      } else showDetail(t(sections[tab].title), item);
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
  for (const [value, key] of [['redact', 'priv.redact'], ['block', 'priv.block']]) {const option = el('option', t(key)); option.value = value; mode.append(option);} mode.value = config.mode; modeLabel.append(mode); form.append(modeLabel);
  const targets = el('div', undefined, 'checks'); const targetInputs = [];
  form.append(el('h2', t('priv.agents')));
  for (const actor of actors.filter(a => a.kind === 'agent')) {const label = el('label', actor.display_name), input = el('input'); input.type = 'checkbox'; input.checked = config.actor_ids.includes(actor.actor_id); label.prepend(input); targets.append(label); targetInputs.push([actor.actor_id, input]);} form.append(targets);
  const detectors = el('div', undefined, 'checks'), detectorInputs = [];
  for (const [kind, key] of [['credential', 'priv.detCred'], ['email', 'priv.detEmail'], ['phone', 'priv.detPhone'], ['identity_number', 'priv.detId']]) {const label = el('label', t(key)), input = el('input'); input.type = 'checkbox'; input.checked = config.rules.some(r => r.kind === kind); label.prepend(input); detectors.append(label); detectorInputs.push([kind, input]);}
  form.append(el('h2', t('priv.rules')), detectors);
  const customInputs = [];
  for (const [kind, key] of [['literal', 'priv.customLiteral'], ['regex', 'priv.customRegex'], ['metadata_key', 'priv.customMeta']]) {const label = el('label', t(key)), input = el('textarea'); input.value = config.rules.filter(r => r.kind === kind).map(r => r.value).join('\n'); label.append(input); form.append(label); customInputs.push([kind, input]);}
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
function scopeNames() {return {'message.send': t('scope.send'), 'message.read': t('scope.read'), 'receipt.write': t('scope.receipt'), 'blob.write': t('scope.blob'), 'memory.write': t('scope.memWrite'), 'memory.read': t('scope.memRead')};}
function openOnboarding() {
  const task = pendingCreation();
  if (task) {
    if (task.type !== 'agent') throw new Error(t('ob.resume'));
    return createAgent(task.name);
  }
  const form = el('form'), label = el('label', t('ob.nameLabel')), input = el('input');
  input.required = true; input.maxLength = 100; input.placeholder = t('ob.namePh'); label.append(input);
  const submit = el('button', t('ob.generate')); submit.type = 'submit';
  form.append(label, el('p', t('ob.defaultNote')), submit);
  form.addEventListener('submit', event => {event.preventDefault(); if (input.value.trim()) busy(submit, () => createAgent(input.value.trim()));});
  openDetail(t('ob.onboardTitle'), form); input.focus();
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
  body.append(el('p', t('ob.saveNote')), el('pre', JSON.stringify(info, null, 2)), el('h3', t('ob.allows')), el('p', task.scopes.map(s => scopeNames()[s] || s).join('；')), el('p', task.expires_at ? t('ob.expiresAt', {date: date(task.expires_at)}) : t('ob.forever')), el('p', t('ob.onboarded')));
  openDetail(t('ob.connTitle'), body); sid = task.sid; finishCreation(); await loadSpaces();
}
async function createAgent(name) {
  let task = pendingCreation();
  if (task && task.type !== 'agent') throw new Error(t('ob.resume'));
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
  body.append(el('p', t('ob.connNote')));
  for (const token of tokens.items) {
    const expired = token.expires_at && new Date(token.expires_at).getTime() <= Date.now();
    const row = el('div', undefined, 'card');
    row.append(el('p', token.revoked_at ? t('token.revoked') : expired ? t('token.expired') : t('token.valid')), el('p', token.scopes.map(s => scopeNames()[s] || s).join('；')), el('p', token.expires_at ? t('token.expiresAt', {date: date(token.expires_at)}) : t('token.forever')), el('code', token.token_id));
    if (!token.revoked_at) row.append(button(t('token.revokeBtn'), async () => {if (!confirm(t('token.confirmRevoke'))) return; await api(root + `/tokens/${token.token_id}`, 'DELETE', {}); if (spaceId === sid) await manageConnections(actor);}));
    body.append(row);
  }
  if (tokens.has_more) body.append(el('p', t('ob.only100')));
  const form = el('form', undefined, 'connection-form'); form.append(el('h3', t('ob.issueNew')));
  const consumerLabel = el('label', t('ob.consumer')), consumer = el('select');
  for (const c of consumers.filter(c => c.actor_id === actor.actor_id && !c.disabled)) {const option = el('option', c.consumer_id); option.value = c.consumer_id; consumer.append(option);}
  consumer.required = true; consumerLabel.append(consumer); form.append(consumerLabel);
  const scopeInputs = [], scopes = el('fieldset'), choices = el('div', undefined, 'checks'); scopes.append(el('legend', t('ob.allow')));
  for (const [scope, name] of Object.entries(scopeNames())) {const label = el('label', name), check = el('input'); check.type = 'checkbox'; check.checked = scope === 'message.send'; label.prepend(check); choices.append(label); scopeInputs.push([scope, check]);} scopes.append(choices); form.append(scopes);
  function resourceChoices(title, resources, key) {
    if (!resources.length) return [];
    const field = el('fieldset'), list = el('div', undefined, 'checks'), inputs = []; field.append(el('legend', title));
    for (const r of resources) {const label = el('label', r.display_name), check = el('input'); check.type = 'checkbox'; label.prepend(check); list.append(label); inputs.push([r[key], check]);}
    field.append(list); form.append(field); return inputs;
  }
  const recipients = resourceChoices(t('ob.recipients'), actors.filter(a => a.kind === 'agent' && !a.disabled && a.actor_id !== actor.actor_id), 'actor_id');
  const read = resourceChoices(t('ob.read'), collections.filter(c => c.source_actor_id !== actor.actor_id), 'collection_id');
  const write = resourceChoices(t('ob.write'), collections.filter(c => c.source_actor_id !== actor.actor_id), 'collection_id');
  const ownLabel = el('label', t('ob.own')), own = el('input'); own.type = 'checkbox'; ownLabel.prepend(own); form.append(ownLabel, el('p', t('ob.ownNote')));
  const ttlLabel = el('label', t('ob.ttl')), ttl = el('select');
  for (const [value, key] of [['0', 'ob.ttlForever'], ['3600', 'ob.ttl1h'], ['86400', 'ob.ttl1d'], ['604800', 'ob.ttl7d']]) {const option = el('option', t(key)); option.value = value; ttl.append(option);} ttlLabel.append(ttl); form.append(ttlLabel);
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
      if (recipientIds.length && !selected.includes('message.send')) throw new Error(t('ob.needSend'));
      if (read.some(([, i]) => i.checked) && !selected.includes('memory.read')) throw new Error(t('ob.needRead'));
      if (write.some(([, i]) => i.checked) && !selected.includes('memory.write')) throw new Error(t('ob.needWrite'));
      task = {type: 'connection', name: actor.display_name, sid: spaceId, actor, consumer: {consumer_id: consumer.value}, prefix: crypto.randomUUID(), scopes: selected, grants: {recipient_ids: recipientIds.length ? [spaces.find(s => s.space_id === spaceId).default_actor_id, ...recipientIds] : [], memory_read_collection_ids: read.filter(([, i]) => i.checked).map(([id]) => id), memory_write_collection_ids: write.filter(([, i]) => i.checked).map(([id]) => id)}, create_collection: own.checked, expires_at: Number(ttl.value) ? new Date(Date.now() + Number(ttl.value) * 1000).toISOString() : null}; saveCreation(task);
    }
    if (task.create_collection && !task.collection) {task.collection = await api(root + '/collections', 'POST', {request_id: task.prefix + '-collection', source_actor_id: actor.actor_id, display_name: actor.display_name + t('ob.collSuffix')}); saveCreation(task);}
    await issueConnection(task);
  });});
  body.append(form); openDetail(t('ob.connFor', {name: actor.display_name}), body);
}
async function inspectItems(kind, spaceId) {
  const items = []; let next = null;
  do {const page = await api(`/admin/v0/spaces/${spaceId}/inspect`, 'POST', {kind, limit: 100, cursor: next}); items.push(...page.items); next = page.has_more ? page.next_cursor : null;} while (next);
  return items;
}
$('#add-agent').addEventListener('click', () => busy($('#add-agent'), () => openOnboarding()));
// Strings resolve through the locale dictionary, so the first paint waits
// for it; afterwards a language switch re-renders the current tab.
window.WaypostI18n.ready.then(() => {api('/admin/session').then(entered).catch(() => showLogin());});
document.addEventListener('waypost:lang', () => {
  if (!$('#login').hidden) return;
  try { $('#detail').close(); } catch (err) {}
  $('#detail-body').replaceChildren();
  load();
});

$('#owner-credential').addEventListener('click', () => busy($('#owner-credential'), async () => {
  if (!sid) throw new Error(t('ob.needSpace'));
  const path = `/admin/v0/spaces/${sid}/owner-credential`;
  const current = await api(path);
  if (current.version && !confirm(t('ob.confirmReissue'))) return;
  const result = await api(path, 'POST', {expected_version: current.version});
  showDetail(t('ob.ownerTitle'), {dashboard_url: `${location.origin}/dashboard`, space_id: sid, credential: result.credential, note: t('ob.ownerNote')});
}));
