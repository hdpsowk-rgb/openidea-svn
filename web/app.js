import {reconcileSelection, committable, shouldRefresh, acceptResponse, snapshotChanged} from './model.js';

const $ = id => document.getElementById(id);
const bridge = window.openidea;
const state = {client:`svn-${Date.now().toString(36)}-${Math.random().toString(36).slice(2,10)}`,root:null,pendingRoot:null,roots:[],generation:0,entries:[],selected:new Set(),active:null,logs:[],historyPaths:[],busy:false,lastRefresh:0,visible:true,documentVisible:!document.hidden,tab:'changes',info:null,refreshSettings:{enabled:false,intervalSeconds:30}};
const labels = {modified:'修改',added:'新增',deleted:'删除',missing:'缺失',unversioned:'未跟踪',conflicted:'冲突',replaced:'替换（需终端确认）',external:'外部副本',normal:'属性修改',obstructed:'阻塞',incomplete:'不完整',ignored:'已忽略'};
const badges = {modified:'M',added:'A',deleted:'D',missing:'!',unversioned:'?',conflicted:'C',replaced:'R',external:'X',normal:'P',obstructed:'~',incomplete:'!'};

function send(action, extra={}, next=true) {
  if (!bridge) return;
  if (next) state.generation++;
  bridge.postMessage(JSON.stringify({client:state.client,generation:state.generation,root:state.root,action,...extra}));
}
function request(action, extra={}) {
  if (state.busy) return;
  if (action==='refresh') state.lastRefresh=Date.now();
  notice(''); setBusy(true, '正在执行…'); send(action,extra);
}
function notice(text) { $('notice').textContent=text; $('notice').classList.toggle('hidden',!text); }
function output(text) { if (!text) return; const el=$('output');el.textContent=(el.textContent+text).slice(-500000);el.scrollTop=el.scrollHeight; }
function setBusy(busy,label) {
  state.busy=busy;
  $('message').disabled=busy;
  $('project-root').disabled=busy;
  $('auto-refresh').disabled=busy;
  $('refresh-interval').disabled=busy||!state.refreshSettings.enabled;
  $('status').textContent=label || (busy?'正在执行…':`${state.entries.length} 项本地变更`);
  $('cancel').classList.toggle('hidden',!busy);
  for (const el of document.querySelectorAll('[data-write],#refresh,#reload-log,#more-log,#authenticate,#diff,#open')) el.disabled=busy;
  updateSelection();
}
function node(tag, cls, text) {const el=document.createElement(tag);if(cls)el.className=cls;if(text!==undefined)el.textContent=text;return el;}
function visibleEntries() { const query=$('filter').value.trim().toLocaleLowerCase(); return state.entries.filter(e=>e.path.toLocaleLowerCase().includes(query)); }
function updateSelection() {
  const selected=[...state.selected];$('selected-count').textContent=selected.length;
  $('selection-summary').textContent=selected.length?selected.join('\n'):'尚未选择文件';
  $('commit').disabled=state.busy||!selected.length||!$('message').value.trim();
  const eligible=visibleEntries().filter(committable);
  const checked=eligible.filter(e=>state.selected.has(e.path)).length;
  $('check-all').checked=eligible.length>0&&checked===eligible.length;
  $('check-all').indeterminate=checked>0&&checked<eligible.length;
}
function renderFiles() {
  const files=$('files');files.replaceChildren();const entries=visibleEntries();
  $('empty').classList.toggle('hidden',state.entries.length!==0||!state.info);
  $('change-count').textContent=state.entries.length;
  for(const entry of entries) {
    const row=node('div',`file${state.active===entry.path?' active':''}`);row.tabIndex=0;row.title=entry.path;
    const checkbox=node('input');checkbox.type='checkbox';checkbox.checked=state.selected.has(entry.path);checkbox.disabled=!committable(entry);checkbox.setAttribute('aria-label',`选择 ${entry.path}`);
    checkbox.addEventListener('click',e=>e.stopPropagation());checkbox.addEventListener('change',()=>{if(checkbox.checked)state.selected.add(entry.path);else state.selected.delete(entry.path);updateSelection();});
    const status=entry.treeConflicted?'conflicted':entry.status;
    row.append(checkbox,node('span',`badge ${status}`,badges[status]||'?'),node('span','path',entry.path),node('span','kind',labels[status]||status));
    row.addEventListener('click',()=>{state.active=entry.path;renderFiles();});
    row.addEventListener('dblclick',()=>request('diff',{paths:[entry.path]}));
    row.addEventListener('keydown',e=>{if(e.key==='Enter'){e.preventDefault();request('diff',{paths:[entry.path]});}if(e.key===' '&&!checkbox.disabled){e.preventDefault();checkbox.checked=!checkbox.checked;checkbox.dispatchEvent(new Event('change'));}});
    files.append(row);
  }
  if(!entries.length&&state.entries.length)files.append(node('div','empty muted','没有匹配的路径'));
  updateSelection();
}
function switchTab(tab, load=true) {
  state.tab=tab;
  for(const button of document.querySelectorAll('[data-tab]')) {const active=button.dataset.tab===tab;button.setAttribute('aria-selected',String(active));$(`${button.dataset.tab}-page`).classList.toggle('hidden',!active);}
  if(load&&tab==='history'&&!state.logs.length&&!state.busy)request('log',{paths:state.historyPaths});
}
function renderHistory() {
  $('history-scope').textContent=state.historyPaths.length?`文件：${state.historyPaths.join(', ')}`:'当前项目 · 双击路径查看历史差异';
  $('history-scope').title=state.historyPaths.join(', ');
  $('project-log').classList.toggle('hidden',!state.historyPaths.length);
  const list=$('logs');list.replaceChildren();
  for(const log of state.logs) {
    const row=node('div','log');row.tabIndex=0;row.setAttribute('role','button');row.setAttribute('aria-label',`版本 ${log.revision} ${log.message}`);
    const meta=node('div','meta');meta.append(node('strong','',`r${log.revision}`),node('span','truncate',log.author||'未知作者'));
    row.append(meta,node('div','subject',log.message.split('\n')[0]||'（无提交说明）'));
    const activate=()=>{for(const r of list.children)r.classList.remove('active');row.classList.add('active');renderLogDetail(log);};
    row.addEventListener('click',activate);row.addEventListener('keydown',e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();activate();}});list.append(row);
  }
  if(!state.logs.length)list.append(node('div','empty muted','没有提交记录'));
  $('more-log').classList.toggle('hidden',state.logs.length===0||state.logs.at(-1).revision<=1);
}
function renderLogDetail(log) {
  const el=$('log-detail');el.replaceChildren(node('strong','',`版本 ${log.revision}`),node('div','muted small',`${log.author} · ${new Date(log.date).toLocaleString()}`),node('pre','',log.message));
  for(const path of log.paths) {
    const button=node('button','log-path');button.title=path.path;button.append(node('span','badge',path.action),node('span','truncate',path.path));
    button.addEventListener('dblclick',()=>request('history-diff',{path:path.path,revision:log.revision}));
    button.addEventListener('keydown',e=>{if(e.key==='Enter'){e.preventDefault();request('history-diff',{path:path.path,revision:log.revision});}});el.append(button);
  }
}
function renderRoots(roots) {
  state.roots=roots;
  const select=$('project-root');select.replaceChildren();
  for(const root of roots){const label=root.replace(/\\/g,'/').split('/').filter(Boolean).at(-1)||root;const duplicate=roots.filter(r=>r.replace(/\\/g,'/').split('/').filter(Boolean).at(-1)===label).length>1;const option=node('option','',duplicate?root:label);option.value=root;option.title=root;select.append(option);}
  select.value=state.pendingRoot||state.root||roots[0]||'';select.title=select.value;
}
function resetDirectory() {
  state.entries=[];state.selected=new Set();state.active=null;state.logs=[];state.historyPaths=[];state.info=null;state.lastRefresh=0;
  $('filter').value='';$('message').value='';$('output').textContent='';$('repository').textContent='正在读取工作副本…';$('repository').title='';
  renderFiles();renderHistory();$('log-detail').replaceChildren(node('div','empty muted','选择一条提交记录'));switchTab('changes',false);
}
function selectRoot(directory) {
  if(state.busy||directory===state.root){$('project-root').value=state.root;return;}
  state.pendingRoot=directory;request('select-root',{directory});
}

