# Atelier — Spec

Atelier is a personal desktop app for one book cover designer to browse and track
their own cover and typography projects. It replaces **Bookrack** (v0.1.3) as a
separate app with its own repository, identity and update channel.

When a design choice is unclear, favour what makes it fastest for one designer to
find, review and update their own covers.

**Design reference:** the Atelier design canvas (private Claude Design artifact,
`https://claude.ai/artifact/UR1z85BW9byhY3atSMVcgU`). Where this spec and the
canvas disagree, this spec wins; raise the difference rather than guessing.

---

## 1. Decisions

| Topic | Decision |
|---|---|
| Stack | **Tauri 2** shell, **Rust** core, **SQLite** (`rusqlite`, WAL), **Svelte 5** frontend |
| Identity | Product name `Atelier`, identifier `com.mani.atelier` |
| Repository | Public GitHub repo `Mukashif/atelier`; code and releases together |
| Data location | Per-user app data dir (`%APPDATA%\com.mani.atelier`) |
| Thumbnails | Stored in the app cache dir, **never** in project folders |
| Users / machines | One user, one PC, library on a local drive |
| Library size | 500–2,000 projects (must stay smooth at 10,000) |
| Target display | 27" 2560×1440 at 100% Windows scaling; must also work at 1920×1080 and 125% scaling |
| Theme | Dark only, monochrome (see §8) |
| First screen | Desk (user can switch to Library in Preferences) |
| Bookrack | Left untouched. Atelier only **reads** Bookrack data, to import it |

**Dropped from Bookrack:** sidebar, dashboard donut chart, project notes, Custom
Order / drag-to-reorder of projects, fireflies and other ambient effects.

**The app never modifies, moves or deletes anything inside the library root.**

---

## 2. Architecture

```
src-tauri/            Tauri shell: window, commands, image protocol, updater
crates/atelier-core/  UI-independent Rust: scan, db, thumbnails, import, query
src/                  Svelte 5 frontend (thin: renders what the core returns)
src/lib/tokens.css    All design tokens (§8); components use tokens only
```

- The **core crate** owns all logic and has its own tests. Tauri commands are thin
  wrappers around it.
- **Filtering, search and sorting run in Rust/SQLite.** The frontend asks for the
  rows it needs; it never holds or sorts the full library.
- **Images** are served by a custom `thumb://` URI scheme handled in Rust. It serves
  only files from the thumbnail cache and image files under the current library
  root. The webview has no general filesystem access.
- No image bytes pass through IPC.

---

## 3. Data model

### 3.1 Database (`atelier.db` in the app data dir)

```sql
settings(key TEXT PRIMARY KEY, value TEXT)            -- JSON values

projects(
  id             INTEGER PRIMARY KEY,
  rel_path       TEXT NOT NULL UNIQUE,   -- relative to root, '/' separators
  folder_name    TEXT NOT NULL,
  month_folder   TEXT NOT NULL,
  year INTEGER, month INTEGER,           -- parsed from month folder; NULL = undated
  cover_rel      TEXT,                   -- auto-detected cover, relative to project
  cover_mtime    INTEGER,
  cover_kind     TEXT,                   -- 'image' | 'unsupported' | NULL (none)
  present        INTEGER NOT NULL DEFAULT 1,  -- 0 = not found on last scan (orphan)
  -- user edits
  title_override TEXT,
  category       TEXT,
  status         TEXT,
  manual_cover   TEXT,                   -- absolute path to user-chosen image
  is_favorite    INTEGER NOT NULL DEFAULT 0,
  favorited_at   INTEGER,                -- ms; Favorites sorts by this
  is_archived    INTEGER NOT NULL DEFAULT 0,
  archived_at    INTEGER,                -- ms
  last_opened_at INTEGER                 -- ms
)

project_tags(project_id INTEGER, tag TEXT, PRIMARY KEY(project_id, tag))
projects_fts   -- FTS5 over title, folder_name, tags
```

- Identity is `rel_path`. Projects not found on a scan get `present = 0`, never
  deleted, so their edits survive a rename until re-linked or forgotten (§6.8).
