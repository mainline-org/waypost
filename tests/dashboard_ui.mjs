import { JSDOM } from '../examples/mcp-client/node_modules/jsdom/lib/api.js';
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import test from 'node:test';

const fixtures = {
  spaces: [{space_id:'space-one',display_name:'One',default_actor_id:'actor-owner'}],
  actors: [{actor_id:'actor-agent',display_name:'Assistant',kind:'agent',disabled:0}],
  consumers: [{consumer_id:'consumer-one',actor_id:'actor-agent',disabled:0}],
};
const wait = async predicate => {for(let i=0;i<100;i++){if(predicate()) return; await new Promise(r=>setTimeout(r,10));} throw new Error('UI did not reach expected state');};

test('permission management opens and includes later collection pages with a 20-item deployment limit', async () => {
  const dom = new JSDOM(readFileSync(new URL('../dashboard/index.html', import.meta.url),'utf8'),{url:'https://service.example/dashboard',runScripts:'outside-only'});
  const {window:w}=dom;const d=w.document;const requests=[];
  w.HTMLDialogElement.prototype.showModal=function(){this.open=true;};
  w.HTMLDialogElement.prototype.close=function(){this.open=false;this.dispatchEvent(new w.Event('close'));};
  w.fetch=async (path,options={})=>{
    const body=options.body?JSON.parse(options.body):{};requests.push({path,body});
    let data;
    if(path.endsWith('/primary-assistant')) data={revision:0,primary_assistant:null};
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
