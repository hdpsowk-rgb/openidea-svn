import test from 'node:test';
import assert from 'node:assert/strict';

class Element {
  constructor() { this.value='';this.children=[];this.listeners={};this.dataset={};this.classList={toggle(){},add(){},remove(){}}; }
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
    reply({logs:[{revision:5,author:'a',date:'2026-11-17',message:'m',paths:[]}]});
    get('reload-log').click();assert.deepEqual(sent.at(-1).paths,['one.txt']);
    reply({logs:[{revision:5,author:'a',date:'2026-11-17',message:'m',paths:[]}]});
    get('more-log').click();assert.deepEqual(sent.at(-1).paths,['one.txt']);assert.equal(sent.at(-1).revision,4);
    reply({logs:[],append:true});get('project-log').click();assert.deepEqual(sent.at(-1).paths,[]);
  } finally {globalThis.setInterval=realInterval;delete globalThis.window;delete globalThis.document;delete globalThis.IntersectionObserver;}
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
