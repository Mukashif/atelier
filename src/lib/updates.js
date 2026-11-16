// Update checks (SPEC §10). The launch check is silent; Settings reports errors.
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

/** Returns the available update, or null. Throws only when `loud`. */
export async function findUpdate({ loud = false } = {}) {
  try {
    const update = await check();
    return update?.available === false ? null : update;
  } catch (e) {
    if (loud) throw e;
    console.info('[atelier] update check skipped:', e);
    return null;
  }
}

/** Download, install (Windows: passive installer) and restart. */
export async function installAndRestart(update, onProgress = () => {}) {
  let total = 0;
  let done = 0;
  await update.downloadAndInstall((ev) => {
    if (ev.event === 'Started') total = ev.data.contentLength ?? 0;
    if (ev.event === 'Progress') done += ev.data.chunkLength;
    onProgress(total ? Math.round((done / total) * 100) : null);
  });
  await relaunch();
}
