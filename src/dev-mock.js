// Browser-only preview: fakes the Rust commands with sample data so the UI
// can be developed and tested in a normal browser (npm run dev, then open
// http://localhost:1420). Never loaded inside the Tauri app.
import { mockIPC, mockConvertFileSrc, mockWindows } from '@tauri-apps/api/mocks';

mockWindows('main');

const statuses = ['In-Progress', 'Done', 'Delivered'];
const categories = ['Book Covers', 'Typography', 'Misc'];
const names = ['Salt Orchard', 'Ember', 'Ninth Tide', 'Glass Heron', 'Quiet Cartography', 'Small Hours', 'Last Archivist',
  'Iron and Ivy', 'Paper Lanterns', 'Winter Ledger', 'Marrow & Moss', 'Hollow Crown', 'Letters to the Sea', 'Fox Who Stayed'];
const months = ['November 2025', 'October 2025', 'September 2025', 'August 2025', 'Misc'];

let id = 0;
const projects = [];
months.forEach((m, mi) => {
  const n = mi === 1 ? 30 : 9;
  for (let i = 0; i < n; i++) {
    const name = `${names[(i + mi * 3) % names.length]}${i >= names.length ? ` ${i}` : ''}`;
    projects.push({
      id: ++id, relPath: `${m}/${name}`, title: name, folderName: name, monthFolder: m,
      year: m === 'Misc' ? null : 2025, month: m === 'Misc' ? null : 11 - mi,
      category: categories[i % 3], status: statuses[(i + mi) % 3], tags: i % 4 === 0 ? ['fantasy'] : [],
      isFavorite: i % 7 === 0, isArchived: i % 11 === 10, present: true,
      coverKind: i % 9 === 4 ? 'unsupported' : i % 13 === 6 ? null : 'image',
      coverRel: i % 9 === 4 ? 'cover_final.psd' : 'cover.jpg', manualCover: null, coverMtime: 1,
      lastOpenedAt: null, favoritedAt: null, archivedAt: null,
    });
  }
});

function list(q) {
  let rows = projects.filter((p) => p.isArchived === !!q.archived && (!q.favoritesOnly || p.isFavorite));
  if (q.search) {
    const s = q.search.toLowerCase();
    rows = rows.filter((p) => p.title.toLowerCase().includes(s) || p.tags.some((t) => t.includes(s)));
  }
  rows = [...rows];
  if (q.sort === 'name') rows.sort((a, b) => a.title.localeCompare(b.title));
  else rows.sort((a, b) => (a.year == null) - (b.year == null) || (b.year ?? 0) - (a.year ?? 0) || (b.month ?? 0) - (a.month ?? 0) || a.title.localeCompare(b.title));
  return rows.map((p) => ({ ...p, tags: [...p.tags] }));
}

function edit(ids, e) {
  for (const p of projects.filter((p) => ids.includes(p.id))) {
    if (e.kind === 'title') p.title = e.value;
    if (e.kind === 'status') p.status = e.value;
    if (e.kind === 'category') p.category = e.value;
    if (e.kind === 'favorite') p.isFavorite = e.value;
    if (e.kind === 'archived') p.isArchived = e.value;
    if (e.kind === 'addTag') p.tags.push(e.value);
    if (e.kind === 'removeTag') p.tags = p.tags.filter((t) => t !== e.value);
  }
  return ids.length;
}

mockConvertFileSrc('windows');
mockIPC(
  (cmd, args) => {
    switch (cmd) {
      case 'app_info':
        return {
          root: 'E:\\Sample Library', rootExists: true, bookrack: null, categories, statuses, tags: [['fantasy', 5]],
          counts: { library: list({}).length, favorites: list({ favoritesOnly: true }).length, archive: list({ archived: true }).length, missing: 0 },
          busy: false,
        };
      case 'list_projects': return list(args.query);
      case 'apply_edit': return edit(args.ids, args.edit);
      default: return null;
    }
  },
  { shouldMockEvents: true },
);
console.info('[atelier] browser preview with sample data');
