export const basename = (p: string) => p.split(/[\\/]/).pop() ?? p;

export function fmtBytes(b: number) {
  if (b >= 1e9) return `${(b / 1e9).toFixed(1)} GB`;
  if (b >= 1e6) return `${Math.round(b / 1e6)} MB`;
  return `${Math.max(1, Math.round(b / 1e3))} KB`;
}

export function fmtDur(sec: number) {
  if (!isFinite(sec)) return '—';
  if (sec < 60) return `${Math.max(1, Math.round(sec))} s`;
  const m = Math.round(sec / 60);
  if (m < 60) return `${m} min`;
  return `${Math.floor(m / 60)} h ${m % 60} min`;
}

export function fmtClock(sec: number) {
  const s = Math.round(sec);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

export const fmtFps = (f: number) => (Math.abs(f - Math.round(f)) < 0.01 ? String(Math.round(f)) : f.toFixed(2));