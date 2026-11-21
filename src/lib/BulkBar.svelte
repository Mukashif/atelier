<script>
  // Selection bar shown in select mode (SPEC §6.5).
  import Menu from './Menu.svelte';
  import { categoryColor, statusColor } from './api.js';

  let { count, statuses, categories, tags, onEdit, onClear, onDone } = $props();

  let statusOpen = $state(false);
  let catOpen = $state(false);
  let tagOpen = $state(false);
  let tagDraft = $state('');

  function apply(edit) {
    statusOpen = catOpen = tagOpen = false;
    if (count) onEdit(edit);
  }
  function addTag(t) {
    t = t.trim();
    tagDraft = '';
    if (t) apply({ kind: 'addTag', value: t });
  }
</script>

<div class="bar" role="toolbar" aria-label="Bulk edit">
  <span class="count"><span class="tick" aria-hidden="true">✓</span>{count} selected</span>
  <button class="link" onclick={onClear} disabled={!count}>Clear</button>
  <span class="div"></span>

  <div class="wrap">
    <button class="b" data-menu-trigger aria-haspopup="menu" aria-expanded={statusOpen} disabled={!count} onclick={() => (statusOpen = !statusOpen)}>Set status ▾</button>
    <Menu bind:open={statusOpen} width={230}>
      {#each statuses as s}
        <button class="mi" onclick={() => apply({ kind: 'status', value: s })}><span class="dot" style:background={statusColor(s, statuses)}></span>{s}<span class="n">all {count}</span></button>
      {/each}
    </Menu>
  </div>
  <div class="wrap">
    <button class="b" data-menu-trigger aria-haspopup="menu" aria-expanded={catOpen} disabled={!count} onclick={() => (catOpen = !catOpen)}>Set category ▾</button>
    <Menu bind:open={catOpen} width={230}>
      {#each categories as c}
        <button class="mi" onclick={() => apply({ kind: 'category', value: c })}><span class="sw" style:background={categoryColor(c)}></span>{c}</button>
      {/each}
    </Menu>
  </div>
  <div class="wrap">
    <button class="b" data-menu-trigger aria-haspopup="dialog" aria-expanded={tagOpen} disabled={!count} onclick={() => (tagOpen = !tagOpen)}>Add tag ▾</button>
    <Menu bind:open={tagOpen} width={320}>
      <input class="tag-input" placeholder="Type a tag, Enter to add" bind:value={tagDraft}
        onkeydown={(e) => e.key === 'Enter' && addTag(tagDraft)} aria-label="New tag" />
      {#if tags.length}
        <span class="hint">Most used</span>
        <div class="chips">
          {#each tags.slice(0, 10) as [t]}<button class="chip" onclick={() => addTag(t)}>{t}</button>{/each}
        </div>
      {/if}
    </Menu>
  </div>
  <button class="b" disabled={!count} onclick={() => apply({ kind: 'favorite', value: true })}>Add to favorites</button>
  <button class="b" disabled={!count} onclick={() => apply({ kind: 'archived', value: true })}>Archive</button>

  <span class="hint grow">Click to select · Shift-click selects a range · Ctrl A selects all</span>
  <button class="done" onclick={onDone}>Done</button>
</div>

<style>
  .bar {
    flex: none; display: flex; align-items: center; gap: 6px; height: 64px; padding: 0 var(--gutter);
    background: #1c1c1a; border-bottom: 1px solid var(--border); flex-wrap: wrap;
  }
  .count { display: flex; align-items: center; gap: 10px; font-weight: 600; font-size: 16px; margin-right: 6px; }
  .tick { width: 24px; height: 24px; border-radius: 6px; background: var(--text); color: var(--on-text); display: flex; align-items: center; justify-content: center; font-size: 14px; }
  .link { border: 0; background: none; color: var(--text-2); text-decoration: underline; font-size: 13px; padding: 0 8px; height: 36px; }
  .div { width: 1px; height: 26px; background: var(--border); margin: 0 8px; }
  .wrap { position: relative; }
  .b { height: 40px; padding: 0 14px; border: 0; border-radius: 8px; background: transparent; color: var(--text); font-size: 14px; }
  .b:hover:not(:disabled), .b[aria-expanded='true'] { background: var(--line); }
  button:disabled { opacity: 0.4; cursor: default; }
  .dot { width: 8px; height: 8px; border-radius: 50%; }
  .sw { width: 10px; height: 10px; border-radius: 3px; }
  .tag-input { height: 38px; padding: 0 12px; border: 1px solid var(--border); border-radius: 8px; background: var(--bg); outline: none; margin: 4px; }
  .hint { font-size: 12px; color: var(--text-3); padding: 4px; }
  .grow { margin-left: auto; padding: 0 16px 0 0; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; padding: 4px; }
  .chip { height: 30px; padding: 0 12px; border: 1px solid var(--border); border-radius: 999px; background: transparent; font-size: 13px; }
  .chip:hover { border-color: var(--text); }
  .done { height: 40px; padding: 0 18px; border: 0; border-radius: 8px; background: var(--text); color: var(--on-text); font-weight: 600; font-size: 14px; }
</style>
