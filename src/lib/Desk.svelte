<script>
  // Desk (SPEC §6.2): what's in progress, what needs attention, this month.
  import Cover from './Cover.svelte';
  import { categoryColor } from './api.js';

  let { all, statuses, categories, versions = {}, busy = false, favorites = 0, onOpenProject, onGo, onEdit, onPickCover, onOpenMonth, onOpenRoot } = $props();

  const DAY = 86_400_000;
  const now = new Date();
  const greeting = now.getHours() < 12 ? 'Good morning.' : now.getHours() < 18 ? 'Good afternoon.' : 'Good evening.';
  const dateLine = now.toLocaleDateString('en-GB', { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' });

  const first = $derived(statuses[0] ?? 'In-Progress');
  const inProgress = $derived(
    all.filter((p) => p.status === first).sort((a, b) => (b.lastOpenedAt ?? 0) - (a.lastOpenedAt ?? 0)),
  );
  const isStale = (p) => p.lastOpenedAt != null && Date.now() - p.lastOpenedAt > 30 * DAY;
  const hasCover = (p) => p.manualCover != null || p.coverKind === 'image';
  const stale = $derived(inProgress.filter(isStale));
  const attention = $derived([
    ...all.filter((p) => !hasCover(p)).map((p) => ({ p, reason: p.coverKind === 'unsupported' ? 'Cover can’t be previewed' : 'No cover image', fix: 'Pick cover' })),
    ...stale.map((p) => ({ p, reason: `Untouched ${Math.floor((Date.now() - p.lastOpenedAt) / DAY)} days`, fix: 'Open' })),
  ]);
  const thisMonth = $derived(all.filter((p) => p.year === now.getFullYear() && p.month === now.getMonth() + 1));
  const completed = $derived(all.filter((p) => p.status !== first).length);
  const pct = $derived(all.length ? Math.round((completed / all.length) * 100) : 0);
  const byCategory = $derived.by(() => {
    const m = new Map(categories.map((c) => [c, 0]));
    for (const p of all) m.set(p.category, (m.get(p.category) ?? 0) + 1);
    return [...m.entries()].filter(([, n]) => n > 0).sort((a, b) => b[1] - a[1]);
  });
  const fmt = (n) => n.toLocaleString('en-GB');

  function relative(ms) {
    if (!ms) return 'Not opened from Atelier yet';
    const d = Math.floor((Date.now() - ms) / DAY);
    if (d > 30) return `Untouched ${d} days`;
    if (d >= 1) return `Opened ${d}d ago`;
    const h = Math.floor((Date.now() - ms) / 3_600_000);
    return h >= 1 ? `Opened ${h}h ago` : 'Opened just now';
  }
</script>

<div class="desk">
  <section class="hero">
    <div class="greet">
      <div class="date">{dateLine}</div>
      <h1 class="display">{greeting}</h1>
    </div>
    <div class="stats">
      <button class="stat" onclick={() => onGo('library')}>
        <span class="l">Projects</span><span class="v display">{fmt(all.length)}</span>
        <span class="note">{thisMonth.length ? `+${thisMonth.length} this month` : 'None yet this month'}</span>
      </button>
      <button class="stat" onclick={() => onGo('library', { statuses: [first] })}>
        <span class="l">In progress</span><span class="v display">{fmt(inProgress.length)}</span>
        <span class="note" class:warn={stale.length}>{stale.length ? `${stale.length} untouched 30d+` : 'All recently opened'}</span>
      </button>
      <button class="stat" onclick={() => onGo('library', { statuses: statuses.slice(1) })}>
        <span class="l">Completed</span><span class="v display">{pct}%</span>
        <span class="note">{fmt(completed)} {statuses.slice(1).join(' or ').toLowerCase()}</span>
      </button>
      <button class="stat" onclick={() => onGo('favorites')}>
        <span class="l">Favorites</span><span class="v display">{fmt(favorites)}</span>
        <span class="note">Portfolio picks</span>
      </button>
    </div>
  </section>

  <section>
    <div class="head">
      <h2 class="display">On the desk</h2>
      <span class="sub">{fmt(inProgress.length)} {first.toLowerCase()}, most recently opened first</span>
      <button class="link" onclick={() => onGo('library', { statuses: [first] })}>See all in Library →</button>
    </div>
    {#if inProgress.length}
      <div class="covers">
        {#each inProgress.slice(0, 20) as p (p.id)}
          <button class="card" onclick={() => onOpenProject(p.id)} title="Open {p.title} in the Library">
            <Cover {p} v={versions[p.id] ?? 0} {busy} />
            <span class="t">{p.title}</span>
            <span class="w" class:warn={isStale(p)}>{relative(p.lastOpenedAt)}</span>
          </button>
        {/each}
      </div>
    {:else}
      <p class="empty">Nothing in progress. Nice.</p>
    {/if}
  </section>

  <section class="cols">
    <div class="col wide">
      <div class="head">
        <h2 class="display small">Needs attention</h2>
        <span class="sub">{attention.length ? `${Math.min(5, attention.length)} of ${attention.length}` : ''}</span>
      </div>
      {#each attention.slice(0, 5) as a (a.p.id)}
        <div class="row">
          <span class="dot" class:warn={a.fix === 'Pick cover'}></span>
          <button class="rt" onclick={() => onOpenProject(a.p.id)}>{a.p.title}<span class="m">{a.p.monthFolder}</span></button>
          <span class="reason">{a.reason}</span>
          <button class="act" onclick={() => (a.fix === 'Pick cover' ? onPickCover(a.p) : onOpenProject(a.p.id, true))}>{a.fix}</button>
          <button class="act quiet" onclick={() => onEdit([a.p.id], { kind: 'archived', value: true })}>Archive</button>
        </div>
      {:else}
        <p class="empty">All caught up.</p>
      {/each}
    </div>

    <div class="col">
      <div class="head"><h2 class="display small">By category</h2></div>
      <div class="bar" role="img" aria-label={byCategory.map(([c, n]) => `${c} ${n}`).join(', ')}>
        {#each byCategory as [c, n]}<span style:flex={n} style:background={categoryColor(c)}></span>{/each}
      </div>
      {#each byCategory as [c, n]}
        <button class="legend" onclick={() => onGo('library', { categories: [c] })}>
          <span class="sw" style:background={categoryColor(c)}></span><span class="c">{c}</span>
          <span class="num">{fmt(n)}</span><span class="pct">{Math.round((n / all.length) * 100)}%</span>
        </button>
      {/each}

      <div class="month">
        <div class="mh">
          <span class="mn display">{String(now.getMonth() + 1).padStart(2, '0')}</span>
          <div>
            <div class="mt">{now.toLocaleDateString('en-GB', { month: 'long', year: 'numeric' })}</div>
            <div class="ms">This month · {thisMonth.length} {thisMonth.length === 1 ? 'project' : 'projects'} so far</div>
          </div>
        </div>
        <div class="mb">
          <button class="dark" onclick={onOpenMonth}>Open month folder</button>
          <button class="outline" onclick={onOpenRoot}>Library folder</button>
        </div>
      </div>
    </div>
  </section>
</div>

<style>
  .desk { padding: 56px var(--gutter) 80px; display: flex; flex-direction: column; gap: 56px; }
  .hero { display: flex; align-items: flex-end; gap: 40px; flex-wrap: wrap; }
  .greet { flex: 1 1 340px; }
  .date { font-size: 12px; letter-spacing: 0.14em; text-transform: uppercase; color: var(--text-2); }
  h1 { margin: 10px 0 0; font-size: clamp(64px, 6vw, 104px); line-height: 0.9; letter-spacing: -0.02em; }
  .stats { display: flex; flex-wrap: wrap; border-top: 2px solid var(--text); }
  .stat { width: 200px; padding: 14px 24px 0 0; display: flex; flex-direction: column; gap: 4px; border: 0; background: none; text-align: left; color: var(--text); }
  .stat:hover .v { color: #fff; }
  .l { font-size: 12px; color: var(--text-2); }
  .v { font-size: 60px; line-height: 1; font-stretch: 75%; font-weight: 600; }
  .note { font-size: 12px; color: var(--text-3); }
  .warn { color: var(--warning); }
  .head { display: flex; align-items: baseline; gap: 14px; border-bottom: 1px solid var(--line); padding-bottom: 10px; margin-bottom: 18px; }
  h2 { margin: 0; font-size: 26px; font-stretch: 75%; }
  h2.small { font-size: 22px; }
  .sub { color: var(--text-2); font-size: 14px; }
  .link { margin-left: auto; border: 0; background: none; color: var(--text); font-size: 13px; text-decoration: underline; text-decoration-color: var(--text-4); }
  .covers { display: grid; grid-template-columns: repeat(auto-fill, minmax(190px, 1fr)); gap: 36px; }
  .card { display: flex; flex-direction: column; gap: 10px; border: 0; background: none; padding: 0; text-align: left; color: var(--text); min-width: 0; }
  .card :global(.cover) { transition: transform var(--t-med) ease-out; }
  .card:hover :global(.cover) { transform: translateY(-4px); }
  .t { font-size: 13px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .w { font-size: 12px; color: var(--text-2); }
  .empty { color: var(--text-3); margin: 0; }
  .cols { display: flex; gap: 48px; flex-wrap: wrap; }
  .col { flex: 2 1 320px; display: flex; flex-direction: column; gap: 8px; }
  .col.wide { flex: 3 1 480px; gap: 0; }
  .row { display: flex; align-items: center; gap: 14px; padding: 8px 10px; margin: 0 -10px; border-radius: 6px; }
  .row:hover { background: var(--hover); }
  .row:hover .act { opacity: 1; }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--text); flex: none; }
  .dot.warn { background: var(--warning); }
  .rt { flex: 1; min-width: 0; border: 0; background: none; padding: 0; text-align: left; color: var(--text); font-weight: 500; font-size: 15px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .m { color: var(--text-3); font-size: 12px; margin-left: 8px; font-weight: 400; }
  .reason { font-size: 12px; color: var(--text-2); }
  .act { opacity: 0; transition: opacity var(--t-fast); height: 28px; padding: 0 10px; border: 1px solid var(--text); border-radius: 6px; background: none; color: var(--text); font-size: 12px; }
  .act.quiet { border-color: transparent; color: var(--text-2); }
  .bar { display: flex; height: 14px; gap: 2px; margin-bottom: 6px; }
  .legend { display: flex; align-items: center; gap: 10px; font-size: 13px; border: 0; background: none; padding: 2px 0; color: var(--text); text-align: left; }
  .sw { width: 10px; height: 10px; }
  .c { flex: 1; }
  .num { font-variant-numeric: tabular-nums; }
  .pct { width: 40px; text-align: right; color: var(--text-3); font-variant-numeric: tabular-nums; }
  .month { margin-top: 20px; padding: 20px; background: var(--text); color: var(--on-text); border-radius: 6px; display: flex; flex-direction: column; gap: 14px; }
  .mh { display: flex; align-items: baseline; gap: 12px; }
  .mn { font-size: 44px; line-height: 0.9; }
  .mt { font-weight: 600; }
  .ms { font-size: 12px; color: #55554f; }
  .mb { display: flex; gap: 8px; flex-wrap: wrap; }
  .dark { height: 38px; padding: 0 14px; border: 0; border-radius: 6px; background: var(--on-text); color: var(--text); font-size: 13px; font-weight: 600; }
  .dark:hover { background: var(--line); }
  .outline { height: 38px; padding: 0 14px; border: 1px solid var(--text-3); border-radius: 6px; background: none; color: var(--on-text); font-size: 13px; }
</style>
