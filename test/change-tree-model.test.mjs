import test from 'node:test';
import assert from 'node:assert/strict';
import * as model from '../web/model.js';

test('change tree keeps explicit directory changes and distinct full paths',()=>{
  const entries=[{path:'z.txt',status:'modified'},{path:'src/b/same.js',status:'modified'},{path:'src/a/same.js',status:'added'},{path:'src/a',kind:'dir',status:'added'},{path:'__proto__/constructor/file.txt',status:'modified'}];
  const tree=model.buildChangeTree(entries);assert.deepEqual(tree.map(n=>n.path),['__proto__','src','z.txt']);
  const src=tree.find(n=>n.path==='src');assert.equal(src.entry,undefined);assert.deepEqual(src.children.map(n=>n.path),['src/a','src/b']);assert.equal(src.children[0].entry,entries[3]);
  assert.deepEqual(src.entries.map(e=>e.path).sort(),['src/a','src/a/same.js','src/b/same.js']);assert.equal(src.children[1].children[0].name,'same.js');
  assert.deepEqual(entries.map(e=>e.path),['z.txt','src/b/same.js','src/a/same.js','src/a','__proto__/constructor/file.txt']);
});
test('directory checkbox state and updates include only eligible explicit entries',()=>{
  const entries=[{path:'folder/a',status:'modified'},{path:'folder/new',status:'unversioned'},{path:'folder/conflict',status:'added',treeConflicted:true},{path:'folder/b',status:'added'}];
  const selected=new Set(['other/file','folder/a']);const state=model.groupSelection(entries,selected);assert.equal(state.checked,false);assert.equal(state.indeterminate,true);assert.equal(state.disabled,false);
  const all=model.selectGroup(selected,entries,true);assert.deepEqual([...all].sort(),['folder/a','folder/b','other/file']);assert.deepEqual([...selected],['other/file','folder/a']);
  assert.equal(model.groupSelection(entries,all).checked,true);assert.deepEqual([...model.selectGroup(all,[entries[0]],false)].sort(),['folder/b','other/file']);assert.equal(model.groupSelection([entries[1]],all).disabled,true);
});
