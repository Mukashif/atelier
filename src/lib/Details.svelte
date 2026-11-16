<script>
  // Details panel that opens under the clicked cover's row (SPEC §6.3).
  import Cover from './Cover.svelte';
  import { categoryColor } from './api.js';

  let {
    p,
    v = 0,
    busy = false,
    statuses,
    categories,
    notchLeft,
    renameRequest = 0,
    onEdit,
    onOpen,
    onClose,
  } = $props();

  let panel = $state();
  let renaming = $state(false);
  let draft = $state('');
  let tagDraft = $state('');
  let titleInput = $state();

  // Scroll just enough to show the panel whenever it opens for a project.
  $effect(() => {
    p.id;
    panel?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  });

  // F2 from the grid starts renaming.
  $effect(() => {
    if (renameRequest > 0) startRename();
  });

  function startRename() {
    draft = p.title;
    renaming = true;
    queueMicrotask(() => {
      titleInput?.focus();
      titleInput?.select();
    });
  }
  function commitRename() {
    const t = draft.trim();
    renaming = false;
    if (t && t !== p.title) onEdit({ kind: 'title', value: t });
  }
  function onTitleKey(e) {
    if (e.key === 'Enter') commitRename();
    else if (e.key === 'Escape') { e.stopPropagation(); renaming = false; }
  }
  function addTag() {
    const t = tagDraft.trim();
    tagDraft = '';
    if (t && !p.tags.includes(t)) onEdit({ kind: 'addTag', value: t });
  }
  const allCategories = $derived(categories.includes(p.category) ? categories : [...categories, p.category]);
</script>

<div class="wrap" bind:this={panel}>
  <span class="notch" style:left={notchLeft} aria-hidden="true"></span>
  <section class="panel" aria-label="Project details">
    <div class="cover"><Cover {p} {v} {busy} large /></div>

    <div class="main">
      <div class="head">
        <div class="titles">
          {#if renaming}
            <input
              class="title-input display"
              bind:this={titleInput}
              bind:value={draft}
              onkeydown={onTitleKey}
              onblur={commitRename}
              aria-label="Rename project"
            />
            <span class="hint">Enter to save · Esc to cancel</span>
          {:else}
            <h2 class="display title">{p.title}</h2>
          {/if}
          <span class="mono path">{p.monthFolder} / {p.folderName}</span>
        </div>
        <button class="icon-btn" aria-label="Close details" onclick={onClose}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18"></path></svg>
        </button>
      </div>
      <div class="actions">
        <button class="btn primary" onclick={onOpen}>Open folder</button>
        <button class="btn" onclick={startRename}>Rename</button>
        <button class="btn" onclick={() => onEdit({ kind: 'favorite', value: !p.isFavorite })}>
          {p.isFavorite ? '★ Favourite' : '☆ Favourite'}
        </button>
        <button class="btn quiet" onclick={() => onEdit({ kind: 'archived', value: !p.isArchived })}>
          {p.isArchived ? 'Unarchive' : 'Archive'}
        </button>
      </div>
    </div>

    <div class="fields">
      <span class="label">Status</span>
      <div class="seg" role="radiogroup" aria-label="Status">
        {#each statuses as s}
          <button
            role="radio"
            aria-checked={p.status === s}
            class:on={p.status === s}
            onclick={() => onEdit({ kind: 'status', value: s })}>{s}</button
          >
        {/each}
      </div>

      <span class="label">Category</span>
      <div class="cat">
        <span class="swatch" style:background={categoryColor(p.category)}></span>
        <select value={p.category} onchange={(e) => onEdit({ kind: 'category', value: e.currentTarget.value })} aria-label="Category">
          {#each allCategories as c}<option value={c}>{c}</option>{/each}
        </select>
      </div>

      <span class="label">Tags</span>
      <div class="tags">
        {#each p.tags as t}
          <span class="tag">{t}<button aria-label="Remove tag {t}" onclick={() => onEdit({ kind: 'removeTag', value: t })}>×</button></span>
        {/each}
        <input
          class="tag-input"
          placeholder="Add tag…"
          bind:value={tagDraft}
          onkeydown={(e) => e.key === 'Enter' && addTag()}
          onblur={addTag}
          aria-label="Add tag"
        />
      </div>
    </div>
  </section>
</div>

<style>
  .wrap { position: relative; margin-top: -6px; scroll-margin: 24px; }
  .notch {
    position: absolute; top: -8px; width: 16px; height: 16px; z-index: 1;
    background: var(--panel); border-left: 1px solid var(--border); border-top: 1px solid var(--border);
    transform: rotate(45deg);
  }
  .panel {
    display: flex; gap: 36px; padding: 28px; flex-wrap: wrap;
    background: var(--panel); border: 1px solid var(--border); border-radius: var(--r-panel);
    animation: open var(--t-med) var(--ease);
  }
  @keyframes open { from { opacity: 0; transform: translateY(-6px); } }
  .cover { flex: 0 0 200px; }
  .main { flex: 1 1 420px; display: flex; flex-direction: column; gap: 16px; min-width: 0; }
  .head { display: flex; align-items: flex-start; gap: 12px; }
  .titles { flex: 1; display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .title { margin: 0; font-size: 40px; line-height: 1; font-stretch: 72%; }
  .title-input {
    font-size: 36px; line-height: 1.1; padding: 2px 8px; margin-left: -9px;
    background: var(--bg); border: 1px solid var(--text); border-radius: 4px; outline: none;
  }
  .hint { font-size: 12px; color: var(--text-3); }
  .path { font-size: 13px; color: var(--text-3); overflow-wrap: anywhere; }
  .actions { display: flex; gap: 8px; flex-wrap: wrap; }
  .fields {
    flex: 1 1 520px; display: grid; grid-template-columns: 100px minmax(0, 1fr);
    gap: 14px 18px; align-items: center; font-size: 14px; align-content: start;
  }
  .label { color: var(--text-3); }
  .seg { display: grid; grid-auto-flow: column; grid-auto-columns: minmax(0, 1fr); gap: 4px; padding: 4px; background: var(--bg); border-radius: 8px; }
  .seg button { height: 34px; border: 0; border-radius: var(--r-control); background: transparent; color: var(--text-2); font-size: 13px; }
  .seg button:hover { color: var(--text); }
  .seg button.on { background: var(--pressed); color: var(--text); font-weight: 600; }
  .cat { display: flex; align-items: center; gap: 10px; }
  .swatch { width: 10px; height: 10px; border-radius: 2px; flex: none; }
  select { flex: 1; height: 38px; padding: 0 10px; border: 1px solid var(--line); border-radius: var(--r-control); background: var(--bg); }
  .tags { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; }
  .tag { height: 28px; padding: 0 4px 0 10px; display: inline-flex; align-items: center; gap: 2px; border-radius: 999px; background: var(--bg); font-size: 12px; }
  .tag button { width: 20px; height: 20px; border: 0; border-radius: 50%; background: transparent; color: var(--text-3); }
  .tag button:hover { color: var(--text); }
  .tag-input { height: 28px; width: 120px; padding: 0 10px; border: 1px dashed var(--border); border-radius: 999px; background: transparent; font-size: 12px; outline: none; }
  .tag-input:focus { border-style: solid; border-color: var(--text); }
</style>
