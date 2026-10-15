'use strict';
// The browser exports a fixed version; the service never buffers a whole library.
window.WaypostMemory = (() => {
  const encoder = new TextEncoder(), maxBytes = 32 * 1024 * 1024, maxItems = 10000;
  const crcTable = Array.from({length:256}, (_, n) => {for(let i=0;i<8;i++) n=(n&1)?0xedb88320^(n>>>1):n>>>1;return n>>>0;});
  function crc32(bytes) {let crc=0xffffffff;for(const byte of bytes) crc=crcTable[(crc^byte)&255]^(crc>>>8);return (crc^0xffffffff)>>>0;}
  function zip(entries) {
    const local=[],central=[];let offset=0,centralSize=0;
    for(const {name,bytes} of entries) {
      const path=encoder.encode(name),crc=crc32(bytes),header=new Uint8Array(30),h=new DataView(header.buffer);
      h.setUint32(0,0x04034b50,true);h.setUint16(4,20,true);h.setUint16(6,0x800,true);h.setUint16(12,33,true);h.setUint32(14,crc,true);h.setUint32(18,bytes.length,true);h.setUint32(22,bytes.length,true);h.setUint16(26,path.length,true);
      const index=new Uint8Array(46),c=new DataView(index.buffer);
      c.setUint32(0,0x02014b50,true);c.setUint16(4,20,true);c.setUint16(6,20,true);c.setUint16(8,0x800,true);c.setUint16(14,33,true);c.setUint32(16,crc,true);c.setUint32(20,bytes.length,true);c.setUint32(24,bytes.length,true);c.setUint16(28,path.length,true);c.setUint32(42,offset,true);
      local.push(header,path,bytes);central.push(index,path);offset+=header.length+path.length+bytes.length;centralSize+=index.length+path.length;
    }
    const end=new Uint8Array(22),e=new DataView(end.buffer);e.setUint32(0,0x06054b50,true);e.setUint16(8,entries.length,true);e.setUint16(10,entries.length,true);e.setUint32(12,centralSize,true);e.setUint32(16,offset,true);
    return new Blob([...local,...central,end],{type:'application/zip'});
  }
  async function checkedBytes(context,item) {
    if(!context.active()) throw new Error('空间或登录状态已变化，请重新打开资料。');
    const bytes=await context.readBytes(`/admin/v0/spaces/${context.spaceId}/blobs/${encodeURIComponent(item.blob_id)}/content`);
    const digest=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),b=>b.toString(16).padStart(2,'0')).join('');
    if(bytes.length!==item.size_bytes||digest!==item.sha256) throw new Error('文件校验失败，未导出。请重新读取资料。');
    if(!context.active()) throw new Error('空间或登录状态已变化，请重新打开资料。');
    return bytes;
  }
  async function open(context) {
    const {spaceId,collection,api,el,button,openDetail}=context,root=`/admin/v0/spaces/${spaceId}/memory/${collection.collection_id}`;
    let latest=null,current=null,history=[];
    // Drafts live only in this open library, never in browser storage.
    const drafts=new Map();
    const read=(revision,cursor=null)=>api(root+'/read','POST',{revision,cursor,limit:20,include_tombstones:true});
    async function showItem(item,page,retained=null) {
      const bytes=retained?null:await checkedBytes(context,item);if(!context.active())return;
      const isText=/^text\//i.test(item.content_type)||/^application\/(?:json|[^;]+\+json)(?:;|$)/i.test(item.content_type);
      if(!isText){openDetail(item.path,el('p','此格式支持导出原始文件，不能在网页中编辑。'));return;}
      let text;try{text=retained?retained.text:new TextDecoder('utf-8',{fatal:true}).decode(bytes);}catch{throw new Error('这份文件不是有效 UTF-8 文本，可以导出原始文件。');}
      const editable=!!retained||page.revision===latest;
      const draft=retained||{text,baseRevision:page.revision,item,page,attempt:null,conflict:false};
      const form=el('form'),editor=el('textarea'),status=el('p'),comparison=el('div');
      editor.value=text;editor.dataset.memoryEditor='';editor.setAttribute('aria-label','资料内容');editor.rows=12;editor.readOnly=!editable;
      status.textContent=`版本 ${draft.baseRevision}${editable?' · 修改会生成新版本，旧内容保留。':' · 历史版本，只读。'}`;
      form.append(status,editor);
      function remember(){if(editable){draft.text=editor.value;drafts.set(item.item_id,draft);}}
      editor.addEventListener('input',remember);
      let save;
      async function compareLatest() {
        remember();draft.conflict=true;save.disabled=true;save.setAttribute('aria-disabled','true');
        status.textContent='资料已被更新，草稿已保留。请核对最新内容，再决定如何修改。';
        const head=await read(null);if(!context.active())return;latest=head.revision;
        history=(await api(`/admin/v0/spaces/${spaceId}/inspect`,'POST',{kind:'memory_revision',resource_id:collection.collection_id})).revisions;
        let target=head.items.find(value=>value.item_id===item.item_id),part=head;const seen=new Set();
        while(!target&&part.has_more){
          if(!part.next_cursor||seen.has(part.next_cursor))throw new Error('资料分页不完整；草稿仍保留在此处。');
          seen.add(part.next_cursor);part=await read(head.revision,part.next_cursor);
          if(part.revision!==head.revision)throw new Error('资料版本变化，请重新比较；草稿仍保留在此处。');
          target=part.items.find(value=>value.item_id===item.item_id);
        }
        if(!context.active())return;comparison.replaceChildren(el('p',`当前版本 ${head.revision}`));
        if(!target||target.deleted_in_revision){comparison.append(el('p','这条资料已删除。你的草稿仍在上方，可复制保存。'));return;}
        const latestText=new TextDecoder('utf-8',{fatal:true}).decode(await checkedBytes(context,target));
        if(!context.active())return;
        const preview=el('textarea');preview.value=latestText;preview.readOnly=true;preview.rows=8;preview.dataset.memoryLatest='';preview.setAttribute('aria-label',`版本 ${head.revision} 的内容`);comparison.append(preview);
        const rebase=button(`已核对，以版本 ${head.revision} 为基础继续编辑`,async()=>{
          draft.baseRevision=head.revision;draft.attempt=null;draft.conflict=false;save.disabled=false;save.setAttribute('aria-disabled','false');rebase.disabled=true;rebase.setAttribute('aria-disabled','true');
          status.textContent=`正在以版本 ${head.revision} 为基础编辑。请把需要保留的最新内容合入上方草稿，再保存。`;
        });rebase.dataset.memoryRebase='';comparison.append(rebase);
      }
      if(editable){
        save=button('保存为新版本',async()=>{
          remember();
          if(!draft.attempt||draft.attempt.text!==editor.value)draft.attempt={request_id:crypto.randomUUID(),expected_revision:draft.baseRevision,item_id:item.item_id,text:editor.value};
          const attempt=draft.attempt;
          try {
            if(!attempt.blob_id){const prepared=await api(root+'/edit/prepare','POST',attempt);if(!context.active())return;attempt.blob_id=prepared.blob_id;}
            const result=await api(root+'/edit','POST',{request_id:attempt.request_id,expected_revision:attempt.expected_revision,item_id:attempt.item_id,blob_id:attempt.blob_id});
            if(!context.active())return;drafts.delete(item.item_id);latest=result.revision;await showVersion(result.revision,`已保存为版本 ${result.revision}。`);
          }catch(error){if(error.code==='REVISION_CONFLICT'){await compareLatest();return;}throw error;}
        });save.dataset.memorySave='';form.append(save);
      }
      form.append(comparison);form.addEventListener('submit',event=>event.preventDefault());
      const back=button('返回资料版本',()=>{if(editable&&editor.value!==text)remember();return showVersion(null);});back.dataset.memoryReturn='';form.append(back);openDetail(item.path,form);
      if(retained&&(draft.conflict||draft.baseRevision!==latest))await compareLatest();
    }
    async function exportVersion(revision) {
      const items=[],seen=new Set(),cursors=new Set();let cursor=null,total=0;
      do {
        const page=await read(revision,cursor);
        if(!context.active())return;
        if(page.revision!==revision||page.collection_id!==collection.collection_id)throw new Error('资料版本变化，导出已停止。');
        for(const item of page.items) {
          if(seen.has(item.item_id))throw new Error('资料分页出现重复，导出已停止。');
          seen.add(item.item_id);items.push(item);
          if(!item.deleted_in_revision) {
            if(!Number.isSafeInteger(item.size_bytes)||item.size_bytes<0)throw new Error('文件大小无效。');
            total+=item.size_bytes;
          }
          if(items.length>maxItems||total>maxBytes)throw new Error('浏览器导出最多 32 MiB、10,000 项。请使用参考客户端导出完整集合。');
        }
        cursor=page.has_more?page.next_cursor:null;
        if(page.has_more&&(!cursor||cursors.has(cursor)))throw new Error('资料分页不完整，导出已停止。');
        if(cursor)cursors.add(cursor);
      }while(cursor);
      const entries=[],records=[];
      for(const item of items) {
        const record={...item,archive_path:null};
        if(!item.deleted_in_revision) {
          // Original names remain metadata; generated archive paths cannot escape.
          record.archive_path=`files/${String(entries.length+1).padStart(6,'0')}.bin`;
          entries.push({name:record.archive_path,bytes:await checkedBytes(context,item)});
        }
        records.push(record);
      }
      const manifest=encoder.encode(JSON.stringify({format:'waypost-memory-v1',space_id:spaceId,collection_id:collection.collection_id,display_name:collection.display_name,source_actor_id:current.source_actor_id,scope:current.scope,revision,items:records},null,2));
      const overhead=22+[...entries,{name:'manifest.json'}].reduce((sum,entry)=>sum+76+encoder.encode(entry.name).length*2,0);
      if(total+manifest.length+overhead>maxBytes)throw new Error('导出超过浏览器 32 MiB 上限，请使用参考客户端。');
      if(!context.active())return;
      const url=URL.createObjectURL(zip([{name:'manifest.json',bytes:manifest},...entries])),link=el('a');
      link.href=url;link.download=`waypost-memory-${collection.collection_id}-v${revision}.zip`;document.body.append(link);link.click();link.remove();setTimeout(()=>URL.revokeObjectURL(url),1000);
    }
    async function showVersion(revision,notice='') {
      const page=await read(revision);if(!context.active())return;current=page;if(latest===null||revision===null)latest=page.revision;
      const body=el('div',undefined,'memory-library'),label=el('label','查看版本'),select=el('select');
      const revisions=[...new Set([latest,page.revision,...history.map(item=>item.revision)])].sort((a,b)=>b-a);
      for(const value of revisions){const option=el('option',`版本 ${value}${value===latest?' · 当前':''}`);option.value=String(value);select.append(option);}select.value=String(page.revision);label.append(select);
      select.addEventListener('change',()=>context.run(select,()=>showVersion(Number(select.value))));
      body.append(label,el('p',`版本 ${page.revision} · 来源：${context.sourceName || page.source_actor_id}`,'fine'));
      if(notice)body.append(el('p',notice,'save-confirmation'));
      for(const draft of drafts.values()){
        const resume=button(`继续编辑未保存的草稿：${draft.item.path}`,()=>showItem(draft.item,draft.page,draft));resume.dataset.memoryDraft=draft.item.item_id;body.append(resume);
      }
      const download=button('导出这个版本',()=>exportVersion(page.revision));download.dataset.memoryExport='';body.append(download,el('p','下载 ZIP，包含原始文件、版本说明和删除标记。删除条目的旧正文不在当前包中，可选择历史版本另行导出。最多 32 MiB。','fine'));
      const list=el('div',undefined,'memory-items');body.append(list);
      function appendItems(items) {for(const item of items) {
        if(item.deleted_in_revision){list.append(el('p',`${item.path} · 已删除`,'fine'));continue;}
        const entry=button(item.path,()=>showItem(item,page,page.revision===latest?drafts.get(item.item_id):null));entry.dataset.memoryItem=item.item_id;list.append(entry);
      }}
      appendItems(page.items);
      let nextCursor=page.next_cursor;
      if(page.has_more){const more=button('加载更多资料',async()=>{
        if(!nextCursor)throw new Error('资料分页不完整，请重新打开。');
        const next=await read(page.revision,nextCursor);if(!context.active())return;
        if(next.revision!==page.revision)throw new Error('资料版本变化，请重新打开。');
        appendItems(next.items);nextCursor=next.next_cursor;more.hidden=!next.has_more;
      });body.append(more);}
      if(!page.items.length)list.append(el('p','这个版本还没有资料。'));
      openDetail(collection.display_name,body);
    }
    history=(await api(`/admin/v0/spaces/${spaceId}/inspect`,'POST',{kind:'memory_revision',resource_id:collection.collection_id})).revisions;
    await showVersion(null);
  }
  return {open};
})();
