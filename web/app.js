import {iconFor} from './file-icons.js';
import {reconcileSelection, committable, shouldRefresh, acceptResponse, snapshotChanged, buildChangeTree, groupSelection, selectGroup} from './model.js';

const $ = id => document.getElementById(id);
const bridge = window.openidea;
const state = {client:`svn-${Date.now().toString(36)}-${Math.random().toString(36).slice(2,10)}`,root:null,pendingRoot:null,roots:[],generation:0,entries:[],selected:new Set(),active:null,activeFolder:null,includeIgnored:false,logs:[],historyPaths:[],pendingHistoryPaths:null,historyHasMore:false,historyLimitReached:false,busy:false,inFlightAction:null,showRefreshPending:false,lastRefresh:0,visible:true,documentVisible:!document.hidden,tab:'changes',info:null,refreshSettings:{enabled:false,intervalSeconds:30},viewMode:'tree',collapsed:new Set()};
const labels = {modified:'修改',added:'新增',deleted:'删除',missing:'缺失',unversioned:'未跟踪',conflicted:'冲突',replaced:'替换（需终端确认）',external:'外部副本',normal:'属性修改',obstructed:'阻塞',incomplete:'不完整',ignored:'已忽略'};
const badges = {modified:'M',added:'A',deleted:'D',missing:'!',unversioned:'?',conflicted:'C',replaced:'R',external:'X',ignored:'I',normal:'P',obstructed:'~',incomplete:'!'};

