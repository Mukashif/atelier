<script>
  // Library / Favorites / Archive grid (SPEC §6.3, §6.12).
  // Month rows: a label column that stays in view + a wrapping grid. The
  // details panel is inserted as an extra row under the clicked cover's row.
  import { untrack } from 'svelte';
  import Cover from './Cover.svelte';
  import Details from './Details.svelte';
  import { statusColor } from './api.js';

  let {
    projects,
    view = 'library',
    grouped = true,
    versions = {},
    busy = false,
    statuses,
    categories,
    onEdit,
    onOpen,
    selectMode = false,
    selectedIds = [],
    onToggleSelect = () => {},
    onSetSelected = () => {},
    onContext = () => {},
    onChangeCover = () => {},
    request = null,
  } = $props();

  // Another screen asked to select (and maybe rename) a project.
  $effect(() => {
    if (!request) return;
    const r = request;
    untrack(() => {
      selectedId = r.id;
      if (r.rename) renameRequest += 1;
    });
    queueMicrotask(() => document.querySelector(`[data-id="${request.id}"]`)?.scrollIntoView({ block: "center" }));
  });
  const selectedSet = $derived(new Set(selectedIds));
  let anchor = null; // last clicked id, for Shift-click ranges

  function clickTile(e, p, items) {
    if (!selectMode) return select(p.id);
    if (e.shiftKey && anchor != null) {
      const a = items.findIndex((x) => x.id === anchor);
      const b = items.findIndex((x) => x.id === p.id);
      if (a >= 0 && b >= 0) {
        const [lo, hi] = a < b ? [a, b] : [b, a];
        onSetSelected(items.slice(lo, hi + 1).map((x) => x.id), true);
        anchor = p.id;
        return;
      }
    }
    anchor = p.id;
    onToggleSelect(p.id);
  }

  const GAP = 22;
  const minWidth = $derived(view === 'favorites' ? 210 : 150);

  let selectedId = $state(null);
  let renameRequest = $state(0);
  let gridWidth = $state(0);
  const cols = $derived(Math.max(1, Math.floor((gridWidth + GAP) / (minWidth + GAP))));

  // Group consecutive projects (already sorted by the core) into months.
  const groups = $derived.by(() => {
    if (!grouped) return [{ key: 'all', label: null, items: projects }];
    // Keyed by month folder, in order of first appearance, so keys stay
    // unique even if the list isn't in month order.
    const byKey = new Map();
    for (const p of projects) {
      const g = byKey.get(p.monthFolder);
      if (g) g.items.push(p);
      else byKey.set(p.monthFolder, { key: p.monthFolder, label: labelFor(p), items: [p] });
    }
    return [...byKey.values()];
  });

  function labelFor(p) {
    return {
      num: p.month ? String(p.month).padStart(2, '0') : '—',
      name: p.year ? p.monthFolder : `${p.monthFolder} · undated`,
    };
  }

  const rowsOf = (items) => {
    const rows = [];
    for (let i = 0; i < items.length; i += cols) rows.push(items.slice(i, i + cols));
    return rows;
  };

  const selected = $derived(projects.find((p) => p.id === selectedId) ?? null);
  // Close the panel if the selected project left this view (archived, filtered…).
  $effect(() => {
    if (selectedId != null && !selected) selectedId = null;
  });

  function notchLeft(row) {
    const c = row.findIndex((p) => p.id === selectedId);
    return `calc((100% - ${(cols - 1) * GAP}px) * ${((c + 0.5) / cols).toFixed(4)} + ${c * GAP - 8}px)`;
  }

  function select(id) {
    selectedId = selectedId === id ? null : id;
  }

  // ---- keyboard (SPEC §6.13)
  function move(delta) {
    if (!projects.length) return;
    if (selectedId == null) { selectedId = projects[0].id; return; }
    const g = groups.find((g) => g.items.some((p) => p.id === selectedId));
    const gi = groups.indexOf(g);
    const i = g.items.findIndex((p) => p.id === selectedId);
    let j = i + delta;
    if (j >= 0 && j < g.items.length) selectedId = g.items[j].id;
    else if (j < 0 && gi > 0) selectedId = groups[gi - 1].items.at(-1).id;
    else if (j >= g.items.length && gi < groups.length - 1) selectedId = groups[gi + 1].items[0].id;
    queueMicrotask(() => document.querySelector(`[data-id="${selectedId}"]`)?.focus({ preventScroll: false }));
  }

  function onKey(e) {
    if (selectMode && !['a', 'A'].includes(e.key)) return;
    const typing = e.target.closest?.('input, textarea, select, [contenteditable]');
    if (typing) return;
    switch (e.key) {
      case 'ArrowRight': e.preventDefault(); move(1); break;
      case 'ArrowLeft': e.preventDefault(); move(-1); break;
      case 'ArrowDown': e.preventDefault(); move(cols); break;
      case 'ArrowUp': e.preventDefault(); move(-cols); break;
      case 'Escape': selectedId = null; break;
      case 'a': case 'A':
        if (selectMode && (e.ctrlKey || e.metaKey)) { e.preventDefault(); onSetSelected(projects.map((p) => p.id), true); }
        break;
      case 'Enter': if (selected) { e.preventDefault(); onOpen(selected.id); } break;
      case 'F2': if (selected) { e.preventDefault(); renameRequest += 1; } break;
      case 'f': case 'F':
        if (selected && !e.ctrlKey && !e.metaKey) onEdit([selected.id], { kind: 'favorite', value: !selected.isFavorite });
        break;
      case 'Delete':
        if (selected) onEdit([selected.id], { kind: 'archived', value: !selected.isArchived });
        break;
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="months" class:archive={view === 'archive'}>
  {#each groups as g (g.key)}
    <section class="month" class:wall={!g.label}>
      {#if g.label}
        <div class="label">
          <div class="sticky">
            <span class="num display">{g.label.num}</span>
            <span class="name">{g.label.name}</span>
            <span class="count mono">{g.items.length} {view === 'archive' ? 'ARCHIVED' : 'PROJECTS'}</span>
            {#if selectMode}
              {@const allOn = g.items.every((p) => selectedSet.has(p.id))}
              <button class="btn small" onclick={() => onSetSelected(g.items.map((p) => p.id), !allOn)}>
                {allOn ? 'Deselect all' : 'Select all'} {g.items.length}
              </button>
            {/if}
          </div>
        </div>
      {/if}
      <div class="grid-col" bind:clientWidth={gridWidth}>
        {#each rowsOf(g.items) as row}
          <div class="row" style:grid-template-columns={`repeat(${cols}, minmax(0, 1fr))`} style:gap={`32px ${GAP}px`}>
            {#each row as p (p.id)}
              <button
                class="tile"
                class:selected={!selectMode && p.id === selectedId}
                class:picked={selectMode && selectedSet.has(p.id)}
                class:dim={selectMode && selectedIds.length > 0 && !selectedSet.has(p.id)}
                data-id={p.id}
                aria-pressed={selectMode ? selectedSet.has(p.id) : p.id === selectedId}
                aria-label={p.title}
                onclick={(e) => clickTile(e, p, g.items)}
                ondblclick={() => !selectMode && onOpen(p.id)}
                oncontextmenu={(e) => { e.preventDefault(); if (!selectMode) onContext(e, p); }}
              >
                <span class="cover-wrap">
                  <Cover {p} v={versions[p.id] ?? 0} {busy} />
                  {#if selectMode}<span class="check" class:on={selectedSet.has(p.id)} aria-hidden="true">✓</span>{/if}
                </span>
                <span class="t">{p.title}</span>
                <span class="s">
                  <span class="dot" style:background={statusColor(p.status, statuses)}></span>{p.status}
                  {#if p.isFavorite}<span class="star" aria-label="Favourite">★</span>{/if}
                </span>
              </button>
            {/each}
          </div>
          {#if !selectMode && selected && row.some((p) => p.id === selectedId)}
            <Details
              p={selected}
              v={versions[selected.id] ?? 0}
              {busy}
              {statuses}
              {categories}
              {renameRequest}
              notchLeft={notchLeft(row)}
              onEdit={(edit) => onEdit([selected.id], edit)}
              onOpen={() => onOpen(selected.id)}
              onChangeCover={() => onChangeCover(selected)}
              onClose={() => (selectedId = null)}
            />
          {/if}
        {/each}
      </div>
    </section>
  {/each}
</div>

<style>
  .months { display: flex; flex-direction: column; }
  .month {
    display: flex;
    border-bottom: 1px solid var(--line);
    content-visibility: auto;
    contain-intrinsic-size: auto 640px;
  }
  .label { flex: 0 0 240px; padding: 32px 24px 32px var(--gutter); border-right: 1px solid var(--line); }
  .sticky { position: sticky; top: 24px; display: flex; flex-direction: column; gap: 6px; }
  .num { font-size: 72px; line-height: 0.85; }
  .name { font-weight: 600; }
  .count { font-size: 12px; color: var(--text-3); }
  .grid-col { flex: 1; min-width: 0; padding: 32px var(--gutter) 40px 40px; display: flex; flex-direction: column; gap: 32px; }
  .wall .grid-col { padding-left: var(--gutter); }
  .row { display: grid; }

  .tile {
    display: flex; flex-direction: column; gap: 8px; padding: 0; border: 0; background: transparent;
    text-align: left; min-width: 0; color: var(--text); border-radius: 3px;
  }
  .tile :global(.cover) { transition: transform var(--t-med) ease-out, box-shadow var(--t-med) ease-out; }
  .tile:hover :global(.cover) { transform: translateY(-4px); box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.12), 0 20px 40px rgba(0, 0, 0, 0.6); }
  .tile.selected :global(.cover) { outline: 3px solid var(--text); outline-offset: 4px; }
  .tile:focus-visible { outline: none; }
  .tile:focus-visible :global(.cover) { outline: 2px solid var(--text); outline-offset: 4px; }
  .cover-wrap { position: relative; display: block; }
  .check {
    position: absolute; top: 8px; left: 8px; width: 24px; height: 24px; border-radius: 6px;
    border: 1.5px solid rgba(229, 223, 209, 0.8); background: rgba(18, 18, 18, 0.55);
    color: transparent; display: flex; align-items: center; justify-content: center; font-size: 14px; font-weight: 700;
  }
  .check.on { background: var(--text); border-color: var(--text); color: var(--on-text); }
  .tile.picked :global(.cover) { outline: 3px solid var(--text); outline-offset: 4px; }
  .tile.dim { opacity: 0.62; }
  .tile.dim:hover { opacity: 1; }
  :global(.btn.small) { height: 32px; padding: 0 12px; font-size: 13px; align-self: flex-start; margin-top: 12px; }
  .t { font-size: 13px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .s { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-3); }
  .dot { width: 6px; height: 6px; border-radius: 50%; flex: none; }
  .star { margin-left: auto; color: var(--text); }

  .archive .tile :global(.cover) { filter: grayscale(1); opacity: 0.42; }
  .archive .tile:hover :global(.cover), .archive .tile.selected :global(.cover) { filter: none; opacity: 1; }
  .archive .num { color: var(--text-4); }
</style>
