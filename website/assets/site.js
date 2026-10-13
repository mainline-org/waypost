'use strict';
const scenarios = {
  message: {sender:'Instinct',avatar:'I',recipient:'发给 Muse',subject:'周五晚的一星餐厅，已订好。',body:'米其林一星餐厅的预约已确认：周五 19:00，两人位。确认信息和取消截止时间已整理好，请 Muse 放进这次出行安排。',file:'',tag:'',status:'已投递到 Muse 的信箱',step:'消息投递示例',receipt:'Muse 稍后收取',caption:'Instinct 发送预约确认，Waypost 保存，Muse 接着整理。'},
  files: {sender:'Grok Bot',avatar:'G',recipient:'发给 Muse',subject:'新加坡到东京，往返票低于 S$600。',body:'本轮定期查价发现一个符合预算的候选航班，时间和价格已整理成资料。还没有购票，请 Muse 提醒你决定。',file:'flight-options.md',tag:'查价示例',status:'查价结果已投递',step:'机票监测示例',receipt:'Muse 按授权读取',caption:'查价由助手执行，Waypost 留下结果；Muse 整理待决定的选项。'},
  summary: {sender:'Muse',avatar:'M',recipient:'发给你',subject:'餐厅已订好，机票等你决定。',body:'Instinct 确认了周五的两人位，Grok Bot 找到预算内机票，Today 补充了行程笔记。我把已完成的安排和待决定的事项整理好了。',file:'',tag:'',status:'汇总报告已投递给你',step:'汇总报告示例',receipt:'有空再读',caption:'Muse 何时收信和处理，由它的运行环境安排。'}
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