function selectedOrActive() {return state.active?[state.active]:[...state.selected];}
function fileAction(action) {const paths=selectedOrActive();if(!paths.length){notice('请先点击一个文件，或勾选要操作的文件。');return;}request(action,{paths});}

if(bridge) {
  bridge.onMessage(text=>{
    let message;try{message=typeof text==='string'?JSON.parse(text):text;}catch{return;}
    if(message.type==='menu-pending'){state.pendingMenu=true;if(!state.busy){state.pendingMenu=false;request('refresh');}return;}
    if(message.type==='invalidate'&&message.client===state.client)return;
    if(!acceptResponse(state,message))return;
    if(message.type==='hello'&&message.root!==state.root)resetDirectory();
    if(message.error&&state.pendingRoot){state.pendingRoot=null;$('project-root').value=state.root;}
    if(message.type==='hello')state.pendingRoot=null;
    if(message.root){state.root=message.root;$('root').textContent=message.root;$('root').title=message.root;}
    if(message.settings){state.refreshSettings=message.settings;$('auto-refresh').checked=message.settings.enabled;$('refresh-interval').value=String(message.settings.intervalSeconds);state.lastRefresh=Date.now();}
    if(message.roots)renderRoots(message.roots);
    if(message.output)output(message.output);
    if(message.info){state.info=message.info;$('repository').textContent=`${message.info.url} · r${message.info.revision}`;$('repository').title=message.info.url;}
    if(message.entries){const changed=snapshotChanged(state.entries,message.entries);state.entries=message.entries;state.selected=reconcileSelection(state.selected,state.entries);if(!state.entries.some(e=>e.path===state.active))state.active=null;state.lastRefresh=Date.now();if(changed)renderFiles();}
    if(message.logs){state.logs=message.append?[...state.logs,...message.logs.filter(l=>!state.logs.some(old=>old.revision===l.revision))]:message.logs;if(!message.append)$('log-detail').replaceChildren(node('div','empty muted','选择一条提交记录'));renderHistory();}
    if(message.selectPaths){state.selected=new Set(message.selectPaths.filter(p=>state.entries.some(e=>e.path===p&&committable(e))));state.active=message.selectPaths[0]||null;renderFiles();}
    if(message.clearMessage)$('message').value='';
    if(message.type==='hello'){$('message').value=message.draft||'';updateSelection();}
    if(message.error){notice(message.error);$('auto-refresh').checked=state.refreshSettings.enabled;$('refresh-interval').value=String(state.refreshSettings.intervalSeconds);}
    setBusy(Boolean(message.busy),message.status);
    if(message.menuDirectory&&message.menuDirectory!==state.root&&!state.busy){selectRoot(message.menuDirectory);return;}
    if(message.type==='hello'){request('refresh');}
    if(message.menu){
      const {action,paths=[]}=message.menu;
      state.active=paths[0]||null;
      if(paths.length)state.selected=new Set(paths.filter(p=>state.entries.some(e=>e.path===p&&committable(e))));
      renderFiles();
      if(action==='log'){state.historyPaths=[...paths];switchTab('history',false);if(!state.busy)request('log',{paths:state.historyPaths});}
      else if(['diff','add','revert'].includes(action)&&paths.length)request(action,{paths});
      else if(action==='update')request('update');
      else switchTab('changes');
    }
    if((message.needsRefresh||state.pendingMenu)&&!message.busy&&!state.busy){state.pendingMenu=false;request('refresh');}
  });
  $('refresh-interval').value='30';$('refresh-interval').disabled=true;
  send('hello');
} else {notice('此面板需要在 OpenIDEA 插件工具窗口中运行。');setBusy(false,'尚未连接 OpenIDEA');}

