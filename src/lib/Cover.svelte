<script>
  // One cover tile: image, or a placeholder (SPEC §6.4).
  import { thumbUrl } from './api.js';

  let { p, v = 0, busy = false, large = false } = $props();

  let failed = $state(false);
  // A new version (thumbnail just made) gets another try.
  $effect(() => {
    v;
    failed = false;
  });

  const hasImage = $derived(p.manualCover != null || p.coverKind === 'image');
  const unsupported = $derived(!hasImage && p.coverKind === 'unsupported');
  const fileName = $derived(unsupported && p.coverRel ? p.coverRel.split('/').pop() : '');
</script>

<div class="cover" class:large>
  {#if hasImage && !failed}
    <img
      src={thumbUrl(p.id, v, large)}
      alt=""
      loading="lazy"
      decoding="async"
      draggable="false"
      onerror={() => (failed = true)}
    />
  {:else if hasImage && failed}
    <div class="ph pending" class:pulse={busy}>{busy ? 'Making thumbnail…' : 'No preview yet'}</div>
  {:else if unsupported}
    <div class="ph center">
      <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"></path><path d="M14 3v5h5"></path><path d="M9.5 13.5l5 5M14.5 13.5l-5 5"></path></svg>
      <span class="mono file">{fileName}</span>
      <span>Can’t be previewed</span>
    </div>
  {:else}
    <div class="ph center dashed">
      <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2"></rect><circle cx="9" cy="10" r="2"></circle><path d="M21 16l-5-5-9 9"></path></svg>
      <span>No cover yet</span>
      <span class="dot" aria-label="Needs attention"></span>
    </div>
  {/if}
</div>

<style>
  .cover {
    position: relative;
    width: 100%;
    aspect-ratio: 2 / 3;
    border-radius: var(--r-cover);
    background: var(--panel);
    box-shadow: var(--cover-edge), var(--cover-shadow);
    overflow: hidden;
  }
  img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .ph {
    position: absolute; inset: 0; padding: 12px;
    display: flex; color: var(--text-3); font-size: 12px; text-align: center;
  }
  .pending { align-items: flex-end; color: var(--text-4); background: var(--hover); }
  .center { flex-direction: column; align-items: center; justify-content: center; gap: 8px; }
  .dashed { background: transparent; box-shadow: none; outline: 1.5px dashed var(--border); outline-offset: -1.5px; }
  .file { font-size: 11px; color: var(--text-2); word-break: break-all; }
  .dot { position: absolute; top: 8px; right: 8px; width: 8px; height: 8px; border-radius: 50%; background: var(--warning); }
  .pulse { animation: pulse 1600ms ease-in-out infinite; }
  @keyframes pulse { 0%, 100% { opacity: 0.55; } 50% { opacity: 1; } }
  @media (prefers-reduced-motion: reduce) { .pulse { animation: none; } }
</style>
