/** localStorage helpers that never throw (private mode, quota, disabled storage). */
export function readJson<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(key);
    return v == null ? fallback : (JSON.parse(v) as T);
  } catch {
    return fallback;
  }
}

export function writeJson(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* ignore */
  }
}

export function removeKey(key: string) {
  try { localStorage.removeItem(key); } catch { /* ignore */ }
}