- Writes are small row updates; no whole-file rewrites. Every edit is saved
  immediately; the UI updates first.

### 3.2 Settings

| Key | Default |
|---|---|
| `rootFolder` | `null` |
| `categories` | `["Book Covers", "Typography", "Misc"]` |
| `statuses` | `["In-Progress", "Done", "Delivered"]` (order matters) |
| `defaultCategory` | first entry of `categories`, or `Uncategorised` if empty |
| `openOnLaunch` | `"desk"` (`"desk"` \| `"library"`) |
| `groupByMonth` | `true` |
| `reduceMotion` | `false` (also honour the OS setting) |
| `checkUpdatesOnLaunch` | `true` |
| `lastExportAt` | `null` |

New projects get `defaultCategory` and the **first** status in `statuses`.

---

## 4. Scanning

Layout is two levels: `<root>/<month folder>/<project folder>`.

- Every directory directly under root is a month folder; every directory inside a
  month folder is a project. Files at those levels are ignored.
- **Month/year parsing** (case-insensitive):
  1. Full English month name **and** a whole-word year `19xx`/`20xx` → that month/year.
  2. Else `YYYY` then space, `-`, `_` or `/`, then a 1–2 digit month (`2025-09`).
  3. Else undated.
- **Cover detection:** first `.jpg .jpeg .png .webp` (case-insensitive) in the
  project folder in listing order; if none at this level, descend into
  subdirectories in order, stopping at the first that yields one; max depth 3.
  Ignore `.bookrack_thumb.webp`. If no supported image exists but a likely cover
  file in another format does (`.psd .tif .tiff .ai .pdf .heic`), record it with
  `cover_kind = 'unsupported'` so the UI can say so (§6.4).
- Month folders are scanned in parallel (rayon). Unreadable folders are skipped and
  logged.

### 4.1 Startup

1. Open the database. No root folder → first launch (§6.9).
2. Render the opening screen **from the database immediately**.
3. Rescan in the background; apply the diff (new / changed / missing) in one
   transaction and push one update event.
4. Generate missing or stale thumbnails in the background (§5).
5. Check for updates in the background, silently (§10).

If the root folder itself is missing, show the "library unavailable" state (§6.9);
keep showing cached data read-only.

---

## 5. Thumbnails

