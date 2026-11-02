export function formatBytes(n: number | undefined | null): string {
  if (n == null || !isFinite(n)) return '—';
  if (n < 1024) return `${n} B`;
  const u = ['KB', 'MB', 'GB', 'TB'];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} ${u[i]}`;
}

export function formatDuration(ms: number | undefined | null): string {
  if (ms == null || !isFinite(ms)) return '—';
  if (ms < 1000) return `${Math.round(ms)} ms`;
  const s = ms / 1000;
  if (s < 60) return `${s < 10 ? s.toFixed(1) : Math.round(s)} s`;
  const m = Math.floor(s / 60);
  const rs = Math.round(s % 60);
  if (m < 60) return `${m}m ${rs.toString().padStart(2, '0')}s`;
  return `${Math.floor(m / 60)}h ${(m % 60).toString().padStart(2, '0')}m`;
}

export function formatUptime(secs: number): string {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (d) return `${d}d ${h}h`;
  if (h) return `${h}h ${m}m`;
  return `${m}m ${Math.floor(secs % 60)}s`;
}

const rtf = typeof Intl !== 'undefined' && 'RelativeTimeFormat' in Intl ? new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' }) : null;

/** Unix seconds → relative ("3 min ago"). */
export function formatRelative(ts: number | undefined | null): string {
  if (!ts) return '—';
  const diff = ts - Date.now() / 1000;
  const abs = Math.abs(diff);
  if (!rtf) return new Date(ts * 1000).toLocaleString();
  if (abs < 45) return 'just now';
  if (abs < 3600) return rtf.format(Math.round(diff / 60), 'minute');
  if (abs < 86400) return rtf.format(Math.round(diff / 3600), 'hour');
  if (abs < 86400 * 7) return rtf.format(Math.round(diff / 86400), 'day');
  return new Date(ts * 1000).toLocaleDateString();
}

export function formatDateTime(ts: number | undefined | null): string {
  if (!ts) return '—';
  return new Date(ts * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
}

const ACRONYMS: Record<string, string> = {
  cpu: 'CPU', gpu: 'GPU', os: 'OS', ip: 'IP', ipv4: 'IPv4', ipv6: 'IPv6', url: 'URL', uri: 'URI', id: 'ID', dns: 'DNS', tls: 'TLS',
  ssl: 'SSL', http: 'HTTP', https: 'HTTPS', mime: 'MIME', pdf: 'PDF', css: 'CSS', html: 'HTML', json: 'JSON', csv: 'CSV', mhz: 'MHz',
  ghz: 'GHz', ram: 'RAM', ssd: 'SSD', hdd: 'HDD', mac: 'MAC', utc: 'UTC', dpi: 'DPI', fps: 'FPS', exif: 'EXIF', gps: 'GPS', rgb: 'RGB',
  hsl: 'HSL', hex: 'Hex', sha256: 'SHA-256', sha1: 'SHA-1', md5: 'MD5', mcp: 'MCP', api: 'API', mb: 'MB', kb: 'KB', gb: 'GB', ms: 'ms',
  wh: 'Wh', ttl: 'TTL', asn: 'ASN', cidr: 'CIDR', utf8: 'UTF-8', aa: 'AA', aaa: 'AAA', pid: 'PID', tz: 'TZ',
};

export function humanizeKey(k: string): string {
  const words = k.replace(/[_-]+/g, ' ').replace(/([a-z])([A-Z])/g, '$1 $2').trim().split(/\s+/);
  const out = words.map((w) => ACRONYMS[w.toLowerCase()] ?? w.toLowerCase());
  if (out.length && out[0] === out[0].toLowerCase()) out[0] = out[0].charAt(0).toUpperCase() + out[0].slice(1);
  return out.join(' ');
}

export function plural(n: number, one: string, many = `${one}s`) {
  return `${n} ${n === 1 ? one : many}`;
}
