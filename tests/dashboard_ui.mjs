import { JSDOM } from '../examples/mcp-client/node_modules/jsdom/lib/api.js';
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import test from 'node:test';
import { webcrypto } from 'node:crypto';

const fixtures = {
  spaces: [{space_id:'space-one',display_name:'One',default_actor_id:'actor-owner'}],
  actors: [{actor_id:'actor-agent',display_name:'Assistant',kind:'agent',disabled:0}],
  consumers: [{consumer_id:'consumer-one',actor_id:'actor-agent',disabled:0}],
};
const wait = async predicate => {for(let i=0;i<100;i++){if(predicate()) return; await new Promise(r=>setTimeout(r,10));} throw new Error('UI did not reach expected state');};

test('a disabled primary assistant directs the owner to choose another instead of waiting', async () => {
  const dom=new JSDOM(readFileSync(new URL('../dashboard/index.html',import.meta.url),'utf8'),{url:'https://service.example/dashboard',runScripts:'outside-only'});
  const {window:w}=dom; const d=w.document;
  w.fetch=async(path,options={})=>{
    const body=options.body?JSON.parse(options.body):{};
    const data=path==='/admin/session'?{csrf_token:'test',operator:false}:path==='/admin/v0/registry'?{items:body.kind==='actors'?fixtures.actors.map(actor=>({...actor,disabled:true})):fixtures[body.kind],has_more:false}:path.endsWith('/primary-assistant')?{revision:1,primary_assistant:{actor_id:'actor-agent',display_name:'Assistant',disabled:true}}:{items:[],has_more:false};
    return {ok:true,status:200,json:async()=>({ok:true,data})};
  };
  try {
    w.eval(readFileSync(new URL('../dashboard/onboarding-copy.js',import.meta.url),'utf8'));
    w.eval(readFileSync(new URL('../dashboard/app.js',import.meta.url),'utf8'));
    await wait(()=>d.querySelector('#content .empty'));
    assert(d.querySelector('#content').textContent.includes('已停用'));
    assert(!d.querySelector('#content').textContent.includes('等待主助手'));
    assert.equal(d.querySelector('#service-management').hidden,true);
    d.querySelector('#content .empty button').click();await wait(()=>d.querySelector('.role-guide'));
    assert(d.querySelector('.role-guide').textContent.includes('选择其他助手'));
  }finally{w.close();}
});

test('viewing space sign-in explains the target and waits for an explicit reset', async () => {
  const dom = new JSDOM(readFileSync(new URL('../dashboard/index.html',import.meta.url),'utf8'),{url:'https://service.example/dashboard',runScripts:'outside-only'});
  const {window:w}=dom; const d=w.document; const writes=[];
  w.HTMLDialogElement.prototype.showModal=function(){this.open=true;};
  w.HTMLDialogElement.prototype.close=function(){this.open=false;this.dispatchEvent(new w.Event('close'));};
  w.fetch=async(path,options={})=>{
    const body=options.body?JSON.parse(options.body):{}; let data;
    if(path==='/admin/session') data={csrf_token:'test',operator:true};
    else if(path==='/admin/v0/registry') data={items:fixtures[body.kind],has_more:false};
    else if(path.endsWith('/primary-assistant')) data={revision:0,primary_assistant:null};
    else if(path.endsWith('/owner-credential')) {if(options.method==='POST'){writes.push(body);data={credential:'wp_user_new'};}else data={version:1,active:true};}
    else data={items:[],has_more:false};
    return {ok:true,status:200,json:async()=>({ok:true,data})};
  };
  try {
    w.eval(readFileSync(new URL('../dashboard/onboarding-copy.js',import.meta.url),'utf8'));
    w.eval(readFileSync(new URL('../dashboard/app.js',import.meta.url),'utf8'));
    await wait(()=>!d.querySelector('#app').hidden && d.querySelector('#content').children.length);
    d.querySelector('#owner-credential').click();await wait(()=>d.querySelector('#detail-body form'));
    assert.equal(writes.length,0);
    assert(d.querySelector('#detail-body').textContent.includes('旧登录密钥和已登录会话立即失效'));
    assert(d.querySelector('#detail-body').textContent.includes('One'));
    d.querySelector('#detail-body form').dispatchEvent(new w.Event('submit',{cancelable:true}));
    await wait(()=>d.querySelector('#detail-body pre'));
    assert.deepEqual(writes,[{expected_version:1}]);
    assert.equal(d.querySelector('#detail-body pre').textContent,'wp_user_new');
  } finally {w.close();}
});

test('a redeemed personal credential remains visible when space loading fails', async () => {
  const dom = new JSDOM(readFileSync(new URL('../dashboard/index.html', import.meta.url),'utf8'),{url:'https://service.example/dashboard#invite=wpi_test',runScripts:'outside-only'});
  const {window:w}=dom;
  w.HTMLDialogElement.prototype.showModal=function(){this.open=true;};
  w.HTMLDialogElement.prototype.close=function(){this.open=false;this.dispatchEvent(new w.Event('close'));};
  w.fetch=async path=>path==='/admin/invitation'
    ?{ok:true,status:200,json:async()=>({ok:true,data:{credential:'wp_user_once',csrf_token:'test',operator:false}})}
    :{ok:false,status:503,json:async()=>({ok:false,error:{message:'Storage temporarily unavailable'}})};
  try {
    w.eval(readFileSync(new URL('../dashboard/onboarding-copy.js',import.meta.url),'utf8'));
    w.eval(readFileSync(new URL('../dashboard/app.js',import.meta.url),'utf8'));
    await wait(()=>w.document.querySelector('#notice').textContent.includes('Storage temporarily unavailable'));
    assert.equal(w.location.hash,'');
    assert(w.document.querySelector('#detail').open);
    assert(w.document.querySelector('#detail-body').textContent.includes('wp_user_once'));
    assert.equal(w.document.querySelector('#login').hidden,true);
    assert.equal(w.document.querySelector('#add-agent').hidden,true);
  } finally {dom.window.close();}
});