- Cache: `<app cache>/thumbs/<project id>.webp`.
- Fit within **300 × 450** (generate **600 × 900** as well if covers display larger
  than 300 px wide at the user's scaling), keep aspect ratio, WebP quality 80.
- Regenerate when missing or when the source mtime is newer.
- Generated in parallel; results reach the UI in batches, never one event per project.
- Source: manual cover if set, else the auto-detected cover.
- **On first import from Bookrack**, copy existing `.bookrack_thumb.webp` files into
  the cache instead of regenerating. Do not delete them.
- Display order: thumbnail → original (only if the webview can decode it) →
  placeholder (§6.4). Never retry an image that failed to load.

---

## 6. Screens

Frameless window, custom title bar, default size fills the work area on a 1440p
display, minimum 1280×800.

### 6.1 Top bar (every screen)

- Left: `ATELIER` wordmark.
- Tabs: **Desk · Library (count) · Favorites (count) · Archive**. Active tab: cream
  text with a 2 px cream underline. No sidebar.
- Right: search (`Ctrl K` / `Ctrl F`), Rescan, Settings, window controls.
- An **"Update ready · x.y.z"** chip appears here when an update is found (§10).
- Search is debounced ~150 ms, matches title, folder name and tags (FTS), and
  switches to the Library.

### 6.2 Desk (default first screen)

- Date line and large greeting ("Good evening.").
- Stat row: **Projects** (+N this month), **In progress** (N untouched 30d+),
  **Completed** (% and count of Done + Delivered), **Favorites**. Each opens the
  Library filtered accordingly. All numbers computed, never hardcoded.
- **On the desk:** In-progress projects as large covers, most recently opened first;
  projects untouched 30+ days show that in the warning colour.
- **Needs attention** (max 5, archived excluded): no cover, or In-progress and not
  opened for 30+ days. Hover actions: Pick cover / Open, Archive.
- **By category:** one stacked bar plus a legend with counts and percentages.
- **This month** block (cream on dark): month number and name, project count,
  **Open month folder** (creates `<root>/<Month YYYY>` if missing, platform path
  join) and **Library folder**.

### 6.3 Library

- **Toolbar:** filter pills **Category ▾ · Status ▾ · Tags ▾** (§6.10), result
  count, Sort (**Newest month first** / **Name**), **Select** (enters select mode, §6.5).
- **Month rows** (when `groupByMonth`): a left label column (big month number,
  month name, count, Open folder) that stays in view while scrolling through that
  month, and a grid of covers that **wraps**, so every cover in the month is visible.
  Newest month first; Undated last. With grouping off, one continuous grid.
- **Covers:** min width ~150 px, 2:3, title below, status dot + label. Hover lifts
  the cover (pure CSS).
- **Details open under the clicked cover's row**, never on top of other covers:
  rows after it move down; a notch on the panel points at the selected cover; the
  selected cover gets a cream outline; the view scrolls just enough to show the
  panel. Clicking the cover again or ✕ closes it.
  - Panel: larger cover, title, `<month>/<folder>`, Open folder (primary), Change
    cover, Rename, Favourite, Archive, Status (segmented), Category (select), Tags
    (chips + add field).
- **Keyboard:** ← → move one cover, ↑ ↓ move one row (panel follows), Enter opens the
  folder, F2 renames, F toggles favourite, Del archives, Esc closes the panel.
- The column count is computed from the available width so the panel always lands
  under the correct row. Virtualise by row; the details panel is one extra row.
- Opening a project's folder sets `last_opened_at`.
- Scanning indicator (§6.7) when a scan is running.

### 6.4 Cover placeholders

- **No cover yet:** dashed outline, image icon, small warning dot; hover shows
  **Pick cover**.
- **Can't be previewed:** file name (e.g. `cover_final.psd`), "This file type can't
  be previewed", hover shows **Choose another**.
- **Thumbnail being made:** dark tile with a gentle pulse (off under reduced motion).

### 6.5 Select mode (bulk edit)

- Entered from **Select** in the Library toolbar. Covers show checkboxes; selected
  ones get a cream outline, unselected ones dim slightly.
- Click toggles, **Shift-click** selects a range within a month, **Ctrl A** selects
  all visible, each month's label column has **Select all N / Deselect all**.
- The toolbar becomes a pinned **selection bar**: count, Clear, **Set status ▾**,
  **Set category ▾**, **Add tag ▾** (type a tag or pick from most used), Favourite,
  Archive, hint text, and **Done** to leave select mode.
- Every bulk change shows an Undo toast ("Set 5 projects to Done · Undo").

### 6.6 Right-click menu and renaming

- Right-click a cover: **Open folder** (Enter), **Rename** (F2), **Change cover…**,
  **Status ▸**, **Category ▸** (submenus tick the current value), **Add to /
  Remove from favourites** (F), **Archive / Unarchive** (Del). Opens to the left
  near the right edge.
- **Rename in place:** title becomes a text field with a cream outline and the hint
  "Enter to save · Esc to cancel". Blank or unchanged is ignored.

### 6.7 Scanning and background work

- A 2 px cream progress line under the top bar while scanning.
- A quiet status in the toolbar: "Scanning folders 412 / 1,284 · Making thumbnails
  38 left · you can keep working". Never blocks the UI.

### 6.8 Settings

One scrolling page; a section list on the left stays in view; each section has its
explanation on the left and controls on the right.

- **Library folder:** path, project/month counts, last scan time, Open in Explorer,
  Rescan now, Change folder…; **orphaned projects** box listing projects whose
  folders were not found, with a suggested match (same folder name or month) →
  **Re-link**, or **Choose folder…**, or **Forget**.
- **Categories:** list with colour, count, hover Rename / Delete; add field; "New
  projects start in" selector. Rename updates every project and toasts "Updated N
  projects". Delete only removes it from the list.
- **Statuses:** same, plus drag handles to reorder; the first is given to new projects.
- **Preferences:** Open on launch (Desk / Library), Group library by month, Reduce
  motion, Check for updates on launch.
- **Backup & data:** Export backup (`.atelier`, shows last export), Restore backup
  (`.atelier` or Bookrack `.bookrack`; replaces current edits), Import from Bookrack
  again, data folder path + Open, **Reset library data** (danger; §6.11).
- **Updates:** current version, status ("You're up to date · checked …"), Check now.

### 6.9 First run, empty and error states

- **Bookrack found:** "Move in." screen — numbered steps: 01 Bookrack data found
  (location; link to import a `.bookrack` backup instead), 02 Library folder
  (Change), 03 What will come across (counts), then **Import N projects** or "Start
  fresh". Lists what comes across and what's left behind (custom sort order,
  fireflies setting).
