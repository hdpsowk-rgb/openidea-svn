import test from 'node:test';
import assert from 'node:assert/strict';
class Element {
  constructor(){this.value='';this.checked=false;this.textContent='';this.children=[];this.listeners={};this.dataset={};this.classList={toggle(){},add(){},remove(){}};}
  addEventListener(type,fn){this.listeners[type]=fn;}setAttribute(){}append(...items){this.children.push(...items);}replaceChildren(...items){this.children=items;}click(){this.listeners.click?.({});}
}
test('default-off refresh stays quiet through timer, save and visibility events; enabled interval is respected',async()=>{
  const elements=new Map();const get=id=>{if(!elements.has(id))elements.set(id,new Element());return elements.get(id);};
  const tabs=['changes','history','output'].map(name=>{const el=get(name+'-tab');el.dataset.tab=name;return el;});
  const sent=[];const events={};let receive,tick,intersect;let now=100000;
  const realNow=Date.now,realInterval=globalThis.setInterval;Date.now=()=>now;globalThis.setInterval=fn=>{tick=fn;return 0;};
  globalThis.window={openidea:{postMessage(text){sent.push(JSON.parse(text));},onMessage(fn){receive=fn;}}};
  globalThis.document={hidden:false,getElementById:get,createElement(){return new Element();},querySelectorAll(s){return s==='[data-tab]'?tabs:[];},querySelector(){return get('main');},addEventListener(event,fn){events[event]=fn;}};
  globalThis.IntersectionObserver=class{constructor(fn){intersect=fn;}observe(){intersect([{isIntersecting:true}]);}};
  try {
    await import('../web/app.js?refresh-settings');
    const reply=extra=>{const r=sent.at(-1);receive({client:r.client,generation:r.generation,root:'/project',busy:false,...extra});};
    reply({type:'hello',settings:{enabled:false,intervalSeconds:30}});assert.equal(sent.at(-1).action,'refresh');reply({entries:[],info:{url:'file:///repo',revision:1}});
    const count=sent.length;now+=60000;tick();assert.equal(sent.length,count,'default-off timer triggered refresh');
    receive({type:'invalidate',client:sent[0].client});document.hidden=true;events.visibilitychange();document.hidden=false;events.visibilitychange();tick();assert.equal(sent.length,count,'save/focus bypassed OFF');
    assert.equal(get('auto-refresh').checked,false);assert.equal(get('refresh-interval').disabled,true);
    get('auto-refresh').checked=true;get('auto-refresh').listeners.change();assert.equal(sent.at(-1).action,'settings');reply({settings:{enabled:true,intervalSeconds:30}});
    get('refresh-interval').value='15';get('refresh-interval').listeners.change();assert.equal(sent.at(-1).settings.intervalSeconds,15);reply({settings:{enabled:true,intervalSeconds:15}});
    const enabledCount=sent.length;now+=14000;receive({type:'invalidate',client:sent[0].client});tick();assert.equal(sent.length,enabledCount,'file event bypassed configured interval');
    now+=1000;document.hidden=true;events.visibilitychange();tick();assert.equal(sent.length,enabledCount,'hidden panel refreshed');
    intersect([{isIntersecting:false}]);document.hidden=false;events.visibilitychange();tick();assert.equal(sent.length,enabledCount,'document focus refreshed a collapsed panel');
    intersect([{isIntersecting:true}]);tick();assert.equal(sent.at(-1).action,'refresh');reply({entries:[]});
    for(const value of ['', '0','4','1.5','3601']){const before=sent.length;get('refresh-interval').value=value;get('refresh-interval').listeners.change();assert.equal(sent.length,before,`accepted invalid interval ${value}`);}
    get('auto-refresh').checked=false;get('auto-refresh').listeners.change();reply({settings:{enabled:false,intervalSeconds:15}});
    const offCount=sent.length;now+=3600000;tick();assert.equal(sent.length,offCount);get('refresh').click();assert.equal(sent.at(-1).action,'refresh');tick();assert.equal(sent.at(-1).action,'poll','progress polling was disabled with auto refresh');
  } finally{Date.now=realNow;globalThis.setInterval=realInterval;delete globalThis.window;delete globalThis.document;delete globalThis.IntersectionObserver;}
});
