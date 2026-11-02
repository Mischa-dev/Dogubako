// Copies the app screenshots kept in the repository (docs/screenshots) into the
// site, so the docs always show the same pictures as the README.
//   npm run gen:screenshots
import { copyFileSync, mkdirSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const from = join(here, '..', '..', 'docs', 'screenshots');
const to = join(here, '..', 'src', 'assets', 'screenshots');
mkdirSync(to, { recursive: true });
let n = 0;
for (const name of readdirSync(from)) {
  if (!/\.(png|jpe?g|webp)$/i.test(name)) continue;
  copyFileSync(join(from, name), join(to, name));
  n++;
}
console.log(`Copied ${n} screenshots from docs/screenshots.`);