- **No Bookrack, no library:** "Choose your library." with expected layout,
  **Choose folder…**, "Restore a backup instead".
- **Library folder missing:** warning label, crossed-out path, "Your edits are
  safe", **Locate folder…**, **Try again**.
- **Search with no results**, **filters hide everything** (Clear all filters),
  **Favorites empty** (how to add one), **Archive empty** (what archiving does).

### 6.10 Filter menus

- Open from each pill. Checkbox rows with colour swatch (categories), status dot or
  plain (tags) and counts; hover a row for **Only**; Clear at the bottom. Tags menu
  has a search field. Within a menu, options combine with OR; across menus, AND.
- A pill with an active filter turns cream and shows its count ("Category · 1").

### 6.11 Components

- **Toasts:** bottom centre, max 3 stacked. Success (cream, teal tick) and info
  with Undo fade after 5 s; errors (dark, red edge, explanation, fix buttons) stay
  until dismissed; progress toasts show a bar.
- **Update dialog:** version change, what's new, install time note, **Later** /
  **Install and restart**. Opened from the header chip only; never pops up by itself.
- **Confirm dialog** (only for irreversible actions): says exactly what is removed
  and what is kept; Reset includes "Export a backup first" (on by default); the safe
  button has focus; Esc cancels.

### 6.12 Favorites and Archive

- **Favorites:** one wall (no month grouping), larger covers, newest favourited
  first (or Name), filled star on each.
- **Archive:** banner explaining archived projects are hidden from Library and Desk
  and their folders are untouched; month rows like the Library; covers **greyscale
  and faded until hover**, then full colour with **Unarchive** (Undo toast).
- Archived projects never appear in the Library, Desk, search results or counts
  except here.

### 6.13 Keyboard shortcuts

| Keys | Action |
|---|---|
| `Ctrl K`, `Ctrl F` | Focus search |
| `Ctrl ,` | Settings |
| `← → ↑ ↓` | Move between covers |
| `Enter` | Open folder |
| `F2` | Rename |
| `F` | Toggle favourite |
| `Del` | Archive |
| `Ctrl A` | Select all (select mode) |
| `Esc` | Close menu/panel, clear search and selection, leave select mode |

Shortcuts other than Esc are ignored while typing. Mouse back/forward move through
view history.

---

## 7. Importing from Bookrack

On first run (and from Settings):

1. Look for Bookrack's `settings.json` and `projects-meta.json` in the Bookrack
   install's `data/` folder (resource dir) and `%APPDATA%\com.mani.bookrack`. If none
   are found, offer **Restore a backup** (`.bookrack`) instead.
2. Read Bookrack settings (`rootFolder`, `categories`, `statuses`). Ignore
   `disableFireflies`.
3. Read project meta (keyed by absolute `folderPath`), convert each to a path
   relative to the root; entries outside the root are reported, not imported.
4. Map `titleOverride`, `category`, `status`, `tags`, `manualThumbnailPath`,
   `isFavorite`, `isArchived`, `lastOpenedAt`. Drop `notes` and `sortOrder` (kept in
   the raw copy, step 6).
5. Scan; imported entries with no matching folder become orphans.
6. Save the raw source files to `<app data>/imports/bookrack-<timestamp>/`.
7. Copy existing `.bookrack_thumb.webp` files into the thumbnail cache.
8. Show a summary: imported, orphans, skipped.

Import never writes to Bookrack's files. Running it again asks before replacing
Atelier's edits.

