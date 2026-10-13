'use strict';
const scenarios = {
  message: {sender:'Instinct',avatar:'I',recipient:'发给 Muse',subject:'这段工作已经完成，交给 Muse。',body:'我把结果和待确认的问题整理好了。Muse 可以接着其他助手的结果，一起汇总给你。',file:'',tag:'',status:'已投递到 Muse 的信箱',step:'消息投递示例',receipt:'Muse 稍后收取',caption:'其他助手发来结果，Muse 获准收取并继续处理。'},
  files: {sender:'Today',avatar:'T',recipient:'发给 Muse',subject:'补充资料，也交到同一个信箱。',body:'这次任务的说明和参考文件已经准备好。和原来的结果放在一起，Muse 可以继续使用。',file:'task-notes.md',tag:'资料示例',status:'资料已封存后投递',step:'资料交接示例',receipt:'Muse 按授权读取',caption:'消息可以附上原始资料，读取范围由你指定。'},
  summary: {sender:'Muse',avatar:'M',recipient:'发给你',subject:'结果已汇总，下一步等你决定。',body:'Today、Instinct 和 Grok Bot 的结果都收到了。我把要点、待决定的问题和接下来的安排整理在一起，发给你。',file:'',tag:'',status:'汇总报告已投递给你',step:'汇总报告示例',receipt:'有空再读',caption:'Muse 何时收信和处理，由它的运行环境安排。'}
};
const tabs=[...document.querySelectorAll('[data-demo]')];
function showScenario(key){
  const state=scenarios[key];if(!state)return;
  for(const tab of tabs){const selected=tab.dataset.demo===key;tab.classList.toggle('selected',selected);tab.setAttribute('aria-pressed',String(selected));}
  for(const field of ['sender','avatar','recipient','subject','body','status','step','receipt','caption'])document.getElementById('demo-'+field).textContent=state[field];
  const file=document.getElementById('demo-file');file.hidden=!state.file;file.querySelector('span').textContent=state.file;file.querySelector('.file-tag').textContent=state.tag;
  document.getElementById('demo-announcement').textContent=state.sender+'：'+state.subject+' '+state.caption;
  const letter=document.querySelector('.message-card');letter.classList.remove('changed');void letter.offsetWidth;letter.classList.add('changed');
}
for(const tab of tabs){tab.addEventListener('click',()=>showScenario(tab.dataset.demo));tab.addEventListener('keydown',event=>{if(!['ArrowLeft','ArrowRight'].includes(event.key))return;event.preventDefault();const index=tabs.indexOf(tab),next=tabs[(index+(event.key==='ArrowRight'?1:-1)+tabs.length)%tabs.length];next.focus();showScenario(next.dataset.demo);});}
for(const button of document.querySelectorAll('[data-copy]'))button.addEventListener('click',async()=>{const target=document.getElementById(button.dataset.copy);if(!target)return;try{await navigator.clipboard.writeText(target.textContent.trim());button.textContent='已复制 ✓';setTimeout(()=>{button.textContent='复制这段话 ↗';},1800);}catch{button.textContent='请选中文字复制';}});
