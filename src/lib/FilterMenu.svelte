<script>
  // A filter pill + its checkbox menu (SPEC §6.10). OR within a menu.
  import Menu from './Menu.svelte';

  let { label, options, selected, onChange, searchable = false } = $props();
  // options: [{ name, count, swatch?: css colour, round?: bool }]

  let open = $state(false);
  let find = $state('');
  const shown = $derived(
    find ? options.filter((o) => o.name.toLowerCase().includes(find.toLowerCase())) : options,
  );

  function toggle(name) {
    onChange(selected.includes(name) ? selected.filter((s) => s !== name) : [...selected, name]);
  }
</script>

<div class="wrap">
  <button
    class="pill"
    class:active={selected.length > 0}
    class:open
    data-menu-trigger
    aria-haspopup="menu"
    aria-expanded={open}
    onclick={() => (open = !open)}
  >
    {label}{#if selected.length}&nbsp;· {selected.length}{/if}
    <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" aria-hidden="true"><path d="M6 9l6 6 6-6"></path></svg>
  </button>
  <Menu bind:open width={280}>
    {#if searchable}
      <input class="find" placeholder="Find…" bind:value={find} aria-label="Find {label.toLowerCase()}" />
    {/if}
    {#each shown as o (o.name)}
      <div class="row">
        <button class="mi" role="menuitemcheckbox" aria-checked={selected.includes(o.name)} onclick={() => toggle(o.name)}>
          <span class="box" class:on={selected.includes(o.name)}>{selected.includes(o.name) ? '✓' : ''}</span>
          {#if o.swatch}<span class="sw" class:round={o.round} style:background={o.swatch}></span>{/if}
          <span class="name">{o.name}</span>
          <span class="n">{o.count.toLocaleString('en-GB')}</span>
        </button>
        <button class="only" onclick={() => onChange([o.name])}>Only</button>
      </div>
    {:else}
      <span class="none">Nothing to filter by yet</span>
    {/each}
    <div class="foot">
      <button class="clear" onclick={() => onChange([])} disabled={!selected.length}>Clear</button>
      <span>{selected.length ? `${selected.length} selected · any of them` : ''}</span>
    </div>
  </Menu>
</div>

<style>
  .wrap { position: relative; }
  .pill {
    height: 34px; padding: 0 12px 0 14px; display: inline-flex; align-items: center; gap: 6px;
    border-radius: 999px; border: 1px solid var(--border); background: transparent; color: var(--text); font-size: 13px;
  }
  .pill:hover, .pill.open { border-color: var(--text); }
  .pill.active { background: var(--text); color: var(--on-text); border-color: var(--text); }
  .find { height: 34px; margin: 2px 2px 6px; padding: 0 10px; border: 1px solid var(--border); border-radius: 8px; background: var(--bg); outline: none; }
  .row { position: relative; display: flex; align-items: center; }
  .row:hover .only { opacity: 1; }
  .box {
    width: 18px; height: 18px; flex: none; border-radius: 5px; border: 1.5px solid var(--text-4);
    display: flex; align-items: center; justify-content: center; font-size: 12px; font-weight: 700; color: var(--on-text);
  }
  .box.on { background: var(--text); border-color: var(--text); }
  .sw { width: 9px; height: 9px; border-radius: 2px; flex: none; }
  .sw.round { border-radius: 50%; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .only {
    position: absolute; right: 52px; height: 24px; padding: 0 8px; border: 0; border-radius: 5px;
    background: var(--bg); color: var(--text-2); font-size: 12px; opacity: 0; transition: opacity var(--t-fast);
  }
  .only:hover { color: var(--text); }
  .none { padding: 10px; color: var(--text-3); font-size: 13px; }
  .foot { display: flex; justify-content: space-between; align-items: center; padding: 8px 10px 4px; margin-top: 4px; border-top: 1px solid var(--line); font-size: 12px; color: var(--text-3); }
  .clear { border: 0; background: none; color: var(--text-2); text-decoration: underline; font-size: 13px; padding: 0; }
  .clear:disabled { opacity: 0.4; }
</style>
