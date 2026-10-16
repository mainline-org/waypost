import {JSDOM} from '../examples/mcp-client/node_modules/jsdom/lib/api.js';
import {readFileSync} from 'node:fs';
import {webcrypto,createHash} from 'node:crypto';
import test from 'node:test';
import assert from 'node:assert/strict';

const wait = async predicate => {for (let i=0;i<150;i++) {if(predicate()) return; await new Promise(r=>setTimeout(r,10));} throw new Error('Expected user interface was not reached');};
async function dashboard(extra = () => null) {
  const dom=new JSDOM(readFileSync(new URL('../dashboard/index.html',import.meta.url),'utf8'),{url:'https://service.example/dashboard',runScripts:'outside-only'}),w=dom.window;
  Object.defineProperty(w,'crypto',{value:webcrypto});
  w.TextEncoder=TextEncoder;w.TextDecoder=TextDecoder;w.Blob=Blob;
  w.HTMLDialogElement.prototype.showModal=function(){this.open=true;};
  w.HTMLDialogElement.prototype.close=function(){this.open=false;this.dispatchEvent(new w.Event('close'));};
  const actors=[{actor_id:'agent-one',display_name:'Research assistant',kind:'agent',disabled:false}];
  w.fetch=async(path,options={})=>{
    if (typeof path === 'string' && path.includes('/dashboard/locales/')) {
      const file = path.split('/').pop();
      return {ok:true,status:200,json:async()=>JSON.parse(readFileSync(new URL('../dashboard/locales/'+file,import.meta.url),'utf8'))};
    }
    const body=options.body?JSON.parse(options.body):{};
    let data=await extra(path,body,options);
    if(data instanceof Response)return data;
    if(data===null) data=path==='/admin/session'?{csrf_token:'test',operator:false,limits:{max_page_size:20,max_blob_bytes:262144,max_collection_items:32}}:path==='/admin/v0/registry'?{items:body.kind==='spaces'?[{space_id:'space-one',display_name:'My space',default_actor_id:'owner'}]:body.kind==='actors'?actors:[],has_more:false}:path.endsWith('/primary-assistant')?{revision:0,primary_assistant:null}:path.endsWith('/connection-status')?{actors:[{actor_id:'agent-one',report_count:0,last_report_at:null}]}:{items:[],has_more:false};
    return {ok:true,status:200,json:async()=>({ok:true,data})};
  };
  for(const file of ['i18n.js','memory.js','app.js'])w.eval(readFileSync(new URL('../dashboard/'+file,import.meta.url),'utf8'));
  await w.WaypostI18n.set('zh');
  return {dom,w,d:w.document};
}

test('assistant setup shows actual delivery history instead of treating an issued credential as a working connection',async()=>{
  let delivered=false;
  const {w,d}=await dashboard(path=>path.endsWith('/connection-status')?{actors:[{actor_id:'agent-one',report_count:delivered?1:0,last_report_at:delivered?'2026-10-14T03:00:00Z':null}]}:null);
  try {
    await wait(()=>!d.querySelector('#app').hidden&&d.querySelector('#content').children.length);
    d.querySelector('[data-tab=agents]').click();await wait(()=>d.querySelector('[data-connection-status]'));
    assert.match(d.querySelector('[data-connection-status]').textContent,/等待第一封信/);
    delivered=true;d.querySelector('#refresh').click();await wait(()=>d.querySelector('[data-connection-status]')?.textContent.includes('1'));
    assert.match(d.querySelector('[data-connection-status]').textContent,/已投递/);
  } finally {w.close();}
});

