import { JSDOM } from 'jsdom';
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';

const origin=process.env.WAYPOST_URL||'http://127.0.0.1:8787',admin=process.env.WAYPOST_ADMIN_SECRET;
if(!admin)throw new Error('Provide WAYPOST_ADMIN_SECRET privately.');
const wait=async predicate=>{for(let i=0;i<500;i++){if(predicate())return;await new Promise(r=>setTimeout(r,20));}throw new Error('Expected Dashboard state was not reached');};
const windows=[]; let pendingRequests=0;
function page(hash=''){
 const dom=new JSDOM(readFileSync(new URL('../../dashboard/index.html',import.meta.url),'utf8'),{url:origin+'/dashboard'+hash,runScripts:'outside-only'}),w=dom.window;windows.push(w);let cookie='';
 w.HTMLDialogElement.prototype.showModal=function(){this.open=true;};
 w.HTMLDialogElement.prototype.close=function(){this.open=false;this.dispatchEvent(new w.Event('close'));};
 w.confirm=()=>true;
 w.fetch=async(path,options={})=>{
   pendingRequests++;
   try {
   const headers={...options.headers,'user-agent':'Waypost/0.1.0',...(cookie?{cookie}: {})};
   const response=await fetch(new URL(path,origin),{...options,headers});
   const next=response.headers.get('set-cookie');if(next)cookie=next.split(';')[0];
   const value=await response.json();
   return {ok:response.ok,status:response.status,json:async()=>value};
   } finally {pendingRequests--;}
 };
 w.eval(readFileSync(new URL('../../dashboard/i18n.js',import.meta.url),'utf8'));
 w.eval(readFileSync(new URL('../../dashboard/app.js',import.meta.url),'utf8'));
 return w;
}
function submit(form){form.dispatchEvent(new form.ownerDocument.defaultView.Event('submit',{bubbles:true,cancelable:true}));}
try{
 const requester=page('#request-access'),requestDoc=requester.document,email=`onboarding-${randomUUID()}@example.test`;
 await wait(()=>requestDoc.querySelector('#detail-body input[type=email]'));
 requestDoc.querySelector('#detail-body input[type=email]').value=email;submit(requestDoc.querySelector('#detail-body form'));
 await wait(()=>!requestDoc.querySelector('#detail-body form'));
 assert.equal(requester.location.hash,'');
 const operator=page(),d=operator.document;
 await new Promise(r=>setTimeout(r,150));d.querySelector('#secret').value=admin;submit(d.querySelector('#login-form'));
 await wait(()=>!d.querySelector('#app').hidden);d.querySelector('#service-management').open=true;d.querySelector('[data-tab=access]').click();
 await wait(()=>[...d.querySelectorAll('#content .card')].some(c=>c.textContent.includes(email)));
 const requestCard=[...d.querySelectorAll('#content .card')].find(c=>c.textContent.includes(email));requestCard.querySelector('[data-action=approve-access]').click();
 await wait(()=>d.querySelector('#detail-body form'));submit(d.querySelector('#detail-body form'));
 await wait(()=>d.querySelector('#detail-body pre'));
 const inviteUrl=d.querySelector('#detail-body pre').textContent;
 assert(inviteUrl.includes('#invite='));assert.equal(new URL(inviteUrl).origin,origin);assert(!operator.sessionStorage.getItem('waypost-pending-creation'));
 const personal=page(new URL(inviteUrl).hash),p=personal.document;
 await wait(()=>!p.querySelector('#app').hidden&&p.querySelector('#detail-body pre')&&!p.querySelector('#add-agent').hidden);
 assert.equal(personal.location.hash,'');assert.equal(p.querySelector('#new-space').hidden,true);assert.equal(p.querySelector('#invite-user').hidden,true);
 const loginCredential=p.querySelector('#detail-body pre').textContent;assert(loginCredential.startsWith('wp_user_'));
 p.querySelector('#close-detail').click();assert.equal(p.querySelector('#detail-body').textContent,'');
 p.querySelector('#add-agent').click();await wait(()=>p.querySelector('#detail-body form input'));
 p.querySelector('#detail-body form input').value='Primary role test';submit(p.querySelector('#detail-body form'));
 await wait(()=>p.querySelector('#detail-body pre'));
 assert.deepEqual(JSON.parse(p.querySelector('#detail-body pre').textContent).headers.Authorization.startsWith('Bearer wp_'),true);
 p.querySelector('#close-detail').click();await new Promise(r=>setTimeout(r,200));p.querySelector('[data-tab=agents]').click();await wait(()=>p.querySelector('[data-action=choose-primary]'));
 p.querySelector('[data-action=choose-primary]').click();await wait(()=>p.querySelector('[data-action=confirm-primary]'));p.querySelector('[data-action=confirm-primary]').click();
 await wait(()=>p.querySelector('.connection-form'));const form=p.querySelector('.connection-form');
 const choices=[...form.querySelector('fieldset .checks').querySelectorAll('input')];assert.equal(choices.filter(i=>i.checked).length,3);
 submit(form);await wait(()=>p.querySelector('#detail-body pre'));
 const primaryInfo=JSON.parse(p.querySelector('#detail-body pre').textContent);p.querySelector('#close-detail').click();
 await new Promise(r=>setTimeout(r,200));p.querySelector('#add-agent').click();await wait(()=>p.querySelector('#detail-body form input'));
 const add=p.querySelector('#detail-body form');add.querySelector('input').value='Connected role test';assert.equal(add.querySelector('select').value,primaryInfo.actor_id);submit(add);
 await wait(()=>p.querySelector('#detail-body pre'));const producer=JSON.parse(p.querySelector('#detail-body pre').textContent);assert.equal(producer.default_recipient_id,primaryInfo.actor_id);
 p.querySelector('#close-detail').click();assert(!p.querySelector('#detail-body').textContent.includes(loginCredential));
 p.querySelector('#logout').click();await wait(()=>p.querySelector('#app').hidden);
 console.log(JSON.stringify({result:'passed',runtime:origin,request_to_invite:true,private_invitation_removed_from_history:true,scoped_personal_login:true,primary_configuration:true,connected_default_recipient:true,credential_clearing:true}));
}finally{
 // A logout can invalidate earlier list reads. Let their UI handlers settle
 // before destroying the simulated documents, as a browser's page lifecycle does.
 do {await wait(()=>pendingRequests===0);await new Promise(r=>setTimeout(r,20));} while(pendingRequests);
 windows.forEach(w=>w.close());
}
