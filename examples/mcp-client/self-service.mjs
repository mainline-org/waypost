// Runs against an explicitly selected disposable native or local Worker service.
import {JSDOM} from 'jsdom';
import {readFileSync} from 'node:fs';
import {randomUUID,createHash,webcrypto} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import assert from 'node:assert/strict';

const origin=process.env.WAYPOST_URL||'http://127.0.0.1:8787',admin=process.env.WAYPOST_ADMIN_SECRET;
if(!admin)throw new Error('Provide the disposable service admin credential privately.');
if(!['127.0.0.1','localhost'].includes(new URL(origin).hostname))throw new Error('Self-service integration tests require a local disposable service.');
const prefix=randomUUID(),headers={'user-agent':'Waypost/0.1.0','content-type':'application/json'};
const wait=async predicate=>{for(let i=0;i<500;i++){if(predicate())return;await new Promise(r=>setTimeout(r,20));}throw new Error('Expected self-service state was not reached');};
async function call(path,body,token=admin,method='POST') {
  const response=await fetch(origin+path,{method,headers:{...headers,authorization:`Bearer ${token}`},body:method==='GET'?undefined:JSON.stringify(body)}),value=await response.json();
  assert.equal(response.status,200,value.error?.code||'HTTP operation failed');return value.data;
}
const space=await call('/admin/v0/spaces',{request_id:prefix+'-space',display_name:'Self-service integration'}),root='/admin/v0/spaces/'+space.space_id;
const actor=await call(root+'/actors',{request_id:prefix+'-actor',display_name:'Fixture producer',kind:'agent'});
const consumer=await call(root+'/consumers',{request_id:prefix+'-consumer',actor_id:actor.actor_id,view:'inbox',filter:{}});
const source=await call(root+'/tokens',{request_id:prefix+'-token',actor_id:actor.actor_id,consumer_id:consumer.consumer_id,scopes:['message.send','blob.write','memory.read','memory.write'],resource_grants:{}});
const collection=await call(root+'/collections',{request_id:prefix+'-collection',source_actor_id:actor.actor_id,scope:'trial',display_name:'Travel preferences'});
const contents=[Buffer.from('Budget: S$600.\n'),Buffer.from([0,255,13,10,128]),Buffer.from('Keep the original filename as metadata.\n')],items=[];
for(const [index,bytes] of contents.entries()) {
  const prepared=await call('/v0/uploads',{request_id:prefix+'-upload-'+index,content_type:index===1?'application/octet-stream':'text/plain',size_bytes:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex')},source.token);
  const uploaded=await fetch(origin+'/v0/uploads/'+prepared.upload_id+'/content',{method:'PUT',headers:{authorization:`Bearer ${source.token}`,'content-type':index===1?'application/octet-stream':'text/plain','user-agent':'Waypost/0.1.0'},body:bytes});assert.equal(uploaded.status,200);
  const blob=await call('/v0/uploads/'+prepared.upload_id+'/commit',{request_id:prefix+'-seal-'+index},source.token);
  items.push({item_id:'item-'+index,blob_id:blob.blob_id,path:index===0?'preferences.md':index===1?'original.bin':'资料/备注.txt',metadata:{source:'synthetic fixture'}});
}
await call('/v0/memory/collections/'+collection.collection_id+'/revisions',{request_id:prefix+'-v1',expected_revision:0,mode:'snapshot',items},source.token);
const owner=await call(root+'/owner-credential',{expected_version:0});
const dom=new JSDOM(readFileSync(new URL('../../dashboard/index.html',import.meta.url),'utf8'),{url:origin+'/dashboard',runScripts:'outside-only'}),w=dom.window,d=w.document;
Object.defineProperty(w,'crypto',{value:webcrypto});Object.defineProperty(d,'visibilityState',{value:'visible'});
w.TextEncoder=TextEncoder;w.TextDecoder=TextDecoder;w.Blob=Blob;
w.HTMLDialogElement.prototype.showModal=function(){this.open=true;};w.HTMLDialogElement.prototype.close=function(){this.open=false;this.dispatchEvent(new w.Event('close'));};
const downloads=[];w.URL.createObjectURL=blob=>{downloads.push(blob);return 'blob:fixture';};w.URL.revokeObjectURL=()=>{};w.HTMLAnchorElement.prototype.click=function(){};
let cookie='',pending=0;
w.fetch=async(path,options={})=>{
  pending++;
  try {const response=await fetch(new URL(path,origin),{...options,headers:{...options.headers,'user-agent':'Waypost/0.1.0',origin,...(cookie?{cookie}:{})}});const next=response.headers.get('set-cookie');if(next)cookie=next.split(';')[0];return response;}finally{pending--;}
};
try {
  for(const file of ['onboarding-copy.js','memory.js','app.js'])w.eval(readFileSync(new URL('../../dashboard/'+file,import.meta.url),'utf8'));
  await new Promise(r=>setTimeout(r,100));d.querySelector('#secret').value=owner.credential;d.querySelector('#login-form').dispatchEvent(new w.Event('submit',{cancelable:true}));
  await wait(()=>!d.querySelector('#app').hidden&&d.querySelector('#content').children.length);
  d.querySelector('[data-tab=agents]').click();await wait(()=>d.querySelector('[data-connection-status]'));
  assert.match(d.querySelector('[data-connection-status]').textContent,/等待第一封信/);
  await call('/v0/messages',{request_id:prefix+'-report',kind:'result',subject:'Real fixture report',body:{text:'A task result delivered over HTTP.'}},source.token);
  d.querySelector('#refresh').click();await wait(()=>d.querySelector('[data-connection-status]')?.textContent.includes('已投递 1'));
  d.querySelector('[data-tab=collections]').click();await wait(()=>d.querySelector('#content td button'));
  d.querySelector('#content td button').click();await wait(()=>d.querySelector('[data-memory-item]'));
  d.querySelector('[data-memory-item="item-0"]').click();await wait(()=>d.querySelector('[data-memory-editor]'));
  assert.equal(d.querySelector('[data-memory-editor]').value,'Budget: S$600.\n');d.querySelector('[data-memory-editor]').value='Budget: S$800.\n';d.querySelector('[data-memory-save]').click();
  await wait(()=>d.querySelector('[data-memory-export]')&&d.querySelector('#detail-body').textContent.includes('版本 2'));
  d.querySelector('[data-memory-export]').click();await wait(()=>downloads.length===1);
  const archive=Buffer.from(await downloads[0].arrayBuffer());
  const check=spawnSync('python3',['-c',`import sys,io,json,zipfile,base64
z=zipfile.ZipFile(io.BytesIO(sys.stdin.buffer.read()))
assert z.testzip() is None
m=json.loads(z.read('manifest.json'))
assert all(not p.startswith('/') and '..' not in p.split('/') for p in z.namelist())
print(json.dumps({'manifest':m,'files':{i['item_id']:base64.b64encode(z.read(i['archive_path'])).decode() for i in m['items'] if i['archive_path']}}))`],{input:archive});
  assert.equal(check.status,0,'Independent ZIP verification failed');const exported=JSON.parse(check.stdout.toString());
  assert.equal(exported.manifest.revision,2);assert.equal(exported.manifest.source_actor_id,actor.actor_id);assert.equal(exported.manifest.items.length,3);
  assert.equal(Buffer.from(exported.files['item-0'],'base64').toString(),'Budget: S$800.\n');assert.deepEqual(Buffer.from(exported.files['item-1'],'base64'),contents[1]);
  const history=await call(root+'/memory/'+collection.collection_id+'/read',{revision:1,limit:20},owner.credential);
  const original=history.items.find(item=>item.item_id==='item-0');const response=await fetch(origin+root+'/blobs/'+original.blob_id+'/content',{headers:{authorization:'Bearer '+owner.credential,'user-agent':'Waypost/0.1.0'}});
  assert.deepEqual(Buffer.from(await response.arrayBuffer()),contents[0]);
  assert.equal(d.querySelector('#detail-error').textContent,'');
  d.querySelector('#close-detail').click();d.querySelector('[data-tab=inbox]').click();await wait(()=>d.querySelector('.message-row'));
  await call('/v0/messages',{request_id:prefix+'-second',kind:'result',subject:'New report after returning',body:{text:'Ready to continue.'}},source.token);d.dispatchEvent(new w.Event('visibilitychange'));
  await wait(()=>!d.querySelector('#new-messages').hidden);d.querySelector('#new-messages').click();await wait(()=>d.querySelectorAll('.message-row').length===2);
  d.querySelector('#logout').click();await wait(()=>d.querySelector('#app').hidden);
  console.log(JSON.stringify({result:'passed',runtime:origin,actual_http_dashboard:true,delivery_status:true,owner_edit_retains_history:true,fixed_version_export:true,binary_bytes:true,zip_crc_verified:true,safe_archive_paths:true,new_report_notice:true,logout:true}));
}finally{await wait(()=>pending===0);w.close();}
