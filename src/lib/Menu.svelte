<script>
  // A dropdown anchored under its trigger. Closes on outside click or Esc.
  let { open = $bindable(false), align = 'left', width = 260, children } = $props();
  let el = $state();

  function onDocDown(e) {
    if (open && el && !el.contains(e.target) && !e.target.closest?.('[data-menu-trigger]')) open = false;
  }
  function onKey(e) {
    if (open && e.key === 'Escape') {
      e.stopPropagation();
      open = false;
    }
  }
</script>

<svelte:document onmousedown={onDocDown} onkeydown={onKey} />

{#if open}
  <div class="menu" class:right={align === 'right'} style:width={`${width}px`} bind:this={el} role="menu">
    {@render children()}
  </div>
{/if}

<style>
  .menu {
    position: absolute; top: calc(100% + 6px); left: 0; z-index: 40;
    padding: 6px; background: var(--menu); border: 1px solid var(--border); border-radius: 10px;
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.7); display: flex; flex-direction: column; gap: 2px;
    animation: pop var(--t-fast) var(--ease);
  }
  .right { left: auto; right: 0; }
  @keyframes pop { from { opacity: 0; transform: translateY(-4px); } }
  .menu :global(.mi) {
    width: 100%; min-height: 36px; padding: 0 10px; display: flex; align-items: center; gap: 10px;
    border: 0; border-radius: 6px; background: transparent; color: var(--text); text-align: left; font-size: 14px;
  }
  .menu :global(.mi:hover), .menu :global(.mi:focus-visible) { background: var(--line); outline: none; }
  .menu :global(.mi .n) { margin-left: auto; font-size: 12px; color: var(--text-3); font-variant-numeric: tabular-nums; }
  .menu :global(.sep) { height: 1px; background: var(--line); margin: 4px; }
</style>
