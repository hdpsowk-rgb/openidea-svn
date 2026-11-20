import test from 'node:test';
import assert from 'node:assert/strict';

class Element {
  constructor() { this.value='';this.children=[];this.listeners={};this.dataset={};const classes=new Set();this.classList={toggle(name,force){if(force??!classes.has(name))classes.add(name);else classes.delete(name);},add(name){classes.add(name);},remove(name){classes.delete(name);},contains(name){return classes.has(name);}}; }
  addEventListener(type,fn){this.listeners[type]=fn;}
  setAttribute(){} append(...children){this.children.push(...children);}
  replaceChildren(...children){this.children=children;}
  click(){this.listeners.click?.({});}
}

test('file history menu, reload and pagination retain the selected file scope', async () => {
  const elements=new Map();const get=id=>{if(!elements.has(id))elements.set(id,new Element());return elements.get(id);};
  const tabs=['changes','history','output'].map(name=>{const element=get(name+'-tab');element.dataset.tab=name;return element;});
  const sent=[];let receive;
  globalThis.window={openidea:{postMessage(text){sent.push(JSON.parse(text));},onMessage(fn){receive=fn;}}};
  globalThis.document={hidden:false,getElementById:get,createElement(){return new Element();},querySelectorAll(selector){return selector==='[data-tab]'?tabs:[];},querySelector(){return get('main');},addEventListener(){}};
  globalThis.IntersectionObserver=class {observe(){}};
  const realInterval=globalThis.setInterval;globalThis.setInterval=()=>0;
  try {
    await import('../web/app.js?history-menu-test');
    const reply=extra=>{const request=sent.at(-1);receive(JSON.stringify({client:request.client,generation:request.generation,root:'/project',busy:false,...extra}));};
    reply({type:'hello'});reply({entries:[{path:'one.txt',status:'modified'}],info:{url:'file:///repo',revision:1}});
    receive(JSON.stringify({type:'menu-pending'}));
    reply({entries:[{path:'one.txt',status:'modified'}],menu:{action:'log',paths:['one.txt']}});
    assert.deepEqual(sent.filter(r=>r.action==='log').map(r=>r.paths),[['one.txt']]);
    reply({logs:[{revision:5,author:'a',date:'2026-11-17',message:'m',paths:[]}],hasMore:true});
    get('reload-log').click();assert.deepEqual(sent.at(-1).paths,['one.txt']);
    reply({logs:[{revision:5,author:'a',date:'2026-11-17',message:'m',paths:[]}],hasMore:true});
    get('more-log').click();assert.deepEqual(sent.at(-1).paths,['one.txt']);assert.equal(sent.at(-1).revision,4);
    reply({logs:[],append:true});get('project-log').click();assert.deepEqual(sent.at(-1).paths,[]);
  } finally {globalThis.setInterval=realInterval;delete globalThis.window;delete globalThis.document;delete globalThis.IntersectionObserver;}
});

async function historyPanel(name) {
  const elements=new Map();const get=id=>{if(!elements.has(id))elements.set(id,new Element());return elements.get(id);};
  const tabs=['changes','history','output'].map(name=>{const el=get(name+'-tab');el.dataset.tab=name;return el;});
  const sent=[];let receive;const realInterval=globalThis.setInterval;globalThis.setInterval=()=>0;
  globalThis.window={openidea:{postMessage(text){sent.push(JSON.parse(text));},onMessage(fn){receive=fn;}}};
  globalThis.document={hidden:false,getElementById:get,createElement(){return new Element();},querySelectorAll(s){return s==='[data-tab]'?tabs:[];},querySelector(){return get('main');},addEventListener(){}};
  globalThis.IntersectionObserver=class{observe(){}};
  await import(`../web/app.js?${name}`);
  const reply=extra=>{const r=sent.at(-1);receive({client:r.client,generation:r.generation,root:'/project',busy:false,...extra});};
  reply({type:'hello'});reply({entries:[],info:{url:'file:///repo',revision:10},menu:{action:'log',paths:['one.txt']}});
  return {get,sent,reply,cleanup(){globalThis.setInterval=realInterval;delete globalThis.window;delete globalThis.document;delete globalThis.IntersectionObserver;}};
}

