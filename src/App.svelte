<script>
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { api, listen } from './lib/api.js';
  import Library from './lib/Library.svelte';
  import FirstRun from './lib/FirstRun.svelte';

  const win = getCurrentWindow();

  let info = $state(null);
  let view = $state('library'); // 'desk' | 'library' | 'favorites' | 'archive'
  let projects = $state([]);
  let search = $state('');
  let debounced = $state('');
  let sort = $state('newest');
  let busy = $state(false);
  let progress = $state({ phase: 'idle', done: 0, total: 0 });
  let versions = $state({});
  let toast = $state(null);
  let searchInput = $state();
  let scroller = $state();

  const query = $derived({
    archived: view === 'archive',
    favoritesOnly: view === 'favorites',
    search: debounced || null,
    sort: view === 'favorites' && sort === 'newest' ? 'newest' : sort,
  });

  async function loadInfo() {
    info = await api.info();
    busy = info.busy;
  }
  // The sort the current `projects` were loaded with; the grid groups by
  // this, not by the button, so it never regroups a list in the wrong order.
  let loadedSort = $state('newest');
  let loadSeq = 0;
  async function loadProjects() {
    if (!info?.root) return;
    const q = query;
    const seq = ++loadSeq;
    const rows = await api.list(q);
    if (seq !== loadSeq) return; // a newer request superseded this one
    projects = rows;
    loadedSort = q.sort;
  }

  // Re-query when the view, search or sort changes.
  $effect(() => {
    query;
    loadProjects();
  });

  // Debounce search ~150 ms; typing switches to the Library.
  let timer;
  $effect(() => {
    const s = search;
    clearTimeout(timer);
    timer = setTimeout(() => {
      debounced = s.trim();
      if (debounced && view !== 'library') view = 'library';
    }, 150);
  });

  onMount(() => {
    loadInfo().then(loadProjects);
    const offs = [
      listen('library-changed', async () => { await loadInfo(); await loadProjects(); }),
      listen('import-done', async (e) => {
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

  function showToast(text, error = false, undo = null) {
    toast = { text, error, undo };
    clearTimeout(showToast.t);
    if (!error) showToast.t = setTimeout(() => (toast = null), 5000);
  }

  async function edit(ids, e) {
    // Update the screen first, then save (SPEC §3.1).
    const before = projects.filter((p) => ids.includes(p.id)).map((p) => ({ ...p, tags: [...p.tags] }));
    projects = projects.map((p) => (ids.includes(p.id) ? applyLocal(p, e) : p));
    try {
      await api.edit(ids, e);
      if (e.kind === 'archived' || e.kind === 'favorite') {
        const leaves = (e.kind === 'archived') !== (view === 'archive') || (e.kind === 'favorite' && view === 'favorites' && !e.value);
        if (e.kind === 'archived') {
          const title = before[0]?.title ?? 'project';
          showToast(`${e.value ? 'Archived' : 'Unarchived'} “${title}”`, false, () => edit(ids, { kind: 'archived', value: !e.value }));
        }
        if (leaves) projects = projects.filter((p) => !ids.includes(p.id) || applyLocal(p, e) === p);
        await loadInfo();
        await loadProjects();
      }
    } catch (err) {
      projects = projects.map((p) => before.find((b) => b.id === p.id) ?? p);
      showToast(`Couldn’t save: ${err}`, true);
    }
  }

  function applyLocal(p, e) {
    switch (e.kind) {
      case 'title': return { ...p, title: e.value };
      case 'status': return { ...p, status: e.value };
      case 'category': return { ...p, category: e.value };
      case 'favorite': return { ...p, isFavorite: e.value };
      case 'archived': return { ...p, isArchived: e.value };
      case 'addTag': return { ...p, tags: [...p.tags, e.value].sort() };
      case 'removeTag': return { ...p, tags: p.tags.filter((t) => t !== e.value) };
      default: return p;
    }
  }

  async function openFolder(id) {
    try {
      await api.openFolder(id);
    } catch (err) {
      showToast(`Couldn’t open the folder: ${err}`, true);
    }
  }

  function onGlobalKey(e) {
    const typing = e.target.closest?.('input, textarea, select');
    if ((e.ctrlKey || e.metaKey) && (e.key === 'k' || e.key === 'f')) {
      e.preventDefault();
      searchInput?.focus();
      searchInput?.select();
    } else if (e.key === 'Escape' && typing === searchInput) {
      search = '';
      searchInput.blur();
    }
  }

  function setView(v) {
    view = v;
    scroller?.scrollTo({ top: 0 });
  }

  const tabs = $derived([
    { id: 'desk', name: 'Desk', count: null, soon: true },
    { id: 'library', name: 'Library', count: info?.counts.library },
    { id: 'favorites', name: 'Favorites', count: info?.counts.favorites },
    { id: 'archive', name: 'Archive', count: info?.counts.archive },
  ]);
  const scanning = $derived(busy && progress.phase !== 'idle');
  const pct = $derived(progress.total ? Math.round((progress.done / progress.total) * 100) : 12);
</script>

<svelte:window onkeydown={onGlobalKey} />

<div class="app">
  <header data-tauri-drag-region>
    <span class="wordmark display" data-tauri-drag-region>ATELIER</span>
    {#if info?.root}
      <nav aria-label="Views">
        {#each tabs as t}
          <button
            class="tab"
            class:active={view === t.id}
            aria-current={view === t.id ? 'page' : undefined}
            disabled={t.soon}
            title={t.soon ? 'Desk arrives in the next milestone' : undefined}
            onclick={() => setView(t.id)}
          >
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
    {/if}
    <span class="sep"></span>
    <button class="icon-btn win" aria-label="Minimise" onclick={() => win.minimize()}><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M5 12h14"></path></svg></button>
    <button class="icon-btn win" aria-label="Maximise" onclick={() => win.toggleMaximize()}><svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><rect x="5" y="5" width="14" height="14" rx="1"></rect></svg></button>
    <button class="icon-btn win close" aria-label="Close" onclick={() => win.close()}><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"></path></svg></button>
  </header>

  <div class="progress" role="progressbar" aria-label="Scanning library" aria-valuenow={pct} aria-valuemin="0" aria-valuemax="100" class:on={scanning}>
    <div style:width={`${pct}%`}></div>
  </div>

  {#if !info}
    <div class="loading"></div>
  {:else if !info.root}
    <FirstRun bookrack={info.bookrack} onStarted={async () => { await loadInfo(); await loadProjects(); }} />
  {:else}
    <div class="toolbar">
      <h1 class="display">{view === 'archive' ? 'Archive' : view === 'favorites' ? 'Favorites' : 'Library'}</h1>
      <span class="mono meta">
        {projects.length.toLocaleString('en-GB')} {debounced ? `MATCHING “${debounced.toUpperCase()}”` : 'PROJECTS'}
      </span>
      {#if scanning}
        <span class="status" role="status">
          {progress.phase === 'scan' ? 'Scanning folders…' : `Making thumbnails · ${progress.total - progress.done} left`}
          <span class="muted">· you can keep working</span>
        </span>
      {/if}
      <div class="sort" role="group" aria-label="Sort">
        <span class="muted">Sort</span>
        <button class:on={sort === 'newest'} onclick={() => (sort = 'newest')}>Newest month first</button>
        <button class:on={sort === 'name'} onclick={() => (sort = 'name')}>Name</button>
      </div>
    </div>

    <main class="scroll" bind:this={scroller}>
      {#if projects.length === 0}
        <div class="empty">
          {#if debounced}
            <h2 class="display">Nothing matches “{debounced}”</h2>
            <p>Search looks at titles, folder names and tags across every month.</p>
            <button class="btn primary" onclick={() => (search = '')}>Clear search</button>
          {:else if view === 'favorites'}
            <h2 class="display">No favourites yet</h2>
            <p>Select a cover and press F, or use Favourite in its details.</p>
          {:else if view === 'archive'}
            <h2 class="display">Nothing archived</h2>
            <p>Archiving hides a project from the Library. Its folder stays exactly where it is.</p>
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
          grouped={loadedSort !== 'name' && view !== 'favorites'}
          {versions}
          {busy}
          statuses={info.statuses}
          categories={info.categories}
          onEdit={edit}
          onOpen={openFolder}
        />
      {/if}
    </main>
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
  header {
    flex: none; height: var(--topbar); display: flex; align-items: center; gap: 28px;
    padding: 0 8px 0 var(--gutter); border-bottom: 1px solid var(--line);
  }
  .wordmark { font-size: 24px; font-weight: 800; font-stretch: 75%; letter-spacing: -0.01em; }
  nav { display: flex; align-self: stretch; gap: 22px; }
  .tab {
    display: flex; align-items: center; gap: 6px; padding: 0; border: 0; background: none;
    color: var(--text-2); font-weight: 500; font-size: 15px; box-shadow: none;
  }
  .tab:hover:not(:disabled) { color: var(--text); }
  .tab.active { color: var(--text); font-weight: 600; box-shadow: inset 0 -2px 0 var(--text); }
  .tab:disabled { color: var(--text-4); cursor: default; }
  .count { font-size: 12px; color: var(--text-3); font-weight: 400; }
  .spacer { flex: 1; align-self: stretch; }
  .search {
    width: 440px; height: 36px; display: flex; align-items: center; gap: 10px; padding: 0 12px;
    background: var(--panel); border: 1px solid var(--line); border-radius: 8px; color: var(--text-3);
  }
  .search:focus-within { border-color: var(--text-2); }
  .search input { flex: 1; min-width: 0; border: 0; outline: none; background: transparent; color: var(--text); }
  .search kbd { font-family: var(--font-ui); font-size: 11px; color: var(--text-3); }
  .clear { border: 0; background: var(--line); color: var(--text-2); width: 22px; height: 22px; border-radius: 50%; line-height: 1; }
  .sep { width: 1px; height: 22px; background: var(--line); margin: 0 4px; }
  .win { color: var(--text-2); width: 44px; height: 36px; }
  .win.close:hover { background: #c42b1c; color: #fff; }
  .spin { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .progress { flex: none; height: 2px; background: transparent; }
  .progress.on { background: var(--hover); }
  .progress div { height: 2px; background: var(--text); opacity: 0; transition: width 300ms ease-out, opacity 300ms; }
  .progress.on div { opacity: 1; }

  .toolbar {
    flex: none; display: flex; align-items: baseline; gap: 20px; padding: 20px var(--gutter) 16px;
    border-bottom: 1px solid var(--line); flex-wrap: wrap;
  }
  .toolbar h1 { margin: 0; font-size: 40px; line-height: 1; font-stretch: 70%; }
  .meta { font-size: 12px; color: var(--text-3); }
  .status { font-size: 13px; color: var(--text-2); }
  .muted { color: var(--text-3); }
  .sort { margin-left: auto; display: flex; gap: 18px; font-size: 13px; align-items: baseline; }
  .sort button { padding: 0 0 4px; border: 0; background: none; color: var(--text-2); }
  .sort button.on { color: var(--text); font-weight: 600; box-shadow: inset 0 -2px 0 var(--text); }

  main { flex: 1; min-height: 0; }
  .empty { max-width: 560px; margin: 160px auto; text-align: center; display: flex; flex-direction: column; align-items: center; gap: 14px; color: var(--text-2); }
  .empty h2 { margin: 0; font-size: 34px; color: var(--text); font-stretch: 75%; }
  .empty p { margin: 0; line-height: 1.55; }
  .loading { flex: 1; }

  .toast {
    position: fixed; left: 50%; bottom: 40px; transform: translateX(-50%); z-index: 50;
    display: flex; align-items: center; gap: 12px; min-height: 52px; padding: 0 8px 0 18px;
    background: var(--text); color: var(--on-text); border-radius: 10px; font-size: 14px; font-weight: 500;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.6); max-width: 70vw;
  }
  .toast.error { background: var(--menu); color: var(--text); border: 1px solid #5a2a24; border-left: 3px solid var(--danger); }
  .undo { height: 36px; padding: 0 14px; border: 0; border-radius: 7px; background: var(--on-text); color: var(--text); font-weight: 600; }
  .x { width: 34px; height: 34px; border: 0; background: none; color: inherit; opacity: 0.6; font-size: 18px; }
</style>