test('owner edits a current text item and exports the new fixed version with its original bytes',async()=>{
  let version=1,text='I prefer tea.',preparedText='';const edits=[],downloads=[];
  const collection={collection_id:'col-one',display_name:'Travel preferences',source_actor_id:'agent-one',scope:'private',current_revision:1};
  const {w,d}=await dashboard((path,body)=>{
    if(path.endsWith('/inspect'))return body.kind==='collections'?{items:[collection],has_more:false}:body.kind==='memory_revision'?{revisions:[{revision:version},{revision:1}],limit:100}:{items:[],has_more:false};
    if(path.endsWith('/memory/col-one/read'))return {collection_id:'col-one',source_actor_id:'agent-one',scope:'private',revision:version,items:[{item_id:'item-one',blob_id:'blob-one',path:'preferences.md',metadata:{source:'you'},size_bytes:Buffer.byteLength(text),sha256:createHash('sha256').update(text).digest('hex'),content_type:'text/plain'}],has_more:false};
    if(path.endsWith('/memory/col-one/edit/prepare')){preparedText=body.text;return {blob_id:'blob-edit'};}
    if(path.endsWith('/memory/col-one/edit')){assert.equal(body.blob_id,'blob-edit');assert(!('text' in body));edits.push(body);assert.equal(body.expected_revision,1);text=preparedText;version=2;return {revision:2,changed:true};}
    if(path.endsWith('/blobs/blob-one/content'))return new Response(text);
    return null;
  });
  w.URL.createObjectURL=blob=>{downloads.push(blob);return 'blob:test';};w.URL.revokeObjectURL=()=>{};w.HTMLAnchorElement.prototype.click=function(){};
  try {
    await wait(()=>!d.querySelector('#app').hidden&&d.querySelector('#content').children.length);
    d.querySelector('[data-tab=collections]').click();await wait(()=>d.querySelector('#content tbody')||d.querySelector('#content td button'));
    d.querySelector('#content td button').click();await wait(()=>d.querySelector('[data-memory-item]'));
    d.querySelector('[data-memory-item]').click();await wait(()=>d.querySelector('[data-memory-editor]'));
    d.querySelector('[data-memory-editor]').value='I prefer coffee.';
    d.querySelector('[data-memory-save]').click();await wait(()=>edits.length===1&&d.querySelector('#detail-body').textContent.includes('版本 2'));
    d.querySelector('[data-memory-export]').click();await wait(()=>downloads.length===1);
    const bytes=Buffer.from(await downloads[0].arrayBuffer());
    assert.equal(bytes.subarray(0,4).toString('hex'),'504b0304');
    assert(bytes.includes(Buffer.from('I prefer coffee.')));
    assert(bytes.includes(Buffer.from('manifest.json')));
  }finally {w.close();}
});

