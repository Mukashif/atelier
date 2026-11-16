<script>
  // Settings (SPEC §6.8): one scrolling page, section list on the left.
  import { onMount } from 'svelte';
  import { open, save } from '@tauri-apps/plugin-dialog';
  import { api, categoryColor, statusColor } from './api.js';
  import Dialog from './Dialog.svelte';

  let { onChanged, toast, onCheckUpdates } = $props();
  let confirm = $state(null); // { kind: 'reset' | 'restore', path?, backupFirst }
  let checking = $state(false);
  let updateNote = $state('');

  async function pickRestore() {
    const path = await open({ title: 'Restore a backup', filters: [{ name: 'Atelier or Bookrack backup', extensions: ['atelier', 'bookrack', 'json'] }] });
    if (typeof path === 'string') confirm = { kind: 'restore', path };
  }
  async function doRestore() {
    const path = confirm.path;
    confirm = null;
    run(() => api.restoreBackup(path), (r) => `Restored ${r.restored} projects from ${r.kind === 'bookrack' ? 'a Bookrack backup' : 'the backup'}${r.missing ? ` · ${r.missing} folders not found` : ''} · rescanning`);
  }
  async function doReset() {
    let backupTo = null;
    if (confirm.backupFirst) {
      const d = new Date().toISOString().slice(0, 10);
      backupTo = await save({ title: 'Save a backup before resetting', defaultPath: `atelier-backup-${d}.atelier`, filters: [{ name: 'Atelier backup', extensions: ['atelier'] }] });
      if (!backupTo) return; // cancelled: keep the dialog open, reset nothing
    }
    confirm = null;
    run(() => api.resetData(backupTo), (n) => `Reset edits on ${n.toLocaleString('en-GB')} projects${backupTo ? ' · backup saved' : ''}`);
  }
  async function checkNow() {
    checking = true;
    updateNote = '';
    try {
      const found = await onCheckUpdates();
      updateNote = found ? `Version ${found} is available` : `You’re up to date · checked just now`;
    } catch (e) {
      updateNote = `Couldn’t check: ${e}`;
    }
    checking = false;
  }

  let s = $state(null);
  let orphans = $state([]);
  let newCat = $state('');
  let newStatus = $state('');
  let editing = $state(null); // { kind, old, value }

  async function load() {
    [s, orphans] = await Promise.all([api.settings(), api.orphans()]);
  }
  onMount(load);

  async function run(fn, okMsg) {
    try {
      const r = await fn();
      if (okMsg) toast(typeof okMsg === 'function' ? okMsg(r) : okMsg);
      await load();
      onChanged();
    } catch (e) {
      toast(String(e), true);
    }
  }

  const listOf = (kind) => (kind === 'categories' ? s.categories : s.statuses);
  const countOf = (kind, name) => s.counts[`${kind === 'categories' ? 'category' : 'status'}:${name}`] ?? 0;

  function add(kind) {
    const v = (kind === 'categories' ? newCat : newStatus).trim();
    if (!v) return;
    if (listOf(kind).some((x) => x.toLowerCase() === v.toLowerCase())) return toast(`“${v}” is already in the list`, true);
    if (kind === 'categories') newCat = ''; else newStatus = '';
    run(() => api.setList(kind, [...listOf(kind), v]));
  }
  function remove(kind, name) {
    run(() => api.setList(kind, listOf(kind).filter((x) => x !== name)), `Removed “${name}” from the list. Projects using it keep it.`);
  }
  function move(kind, i, d) {
    const l = [...listOf(kind)];
    const j = i + d;
    if (j < 0 || j >= l.length) return;
    [l[i], l[j]] = [l[j], l[i]];
    run(() => api.setList(kind, l));
  }
  function commitRename() {
    const e = editing;
    editing = null;
    if (!e || !e.value.trim() || e.value.trim() === e.old) return;
    run(() => api.renameListItem(e.kind, e.old, e.value.trim()), (n) => `Renamed to “${e.value.trim()}” · updated ${n} ${n === 1 ? 'project' : 'projects'}`);
  }

  async function changeFolder() {
    const picked = await open({ directory: true, title: 'Choose your library folder', defaultPath: s.root ?? undefined });
    if (typeof picked === 'string') run(() => api.setRoot(picked), 'Library folder changed · rescanning');
  }
  async function exportBackup() {
    const d = new Date().toISOString().slice(0, 10);
    const path = await save({ title: 'Export backup', defaultPath: `atelier-backup-${d}.atelier`, filters: [{ name: 'Atelier backup', extensions: ['atelier'] }] });
    if (path) run(() => api.exportBackup(path), 'Backup exported');
  }
  function since(ms) {
    if (!ms) return 'Never exported';
    const d = Math.floor((Date.now() - ms) / 86_400_000);
    return `Last export: ${d === 0 ? 'today' : d === 1 ? 'yesterday' : `${d} days ago`}`;
  }

  const sections = [
    ['library', 'Library folder'], ['categories', 'Categories'], ['statuses', 'Statuses'],
    ['prefs', 'Preferences'], ['data', 'Backup & data'], ['updates', 'Updates'],
  ];
  let active = $state('library');
  let scroller = $state();
  function go(id) {
    active = id;
    scroller?.querySelector(`#s-${id}`)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }
