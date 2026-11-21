<script>
  // Right-click menu on a cover (SPEC §6.6).
  import { categoryColor, statusColor } from './api.js';

  let { x, y, p, statuses, categories, onAction, onClose } = $props();

  let el = $state();
  let sub = $state(null); // 'status' | 'category'
  let pos = $state({ left: x, top: y, flip: false });

  $effect(() => {
    if (!el) return;
    const r = el.getBoundingClientRect();
    const left = x + r.width > innerWidth - 8 ? Math.max(8, x - r.width) : x;
    const top = y + r.height > innerHeight - 8 ? Math.max(8, innerHeight - r.height - 8) : y;
    pos = { left, top, flip: left + r.width + 230 > innerWidth };
  });

  function act(kind, value) {
    onAction(kind, value);
    onClose();
  }
  function onDocDown(e) {
    if (el && !el.contains(e.target)) onClose();
  }
  function onKey(e) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onClose();
    }
  }
</script>

<svelte:document onmousedown={onDocDown} onkeydown={onKey} />
<svelte:window onblur={onClose} onresize={onClose} />

<div class="ctx" bind:this={el} role="menu" aria-label="Actions for {p.title}" style:left={`${pos.left}px`} style:top={`${pos.top}px`}>
  <button class="mi" role="menuitem" onclick={() => act('open')}>Open folder<kbd>Enter</kbd></button>
  <button class="mi" role="menuitem" onclick={() => act('rename')}>Rename<kbd>F2</kbd></button>
  <button class="mi" role="menuitem" onclick={() => act('cover')}>Change cover…</button>
  <span class="sep"></span>
  <div class="sub" role="none">
    <button class="mi" class:on={sub === 'status'} role="menuitem" aria-haspopup="menu" aria-expanded={sub === 'status'}
      onmouseenter={() => (sub = 'status')} onclick={() => (sub = 'status')}>
      Status<span class="cur">{p.status}</span><span class="arrow">▸</span>
    </button>
    {#if sub === 'status'}
      <div class="ctx inner" class:flip={pos.flip} role="menu">
        {#each statuses as s}
          <button class="mi" role="menuitemradio" aria-checked={p.status === s} onclick={() => act('status', s)}>
            <span class="dot" style:background={statusColor(s, statuses)}></span>{s}<span class="tick">{p.status === s ? '✓' : ''}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
  <div class="sub" role="none">
    <button class="mi" class:on={sub === 'category'} role="menuitem" aria-haspopup="menu" aria-expanded={sub === 'category'}
      onmouseenter={() => (sub = 'category')} onclick={() => (sub = 'category')}>
      Category<span class="cur">{p.category}</span><span class="arrow">▸</span>
    </button>
    {#if sub === 'category'}
      <div class="ctx inner" class:flip={pos.flip} role="menu">
        {#each categories as c}
          <button class="mi" role="menuitemradio" aria-checked={p.category === c} onclick={() => act('category', c)}>
            <span class="sw" style:background={categoryColor(c)}></span>{c}<span class="tick">{p.category === c ? '✓' : ''}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
  <span class="sep"></span>
  <button class="mi" role="menuitem" onmouseenter={() => (sub = null)} onclick={() => act('favorite', !p.isFavorite)}>
    {p.isFavorite ? 'Remove from favorites' : 'Add to favorites'}<kbd>F</kbd>
  </button>
  <button class="mi" role="menuitem" onmouseenter={() => (sub = null)} onclick={() => act('archived', !p.isArchived)}>
    {p.isArchived ? 'Unarchive' : 'Archive'}<kbd>Del</kbd>
  </button>
</div>

<style>
  .ctx {
    position: fixed; z-index: 60; width: 270px; padding: 6px;
    background: var(--menu); border: 1px solid var(--border); border-radius: 10px;
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.7); display: flex; flex-direction: column; gap: 1px; font-size: 14px;
  }
  .inner { position: absolute; top: -7px; left: calc(100% + 6px); width: 220px; }
  .inner.flip { left: auto; right: calc(100% + 6px); }
  .sub { position: relative; }
  .mi {
    width: 100%; height: 36px; padding: 0 10px; display: flex; align-items: center; gap: 10px;
    border: 0; border-radius: 6px; background: transparent; color: var(--text); text-align: left;
  }
  .mi:hover, .mi.on, .mi:focus-visible { background: var(--line); outline: none; }
  kbd { margin-left: auto; font-family: var(--font-mono); font-size: 11px; color: var(--text-3); }
  .cur { margin-left: auto; font-size: 12px; color: var(--text-3); }
  .arrow { color: var(--text-3); }
  .tick { margin-left: auto; }
  .dot { width: 7px; height: 7px; border-radius: 50%; }
  .sw { width: 9px; height: 9px; border-radius: 2px; }
  .sep { height: 1px; background: var(--line); margin: 5px 4px; }
</style>
