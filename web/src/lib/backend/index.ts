import { HttpBackend } from './http';
import { isTauri, type Backend } from './types';

export * from './types';
export { HttpBackend } from './http';

export async function createBackend(): Promise<Backend> {
  if (isTauri()) {
    const { TauriBackend } = await import('./tauri');
    const b = new TauriBackend();
    await b.init();
    return b;
  }
  return new HttpBackend();
}

export const asServer = (b: Backend): HttpBackend | null => (b instanceof HttpBackend ? b : null);
