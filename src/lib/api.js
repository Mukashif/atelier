// Thin wrappers over the Rust commands (src-tauri/src/lib.rs).
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
export { listen } from '@tauri-apps/api/event';

export const api = {
  info: () => invoke('app_info'),
  list: (query) => invoke('list_projects', { query }),
  /** edit: { kind: 'status' | 'category' | 'title' | 'favorite' | 'archived' | 'addTag' | 'removeTag' | 'manualCover', value } */
  edit: (ids, edit) => invoke('apply_edit', { ids, edit }),
  openFolder: (id) => invoke('open_folder', { id }),
  setRoot: (path) => invoke('set_root', { path }),
  rescan: () => invoke('rescan'),
  importBookrack: (dataDir, root = null) => invoke('import_bookrack', { dataDir, root }),
};

/** Small (grid) or large (details) thumbnail URL; `v` busts the cache. */
export function thumbUrl(id, v = 0, large = false) {
  return `${convertFileSrc(`${id}${large ? '@2x' : ''}`, 'thumb')}?v=${v}`;
}

/** Status → dot colour token. */
export function statusColor(status, statuses) {
  const i = statuses.indexOf(status);
  if (i === 0) return 'var(--status-progress)';
  if (i === statuses.length - 1) return 'var(--status-delivered)';
  return 'var(--status-done)';
}

/** Category colour from its name: same lightness/chroma, hue from a hash (SPEC §8.1). */
export function categoryColor(name) {
  let h = 0;
  const s = name.toLowerCase();
  for (let i = 0; i < s.length; i++) h = (Math.imul(h, 31) + s.charCodeAt(i)) | 0;
  return `oklch(0.74 0.12 ${Math.abs(h) % 360})`;
}

export function monthLabel(p) {
  return p.monthFolder;
}
