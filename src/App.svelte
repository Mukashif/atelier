<script>
  import { onMount, untrack } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { api, listen, categoryColor, statusColor } from './lib/api.js';
  import Library from './lib/Library.svelte';
  import FirstRun from './lib/FirstRun.svelte';
  import Desk from './lib/Desk.svelte';
  import Settings from './lib/Settings.svelte';
  import FilterMenu from './lib/FilterMenu.svelte';
  import BulkBar from './lib/BulkBar.svelte';
  import ContextMenu from './lib/ContextMenu.svelte';
  import Dialog from './lib/Dialog.svelte';
  import { getVersion } from '@tauri-apps/api/app';
  import { findUpdate, installAndRestart } from './lib/updates.js';

  const win = getCurrentWindow();

  let info = $state(null);
  let prefs = $state({ openOnLaunch: 'desk', groupByMonth: true, reduceMotion: false });
  let view = $state(null); // 'desk' | 'library' | 'favorites' | 'archive' | 'settings'
  let base = $state([]); // what the core returned for this view/search/sort
  let search = $state('');
  let debounced = $state('');
  let sort = $state('newest');
  let filters = $state({ categories: [], statuses: [], tags: [] });
  let busy = $state(false);
  let progress = $state({ phase: 'idle', done: 0, total: 0 });
  let versions = $state({});
  let toast = $state(null);
  let searchInput = $state();
  let scroller = $state();
  let selectMode = $state(false);
  let selectedIds = $state([]);
  let ctx = $state(null); // { x, y, p }
  let request = $state(null); // { id, rename, n } — ask the grid to select/rename
  let update = $state(null); // available update (from the updater plugin)
  let updateOpen = $state(false);
  let keysOpen = $state(false);
  const SHORTCUTS = [
    ['Anywhere', [
      [['Ctrl', 'K'], 'Search'],
      [['Ctrl', ','], 'Settings'],
      [['Alt', '←'], 'Back (or the mouse back button)'],
      [['Alt', '→'], 'Forward'],
      [['?'], 'This list'],
    ]],
    ['Covers', [
      [['←', '→', '↑', '↓'], 'Move between covers'],
      [['Enter'], 'Open folder'],
      [['F2'], 'Rename'],
      [['F'], 'Favourite on / off'],
      [['Del'], 'Archive'],
      [['Ctrl', 'A'], 'Select all (in select mode)'],
      [['Esc'], 'Close, clear search or selection, leave select mode'],
    ]],
  ];
  let installing = $state(null); // null | percent | '…'
  let appVersion = $state('');

  async function checkUpdates(loud = false) {
    const u = await findUpdate({ loud });
    if (u) update = u;
    return u?.version ?? null;
  }
  async function install() {
    installing = '…';
    try {
      await installAndRestart(update, (p) => (installing = p ?? '…'));
    } catch (e) {
      installing = null;
      showToast(`Update failed: ${e}`, true);
    }
  }

  const listView = $derived(view === 'desk' ? 'library' : view);
  const query = $derived({
    archived: listView === 'archive',
    favoritesOnly: listView === 'favorites',
    search: view === 'desk' ? null : debounced || null,
    sort,
  });

  // ---- loading
  let loadedSort = $state('newest');
  let loadSeq = 0;
  async function loadInfo() {
    info = await api.info();
    busy = info.busy;
  }
  async function loadPrefs() {
    const s = await api.settings();
    prefs = { openOnLaunch: s.openOnLaunch, groupByMonth: s.groupByMonth, reduceMotion: s.reduceMotion };
    document.documentElement.dataset.reduceMotion = String(s.reduceMotion);
  }
  async function loadProjects() {
    if (!info?.root || view === 'settings' || view == null) return;
    const q = query;
    const seq = ++loadSeq;
    const rows = await api.list(q);
    if (seq !== loadSeq) return;
    base = rows;
    loadedSort = q.sort;
  }
  $effect(() => {
    query;
    loadProjects();
  });

  // ---- filters (applied here, instantly; SPEC §6.10)
  const matches = (p, f, skip) =>
    (skip === 'categories' || !f.categories.length || f.categories.includes(p.category)) &&
    (skip === 'statuses' || !f.statuses.length || f.statuses.includes(p.status)) &&
    (skip === 'tags' || !f.tags.length || p.tags.some((t) => f.tags.includes(t)));
  const projects = $derived(view === 'desk' ? base : base.filter((p) => matches(p, filters)));
  const filterCount = $derived(filters.categories.length + filters.statuses.length + filters.tags.length);

  function optionsFor(kind) {
    const counts = new Map();
    for (const p of base) {
      if (!matches(p, filters, kind)) continue;
      const vals = kind === 'categories' ? [p.category] : kind === 'statuses' ? [p.status] : p.tags;
      for (const v of vals) counts.set(v, (counts.get(v) ?? 0) + 1);
    }
    const order = kind === 'categories' ? info.categories : kind === 'statuses' ? info.statuses : [];
    const names = kind === 'tags' ? [...counts.keys()].sort((a, b) => counts.get(b) - counts.get(a) || a.localeCompare(b)) : [...new Set([...order, ...counts.keys()])];
    return names.map((name) => ({
      name,
      count: counts.get(name) ?? 0,
      swatch: kind === 'categories' ? categoryColor(name) : kind === 'statuses' ? statusColor(name, info.statuses) : null,
      round: kind === 'statuses',
    }));
  }

  // Debounce search; typing switches to the Library.
  let timer;
  $effect(() => {
    const s = search;
    clearTimeout(timer);
    timer = setTimeout(() => {
      debounced = s.trim();
      if (debounced && !['library', 'favorites', 'archive'].includes(view)) view = 'library';
    }, 150);
  });

  onMount(() => {
    (async () => {
      await loadInfo();
      if (info.root) await loadPrefs();
      getVersion().then((v) => (appVersion = v)).catch(() => {});
      api.settings().then((s) => s.checkUpdatesOnLaunch && checkUpdates(false)).catch(() => {});
      view = info.root ? prefs.openOnLaunch : 'library';
      await loadProjects();
    })();
    const offs = [
      listen('library-changed', async () => { await loadInfo(); await loadProjects(); }),
      listen('import-done', (e) => {
        const r = e.payload;
        showToast(`Imported ${r.imported} projects from Bookrack${r.orphans ? ` · ${r.orphans} folders not found` : ''}`);
      }),
      listen('thumbs-ready', (e) => {
        const next = { ...versions };
        for (const id of e.payload) next[id] = (next[id] ?? 0) + 1;
        versions = next;
      }),
      listen('progress', (e) => (progress = e.payload)),
      listen('busy', (e) => (busy = e.payload)),
      listen('scan-error', (e) => showToast(String(e.payload), true)),
    ];
    return () => offs.forEach((p) => p.then((off) => off()));
  });

  // ---- toasts
  let toastTimer;
  function showToast(text, error = false, undo = null) {
    toast = { text, error, undo };
    clearTimeout(toastTimer);
    if (!error) toastTimer = setTimeout(() => (toast = null), 5000);
  }

  // ---- edits: screen first, then save (SPEC §3.1)
  function applyLocal(p, e) {
    switch (e.kind) {
      case 'title': return { ...p, title: e.value };
      case 'status': return { ...p, status: e.value };
      case 'category': return { ...p, category: e.value };
      case 'favorite': return { ...p, isFavorite: e.value };
      case 'archived': return { ...p, isArchived: e.value };
      case 'addTag': return p.tags.includes(e.value) ? p : { ...p, tags: [...p.tags, e.value].sort() };
      case 'removeTag': return { ...p, tags: p.tags.filter((t) => t !== e.value) };
      default: return p;
    }
  }
  const inverse = (e, before) => (e.kind === 'favorite' || e.kind === 'archived' ? { kind: e.kind, value: !e.value } : null);

  async function edit(ids, e, { quiet = false } = {}) {
    const before = base.filter((p) => ids.includes(p.id)).map((p) => ({ ...p, tags: [...p.tags] }));
    base = base.map((p) => (ids.includes(p.id) ? applyLocal(p, e) : p));
    try {
      await api.edit(ids, e);
    } catch (err) {
      base = base.map((p) => before.find((b) => b.id === p.id) ?? p);
      return showToast(`Couldn’t save: ${err}`, true);
    }
    if (quiet) return;
    const n = ids.length;
    const what = n === 1 ? `“${before[0]?.title ?? 'project'}”` : `${n} projects`;
    const undo = () => restore(before, e);
    if (e.kind === 'archived') showToast(`${e.value ? 'Archived' : 'Unarchived'} ${what}`, false, undo);
    else if (n > 1) {
      const label = { status: `Set ${what} to ${e.value}`, category: `Moved ${what} to ${e.value}`, addTag: `Tagged ${what} “${e.value}”`, favorite: `Added ${what} to Favorites` }[e.kind];
      if (label) showToast(label, false, undo);
    }
    if (e.kind === 'archived' || e.kind === 'favorite') {
      await loadInfo();
      await loadProjects();
    }
  }
  // Put each project back how it was (works for bulk edits with mixed values).
  async function restore(before, e) {
    const groups = new Map();
    for (const p of before) {
      const ed = e.kind === 'addTag' ? (p.tags.includes(e.value) ? null : { kind: 'removeTag', value: e.value })
        : e.kind === 'status' ? { kind: 'status', value: p.status }
        : e.kind === 'category' ? { kind: 'category', value: p.category }
        : e.kind === 'favorite' ? { kind: 'favorite', value: p.isFavorite }
        : e.kind === 'archived' ? { kind: 'archived', value: p.isArchived }
        : null;
      if (!ed) continue;
      const k = JSON.stringify(ed);
      groups.set(k, [...(groups.get(k) ?? []), p.id]);
    }
    for (const [k, ids] of groups) await edit(ids, JSON.parse(k), { quiet: true });
    await loadInfo();
    await loadProjects();
    toast = null;
  }

  async function bulkEdit(e) {
    const ids = [...selectedIds];
    await edit(ids, e);
    if (e.kind === 'archived') selectedIds = [];
  }

  async function openFolder(id) {
    try {
      await api.openFolder(id);
    } catch (err) {
      showToast(`Couldn’t open the folder. It may have been moved or renamed — try Rescan. (${err})`, true);
    }
  }

  async function changeCover(p) {
    const sep = info.root.includes('\\') ? '\\' : '/';
    const picked = await openDialog({
      title: `Choose a cover for “${p.title}”`,
      defaultPath: [info.root, ...p.relPath.split('/')].join(sep),
      filters: [{ name: 'Images', extensions: ['jpg', 'jpeg', 'png', 'webp'] }],
    });
    if (typeof picked !== 'string') return;
    try {
      await api.setCover(p.id, picked);
      base = base.map((x) => (x.id === p.id ? { ...x, manualCover: picked } : x));
      showToast(`New cover for “${p.title}”`);
    } catch (err) {
      showToast(String(err), true);
    }
  }

  function ctxAction(kind, value) {
    const p = ctx.p;
    if (kind === 'open') openFolder(p.id);
    else if (kind === 'rename') request = { id: p.id, rename: true, n: Date.now() };
    else if (kind === 'cover') changeCover(p);
    else edit([p.id], { kind, value });
  }

  // ---- navigation
  function setView(v) {
    view = v;
    selectMode = false;
    selectedIds = [];
    scroller?.scrollTo({ top: 0 });
  }
  function go(v, f = null) {
    filters = { categories: [], statuses: [], tags: [], ...(f ?? {}) };
    search = '';
    debounced = '';
    setView(v);
  }
  function openProject(id, openTheFolder = false) {
    if (openTheFolder) return openFolder(id);
    go('library');
    request = { id, rename: false, n: Date.now() };
  }

  // ---- view history (SPEC §6.13: mouse back/forward). Every view change is
  // recorded, whatever caused it; each entry remembers that view's filters.
  let hist = [];
  let histAt = -1;
  let travelling = false;
  $effect(() => {
    const v = view;
    if (!v) return;
    untrack(() => {
      if (travelling) { travelling = false; return; }
      if (hist[histAt]?.view === v) return;
      hist = [...hist.slice(Math.max(0, histAt - 48), histAt + 1), { view: v, filters }];
      histAt = hist.length - 1;
    });
  });
  $effect(() => {
    const f = filters;
    untrack(() => { if (hist[histAt]) hist[histAt].filters = f; });
  });
  function travel(step) {
    const entry = hist[histAt + step];
    if (!entry || !view) return;
    histAt += step;
    travelling = entry.view !== view;
    filters = entry.filters;
    search = '';
    debounced = '';
    setView(entry.view);
  }
  function onMouseNav(e) {
    // Mouse buttons 3 and 4 are the side back/forward buttons.
    if (e.button !== 3 && e.button !== 4) return;
    e.preventDefault();
    if (e.type === 'mouseup') travel(e.button === 3 ? -1 : 1);
  }

  function onGlobalKey(e) {
    const typing = e.target.closest?.('input, textarea, select');
    if (e.altKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight') && !typing) {
      e.preventDefault();
      return travel(e.key === 'ArrowLeft' ? -1 : 1);
    }
    if (e.key === '?' && !typing && !e.ctrlKey && !e.altKey && !e.metaKey) {
      e.preventDefault();
      keysOpen = !keysOpen;
      return;
    }
    if ((e.ctrlKey || e.metaKey) && (e.key === 'k' || e.key === 'f')) {
      e.preventDefault();
      if (view === 'desk' || view === 'settings') view = 'library';
      queueMicrotask(() => { searchInput?.focus(); searchInput?.select(); });
    } else if ((e.ctrlKey || e.metaKey) && e.key === ',') {
      e.preventDefault();
      setView('settings');
    } else if (e.key === 'Escape' && typing === searchInput) {
      search = '';
      searchInput.blur();
    } else if (e.key === 'Escape' && !typing && selectMode) {
      if (selectedIds.length) selectedIds = [];
      else selectMode = false;
    }
  }

  const tabs = $derived([
    { id: 'desk', name: 'Desk', count: null },
    { id: 'library', name: 'Library', count: info?.counts.library },
    { id: 'favorites', name: 'Favorites', count: info?.counts.favorites },
    { id: 'archive', name: 'Archive', count: info?.counts.archive },
  ]);
  const scanning = $derived(busy && progress.phase !== 'idle');
  const pct = $derived(progress.total ? Math.round((progress.done / progress.total) * 100) : 12);
  const title = $derived({ library: 'Library', favorites: 'Favorites', archive: 'Archive' }[view] ?? '');
  const grouped = $derived(prefs.groupByMonth && loadedSort !== 'name' && view !== 'favorites');
