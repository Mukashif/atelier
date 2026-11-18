// Library cover size: a step on a fixed scale, remembered on this machine.
// Each step is the minimum cover width in px; the grid fits as many columns
// as it can at that width. Favorites runs a notch larger (FAVORITES_SCALE).

export const STEPS = [120, 150, 190, 240, 300, 380];
export const DEFAULT_STEP = 1;
export const FAVORITES_SCALE = 1.4;
const KEY = 'atelier.coverSize';

function load() {
  try {
    const raw = localStorage.getItem(KEY);
    const n = raw == null ? NaN : Number(raw);
    if (Number.isInteger(n) && n >= 0 && n < STEPS.length) return n;
  } catch {}
  return DEFAULT_STEP;
}

export const coverSize = $state({ step: load() });

export function setStep(n) {
  const step = Math.max(0, Math.min(STEPS.length - 1, n));
  if (step === coverSize.step) return false;
  coverSize.step = step;
  try { localStorage.setItem(KEY, String(step)); } catch {}
  return true;
}
