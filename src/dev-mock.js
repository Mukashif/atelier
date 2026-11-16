// Browser-only preview: fakes the Rust commands with sample data so the UI
// can be developed and tested in a normal browser (npm run dev, then open
// http://localhost:1420). Never loaded inside the Tauri app.
import { mockIPC, mockConvertFileSrc, mockWindows } from '@tauri-apps/api/mocks';

mockWindows('main');

let statuses = ['In-Progress', 'Done', 'Delivered'];
let categories = ['Book Covers', 'Typography', 'Misc'];
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
      lastOpenedAt: mi === 0 && i < 6 ? Date.now() - i * 5 * 86_400_000 * (i > 3 ? 4 : 1) : null, favoritedAt: null, archivedAt: null,
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

const prefs = { defaultCategory: categories[0], openOnLaunch: "desk", groupByMonth: true, reduceMotion: false, checkUpdatesOnLaunch: true, lastExportAt: null };
function settings() {
  const counts = {};
  for (const p of list({})) {
    counts[`category:${p.category}`] = (counts[`category:${p.category}`] ?? 0) + 1;
    counts[`status:${p.status}`] = (counts[`status:${p.status}`] ?? 0) + 1;
  }
  return { root: "E:\Sample Library", dataDir: "C:\Users\you\AppData\Roaming\com.mani.atelier", version: "0.1.0", categories, statuses, ...prefs, counts };
}
const orphanList = [{ project: { ...projects[3], id: 999, relPath: "May 2022/Fox Cover", title: "Fox Cover", present: false }, suggestion: projects[5] }];

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
      case "get_settings": return settings();
      case "set_pref": prefs[args.key] = args.value; return null;
      case "set_list": if (args.kind === "categories") categories = args.items; else statuses = args.items; return null;
      case "rename_list_item": {
        const field = args.kind === "categories" ? "category" : "status";
        let n = 0;
        for (const p of projects) if (p[field] === args.old) { p[field] = args.new; n++; }
        if (args.kind === "categories") categories = categories.map((c) => (c === args.old ? args.new : c));
        else statuses = statuses.map((c) => (c === args.old ? args.new : c));
        return n;
      }
      case "orphans": return orphanList;
      case "relink": case "forget": orphanList.length = 0; return null;
      case "open_month_folder": return "November 2025";
      case "plugin:dialog|open": case "plugin:dialog|save": return null;
      case "export_backup": prefs.lastExportAt = Date.now(); return null;
      case "reset_data": { for (const p of projects) { p.isFavorite = false; p.isArchived = false; p.tags = []; } return projects.length; }
      case "restore_backup": return { kind: "atelier", restored: 12, missing: 1 };
      case "plugin:app|version": return "0.1.0";
      case "plugin:updater|check": return null;
      default: return null;
    }
  },
  { shouldMockEvents: true },
);
console.info('[atelier] browser preview with sample data');
