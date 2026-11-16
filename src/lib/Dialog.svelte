<script>
  // Modal dialog (SPEC §6.11). Esc cancels; the safe button gets focus.
  let { title, onCancel, children, actions, width = 560, labelId = 'dlg-title' } = $props();
  let el = $state();

  $effect(() => {
    el?.querySelector('[data-autofocus]')?.focus();
  });
  function onKey(e) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onCancel();
    }
    // Keep Tab inside the dialog.
    if (e.key === 'Tab' && el) {
      const f = [...el.querySelectorAll('button, input, select, [tabindex]:not([tabindex="-1"])')].filter((x) => !x.disabled);
      if (!f.length) return;
      const [first, last] = [f[0], f[f.length - 1]];
      if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
      else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
    }
  }
</script>

<div class="scrim" role="presentation" onmousedown={(e) => e.target === e.currentTarget && onCancel()}>
  <div class="dlg" role="alertdialog" aria-modal="true" aria-labelledby={labelId} style:width={`${width}px`} bind:this={el} onkeydown={onKey} tabindex="-1">
    <h2 id={labelId} class="display">{title}</h2>
    <div class="content">{@render children()}</div>
    <div class="actions">{@render actions()}</div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; z-index: 100; background: rgba(0, 0, 0, 0.6); display: flex; align-items: center; justify-content: center; animation: fade var(--t-fast) ease-out; }
  .dlg {
    max-width: calc(100vw - 48px); padding: 28px; background: var(--panel); border: 1px solid var(--border);
    border-radius: var(--r-dialog); box-shadow: 0 40px 100px rgba(0, 0, 0, 0.75);
    display: flex; flex-direction: column; gap: 16px; outline: none; animation: rise var(--t-med) var(--ease);
  }
  h2 { margin: 0; font-size: 30px; line-height: 1; font-stretch: 75%; }
  .content { display: flex; flex-direction: column; gap: 10px; font-size: 14px; line-height: 1.5; color: #c8c8c2; }
  .actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 4px; }
  @keyframes fade { from { opacity: 0; } }
  @keyframes rise { from { opacity: 0; transform: translateY(8px); } }
</style>