for(const outcome of ['success','failure','conflict','navigation'])test(`the editor is read-only throughout both save phases and recovers after ${outcome}`,async()=>{
  let revision=1,text='Original preference.',releasePrepare,releasePublish,preparedText;
  const {w,d}=await dashboard((path,body)=>{
    if(path.endsWith('/inspect'))return body.kind==='collections'?{items:[{collection_id:'col-one',display_name:'Preferences',source_actor_id:'agent-one'}],has_more:false}:body.kind==='memory_revision'?{revisions:[{revision}]}:{items:[],has_more:false};
    if(path.endsWith('/memory/col-one/read'))return {collection_id:'col-one',revision,items:[{item_id:'item-one',blob_id:'blob-one',path:'preference.md',size_bytes:Buffer.byteLength(text),sha256:createHash('sha256').update(text).digest('hex'),content_type:'text/plain'}],has_more:false};
    if(path.endsWith('/blobs/blob-one/content'))return new Response(text);
    if(path.endsWith('/edit/prepare')){preparedText=body.text;return new Promise(resolve=>{releasePrepare=resolve;});}
    if(path.endsWith('/edit'))return new Promise(resolve=>{releasePublish=resolve;});
    return null;
  });
  try {
    await wait(()=>!d.querySelector('#app').hidden&&d.querySelector('#content').children.length);
    d.querySelector('[data-tab=collections]').click();await wait(()=>d.querySelector('#content td button'));
    d.querySelector('#content td button').click();await wait(()=>d.querySelector('[data-memory-item]'));
    d.querySelector('[data-memory-item]').click();await wait(()=>d.querySelector('[data-memory-editor]'));
    const editor=d.querySelector('[data-memory-editor]');editor.value='My correction.';
    const save=d.querySelector('[data-memory-save]');save.click();await wait(()=>releasePrepare);
    assert.equal(editor.readOnly,true,'Typing must be prevented while the submitted draft is being saved.');
    if(outcome==='conflict'){
      revision=2;text='Updated by another assistant.';
      releasePrepare(Response.json({ok:false,error:{code:'REVISION_CONFLICT',message:'Changed.'}},{status:409}));
      await wait(()=>d.querySelector('[data-memory-rebase]'));
      assert.equal(editor.readOnly,false);
      assert.equal(editor.value,'My correction.');
      assert.equal(d.querySelector('[data-memory-latest]').value,text);
    }else{
      releasePrepare({blob_id:'prepared-blob'});await wait(()=>releasePublish);
      assert.equal(editor.readOnly,true,'The final publication must keep the editor locked too.');
      if(outcome==='navigation'){
        d.querySelector('[data-memory-return]').click();await wait(()=>d.querySelector('[data-memory-draft]'));
        d.querySelector('[data-memory-draft]').click();await wait(()=>d.querySelector('[data-memory-editor]'));
        const continued=d.querySelector('[data-memory-editor]');continued.value='A newer unsaved correction.';continued.dispatchEvent(new w.Event('input'));
        revision=2;text=preparedText;releasePublish({revision:2});await wait(()=>!save.disabled);
        assert.equal(d.querySelector('[data-memory-editor]'),continued,'A completed save must not replace a subsequently opened editor.');
        assert.equal(continued.value,'A newer unsaved correction.');
      }else if(outcome==='failure'){
        releasePublish(Response.json({ok:false,error:{code:'STORAGE_UNAVAILABLE',message:'Try again.'}},{status:503}));
        await wait(()=>d.querySelector('#detail-error').textContent.includes('Try again.'));
        assert.equal(editor.readOnly,false);
        assert.equal(editor.value,'My correction.');
      }else{
        revision=2;text=preparedText;releasePublish({revision:2});
        await wait(()=>d.querySelector('[data-memory-export]')&&d.querySelector('#detail-body').textContent.includes('版本 2'));
        assert.equal(text,'My correction.');
      }
    }
  }finally{w.close();}
});

test('returning to the inbox announces a new report without clearing the current list or acknowledging it',async()=>{
  let arrived=false;const requests=[];
  const old={message_id:'old-report',subject:'Earlier report',producer_id:'agent-one',committed_at:'2026-10-14T02:00:00Z'};
  const recent={message_id:'new-report',subject:'New result ready',producer_id:'agent-one',committed_at:'2026-10-14T03:00:00Z'};
  const {w,d}=await dashboard((path,body)=>{
    requests.push(path);
    return path.endsWith('/inspect')&&body.kind==='inbox'?{items:arrived?[recent,old]:[old],has_more:false}:null;
  });
  try {
    Object.defineProperty(d,'visibilityState',{value:'visible',configurable:true});
    await wait(()=>d.querySelector('.message-row'));
    arrived=true;d.dispatchEvent(new w.Event('visibilitychange'));
    await wait(()=>d.querySelector('#new-messages')&&!d.querySelector('#new-messages').hidden);
    assert.equal(d.querySelectorAll('.message-row').length,1);
    d.querySelector('#new-messages').click();await wait(()=>d.querySelectorAll('.message-row').length===2);
    assert.match(d.querySelector('.message-row').textContent,/New result ready/);
    assert(!requests.some(path=>path.includes('/receipts')));
  }finally {w.close();}
});


