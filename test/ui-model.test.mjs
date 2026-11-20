import test from 'node:test';
import assert from 'node:assert/strict';
import { reconcileSelection, committable, shouldRefresh, acceptResponse } from '../web/model.js';

test('refresh keeps checked paths only while they are still eligible', () => {
  const entries = [{path:'a',status:'modified'}, {path:'b',status:'conflicted'}, {path:'new',status:'added'}];
  assert.deepEqual([...reconcileSelection(new Set(['a','b','gone']),entries)], ['a']);
  assert.equal(committable({status:'normal',props:'modified'}), true);
  assert.equal(committable({status:'modified',treeConflicted:true}), false);
  assert.equal(committable({status:'unversioned'}), false);
});

test('automatic refresh pauses in hidden views and during operations', () => {
  assert.equal(shouldRefresh({enabled:true,visible:false,busy:false,lastRefresh:0,now:99999,interval:5000}),false);
  assert.equal(shouldRefresh({enabled:true,visible:true,busy:true,lastRefresh:0,now:99999,interval:5000}),false);
  assert.equal(shouldRefresh({enabled:true,visible:true,busy:false,lastRefresh:1000,now:5999,interval:5000}),false);
  assert.equal(shouldRefresh({enabled:true,visible:true,busy:false,lastRefresh:1000,now:6000,interval:5000}),true);
});

test('responses from another panel client or an older request are ignored', () => {
  const state = {client:'one',root:'/a',generation:3};
  assert.equal(acceptResponse(state,{client:'two',root:'/a',generation:3}),false);
  assert.equal(acceptResponse(state,{client:'one',root:'/a',generation:2}),false);
  assert.equal(acceptResponse(state,{client:'one',root:'/b',generation:3}),false);
  assert.equal(acceptResponse(state,{client:'one',root:'/a',generation:3}),true);
  assert.equal(acceptResponse({...state,root:null},{client:'one',root:'/a',generation:3}),true);
});

test('unchanged refresh snapshots keep the current file controls mounted', async () => {
  const {snapshotChanged}=await import('../web/model.js');
  const before=[{path:'a',status:'modified',props:'none'}];
  assert.equal(snapshotChanged(before,[{path:'a',status:'modified',props:'none'}]),false);
  assert.equal(snapshotChanged(before,[{path:'a',status:'deleted',props:'none'}]),true);
  assert.equal(snapshotChanged(before,[]),true);
});
