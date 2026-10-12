import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StreamableHTTPClientTransport } from '@modelcontextprotocol/sdk/client/streamableHttp.js';

const origin = process.env.WAYPOST_URL || 'http://127.0.0.1:8787';
const admin = process.env.WAYPOST_ADMIN_SECRET;
if (!admin) throw new Error('Set WAYPOST_ADMIN_SECRET to your private admin credential.');
const prefix = randomUUID();
async function control(path, body) {
  const response = await fetch(origin + path, {method: 'POST', headers: {'authorization': `Bearer ${admin}`, 'content-type': 'application/json'}, body: JSON.stringify(body)});
  const value = await response.json();
  assert.equal(response.status, 200, JSON.stringify(value));
  return value.data;
}
const space = await control('/admin/v0/spaces', {request_id: `${prefix}-space`, display_name: 'MCP trial'});
const actor = await control(`/admin/v0/spaces/${space.space_id}/actors`, {request_id: `${prefix}-actor`, display_name: 'Demo assistant', kind: 'agent'});
const consumer = await control(`/admin/v0/spaces/${space.space_id}/consumers`, {request_id: `${prefix}-consumer`, actor_id: actor.actor_id, view: 'inbox', filter: {}});
const credential = await control(`/admin/v0/spaces/${space.space_id}/tokens`, {request_id: `${prefix}-token`, actor_id: actor.actor_id, consumer_id: consumer.consumer_id, scopes: ['message.send', 'message.read', 'receipt.write', 'blob.write', 'memory.write', 'memory.read'], resource_grants: {}});
const client = new Client({name: 'waypost-demo', version: '0.1.0'}, {capabilities: {}});
try {
  await client.connect(new StreamableHTTPClientTransport(new URL(origin + '/mcp'), {requestInit: {headers: {'authorization': `Bearer ${credential.token}`}}}));
  const tools = await client.listTools();
  assert.equal(tools.tools.length, 11);
  const description = await client.callTool({name: 'waypost_describe', arguments: {}});
  assert.equal(description.isError, false);
  const args = {request_id: `${prefix}-message`, kind: 'result', subject: 'Research complete', body: {text: 'This message travelled through the official MCP SDK.'}};
  const first = await client.callTool({name: 'waypost_send', arguments: args});
  assert.equal(first.isError, false);
  const repeated = await client.callTool({name: 'waypost_send', arguments: args});
  assert.deepEqual(first.structuredContent, repeated.structuredContent);
  const id = first.structuredContent.data.message_id;
  const message = await client.callTool({name: 'waypost_message_get', arguments: {message_id: id}});
  assert.equal(message.structuredContent.data.body.text, args.body.text);
  console.log(JSON.stringify({result: 'passed', tools: tools.tools.length, space_id: space.space_id, message_id: id, duplicate_creates_second_message: false}, null, 2));
} finally {
  await client.close();
}
