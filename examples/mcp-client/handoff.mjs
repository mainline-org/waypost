import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { writeFileSync, mkdirSync } from 'node:fs';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StreamableHTTPClientTransport } from '@modelcontextprotocol/sdk/client/streamableHttp.js';

const origin=process.env.WAYPOST_URL||'http://127.0.0.1:8787';
const admin=process.env.WAYPOST_ADMIN_SECRET;
if(!admin)throw new Error('Provide WAYPOST_ADMIN_SECRET privately.');
const prefix=randomUUID(),clients=[];
const output=process.env.WAYPOST_HANDOFF_OUTPUT||'.ml-cache/handoff';mkdirSync(output,{recursive:true});
async function control(path,body,method='POST'){
 const response=await fetch(origin+path,{method,headers:{authorization:`Bearer ${admin}`,'content-type':'application/json','user-agent':'Waypost/0.1.0'},body:method==='GET'?undefined:JSON.stringify(body)});
 const value=await response.json();assert.equal(response.status,200,value.error?.code||'control failed');return value.data;
}
async function client(token,name){
 const connection=new Client({name,version:'0.1.0'},{capabilities:{}});clients.push(connection);
 await connection.connect(new StreamableHTTPClientTransport(new URL(origin+'/mcp'),{requestInit:{headers:{authorization:`Bearer ${token}`,'user-agent':'Waypost/0.1.0'}}}));return connection;
}
async function tool(connection,name,args){const result=await connection.callTool({name,arguments:args});assert.equal(result.isError,false,result.structuredContent?.error?.code||name);return result.structuredContent.data;}
async function assistant(root,name,scopes,defaultRecipient){
 const actor=await control(root+'/actors',{request_id:`${prefix}-${name}-actor`,display_name:name,kind:'agent'});
 const consumer=await control(root+'/consumers',{request_id:`${prefix}-${name}-consumer`,actor_id:actor.actor_id,view:'inbox',filter:{}});
 const issued=await control(root+'/tokens',{request_id:`${prefix}-${name}-token`,actor_id:actor.actor_id,consumer_id:consumer.consumer_id,scopes,resource_grants:defaultRecipient?{recipient_ids:[defaultRecipient]}:{},...(defaultRecipient?{default_recipient_id:defaultRecipient}:{})});
 return {actor,consumer,connection:await client(issued.token,name)};
}
try{
 const space=await control('/admin/v0/spaces',{request_id:prefix+'-space',display_name:'主助手交接验收 · 模拟助手'});
 const root=`/admin/v0/spaces/${space.space_id}`;
 const primary=await assistant(root,'Primary-assistant-test',['message.send','message.read','receipt.write']);
 await control(root+'/primary-assistant',{expected_revision:0,actor_id:primary.actor.actor_id},'PUT');
 const producer=await assistant(root,'Connected-assistant-test',['message.send'],primary.actor.actor_id);
 const description=await tool(producer.connection,'waypost_describe',{});
 assert.deepEqual(description.scopes,['message.send']);assert.equal(description.default_recipient_id,primary.actor.actor_id);
 await tool(primary.connection,'waypost_describe',{});
 const input={request_id:prefix+'-booking',kind:'result',subject:'[验收示例] 周五预约已确认',body:{text:'模拟预约结果：周五 19:00，两人位。不是实际订单。'}};
 writeFileSync(output+'/outbox.json',JSON.stringify(input),{mode:0o600,flush:true});
 const published=await tool(producer.connection,'waypost_send',input);
 assert.deepEqual(await tool(producer.connection,'waypost_send',input),published);
 const inbox=await tool(primary.connection,'waypost_inbox',{consumer_id:primary.consumer.consumer_id,limit:20});
 const messages=inbox.items.filter(item=>item.item_type==='message');assert.equal(messages.length,1);
 const read=await tool(primary.connection,'waypost_message_get',{message_id:published.message_id});assert.equal(read.body.text,input.body.text);
 writeFileSync(output+'/received.json',JSON.stringify({inbox,read}),{mode:0o600,flush:true});
 await tool(primary.connection,'waypost_ack',{request_id:prefix+'-received',consumer_id:primary.consumer.consumer_id,items:[{item_id:messages[0].item_id,stage:'received'}]});
 const report={request_id:prefix+'-report',kind:'result',subject:'[验收示例] 主助手已汇总',body:{text:'收到模拟预约结果：周五 19:00，两人位。下一步由用户确认行程；本验收没有执行外部预订。'}};
 writeFileSync(output+'/report.json',JSON.stringify(report),{mode:0o600,flush:true});
 const summary=await tool(primary.connection,'waypost_send',report);
 await tool(primary.connection,'waypost_ack',{request_id:prefix+'-processed',consumer_id:primary.consumer.consumer_id,items:[{item_id:messages[0].item_id,stage:'processed'}]});
 const ownerToken=await control(root+'/tokens',{request_id:prefix+'-owner-reader',actor_id:space.default_actor_id,consumer_id:space.default_consumer_id,scopes:['message.read'],resource_grants:{}});
 const owner=await client(ownerToken.token,'Owner-reader-test');await tool(owner,'waypost_describe',{});
 const ownerInbox=await tool(owner,'waypost_inbox',{consumer_id:space.default_consumer_id,limit:20});
 assert.equal(ownerInbox.items.filter(i=>i.item_type==='message').length,1);
 const ownerRead=await tool(owner,'waypost_message_get',{message_id:summary.message_id});assert.equal(ownerRead.body.text,report.body.text);
 const denied=await producer.connection.callTool({name:'waypost_inbox',arguments:{consumer_id:producer.consumer.consumer_id}});assert.equal(denied.isError,true);
 console.log(JSON.stringify({result:'passed',runtime:origin,actors:'simulated primary and connected assistants',durable_same_request:true,received_before_processed:true,owner_received_summary:true,sender_cannot_read:true}));
}finally{await Promise.all(clients.map(c=>c.close()));}
