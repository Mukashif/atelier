<script>
  // First run (SPEC §6.9): "Move in." when Bookrack is found, else "Choose your library."
  import { open } from '@tauri-apps/plugin-dialog';
  import { api } from './api.js';

  let { bookrack = null, onStarted } = $props();

  let root = $state(bookrack?.rootFolder ?? null);
  let error = $state('');
  let working = $state(false);

  async function chooseFolder() {
    const picked = await open({ directory: true, title: 'Choose your library folder', defaultPath: root ?? undefined });
    if (typeof picked === 'string') root = picked;
    return picked;
  }

  async function importNow() {
    error = '';
    working = true;
    try {
      await api.importBookrack(bookrack.dataDir, root);
      onStarted();
    } catch (e) {
      error = String(e);
      working = false;
    }
  }

  async function startFresh() {
    error = '';
    const picked = root && !bookrack ? root : await chooseFolder();
    if (typeof picked !== 'string') return;
    try {
      await api.setRoot(picked);
      onStarted();
    } catch (e) {
      error = String(e);
    }
  }
</script>

<main class="scroll">
  {#if bookrack}
    <div class="cols">
      <section class="intro">
        <span class="kicker">First run</span>
        <h1 class="display big">Move<br />in.</h1>
        <p>Atelier brings your Bookrack library across in one step. It only reads Bookrack’s data — your project folders and Bookrack itself stay exactly as they are.</p>
        <div class="lists">
          <div>
            <span class="kicker">Comes across</span>
            {#each ['Renamed titles', 'Categories and statuses', 'Tags', 'Favorites and archived projects', 'Covers you picked by hand', 'Existing thumbnails, so covers show instantly'] as k}
              <span class="li"><span class="bar"></span>{k}</span>
            {/each}
          </div>
          <div>
            <span class="kicker">Left behind</span>
            <span class="li muted"><span class="bar dim"></span>Custom sort order</span>
            <span class="li muted"><span class="bar dim"></span>Fireflies setting</span>
            <span class="note">A copy of Bookrack’s original files is kept in Atelier’s data folder.</span>
          </div>
        </div>
      </section>

      <section class="steps">
        <div class="step">
          <span class="n display">01</span>
          <div class="body">
            <h2 class="display">Bookrack data found</h2>
            <span class="mono path">{bookrack.dataDir}</span>
          </div>
        </div>
        <div class="step">
          <span class="n display">02</span>
          <div class="body">
            <h2 class="display">Library folder</h2>
            <span class="mono path">{root ?? 'Not set — choose one'}</span>
          </div>
          <button class="btn" onclick={chooseFolder}>Change</button>
        </div>
        <div class="step">
          <span class="n display">03</span>
          <div class="body">
            <h2 class="display">What will come across</h2>
            <div class="figures">
              <div><span class="fig display">{bookrack.projects.toLocaleString('en-GB')}</span><span>projects with edits</span></div>
              <div><span class="fig display">{bookrack.favorites}</span><span>favorites</span></div>
              <div><span class="fig display">{bookrack.archived}</span><span>archived</span></div>
              <div><span class="fig display">{bookrack.tags}</span><span>tags in use</span></div>
            </div>
          </div>
        </div>
        {#if error}<p class="error" role="alert">{error}</p>{/if}
        <div class="go">
          <button class="btn primary large" onclick={importNow} disabled={working || !root}>
            {working ? 'Importing…' : `Import ${bookrack.projects.toLocaleString('en-GB')} projects`}
          </button>
          <button class="link" onclick={startFresh}>Start fresh with an empty library</button>
        </div>
      </section>
    </div>
  {:else}
    <section class="fresh">
      <h1 class="display big">Choose your<br />library.</h1>
      <p>Pick the one folder that holds your month folders. Atelier only reads it — nothing inside is ever moved or changed.</p>
      <p class="mono layout">Library / November 2025 / Project<br />Library / 2025-10 / Project</p>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      <button class="btn primary large" onclick={startFresh}>Choose folder…</button>
    </section>
  {/if}
</main>

<style>
  main { height: 100%; }
  .cols { display: flex; flex-wrap: wrap; gap: 120px; padding: 120px var(--gutter) 96px; max-width: 2200px; margin: 0 auto; }
  .intro { flex: 1 1 520px; display: flex; flex-direction: column; gap: 32px; }
  .kicker { display: block; font-size: 12px; letter-spacing: 0.14em; text-transform: uppercase; color: var(--text-2); margin-bottom: 4px; }
  .big { margin: 0; font-size: clamp(120px, 11vw, 220px); line-height: 0.82; font-stretch: 62%; font-weight: 800; letter-spacing: -0.03em; }
  p { margin: 0; max-width: 560px; font-size: 19px; line-height: 1.5; color: #c8c8c2; }
  .lists { display: flex; gap: 48px; flex-wrap: wrap; margin-top: 16px; }
  .lists > div { display: flex; flex-direction: column; gap: 10px; min-width: 240px; }
  .li { display: flex; align-items: center; gap: 10px; font-size: 14px; }
  .muted { color: var(--text-2); }
  .bar { width: 14px; height: 2px; background: var(--text); }
  .bar.dim { background: var(--text-4); }
  .note { font-size: 13px; color: var(--text-3); max-width: 260px; margin-top: 6px; line-height: 1.5; }
  .steps { flex: 1 1 640px; display: flex; flex-direction: column; border-top: 2px solid var(--text); }
  .step { display: flex; gap: 32px; padding: 32px 0; border-bottom: 1px solid var(--line); align-items: flex-start; }
  .step .btn { align-self: center; }
  .n { font-size: 48px; line-height: 0.9; width: 72px; flex: none; }
  .body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 8px; }
  h2 { margin: 0; font-size: 28px; font-stretch: 75%; }
  .path { font-size: 13px; color: var(--text-2); overflow-wrap: anywhere; }
  .figures { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 24px; margin-top: 12px; }
  .figures div { display: flex; flex-direction: column; gap: 4px; font-size: 13px; color: var(--text-2); }
  .fig { font-size: 52px; line-height: 1; color: var(--text); font-stretch: 75%; font-weight: 600; }
  .go { display: flex; align-items: center; gap: 24px; padding-top: 40px; flex-wrap: wrap; }
  .large { height: 60px; padding: 0 32px; font-size: 17px; }
  .link { border: 0; background: none; color: var(--text-2); text-decoration: underline; font-size: 15px; }
  .error { color: var(--danger); font-size: 14px; margin-top: 16px; }
  .fresh { max-width: 760px; padding: 160px var(--gutter); display: flex; flex-direction: column; gap: 24px; }
  .layout { font-size: 13px; color: var(--text-3); line-height: 1.7; }
  button:disabled { opacity: 0.5; cursor: default; }
</style>
