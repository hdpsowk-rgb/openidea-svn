import test from 'node:test';
import assert from 'node:assert/strict';

class Element {
  constructor(tag='div'){this.tagName=tag;this.value='';this.textContent='';this.children=[];this.listeners={};this.dataset={};this.attributes={};this.classList={toggle(){},add(){},remove(){}};}
  addEventListener(type,fn){this.listeners[type]=fn;}setAttribute(k,v){this.attributes[k]=String(v);}append(...v){this.children.push(...v);}replaceChildren(...v){this.children=v;}focus(){}click(){this.fire('click');}
  fire(type,extra={}){this.listeners[type]?.({target:this,preventDefault(){},stopPropagation(){},...extra});}
}
test('tree grouping, filtering and folder checks preserve exact selected SVN targets',async()=>{
  const elements=new Map();const get=id=>{if(!elements.has(id))elements.set(id,new Element());return elements.get(id);};
  const tabs=['changes','history','output'].map(name=>{const el=get(name+'-tab');el.dataset.tab=name;return el;});
  const sent=[];let receive;const originalInterval=globalThis.setInterval;globalThis.setInterval=()=>0;
  globalThis.window={openidea:{postMessage(s){sent.push(JSON.parse(s));},onMessage(fn){receive=fn;}}};
  globalThis.document={hidden:false,getElementById:get,createElement(tag){return new Element(tag);},querySelectorAll(s){return s==='[data-tab]'?tabs:[];},querySelector(){return get('main');},addEventListener(){}};
  globalThis.IntersectionObserver=class{observe(){}};
  try {
    await import('../web/app.js?change-tree-test');
    const reply=extra=>{const r=sent.at(-1);receive({client:r.client,generation:r.generation,root:r.root||'/project',busy:false,...extra});};
    const entries=[{path:'frontend/src/a.js',status:'modified',kind:'file'},{path:'frontend/src/b.js',status:'added',kind:'file'},{path:'frontend/src/conflict.js',status:'conflicted',kind:'file'},{path:'frontend/new',status:'added',kind:'dir'},{path:'frontend/new/value.txt',status:'added',kind:'file'},{path:'backend/api.php',status:'modified',kind:'file'},{path:'z-untracked.txt',status:'unversioned',kind:'file'}];
    reply({type:'hello',viewMode:'tree'});reply({entries,info:{url:'file:///repo',revision:1}});
    const row=path=>get('files').children.find(el=>el.dataset.path===path);
    const checkbox=path=>row(path)?.children.find(el=>el.tagName==='input');
    const disclosure=path=>row(path)?.children.find(el=>el.tagName==='button');
    assert.ok(row('frontend'),'directory ancestors were not rendered');assert.ok(row('frontend/src'));assert.equal(row('frontend/src/a.js').children.find(el=>el.className==='path').textContent,'a.js');
    assert.equal(row('frontend/src/a.js').title,'frontend/src/a.js');
    row('backend/api.php').click();checkbox('frontend').checked=true;checkbox('frontend').fire('change');
    assert.equal(checkbox('frontend/src/conflict.js').disabled,true);
    get('message').value='selected tree files';get('commit').click();assert.equal(sent.at(-1).action,'commit');
    assert.deepEqual([...sent.at(-1).paths].sort(),['frontend/new','frontend/new/value.txt','frontend/src/a.js','frontend/src/b.js']);reply({});
    get('revert').click();assert.deepEqual([...sent.at(-1).paths].sort(),['frontend/new','frontend/new/value.txt','frontend/src/a.js','frontend/src/b.js'],'stale active file overrode folder selection');reply({});
    disclosure('frontend').click();assert.equal(row('frontend/src/a.js'),undefined);assert.equal(checkbox('frontend').checked,true);
    get('filter').value='a.js';get('filter').fire('input');assert.ok(row('frontend/src/a.js'),'filter did not reveal collapsed ancestors');
    checkbox('frontend').checked=false;checkbox('frontend').fire('change');get('commit').click();assert.deepEqual([...sent.at(-1).paths].sort(),['frontend/new','frontend/new/value.txt','frontend/src/b.js'],'filtered group changed hidden selections');reply({});
    get('filter').value='';get('filter').fire('input');assert.equal(row('frontend/src/a.js'),undefined);assert.equal(checkbox('frontend').indeterminate,true);
    get('expand-all').click();assert.ok(row('frontend/src/a.js'));row('frontend/src/a.js').fire('dblclick');assert.equal(sent.at(-1).action,'diff');assert.deepEqual(sent.at(-1).paths,['frontend/src/a.js']);reply({});
    get('changes-view').value='list';get('changes-view').fire('change');assert.equal(sent.at(-1).action,'view-mode');assert.equal(sent.at(-1).viewMode,'list');reply({viewMode:'list'});
    assert.equal(row('frontend'),undefined);assert.equal(row('frontend/src/a.js').children.find(el=>el.className==='path').textContent,'frontend/src/a.js');
    get('changes-view').value='tree';get('changes-view').fire('change');reply({viewMode:'tree'});assert.ok(row('frontend'));
    get('collapse-all').click();get('refresh').click();reply({entries:[...entries,{path:'frontend/src/another.js',status:'modified',kind:'file'}]});assert.equal(row('frontend/src/a.js'),undefined,'refresh forgot collapsed state');
    get('expand-all').click();row('frontend/src/a.js').click();disclosure('frontend').click();get('revert').click();
    assert.deepEqual([...sent.at(-1).paths].sort(),['frontend/new','frontend/new/value.txt','frontend/src/b.js'],'collapsed hidden active file overrode selection');reply({});
    get('collapse-all').click();row('frontend').fire('keydown',{key:'ArrowDown'});get('add').click();
    assert.deepEqual(sent.at(-1).paths,['z-untracked.txt'],'keyboard destination did not become the active action target');reply({});
    get('filter').value='a.js';get('filter').fire('input');get('diff').click();
    assert.deepEqual([...sent.at(-1).paths].sort(),['frontend/new','frontend/new/value.txt','frontend/src/b.js'],'filtered-away active path remained an action target');

  } finally {globalThis.setInterval=originalInterval;delete globalThis.window;delete globalThis.document;delete globalThis.IntersectionObserver;}
});