test('a simultaneous logout prevents an invitation response from displaying its private link', async () => {
  const dom = new JSDOM(readFileSync(new URL('../dashboard/index.html', import.meta.url),'utf8'),{url:'https://service.example/dashboard',runScripts:'outside-only'});
  const {window:w}=dom; const d=w.document;
  Object.defineProperty(w,'crypto',{value:webcrypto});
  w.HTMLDialogElement.prototype.showModal=function(){this.open=true;};
  w.HTMLDialogElement.prototype.close=function(){this.open=false;this.dispatchEvent(new w.Event('close'));};
  let resolveInvite, resolveLogout;
  w.fetch=async(path,options={})=>{
    const body=options.body?JSON.parse(options.body):{};
    let data;
    if(path.endsWith('/invitations')) return {ok:true,status:200,json:()=>new Promise(resolve=>{resolveInvite=resolve;})};
    if(path==='/admin/session' && options.method==='DELETE') return {ok:true,status:200,json:()=>new Promise(resolve=>{resolveLogout=resolve;})};
    if(path==='/admin/session') data={csrf_token:'test',operator:true};
    else if(path==='/admin/v0/spaces') data={space_id:'new-space'};
    else if(path==='/admin/v0/registry') data={items:fixtures[body.kind],has_more:false};
    else if(path.endsWith('/primary-assistant')) data={revision:0,primary_assistant:null};
    else data={items:[],has_more:false};
    return {ok:true,status:200,json:async()=>({ok:true,data})};
  };
  try {
    w.eval(readFileSync(new URL('../dashboard/onboarding-copy.js',import.meta.url),'utf8'));
    w.eval(readFileSync(new URL('../dashboard/app.js',import.meta.url),'utf8'));
    await wait(()=>!d.querySelector('#app').hidden && d.querySelector('#content').children.length);
    d.querySelector('#invite-user').click();
    d.querySelector('#detail-body form').dispatchEvent(new w.Event('submit',{cancelable:true}));
    await wait(()=>resolveInvite);
    d.querySelector('#logout').click();
    await wait(()=>resolveLogout);
    resolveLogout({ok:true,data:{}});
    resolveInvite({ok:true,data:{invite_token:'private',invite_url:'https://service.example/dashboard#invite=private',expires_at:'2030-01-01T00:00:00Z'}});
    await wait(()=>!d.querySelector('#login').hidden);
    await new Promise(resolve=>setTimeout(resolve,20));
    assert.equal(d.querySelector('#detail').open,false);
    assert(!d.querySelector('#detail-body').textContent.includes('#invite=private'));
  } finally {dom.window.close();}
});

test('permission management opens and includes later collection pages with a 20-item deployment limit', async () => {
  const dom = new JSDOM(readFileSync(new URL('../dashboard/index.html', import.meta.url),'utf8'),{url:'https://service.example/dashboard',runScripts:'outside-only'});
  const {window:w}=dom;const d=w.document;const requests=[];
  w.HTMLDialogElement.prototype.showModal=function(){this.open=true;};
  w.HTMLDialogElement.prototype.close=function(){this.open=false;this.dispatchEvent(new w.Event('close'));};
  w.fetch=async (path,options={})=>{
    const body=options.body?JSON.parse(options.body):{};requests.push({path,body});
    let data;
    if(path.endsWith('/primary-assistant')) data={revision:0,primary_assistant:null};
    else if(path.endsWith('/connection-status')) data={actors:[]};
    else if(path==='/admin/session') data={csrf_token:'test',operator:false,limits:{max_page_size:20}};
    else if(path==='/admin/v0/registry') data={items:fixtures[body.kind],has_more:false};
    else if(path.endsWith('/inspect')) {
      if(body.limit>20) return {ok:false,status:413,json:async()=>({ok:false,error:{code:'RESOURCE_LIMIT',message:'Page size exceeds the deployment limit.'}})};
      if(body.kind==='collections') data=body.cursor?{items:[{collection_id:'collection-later',source_actor_id:'actor-other',display_name:'Later page'}],has_more:false}:{items:[],has_more:true,next_cursor:'next-page'};
      else data={items:[],has_more:false};
    } else throw new Error('Unexpected HTTP operation '+path);
    return {ok:true,status:200,json:async()=>({ok:true,data})};
  };
  try {
    w.eval(readFileSync(new URL('../dashboard/onboarding-copy.js',import.meta.url),'utf8'));
    w.eval(readFileSync(new URL('../dashboard/app.js',import.meta.url),'utf8'));
    await wait(()=>!d.querySelector('#app').hidden && d.querySelector('#content').children.length);
    d.querySelector('[data-tab=agents]').click();await wait(()=>d.querySelector('.agent'));
    d.querySelector('[data-action=manage-connections]').click();
    await wait(()=>d.querySelector('.connection-form'));
    assert(d.querySelector('#detail-body').textContent.includes('Later page'));
    assert.equal(requests.filter(r=>r.body.kind==='collections').length,2);
    assert(requests.filter(r=>r.path.endsWith('/inspect')).every(r=>r.body.limit<=20));
    assert.equal(d.querySelector('#detail-error').textContent,'');
  } finally {dom.window.close();}
});