test('an owner can compare the latest version, leave and recover the draft, then explicitly save against that version',async()=>{
 const original='Original preference.',remote='Another assistant corrected this.';let revision=1,prepared='',saved=null;
 const {w,d}=await dashboard((path,body)=>{
  if(path.endsWith('/inspect'))return body.kind==='collections'?{items:[{collection_id:'col-one',display_name:'Preferences',source_actor_id:'agent-one'}],has_more:false}:body.kind==='memory_revision'?{revisions:[{revision},{revision:1}]}:{items:[],has_more:false};
  if(path.endsWith('/memory/col-one/read')){const selected=body.revision??revision,content=selected===1?original:remote;return {collection_id:'col-one',revision:selected,items:[{item_id:'item-one',blob_id:'blob-'+selected,path:'preference.md',size_bytes:Buffer.byteLength(content),sha256:createHash('sha256').update(content).digest('hex'),content_type:'text/plain'}],has_more:false};}
  if(path.endsWith('/blobs/blob-1/content'))return new Response(original);
  if(path.endsWith('/blobs/blob-2/content'))return new Response(remote);
  if(path.endsWith('/edit/prepare')){if(body.expected_revision===1){revision=2;return Response.json({ok:false,error:{code:'REVISION_CONFLICT',message:'Collection changed.'}},{status:409});}assert.equal(body.expected_revision,2);prepared=body.text;return {blob_id:'blob-prepared'};}
  if(path.endsWith('/edit')){saved=body;revision=3;return {revision:3,changed:true};}
  return null;
 });
 try {
  await wait(()=>!d.querySelector('#app').hidden&&d.querySelector('#content').children.length);d.querySelector('[data-tab=collections]').click();await wait(()=>d.querySelector('#content td button'));d.querySelector('#content td button').click();await wait(()=>d.querySelector('[data-memory-item]'));d.querySelector('[data-memory-item]').click();await wait(()=>d.querySelector('[data-memory-editor]'));
  d.querySelector('[data-memory-editor]').value='My unsaved correction.';d.querySelector('[data-memory-save]').click();await wait(()=>d.querySelector('[data-memory-latest]'));
  assert.equal(d.querySelector('[data-memory-editor]').value,'My unsaved correction.');
  assert.equal(d.querySelector('[data-memory-latest]').value,remote);
  assert.equal(d.querySelector('[data-memory-save]').disabled,true);
  d.querySelector('[data-memory-return]').click();await wait(()=>d.querySelector('[data-memory-draft]'));
  assert.match(d.querySelector('#detail-body').textContent,/版本 2/);
  d.querySelector('[data-memory-draft]').click();await wait(()=>d.querySelector('[data-memory-rebase]'));
  assert.equal(d.querySelector('[data-memory-editor]').value,'My unsaved correction.');
  d.querySelector('[data-memory-rebase]').click();d.querySelector('[data-memory-save]').click();await wait(()=>saved!==null);
  assert.equal(saved.expected_revision,2);assert.equal(prepared,'My unsaved correction.');
 }finally{w.close();}
});

test('a corrupted original file prevents export instead of downloading an incomplete archive',async()=>{
 const content='Original preference.';let downloaded=false;
 const {w,d}=await dashboard((path,body)=>{
  if(path.endsWith('/inspect'))return body.kind==='collections'?{items:[{collection_id:'col-one',display_name:'Preferences',source_actor_id:'agent-one'}],has_more:false}:body.kind==='memory_revision'?{revisions:[{revision:1}]}:{items:[],has_more:false};
  if(path.endsWith('/memory/col-one/read'))return {collection_id:'col-one',revision:1,items:[{item_id:'item-one',blob_id:'blob-one',path:'preference.md',size_bytes:Buffer.byteLength(content),sha256:'0'.repeat(64),content_type:'text/plain'}],has_more:false};
  if(path.endsWith('/blobs/blob-one/content'))return new Response(content);
  return null;
 });
 w.URL.createObjectURL=()=>{downloaded=true;return 'blob:unexpected';};
 try {
  await wait(()=>!d.querySelector('#app').hidden&&d.querySelector('#content').children.length);d.querySelector('[data-tab=collections]').click();await wait(()=>d.querySelector('#content td button'));d.querySelector('#content td button').click();await wait(()=>d.querySelector('[data-memory-export]'));d.querySelector('[data-memory-export]').click();await wait(()=>d.querySelector('#detail-error').textContent.includes('校验失败'));
  assert.equal(downloaded,false);
 }finally{w.close();}
});
