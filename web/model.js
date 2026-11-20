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