</script>

<svelte:window onkeydown={onGlobalKey} onmousedown={onMouseNav} onmouseup={onMouseNav} />

<div class="app">
  <header data-tauri-drag-region>
    <span class="wordmark display" data-tauri-drag-region>ATELIER</span>
    {#if info?.root}
      <nav aria-label="Views">
        {#each tabs as t}
          <button class="tab" class:active={view === t.id} aria-current={view === t.id ? 'page' : undefined} onclick={() => go(t.id)}>
            {t.name}{#if t.count != null}<span class="count">{t.count.toLocaleString('en-GB')}</span>{/if}
          </button>
        {/each}
      </nav>
    {/if}
    <div class="spacer" data-tauri-drag-region></div>
    {#if info?.root}
      <label class="search">
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><circle cx="11" cy="11" r="7"></circle><path d="M20 20l-3.5-3.5"></path></svg>
        <input bind:this={searchInput} bind:value={search} placeholder="Search covers, folders, tags" aria-label="Search projects" spellcheck="false" />
        {#if search}<button class="clear" aria-label="Clear search" onclick={() => (search = '')}>×</button>{:else}<kbd>Ctrl K</kbd>{/if}
      </label>
      <button class="icon-btn" aria-label="Rescan library" title="Rescan library" onclick={() => api.rescan()} disabled={busy}>
        <svg class:spin={busy} width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 11a8 8 0 0 0-14.9-3M4 4v4h4M4 13a8 8 0 0 0 14.9 3M20 20v-4h-4"></path></svg>
      </button>
      <button class="icon-btn" class:on={view === 'settings'} aria-label="Settings" title="Settings (Ctrl ,)" onclick={() => setView('settings')}>
        <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true"><path d="M4 7h10M18 7h2M4 17h4M12 17h8"></path><circle cx="16" cy="7" r="2"></circle><circle cx="10" cy="17" r="2"></circle></svg>
      </button>
    {/if}
    {#if update}
      <button class="chip" onclick={() => (updateOpen = true)}><span class="g"></span>Update ready · {update.version}</button>
    {/if}
    <span class="sep"></span>
    <button class="icon-btn win" aria-label="Minimise" onclick={() => win.minimize()}><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M5 12h14"></path></svg></button>
    <button class="icon-btn win" aria-label="Maximise" onclick={() => win.toggleMaximize()}><svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><rect x="5" y="5" width="14" height="14" rx="1"></rect></svg></button>
    <button class="icon-btn win close" aria-label="Close" onclick={() => win.close()}><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"></path></svg></button>
  </header>

  <div class="progress" class:on={scanning} role="progressbar" aria-label="Scanning library" aria-valuenow={pct} aria-valuemin="0" aria-valuemax="100">
    <div style:width={`${pct}%`}></div>
  </div>

  {#if !info || view == null}
    <div class="fill"></div>
  {:else if !info.root}
    <FirstRun bookrack={info.bookrack} onStarted={async () => { await loadInfo(); await loadPrefs(); view = 'library'; await loadProjects(); }} />
  {:else if view === 'settings'}
    <div class="fill"><Settings toast={showToast} onCheckUpdates={async () => { const v = await checkUpdates(true); if (v) updateOpen = true; return v; }} onChanged={async () => { await loadInfo(); await loadPrefs(); }} /></div>
  {:else if view === 'desk'}
    <main class="scroll fill" bind:this={scroller}>
      <Desk
        all={base}
        statuses={info.statuses}
        categories={info.categories}
        favorites={info.counts.favorites}
        {versions}
        {busy}
        onOpenProject={openProject}
        onGo={go}
        onEdit={edit}
        onPickCover={changeCover}
        onOpenMonth={async () => { try { const n = await api.openMonthFolder(); showToast(`Opened ${n}`); } catch (e) { showToast(String(e), true); } }}
        onOpenRoot={() => api.openSpecial('root')}
      />
    </main>
  {:else}
    {#if selectMode}
      <BulkBar
        count={selectedIds.length}
        statuses={info.statuses}
        categories={info.categories}
        tags={info.tags}
        onEdit={bulkEdit}
        onClear={() => (selectedIds = [])}
        onDone={() => { selectMode = false; selectedIds = []; }}
      />
    {:else}
      <div class="toolbar">
        <h1 class="display">{title}</h1>
        <FilterMenu label="Category" options={optionsFor('categories')} selected={filters.categories} onChange={(v) => (filters = { ...filters, categories: v })} />
        <FilterMenu label="Status" options={optionsFor('statuses')} selected={filters.statuses} onChange={(v) => (filters = { ...filters, statuses: v })} />
        <FilterMenu label="Tags" options={optionsFor('tags')} selected={filters.tags} onChange={(v) => (filters = { ...filters, tags: v })} searchable />
        {#if filterCount}<button class="linkbtn" onclick={() => (filters = { categories: [], statuses: [], tags: [] })}>Clear filters</button>{/if}
        <span class="mono meta">{projects.length.toLocaleString('en-GB')} {debounced ? `MATCHING “${debounced.toUpperCase()}”` : 'PROJECTS'}</span>
        {#if scanning}
          <span class="status" role="status">
            {progress.phase === 'scan' ? 'Scanning folders…' : `Making thumbnails · ${progress.total - progress.done} left`}
            <span class="muted">· you can keep working</span>
          </span>
        {/if}
        <div class="right">
          <div class="sort" role="group" aria-label="Sort">
            <span class="muted">Sort</span>
            <button class:on={sort === 'newest'} onclick={() => (sort = 'newest')}>{view === 'favorites' ? 'Newest' : 'Newest month first'}</button>
            <button class:on={sort === 'name'} onclick={() => (sort = 'name')}>Name</button>
          </div>
          <button class="pillbtn" onclick={() => { selectMode = true; selectedIds = []; }}>Select</button>
        </div>
      </div>
    {/if}

    <main class="scroll fill" bind:this={scroller}>
      {#if projects.length === 0}
        <div class="empty">
          {#if debounced}
            <h2 class="display">Nothing matches “{debounced}”</h2>
            <p>Search looks at titles, folder names and tags across every month.</p>
            <button class="btn primary" onclick={() => (search = '')}>Clear search</button>
          {:else if filterCount}
            <h2 class="display">No projects match these {filterCount} {filterCount === 1 ? 'filter' : 'filters'}</h2>
            <p>Each filter narrows the list further. Remove one, or start over.</p>
            <button class="btn primary" onclick={() => (filters = { categories: [], statuses: [], tags: [] })}>Clear all filters</button>
          {:else if view === 'favorites'}
            <h2 class="display">No favourites yet</h2>
            <p>Favourites are your portfolio picks. Select a cover and press F, or right-click it.</p>
          {:else if view === 'archive'}
            <h2 class="display">Nothing archived</h2>
            <p>Archiving hides a project from the Library and Desk. Its folder stays exactly where it is on disk.</p>
          {:else if busy}
            <h2 class="display">Reading your library…</h2>
          {:else}
            <h2 class="display">No projects found</h2>
            <p class="mono">{info.root}</p>
          {/if}
        </div>
      {:else}
        <Library
          {projects}
          {view}
          {grouped}
          {versions}
          {busy}
          {selectMode}
          {selectedIds}
          {request}
          statuses={info.statuses}
          categories={info.categories}
          onEdit={edit}
          onOpen={openFolder}
          onChangeCover={changeCover}
          onToggleSelect={(id) => (selectedIds = selectedIds.includes(id) ? selectedIds.filter((x) => x !== id) : [...selectedIds, id])}
          onSetSelected={(ids, on) => (selectedIds = on ? [...new Set([...selectedIds, ...ids])] : selectedIds.filter((x) => !ids.includes(x)))}
          onContext={(e, p) => (ctx = { x: e.clientX, y: e.clientY, p })}
        />
      {/if}
    </main>
  {/if}

  {#if updateOpen && update}
    <Dialog title={`Atelier ${update.version} is ready`} onCancel={() => installing == null && (updateOpen = false)}>
      {#snippet children()}
        <span class="mono small">{appVersion || update.currentVersion} → {update.version}</span>
        {#if update.body}<div class="notes">{update.body}</div>{/if}
        <p class="small muted">Atelier closes, installs and reopens where you left off.</p>
      {/snippet}
      {#snippet actions()}
        <button class="btn" data-autofocus disabled={installing != null} onclick={() => (updateOpen = false)}>Later</button>
        <button class="btn primary" disabled={installing != null} onclick={install}>
          {installing == null ? 'Install and restart' : typeof installing === 'number' ? `Downloading ${installing}%` : 'Installing…'}
        </button>
      {/snippet}
    </Dialog>
  {/if}

  {#if keysOpen}
    <Dialog title="Keyboard shortcuts" width={520} labelId="keys-title" onCancel={() => (keysOpen = false)}>
      {#snippet children()}
        {#each SHORTCUTS as [group, rows]}
          <div class="kgroup">{group}</div>
          {#each rows as [keys, what]}
            <div class="krow">
              <span class="keys">{#each keys as k}<kbd>{k}</kbd>{/each}</span>
              <span>{what}</span>
            </div>
          {/each}
        {/each}
      {/snippet}
      {#snippet actions()}
        <button class="btn" data-autofocus onclick={() => (keysOpen = false)}>Close</button>
      {/snippet}
    </Dialog>
  {/if}

  {#if ctx}
    <ContextMenu x={ctx.x} y={ctx.y} p={ctx.p} statuses={info.statuses} categories={info.categories} onAction={ctxAction} onClose={() => (ctx = null)} />
  {/if}

  {#if toast}
    <div class="toast" class:error={toast.error} role={toast.error ? 'alert' : 'status'}>
      <span>{toast.text}</span>
      {#if toast.undo}<button class="undo" onclick={() => { const u = toast.undo; toast = null; u(); }}>Undo</button>{/if}
      <button class="x" aria-label="Dismiss" onclick={() => (toast = null)}>×</button>
    </div>
  {/if}
</div>

<style>
  .app { height: 100%; display: flex; flex-direction: column; }
  header { flex: none; height: var(--topbar); display: flex; align-items: center; gap: 28px; padding: 0 8px 0 var(--gutter); border-bottom: 1px solid var(--line); }
  .wordmark { font-size: 24px; font-weight: 800; font-stretch: 75%; letter-spacing: -0.01em; }
  nav { display: flex; align-self: stretch; gap: 22px; }
  .tab { display: flex; align-items: center; gap: 6px; padding: 0; border: 0; background: none; color: var(--text-2); font-weight: 500; font-size: 15px; }
  .tab:hover { color: var(--text); }
  .tab.active { color: var(--text); font-weight: 600; box-shadow: inset 0 -2px 0 var(--text); }
  .count { font-size: 12px; color: var(--text-3); font-weight: 400; }
  .spacer { flex: 1; align-self: stretch; }
  .search { width: 440px; height: 36px; display: flex; align-items: center; gap: 10px; padding: 0 12px; background: var(--panel); border: 1px solid var(--line); border-radius: 8px; color: var(--text-3); }
  .search:focus-within { border-color: var(--text-2); }
  .search input { flex: 1; min-width: 0; border: 0; outline: none; background: transparent; color: var(--text); }
  .search kbd { font-family: var(--font-ui); font-size: 11px; color: var(--text-3); }
  .clear { border: 0; background: var(--line); color: var(--text-2); width: 22px; height: 22px; border-radius: 50%; line-height: 1; }
  .icon-btn.on { background: var(--line); }
  .chip { height: 30px; padding: 0 12px; display: flex; align-items: center; gap: 8px; border-radius: 999px; border: 1px solid var(--border); background: none; color: var(--text); font-size: 12px; }
  .chip:hover { border-color: var(--text); }
  .chip .g { width: 6px; height: 6px; border-radius: 50%; background: var(--success); }
  .notes { white-space: pre-wrap; padding: 14px 16px; background: var(--bg); border-radius: 8px; max-height: 260px; overflow: auto; }
  .small { font-size: 13px; }
  .kgroup { margin-top: 8px; font-size: 12px; letter-spacing: 0.08em; text-transform: uppercase; color: var(--text-3); }
  .krow { display: grid; grid-template-columns: 170px 1fr; align-items: center; gap: 12px; min-height: 30px; }
  .keys { display: flex; gap: 4px; }
  kbd {
    min-width: 26px; height: 24px; padding: 0 7px; display: inline-flex; align-items: center; justify-content: center;
    border: 1px solid var(--border); border-bottom-width: 2px; border-radius: 5px; background: var(--bg);
    font-family: 'JetBrains Mono', monospace; font-size: 12px; color: var(--text);
  }
  .sep { width: 1px; height: 22px; background: var(--line); margin: 0 4px; }
  .win { color: var(--text-2); width: 44px; height: 36px; }
  .win.close:hover { background: #c42b1c; color: #fff; }
  .spin { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .progress { flex: none; height: 2px; }
  .progress.on { background: var(--hover); }
  .progress div { height: 2px; background: var(--text); opacity: 0; transition: width 300ms ease-out, opacity 300ms; }
  .progress.on div { opacity: 1; }
  .toolbar { flex: none; display: flex; align-items: center; gap: 10px; padding: 16px var(--gutter); border-bottom: 1px solid var(--line); flex-wrap: wrap; position: relative; z-index: 5; }
  .toolbar h1 { margin: 0 14px 0 0; font-size: 36px; line-height: 1; font-stretch: 70%; }
  .linkbtn { border: 0; background: none; color: var(--text-2); text-decoration: underline; font-size: 13px; }
  .meta { font-size: 12px; color: var(--text-3); margin-left: 8px; }
  .status { font-size: 13px; color: var(--text-2); }
  .muted { color: var(--text-3); }
  .right { margin-left: auto; display: flex; align-items: center; gap: 20px; }
  .sort { display: flex; gap: 18px; font-size: 13px; align-items: center; }
  .sort > * { padding: 4px 0; line-height: 18px; }
  .sort button { border: 0; background: none; color: var(--text-2); }
  .sort button:hover { color: var(--text); }
  /* Same shape as the filter pills on the left of the toolbar. */
  .pillbtn { height: 34px; padding: 0 16px; border-radius: 999px; border: 1px solid var(--border); background: transparent; color: var(--text); font-size: 13px; }
  .pillbtn:hover { border-color: var(--text); }
  .sort button.on { color: var(--text); font-weight: 600; box-shadow: inset 0 -2px 0 var(--text); }
  .fill { flex: 1; min-height: 0; }
  .empty { max-width: 560px; margin: 160px auto; text-align: center; display: flex; flex-direction: column; align-items: center; gap: 14px; color: var(--text-2); }
  .empty h2 { margin: 0; font-size: 34px; color: var(--text); font-stretch: 75%; }
  .empty p { margin: 0; line-height: 1.55; }
  .toast { position: fixed; left: 50%; bottom: 40px; transform: translateX(-50%); z-index: 80; display: flex; align-items: center; gap: 12px; min-height: 52px; padding: 0 8px 0 18px; background: var(--text); color: var(--on-text); border-radius: 10px; font-size: 14px; font-weight: 500; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.6); max-width: 70vw; }
  .toast.error { background: var(--menu); color: var(--text); border: 1px solid #5a2a24; border-left: 3px solid var(--danger); }
  .undo { height: 36px; padding: 0 14px; border: 0; border-radius: 7px; background: var(--on-text); color: var(--text); font-weight: 600; }
  .x { width: 34px; height: 34px; border: 0; background: none; color: inherit; opacity: 0.6; font-size: 18px; }
</style>