function saveRefreshSettings() {
  if(state.busy)return;
  const intervalSeconds=Number($('refresh-interval').value);
  if(!Number.isInteger(intervalSeconds)||intervalSeconds<5||intervalSeconds>3600){
    notice('自动刷新间隔须为 5–3600 秒的整数。');$('refresh-interval').value=String(state.refreshSettings.intervalSeconds);return;
  }
  request('settings',{settings:{enabled:$('auto-refresh').checked,intervalSeconds}});
}
$('auto-refresh').addEventListener('change',saveRefreshSettings);
$('refresh-interval').addEventListener('change',saveRefreshSettings);
$('project-root').addEventListener('change',()=>selectRoot($('project-root').value));
$('filter').addEventListener('input',renderFiles);
$('message').addEventListener('input',()=>{updateSelection();if(!state.busy)send('draft',{message:$('message').value},false);});
$('message').addEventListener('keydown',e=>{if(e.key==='Enter'&&(e.metaKey||e.ctrlKey)&&!$('commit').disabled){e.preventDefault();$('commit').click();}});
$('check-all').addEventListener('change',()=>{for(const entry of visibleEntries().filter(committable)){if($('check-all').checked)state.selected.add(entry.path);else state.selected.delete(entry.path);}renderFiles();});
for(const button of document.querySelectorAll('[data-tab]'))button.addEventListener('click',()=>switchTab(button.dataset.tab));
for(const action of ['refresh','update','cleanup','authenticate'])$(action).addEventListener('click',()=>request(action));
for(const action of ['diff','open','add','delete','revert'])$(action).addEventListener('click',()=>fileAction(action));
$('resolve').addEventListener('change',()=>{const accept=$('resolve').value;if(accept){const paths=selectedOrActive();if(paths.length)request('resolve',{paths,accept});else notice('请先点击需要解决冲突的文件。');}$('resolve').value='';});
$('commit').addEventListener('click',()=>request('commit',{paths:[...state.selected],message:$('message').value}));
$('cancel').addEventListener('click',()=>send('cancel',{},false));
$('reload-log').addEventListener('click',()=>request('log',{paths:state.historyPaths}));
$('project-log').addEventListener('click',()=>{state.historyPaths=[];request('log',{paths:[]});});
$('more-log').addEventListener('click',()=>{const revision=state.logs.at(-1)?.revision;if(revision>1)request('log',{paths:state.historyPaths,revision:revision-1,append:true});});
$('clear-output').addEventListener('click',()=>{$('output').textContent='';});
document.addEventListener('visibilitychange',()=>{state.documentVisible=!document.hidden;});
new IntersectionObserver(entries=>{state.visible=entries.some(entry=>entry.isIntersecting);}).observe(document.querySelector('main'));
setInterval(()=>{
  if(!bridge)return;
  if(state.busy)send('poll',{},false);
  else if(shouldRefresh({enabled:state.refreshSettings.enabled,visible:state.visible&&state.documentVisible,busy:state.busy,lastRefresh:state.lastRefresh,now:Date.now(),interval:state.refreshSettings.intervalSeconds*1000})&&state.root)request('refresh');
},300);
