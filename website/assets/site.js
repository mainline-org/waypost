'use strict';
const scenarios = {
  message: {subject:'本周资料，已经整理好。',body:'访谈笔记和参考来源都在这里。你有空时，就可以接着写了。',file:'interview-notes.md',tag:'资料已封存',status:'已保存到信箱',step:'01 / DELIVERED ↗',receipt:'稍后读取',caption:'收件人不在线，消息也有地方等。'},
  files: {subject:'这些线索，可以接着用了。',body:'笔记新增了一组参考来源。读取修订 2，也能回到第一版核对。',file:'reference-sources.md',tag:'记忆修订 2',status:'资料有版本，也有来源',step:'02 / SHARED ↗',receipt:'获准读取',caption:'共享到指定的记忆集合，让上下文一起交接。'},
  resume: {subject:'回来了，继续上次的工作。',body:'连接恢复以后，先读取未处理的消息。保存好了，再确认完成处理。',file:'pending-message.json',tag:'已可靠保存',status:'恢复上次的收取进度',step:'03 / RECEIVED ↗',receipt:'继续处理',caption:'可靠保存和完成处理，由客户端分别确认。'}
};
const tabs=[...document.querySelectorAll('[data-demo]')];
function showScenario(key){
  const state=scenarios[key];if(!state)return;
  for(const tab of tabs){const selected=tab.dataset.demo===key;tab.classList.toggle('selected',selected);tab.setAttribute('aria-pressed',String(selected));}
  for(const field of ['subject','body','status','step','receipt','caption'])document.getElementById('demo-'+field).textContent=state[field];
  const file=document.getElementById('demo-file');file.querySelector('span').textContent=state.file;file.querySelector('.file-tag').textContent=state.tag;
  document.getElementById('demo-announcement').textContent=state.subject+' '+state.caption;
  const letter=document.querySelector('.message-card');letter.classList.remove('changed');void letter.offsetWidth;letter.classList.add('changed');
}
for(const tab of tabs){tab.addEventListener('click',()=>showScenario(tab.dataset.demo));tab.addEventListener('keydown',event=>{if(!['ArrowLeft','ArrowRight'].includes(event.key))return;event.preventDefault();const index=tabs.indexOf(tab),next=tabs[(index+(event.key==='ArrowRight'?1:-1)+tabs.length)%tabs.length];next.focus();showScenario(next.dataset.demo);});}
for(const button of document.querySelectorAll('[data-copy]'))button.addEventListener('click',async()=>{const target=document.getElementById(button.dataset.copy);if(!target)return;try{await navigator.clipboard.writeText(target.textContent.trim());button.textContent='已复制 ✓';setTimeout(()=>{button.textContent='复制这段话 ↗';},1800);}catch{button.textContent='请选中文字复制';}});