</script>

{#if confirm?.kind === 'reset'}
  <Dialog title="Reset library data?" onCancel={() => (confirm = null)}>
    {#snippet children()}
      <p><span class="rm">Removed:</span> titles you renamed, statuses, categories, tags, favourites, archive and covers you picked.</p>
      <p><span class="kp">Kept:</span> your project folders and files, your category and status lists, your settings.</p>
      <label class="chk"><input type="checkbox" bind:checked={confirm.backupFirst} /> Export a backup first <span class="muted small">(recommended)</span></label>
    {/snippet}
    {#snippet actions()}
      <button class="btn" data-autofocus onclick={() => (confirm = null)}>Cancel</button>
      <button class="btn danger solid" onclick={doReset}>{confirm.backupFirst ? 'Back up and reset' : 'Reset'}</button>
    {/snippet}
  </Dialog>
{:else if confirm?.kind === 'restore'}
  <Dialog title="Restore this backup?" onCancel={() => (confirm = null)}>
    {#snippet children()}
      <p class="mono small">{confirm.path}</p>
      <p>Your current edits will be replaced by the ones in the backup. Your project folders are never touched. Export a backup first if you might want today’s edits back.</p>
    {/snippet}
    {#snippet actions()}
      <button class="btn" data-autofocus onclick={() => (confirm = null)}>Cancel</button>
      <button class="btn primary" onclick={doRestore}>Restore</button>
    {/snippet}
  </Dialog>
{/if}

{#if s}
  <div class="page">
    <nav aria-label="Settings sections">
      <h1 class="display">Settings</h1>
      {#each sections as [id, name]}
        <button class:on={active === id} onclick={() => go(id)}>{name}</button>
      {/each}
    </nav>

    <div class="scroll body" bind:this={scroller}>
      <section id="s-library">
        <div class="about">
          <h2 class="display">Library folder</h2>
          <p>The one folder Atelier reads. It expects month folders inside it, each holding one folder per project. Atelier never changes or deletes anything in here.</p>
        </div>
        <div class="ctl">
          <div class="card row">
            <div class="grow">
              <div class="mono path">{s.root ?? 'Not set'}</div>
            </div>
            <button class="btn" onclick={() => api.openSpecial('root')}>Open in Explorer</button>
            <button class="btn" onclick={() => run(() => api.rescan(), 'Rescanning your library')}>Rescan now</button>
            <button class="btn" onclick={changeFolder}>Change folder…</button>
          </div>

          <div class="orphans" class:ok={!orphans.length}>
            <div class="oh">
              <span class="dot"></span>
              {#if orphans.length}
                <strong>{orphans.length} {orphans.length === 1 ? 'project' : 'projects'} not found on the last scan</strong>
                <span class="muted">Their folders were renamed, moved or deleted. Your edits are kept until you decide.</span>
              {:else}
                <strong>All projects are linked to a folder.</strong>
              {/if}
            </div>
            {#each orphans as o (o.project.id)}
              <div class="orow">
                <div>
                  <div class="ot">{o.project.title}</div>
                  <div class="mono gone">{o.project.relPath}</div>
                </div>
                <div class="muted small">
                  {#if o.suggestion}Looks like <span class="mono now">{o.suggestion.relPath}</span>{:else}No folder with the same name found.{/if}
                </div>
                <div class="acts">
                  {#if o.suggestion}
                    <button class="btn primary small" onclick={() => run(() => api.relink(o.project.id, o.suggestion.id), `Re-linked “${o.project.title}”`)}>Re-link</button>
                  {/if}
                  <button class="btn small" onclick={() => run(() => api.forget(o.project.id), `Forgot “${o.project.title}”`)}>Forget</button>
                </div>
              </div>
            {/each}
          </div>
        </div>
      </section>

      {#each [['categories', 'Categories', 'Renaming updates every project that uses it. Deleting only removes it from this list — projects keep their category until you change them.'], ['statuses', 'Statuses', 'The stages a job moves through, in order. The first one is given to new projects.']] as [kind, title, about]}
        <section id="s-{kind}">
          <div class="about"><h2 class="display">{title}</h2><p>{about}</p></div>
          <div class="ctl">
            <div class="list">
              {#each listOf(kind) as name, i (name)}
                <div class="li">
                  {#if kind === 'categories'}
                    <span class="sw" style:background={categoryColor(name)}></span>
                  {:else}
                    <span class="sd" style:background={statusColor(name, s.statuses)}></span>
                  {/if}
                  {#if editing?.kind === kind && editing.old === name}
                    <!-- svelte-ignore a11y_autofocus -->
                    <input class="ren" bind:value={editing.value} autofocus onblur={commitRename}
                      onkeydown={(e) => { if (e.key === 'Enter') commitRename(); if (e.key === 'Escape') { e.stopPropagation(); editing = null; } }} aria-label="New name" />
                  {:else}
                    <span class="nm">{name}{#if kind === 'statuses' && i === 0}<span class="muted small"> · given to new projects</span>{/if}</span>
                  {/if}
                  <span class="muted small">{countOf(kind, name).toLocaleString('en-GB')} projects</span>
                  {#if kind === 'statuses'}
                    <button class="ib" aria-label="Move {name} up" disabled={i === 0} onclick={() => move(kind, i, -1)}>↑</button>
                    <button class="ib" aria-label="Move {name} down" disabled={i === listOf(kind).length - 1} onclick={() => move(kind, i, 1)}>↓</button>
                  {/if}
                  <button class="ib" aria-label="Rename {name}" onclick={() => (editing = { kind, old: name, value: name })}>✎</button>
                  <button class="ib" aria-label="Delete {name}" onclick={() => remove(kind, name)}>🗑</button>
                </div>
              {/each}
              <div class="li add">
                {#if kind === 'categories'}
                  <input placeholder="New category…" bind:value={newCat} onkeydown={(e) => e.key === 'Enter' && add(kind)} aria-label="New category" />
                {:else}
                  <input placeholder="New status…" bind:value={newStatus} onkeydown={(e) => e.key === 'Enter' && add(kind)} aria-label="New status" />
                {/if}
                <button class="btn small" onclick={() => add(kind)}>Add</button>
              </div>
            </div>
            {#if kind === 'categories'}
              <label class="inline">Projects with no category show as
                <select value={s.defaultCategory} onchange={(e) => run(() => api.setPref('defaultCategory', e.currentTarget.value))}>
                  {#each s.categories as c}<option value={c}>{c}</option>{/each}
                </select>
              </label>
            {/if}
          </div>
        </section>
      {/each}

      <section id="s-prefs">
        <div class="about"><h2 class="display">Preferences</h2><p>How Atelier opens and behaves.</p></div>
        <div class="ctl">
          <div class="pref">
            <div class="grow"><div>Open on launch</div><div class="muted small">Which screen you see first</div></div>
            <div class="seg" role="radiogroup" aria-label="Open on launch">
              {#each [['desk', 'Desk'], ['library', 'Library']] as [v, n]}
                <button role="radio" aria-checked={s.openOnLaunch === v} class:on={s.openOnLaunch === v} onclick={() => run(() => api.setPref('openOnLaunch', v))}>{n}</button>
              {/each}
            </div>
          </div>
          {#each [['groupByMonth', 'Group library by month', 'Month rows; turn off for one long grid'], ['reduceMotion', 'Reduce motion', 'Turn off hover lifts and panel animations'], ['checkUpdatesOnLaunch', 'Check for updates on launch', 'Silent unless a new version is found']] as [key, name, desc]}
            <div class="pref">
              <div class="grow"><div>{name}</div><div class="muted small">{desc}</div></div>
              <button class="switch" role="switch" aria-checked={s[key]} aria-label={name} class:on={s[key]} onclick={() => run(() => api.setPref(key, !s[key]))}><span></span></button>
            </div>
          {/each}
        </div>
      </section>

      <section id="s-data">
        <div class="about"><h2 class="display">Backup &amp; data</h2><p>Your edits — titles, statuses, categories, tags, favourites — live in Atelier’s own data folder, not in your projects.</p></div>
        <div class="ctl">
          <div class="card row">
            <div class="grow"><div>Export backup</div><div class="muted small">{since(s.lastExportAt)}</div></div>
            <button class="btn primary" onclick={exportBackup}>Export…</button>
          </div>
          <div class="card row">
            <div class="grow"><div>Data folder</div><div class="mono small muted">{s.dataDir}</div></div>
            <button class="btn" onclick={() => api.openSpecial('data')}>Open</button>
          </div>
          <div class="card row">
            <div class="grow"><div>Restore backup</div><div class="muted small">Opens an .atelier or Bookrack .bookrack file. Replaces your current edits.</div></div>
            <button class="btn" onclick={pickRestore}>Choose file…</button>
          </div>
          <div class="card row danger-box">
            <div class="grow">
              <div class="danger-t">Reset library data</div>
              <div class="muted small">Clears every edit Atelier stores: titles, statuses, categories, tags, favourites, archive and covers you picked. Your category and status lists are kept. Project folders and files are never touched.</div>
            </div>
            <button class="btn danger" onclick={() => (confirm = { kind: 'reset', backupFirst: true })}>Reset…</button>
          </div>
        </div>
      </section>

      <section id="s-updates">
        <div class="about"><h2 class="display">Updates</h2><p>Atelier will check GitHub for new versions. Nothing else ever leaves your PC.</p></div>
        <div class="ctl">
          <div class="card row">
            <span class="ver display">{s.version}</span>
            <div class="grow muted small">{updateNote || 'Checks GitHub for a newer signed release.'}</div>
            <button class="btn" onclick={checkNow} disabled={checking}>{checking ? 'Checking…' : 'Check now'}</button>
          </div>
        </div>
      </section>
    </div>
  </div>
{/if}

<style>
  .page { height: 100%; display: flex; }
  nav { flex: 0 0 320px; padding: 48px 24px 48px var(--gutter); display: flex; flex-direction: column; gap: 4px; }
  nav h1 { margin: 0 0 24px 12px; font-size: 56px; line-height: 0.9; }
  nav button { height: 40px; padding: 0 12px; border: 0; border-radius: 6px; background: none; color: var(--text-2); text-align: left; font-size: 15px; }
  nav button:hover { color: var(--text); background: var(--hover); }
  nav button.on { color: var(--text); font-weight: 600; background: var(--hover); box-shadow: inset 2px 0 0 var(--text); }
  .body { flex: 1; min-width: 0; padding: 48px var(--gutter) 160px 40px; }
  section { display: flex; gap: 64px; padding: 0 0 48px; margin-bottom: 48px; border-bottom: 1px solid var(--line); flex-wrap: wrap; max-width: 1640px; scroll-margin-top: 24px; }
  .about { flex: 0 0 380px; }
  h2 { margin: 0; font-size: 28px; font-stretch: 75%; }
  .about p { margin: 10px 0 0; color: var(--text-2); line-height: 1.55; }
  .ctl { flex: 1 1 640px; display: flex; flex-direction: column; gap: 16px; min-width: 0; }
  .card { padding: 18px 20px; background: var(--panel); border: 1px solid var(--line); border-radius: 8px; }
  .row { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  .grow { flex: 1; min-width: 200px; }
  .path { font-size: 14px; overflow-wrap: anywhere; }
  .muted { color: var(--text-3); }
  .small { font-size: 13px; }
  .btn.small { height: 34px; font-size: 13px; padding: 0 12px; }
  .orphans { border: 1px solid #5a3a24; border-radius: 8px; overflow: hidden; }
  .orphans.ok { border-color: var(--line); }
  .oh { display: flex; align-items: center; gap: 10px; padding: 14px 20px; background: rgba(255, 138, 76, 0.08); flex-wrap: wrap; }
  .ok .oh { background: none; color: var(--success); }
  .oh .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--warning); }
  .ok .oh .dot { background: var(--success); }
  .orow { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.3fr) auto; gap: 20px; align-items: center; padding: 14px 20px; border-top: 1px solid var(--line); }
  .ot { font-weight: 600; }
  .gone { font-size: 12px; color: var(--text-3); text-decoration: line-through; }
  .now { font-size: 12px; color: var(--text); }
  .acts { display: flex; gap: 8px; }
  .list { border: 1px solid var(--line); border-radius: 8px; overflow: hidden; }
  .li { display: flex; align-items: center; gap: 14px; min-height: 54px; padding: 0 12px 0 20px; border-bottom: 1px solid var(--line); }
  .li:hover { background: var(--hover); }
  .li:hover .ib { opacity: 1; }
  .li.add { border-bottom: 0; }
  .li.add input { flex: 1; height: 36px; padding: 0 10px; border: 1px solid var(--line); border-radius: 6px; background: var(--bg); outline: none; }
  .sw { width: 12px; height: 12px; border-radius: 3px; flex: none; }
  .sd { width: 9px; height: 9px; border-radius: 50%; flex: none; }
  .nm { flex: 1; font-weight: 500; }
  .ren { flex: 1; height: 34px; padding: 0 8px; border: 1px solid var(--text); border-radius: 4px; background: var(--bg); outline: none; }
  .ib { width: 34px; height: 34px; border: 0; border-radius: 6px; background: none; color: var(--text-2); opacity: 0; font-size: 14px; }
  .ib:hover:not(:disabled) { background: var(--line); color: var(--text); }
  .ib:disabled { opacity: 0 !important; }
  .ib:focus-visible { opacity: 1; }
  .inline { display: flex; align-items: center; gap: 12px; font-size: 14px; color: var(--text-2); }
  select { height: 36px; padding: 0 10px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel); }
  .pref { display: flex; align-items: center; gap: 20px; padding: 14px 0; border-bottom: 1px solid #1f1f1d; }
  .seg { display: flex; gap: 2px; padding: 3px; background: var(--panel); border-radius: 8px; }
  .seg button { height: 32px; padding: 0 16px; border: 0; border-radius: 6px; background: none; color: var(--text-2); font-size: 13px; }
  .seg button.on { background: var(--pressed); color: var(--text); font-weight: 600; }
  .switch { position: relative; width: 48px; height: 28px; padding: 0; border: 0; border-radius: 999px; background: var(--border); transition: background var(--t-med); }
  .switch span { position: absolute; top: 3px; left: 3px; width: 22px; height: 22px; border-radius: 50%; background: var(--text); transition: left var(--t-med), background var(--t-med); }
  .switch.on { background: var(--text); }
  .switch.on span { left: 23px; background: var(--on-text); }
  .ver { font-size: 32px; font-stretch: 75%; }
  .danger-box { border-color: #5a2e28; background: none; }
  .danger-t { color: #ff8a78; font-weight: 600; }
  .btn.danger { border-color: var(--danger); color: #ff8a78; }
  .btn.danger:hover { background: rgba(255, 107, 87, 0.12); }
  .btn.danger.solid { background: var(--danger); color: var(--on-text); font-weight: 600; }
  .rm { color: #ff8a78; }
  .kp { color: var(--success); }
  p { margin: 0; }
  .chk { display: flex; align-items: center; gap: 10px; padding: 12px 14px; background: var(--bg); border-radius: 8px; color: var(--text); }
  .chk input { width: 16px; height: 16px; accent-color: var(--text); }
</style>