function send(action, extra={}, next=true) {
  if (!bridge) return;
  if (next) state.generation++;
  bridge.postMessage(JSON.stringify({client:state.client,generation:state.generation,root:state.root,action,...extra}));
}
function request(action, extra={}) {
  if (state.busy) return;
  if (action==='refresh') {state.showRefreshPending=false;state.lastRefresh=Date.now();extra={includeIgnored:$('show-ignored').checked,...extra};}
  if(action==='log')state.pendingHistoryPaths=[...(extra.paths||[])];
  state.inFlightAction=action;
  notice(''); setBusy(true, '正在执行…'); send(action,extra);
}
function notice(text) { $('notice').textContent=text; $('notice').classList.toggle('hidden',!text); }
function output(text) { if (!text) return; const el=$('output');el.textContent=(el.textContent+text).slice(-500000);el.scrollTop=el.scrollHeight; }
function setBusy(busy,label) {
  state.busy=busy;
  $('message').disabled=busy;
  $('project-root').disabled=busy;
  $('auto-refresh').disabled=busy;
  $('changes-view').disabled=busy;
  $('check-all').disabled=busy;
  $('show-ignored').disabled=busy;
  for(const el of document.querySelectorAll('#files input[type=checkbox]'))el.disabled=busy||el.dataset.eligible!=='true';
  $('refresh-interval').disabled=busy||!state.refreshSettings.enabled;
  $('status').textContent=label || (busy?'正在执行…':`${state.entries.length} 项本地变更`);
  $('cancel').classList.toggle('hidden',!busy);
  for (const el of document.querySelectorAll('[data-write],#refresh,#reload-log,#more-log,#authenticate,#diff,#open')) el.disabled=busy;
  $('project-log').disabled=busy;
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
function focusRow(path,activate=false) {const row=[...$('files').children].find(el=>el.dataset.path===path);if(row&&activate)activatePath(row.dataset.entryPath||null,row.dataset.directory==='true'?path:null);row?.focus?.();}
function activatePath(path,folder=null) {
  state.active=path;state.activeFolder=folder;
  for(const row of $('files').children)row.classList.toggle('active',Boolean(path||folder)&&row.dataset.path===(folder||path));
}
function renderFiles() {
  const files=$('files');files.replaceChildren();const entries=visibleEntries();const tree=state.viewMode==='tree';
  files.setAttribute('role',tree?'tree':'list');
  $('changes-view').value=state.viewMode;
  $('expand-all').classList.toggle('hidden',!tree);$('collapse-all').classList.toggle('hidden',!tree);
  $('collapse-all').disabled=Boolean($('filter').value.trim());
  $('empty').classList.toggle('hidden',state.entries.length!==0||!state.info);
  $('change-count').textContent=state.entries.length;
  function appendRow(branch,depth) {
    const entry=branch.entry;const directory=tree&&branch.directory;const hasChildren=directory&&branch.children.length>0;
    const expanded=hasChildren&&(!$('filter').value.trim()? !state.collapsed.has(branch.path):true);
    const scope=directory?branch.entries:(entry?[entry]:[]);
    const row=node('div',`file${directory?' directory':''}${state.active===branch.path||state.activeFolder===branch.path?' active':''}`);row.tabIndex=0;row.title=branch.path;row.dataset.path=branch.path;row.dataset.entryPath=entry?.path||'';row.dataset.directory=String(directory||entry?.kind==='dir');
    row.setAttribute('role',tree?'treeitem':'listitem');if(tree){row.setAttribute('aria-level',String(depth+1));row.setAttribute('style',`--tree-indent:${depth*16}px`);}
    if(hasChildren)row.setAttribute('aria-expanded',String(expanded));
    const toggle=()=>{if(!hasChildren||$('filter').value.trim())return;if(state.collapsed.has(branch.path))state.collapsed.delete(branch.path);else state.collapsed.add(branch.path);renderFiles();focusRow(branch.path);};
    const checkbox=node('input');checkbox.type='checkbox';const group=groupSelection(scope,state.selected);checkbox.checked=group.checked;checkbox.indeterminate=group.indeterminate;checkbox.disabled=group.disabled||state.busy;checkbox.dataset.eligible=String(!group.disabled);
    checkbox.setAttribute('aria-label',directory?`选择目录 ${branch.path} 中的可提交项`:`选择 ${branch.path}`);
    const change=()=>{state.selected=selectGroup(state.selected,scope,checkbox.checked);state.active=null;state.activeFolder=null;renderFiles();focusRow(branch.path);};
    checkbox.addEventListener('click',e=>e.stopPropagation());checkbox.addEventListener('change',change);row.append(checkbox);
    if(tree) {
      if(hasChildren){const disclosure=node('button','disclosure');disclosure.disabled=Boolean($('filter').value.trim());disclosure.setAttribute('aria-label',`${expanded?'收起':'展开'} ${branch.path}`);disclosure.addEventListener('click',e=>{e.stopPropagation();toggle();});row.append(disclosure);}
      else row.append(node('span','disclosure-placeholder'));
    }
    const icon=node('span','file-kind-icon');icon.dataset.icon=iconFor(branch.path,{directory:directory||entry?.kind==='dir',expanded});icon.setAttribute('aria-hidden','true');row.append(icon);
    const status=entry?(entry.treeConflicted?'conflicted':entry.status):null;
    row.append(node('span','path',tree?branch.name:branch.path));
    if(directory)row.append(node('span','group-count muted',String(scope.length)));
    if(status){row.append(node('span',`badge ${status}`,badges[status]||'?'),node('span','kind',labels[status]||status));row.dataset.status=status;}
    row.addEventListener('click',()=>activatePath(entry?entry.path:null,directory||entry?.kind==='dir'?branch.path:null));
    if(directory)row.addEventListener('dblclick',e=>{if(!['input','button'].includes(e.target.tagName.toLowerCase()))toggle();});
    else if(entry&&entry.kind!=='dir')row.addEventListener('dblclick',()=>request('diff',{paths:[entry.path]}));
    row.addEventListener('keydown',e=>{
      if(e.target!==row)return;
      if(e.key==='ArrowDown'||e.key==='ArrowUp'){e.preventDefault();const rows=[...files.children].filter(el=>el.dataset.path);const i=rows.indexOf(row);focusRow(rows[i+(e.key==='ArrowDown'?1:-1)]?.dataset.path,true);}
      if(e.key==='ArrowRight'&&hasChildren){e.preventDefault();if(!expanded){state.collapsed.delete(branch.path);renderFiles();focusRow(branch.path,true);}else focusRow(branch.children[0].path,true);}
      if(e.key==='ArrowLeft'&&tree){e.preventDefault();if(hasChildren&&expanded&&!$('filter').value.trim()){state.collapsed.add(branch.path);renderFiles();focusRow(branch.path,true);}else if(branch.path.includes('/'))focusRow(branch.path.slice(0,branch.path.lastIndexOf('/')),true);}
      if(e.key==='Enter'){e.preventDefault();if(directory)toggle();else if(entry)request('diff',{paths:[entry.path]});}
      if(e.key===' '&&!checkbox.disabled){e.preventDefault();checkbox.checked=!checkbox.checked;change();}
    });
    files.append(row);
    if(expanded)for(const child of branch.children)appendRow(child,depth+1);
  }
  if(tree)for(const branch of buildChangeTree(entries))appendRow(branch,0);
  else for(const entry of entries)appendRow({entry,path:entry.path,name:entry.path,directory:false,children:[]},0);
  if(!entries.length&&state.entries.length)files.append(node('div','empty muted','没有匹配的路径'));
  if(state.active&&![...files.children].some(row=>row.dataset.path===state.active))state.active=null;
  if(state.activeFolder&&![...files.children].some(row=>row.dataset.path===state.activeFolder))state.activeFolder=null;
  updateSelection();
}
function switchTab(tab, load=true) {
  state.tab=tab;
  for(const button of document.querySelectorAll('[data-tab]')) {const active=button.dataset.tab===tab;button.setAttribute('aria-selected',String(active));$(`${button.dataset.tab}-page`).classList.toggle('hidden',!active);}
  if(load&&tab==='history'&&!state.logs.length&&!state.busy)request('log',{paths:state.historyPaths});
}
function renderHistory() {
  $('history-scope').textContent=state.historyPaths.length?`文件：${state.historyPaths.join(', ')}`:'当前项目 · 双击路径查看历史差异';
  if(state.historyLimitReached)$('history-scope').textContent+=' · 已达日志加载上限，请使用文件历史缩小范围';
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
  $('more-log').classList.toggle('hidden',!state.historyHasMore);
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
  state.entries=[];state.selected=new Set();state.active=null;state.activeFolder=null;state.logs=[];state.historyPaths=[];state.pendingHistoryPaths=null;state.historyHasMore=false;state.historyLimitReached=false;state.info=null;state.showRefreshPending=false;state.lastRefresh=0;state.collapsed=new Set();
  $('filter').value='';$('message').value='';$('output').textContent='';$('repository').textContent='正在读取工作副本…';$('repository').title='';
  renderFiles();renderHistory();$('log-detail').replaceChildren(node('div','empty muted','选择一条提交记录'));switchTab('changes',false);
}
function selectRoot(directory) {
  if(state.busy||directory===state.root){$('project-root').value=state.root;return;}
  state.pendingRoot=directory;request('select-root',{directory});
}

function selectedOrActive() {return state.active?[state.active]:[...state.selected];}
function fileAction(action) {if(action==='revert'&&state.activeFolder){request(action,{directory:state.activeFolder,paths:[state.activeFolder]});return;}if(state.activeFolder&&!state.active){notice('请选择具体文件，或使用还原处理整个目录。');return;}const paths=selectedOrActive();if(!paths.length){notice('请先点击一个文件，或勾选要操作的文件。');return;}request(action,{paths});}

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
    if(typeof message.includeIgnored==='boolean'){state.includeIgnored=message.includeIgnored;$('show-ignored').checked=message.includeIgnored;}
    if(message.viewMode){state.viewMode=message.viewMode;renderFiles();}
    if(message.roots)renderRoots(message.roots);
    if(message.output)output(message.output);
    if(message.info){state.info=message.info;$('repository').textContent=`${message.info.url} · r${message.info.revision}`;$('repository').title=message.info.url;}
    if(message.entries){const changed=snapshotChanged(state.entries,message.entries);state.entries=message.entries;state.selected=reconcileSelection(state.selected,state.entries);if(!state.entries.some(e=>e.path===state.active))state.active=null;state.lastRefresh=Date.now();if(changed)renderFiles();}
    if(message.logs){if(state.pendingHistoryPaths!==null)state.historyPaths=state.pendingHistoryPaths;state.pendingHistoryPaths=null;state.logs=message.append?[...state.logs,...message.logs.filter(l=>!state.logs.some(old=>old.revision===l.revision))]:message.logs;state.historyHasMore=Boolean(message.hasMore);state.historyLimitReached=Boolean(message.historyLimitReached);if(!message.append)$('log-detail').replaceChildren(node('div','empty muted','选择一条提交记录'));renderHistory();}
    if(message.selectPaths){state.activeFolder=null;state.selected=new Set(message.selectPaths.filter(p=>state.entries.some(e=>e.path===p&&committable(e))));state.active=message.selectPaths[0]||null;renderFiles();}
    if(message.clearMessage)$('message').value='';
    if(message.type==='hello'){$('message').value=message.draft||'';updateSelection();}
    if(message.error){state.pendingHistoryPaths=null;notice(message.error);$('show-ignored').checked=state.includeIgnored;$('changes-view').value=state.viewMode;$('auto-refresh').checked=state.refreshSettings.enabled;$('refresh-interval').value=String(state.refreshSettings.intervalSeconds);}
    setBusy(Boolean(message.busy),message.status);
    if(!state.busy)state.inFlightAction=null;
    if(message.menuDirectory&&message.menuDirectory!==state.root&&!state.busy){selectRoot(message.menuDirectory);return;}
    if(message.type==='hello'){request('refresh');}
    if(message.menu){
      const {action,paths=[]}=message.menu;
      state.active=paths[0]||null;state.activeFolder=null;
      if(paths.length)state.selected=new Set(paths.filter(p=>state.entries.some(e=>e.path===p&&committable(e))));
      renderFiles();
      if(action==='log'){switchTab('history',false);if(!state.busy)request('log',{paths});}
      else if(['diff','add','revert'].includes(action)&&paths.length)request(action,{paths});
      else if(action==='update')request('update');
      else switchTab('changes');
    }
    if((message.needsRefresh||state.pendingMenu||state.showRefreshPending&&state.visible&&state.documentVisible)&&!message.busy&&!state.busy){state.pendingMenu=false;request('refresh');}
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
$('show-ignored').addEventListener('change',()=>request('refresh'));
$('ignore-action').addEventListener('change',()=>{const action=$('ignore-action').value;if(action)fileAction(action);$('ignore-action').value='';});
$('changes-view').addEventListener('change',()=>request('view-mode',{viewMode:$('changes-view').value}));
$('expand-all').addEventListener('click',()=>{state.collapsed.clear();renderFiles();});
$('collapse-all').addEventListener('click',()=>{if($('filter').value.trim())return;const collect=nodes=>{for(const n of nodes){if(n.directory)state.collapsed.add(n.path);collect(n.children);}};collect(buildChangeTree(state.entries));renderFiles();});
$('project-root').addEventListener('change',()=>selectRoot($('project-root').value));
$('filter').addEventListener('input',renderFiles);
$('message').addEventListener('input',()=>{updateSelection();if(!state.busy)send('draft',{message:$('message').value},false);});
$('message').addEventListener('keydown',e=>{if(e.key==='Enter'&&(e.metaKey||e.ctrlKey)&&!$('commit').disabled){e.preventDefault();$('commit').click();}});
$('check-all').addEventListener('change',()=>{state.active=null;state.activeFolder=null;for(const entry of visibleEntries().filter(committable)){if($('check-all').checked)state.selected.add(entry.path);else state.selected.delete(entry.path);}renderFiles();});
for(const button of document.querySelectorAll('[data-tab]'))button.addEventListener('click',()=>switchTab(button.dataset.tab));
for(const action of ['refresh','update','cleanup','authenticate'])$(action).addEventListener('click',()=>request(action));
for(const action of ['diff','open','add','delete','revert'])$(action).addEventListener('click',()=>fileAction(action));
$('resolve').addEventListener('change',()=>{const accept=$('resolve').value;if(accept){const paths=selectedOrActive();if(paths.length)request('resolve',{paths,accept});else notice('请先点击需要解决冲突的文件。');}$('resolve').value='';});
$('commit').addEventListener('click',()=>request('commit',{paths:[...state.selected],message:$('message').value}));
$('cancel').addEventListener('click',()=>send('cancel',{},false));
$('reload-log').addEventListener('click',()=>request('log',{paths:state.historyPaths}));
$('project-log').addEventListener('click',()=>{if(state.busy)return;request('log',{paths:[]});});
$('more-log').addEventListener('click',()=>{const revision=state.logs.at(-1)?.revision;if(state.historyHasMore&&revision>1)request('log',{paths:state.historyPaths,revision:revision-1,append:true});});
$('clear-output').addEventListener('click',()=>{$('output').textContent='';});
function updateVisibility(key,value) {
  const wasVisible=state.visible&&state.documentVisible;
  state[key]=value;
  if(!wasVisible&&state.visible&&state.documentVisible&&state.root&&state.inFlightAction!=='refresh') {
    state.showRefreshPending=true;
    if(!state.busy)request('refresh');
  }
}
document.addEventListener('visibilitychange',()=>updateVisibility('documentVisible',!document.hidden));
new IntersectionObserver(entries=>updateVisibility('visible',entries.some(entry=>entry.isIntersecting))).observe(document.querySelector('main'));
setInterval(()=>{
  if(!bridge)return;
  if(state.busy)send('poll',{},false);
  else if(shouldRefresh({enabled:state.refreshSettings.enabled,visible:state.visible&&state.documentVisible,busy:state.busy,lastRefresh:state.lastRefresh,now:Date.now(),interval:state.refreshSettings.intervalSeconds*1000})&&state.root)request('refresh');
},300);
