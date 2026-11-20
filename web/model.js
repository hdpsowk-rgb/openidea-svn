export function committable(entry) {
  return !entry.treeConflicted && (['modified', 'added', 'deleted'].includes(entry.status)
    || entry.status === 'normal' && entry.props === 'modified');
}

export function reconcileSelection(selected, entries) {
  return new Set(entries.filter(entry => selected.has(entry.path) && committable(entry)).map(entry => entry.path));
}

export function shouldRefresh({enabled=false, visible, busy, lastRefresh, now, interval}) {
  return enabled && visible && !busy && now - lastRefresh >= interval;
}

export function acceptResponse(state, response) {
  return response.client === state.client && response.generation === state.generation
    && (!state.root || (response.root === state.root || response.root === state.pendingRoot));
}

export function snapshotChanged(previous, next) {
  return JSON.stringify(previous) !== JSON.stringify(next);
}

export function buildChangeTree(entries) {
  const root=new Map();
  for(const entry of entries) {
    let children=root,path='';
    const parts=entry.path.split('/');
    for(let i=0;i<parts.length;i++) {
      path=path?`${path}/${parts[i]}`:parts[i];
      if(!children.has(parts[i]))children.set(parts[i],{path,name:parts[i],entry:null,children:new Map()});
      const branch=children.get(parts[i]);if(i===parts.length-1)branch.entry=entry;children=branch.children;
    }
  }
  function finish(map) {
    const nodes=[...map.values()].map(branch=>{
      const children=finish(branch.children);
      return {...branch,entry:branch.entry||undefined,children,directory:children.length>0||branch.entry?.kind==='dir',entries:[...(branch.entry?[branch.entry]:[]),...children.flatMap(child=>child.entries)]};
    });
    return nodes.sort((a,b)=>Number(b.directory)-Number(a.directory)||a.name.localeCompare(b.name,undefined,{numeric:true}));
  }
  return finish(root);
}
export function groupSelection(entries,selected) {
  const paths=entries.filter(committable).map(entry=>entry.path);
  const count=paths.filter(path=>selected.has(path)).length;
  return {checked:paths.length>0&&count===paths.length,indeterminate:count>0&&count<paths.length,disabled:paths.length===0};
}
export function selectGroup(selected,entries,checked) {
  const result=new Set(selected);
  for(const entry of entries.filter(committable)){if(checked)result.add(entry.path);else result.delete(entry.path);}
  return result;
}