test('project history cannot change scope while a file history request is pending',async()=>{
  const ui=await historyPanel('pending-history-scope');
  try {
    ui.get('project-log').click();
    ui.reply({logs:[{revision:10,author:'a',date:'2026-11-20',message:'file only',paths:[]}],hasMore:false});
    assert.equal(ui.get('history-scope').textContent,'文件：one.txt','pending file logs were mislabeled as project history');
    ui.get('reload-log').click();assert.deepEqual(ui.sent.at(-1).paths,['one.txt']);
    assert.equal(ui.get('project-log').disabled,true);
  } finally {ui.cleanup();}
});

test('history pagination stops at repository history end and the host retention limit',async()=>{
  const ui=await historyPanel('history-exhaustion');
  try {
    const log={revision:10,author:'a',date:'2026-11-20',message:'initial commit above r1',paths:[]};
    ui.reply({logs:[log],hasMore:false});
    assert.equal(ui.get('more-log').classList.contains('hidden'),true,'exhausted history still offered older commits');
    const before=ui.sent.length;ui.get('more-log').click();assert.equal(ui.sent.length,before);
    ui.get('reload-log').click();ui.reply({logs:[log],hasMore:true});
    assert.equal(ui.get('more-log').classList.contains('hidden'),false);
    ui.get('more-log').click();ui.reply({logs:[],append:true,hasMore:false,historyLimitReached:true});
    assert.equal(ui.get('more-log').classList.contains('hidden'),true);
    assert.match(ui.get('history-scope').textContent,/上限/);
  } finally {ui.cleanup();}
});

test('a failed history scope switch keeps the displayed records and reload scope together',async()=>{
  const ui=await historyPanel('failed-history-switch');
  try {
    ui.reply({logs:[{revision:10,author:'a',date:'2026-11-20',message:'file only',paths:[]}],hasMore:false});
    ui.get('project-log').click();assert.deepEqual(ui.sent.at(-1).paths,[]);
    ui.reply({error:'connection failed'});
    assert.equal(ui.get('history-scope').textContent,'文件：one.txt');
    ui.get('reload-log').click();assert.deepEqual(ui.sent.at(-1).paths,['one.txt'],'failed scope switch silently changed subsequent reloads');
  } finally {ui.cleanup();}
});

test('directory switching clears file scope, restores drafts and ignores stale replies', async () => {
  const elements=new Map();const get=id=>{if(!elements.has(id))elements.set(id,new Element());return elements.get(id);};
  const tabs=['changes','history','output'].map(name=>{const element=get(name+'-tab');element.dataset.tab=name;return element;});
  const sent=[];let receive;
  globalThis.window={openidea:{postMessage(text){sent.push(JSON.parse(text));},onMessage(fn){receive=fn;}}};
  globalThis.document={hidden:false,getElementById:get,createElement(){return new Element();},querySelectorAll(selector){return selector==='[data-tab]'?tabs:[];},querySelector(){return get('main');},addEventListener(){}};
  globalThis.IntersectionObserver=class {observe(){}};
  const realInterval=globalThis.setInterval;globalThis.setInterval=()=>0;
  try {
    await import('../web/app.js?directory-test');
    const reply=extra=>{const request=sent.at(-1);receive(JSON.stringify({client:request.client,generation:request.generation,root:request.root||'/project',busy:false,...extra}));};
    reply({type:'hello',roots:['/project','/second'],draft:'first'});
    reply({entries:[{path:'one.txt',status:'modified'}],info:{url:'file:///first',revision:1}});
    const stale={...sent.at(-1),entries:[{path:'wrong.txt',status:'modified'}],info:{url:'file:///wrong',revision:2}};
    get('filter').value='one';get('output').textContent='old output';
    get('project-root').value='/second';get('project-root').listeners.change();
    assert.equal(sent.at(-1).action,'select-root');assert.equal(sent.at(-1).directory,'/second');
    reply({root:'/second',type:'hello',roots:['/project','/second'],draft:''});
    assert.equal(sent.at(-1).action,'refresh');assert.equal(sent.at(-1).root,'/second');
    assert.equal(get('message').value,'');assert.equal(get('filter').value,'');assert.equal(get('output').textContent,'');
    reply({entries:[],info:{url:'file:///second',revision:3}});
    receive(JSON.stringify(stale));assert.equal(get('repository').textContent,'file:///second · r3');
    get('commit').click();assert.deepEqual(sent.at(-1).paths,[]);
    reply({});get('project-root').value='/project';get('project-root').listeners.change();
    reply({root:'/project',type:'hello',draft:'first'});assert.equal(get('message').value,'first');
  } finally {globalThis.setInterval=realInterval;delete globalThis.window;delete globalThis.document;delete globalThis.IntersectionObserver;}
});