**Bookrack backup** (`.bookrack` / `.json`): `{ "version": 1, "exportedAt",
"settings", "projectMeta": [...] }`; valid only if `version === 1` and
`projectMeta` is an array.

**Atelier backup** (`.atelier`): `{ "version": 1, "app": "atelier", "exportedAt",
"settings", "projects": [...] }` keyed by `rel_path`. Restore replaces settings and
edits, then rescans.

---

## 8. Visual design

All values live in `src/lib/tokens.css` as CSS custom properties.

### 8.1 Colour

| Token | Value | Use |
|---|---|---|
| `--bg` | `#121212` | App background |
| `--panel` | `#1A1A1A` | Inputs, details panel, cards |
| `--hover` | `#1C1C1A` / `#232321` | Row / icon hover |
| `--line` | `#2A2A28` | Dividers |
| `--border` | `#3A3A37` | Control borders |
| `--text` | `#E5DFD1` | Main text **and the accent** (selection, active tab, primary buttons with `--bg` text, links with subtle underline) |
| `--text-2` | `#A3A39C` | Secondary text |
| `--text-3` | `#8E8E87` | Muted text, counts |
| `--status-progress` | `#E5DFD1` | In-progress dot |
| `--status-done` | `#A3A39C` | Done dot |
| `--status-delivered` | `#5E5E58` | Delivered dot |
| `--warning` | `#FF8A4C` | No cover, untouched 30d+, missing folders |
| `--success` | `#4FC1B0` | Up to date, all linked, success ticks |
| `--danger` | `#FF6B57` | Reset and other irreversible actions only |

- The interface is **monochrome**: the only colours are warnings, success, danger
  and category colours, so covers are the only colourful thing on screen.
- **Category colours** are derived from the name: 32-bit hash of the lower-cased
  name (`h = h*31 + charCode`), `hue = |h| % 360`, colour
  `oklch(0.74 0.12 hue)`. Same lightness and chroma for every category.
- Status is never shown by colour alone; a label always accompanies the dot.
- Contrast: `--text` ≈ 16:1, `--text-2` ≈ 7:1, `--text-3` ≈ 5.5:1 on `--bg`.

### 8.2 Type

- **Archivo** (variable width; `font-stretch` 62–75%, weights 600–800) for the
  wordmark, headings, month numbers and big figures.
- **Inter** for UI text; base size 15 px.
- **JetBrains Mono** for folder paths, counts in caps and keyboard hints.
- All fonts bundled locally (no network).

### 8.3 Shape and motion

- Radius: covers 2–3 px, controls 6–8 px, dialogs 14 px, pills fully rounded.
- Covers have a 1 px light edge (`rgba(255,255,255,0.07)`) so dark covers stay
  visible on the black background.
- Motion 160–220 ms ease-out (hover lift, panel open, menus). Hover effects are
  pure CSS so scrolling under a still cursor stays smooth. Reduced motion turns
  off lifts, pulses and panel animations.

---

## 9. Performance targets

- Opening screen visible from the database in **< 300 ms** after launch.
- Background rescan of 2,000 projects in **< 2 s** on a local SSD.
- Library scrolls at the display's refresh rate with 10,000 projects.
- Opening the details panel or moving with arrow keys responds within one frame.
- Search results update within one frame of the debounce firing.

---

## 10. Releases and updates

- Releases published to the public `Mukashif/atelier` repo.
- Tauri updater with a **new Atelier signing key** (private key outside the repo;
  public key in `tauri.conf.json`).
- Endpoint:
  `https://github.com/Mukashif/atelier/releases/latest/download/latest.json`.
  No token in the app.
- Windows NSIS installer, per-user, passive install mode, restart after update.
- Launch check is silent; when an update is found, show the header chip (§6.1) and
  the dialog on click (§6.11). The Settings check reports errors.
- Release script bumps versions, builds, signs, creates the GitHub release with the
  installer and `latest.json`. Supports a dry run.

---

## 11. Out of scope

Cloud sync, accounts, multiple roots, multiple machines, editing project files,
light theme, mobile, app icon (designed later, before the first release).
