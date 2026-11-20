/** Longest content a version-40 QR code holds per error-correction level: digits only, uppercase alphanumeric, UTF-8 bytes. */
const CAPACITY: Record<string, { name: string; digits: number; alnum: number; bytes: number }> = {
  L: { name: 'Low', digits: 7089, alnum: 4296, bytes: 2953 },
  M: { name: 'Medium', digits: 5596, alnum: 3391, bytes: 2331 },
  Q: { name: 'Quartile', digits: 3993, alnum: 2420, bytes: 1663 },
  H: { name: 'High', digits: 3057, alnum: 1852, bytes: 1273 },
};

/** Error for content too long to fit a QR code at `ec`, or null when it fits. Mixed content is counted in bytes, a safe upper bound. */
export function qrCapacityError(text: string, ec: string, logo = false): string | null {
  if (!text) return null;
  const c = CAPACITY[ec] ?? CAPACITY.M;
  let used: number, limit: number, unit: string;
  if (/^[0-9]+$/.test(text)) [used, limit, unit] = [text.length, c.digits, 'digits'];
  else if (/^[0-9A-Z $%*+\-./:]+$/.test(text)) [used, limit, unit] = [text.length, c.alnum, 'characters'];
  else [used, limit, unit] = [new TextEncoder().encode(text).length, c.bytes, 'bytes'];
  if (used <= limit) return null;
  const fix = logo ? 'shorten it or remove the logo (a logo needs High error correction)' : 'shorten it or choose a lower error correction';
  return `Too long for a QR code at ${c.name} error correction: ${used.toLocaleString()} of ${limit.toLocaleString()} ${unit}; ${fix}`;
}
