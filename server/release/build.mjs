// The website is self-contained. Release artifacts are hosted by GitHub and are
// never fetched or materialized as a side effect of a normal server build.
import {access, readFile} from 'node:fs/promises';
for (const file of ['index.html', 'site.css', 'site.js', 'docs/index.html', 'admin/index.html', 'admin/admin.js', 'console/index.html', 'console/callback/index.html', 'console/console.js']) {
  await access(new URL('../public/' + file, import.meta.url));
}
for (const file of ['index.html', 'docs/index.html']) {
  const html = await readFile(new URL('../public/' + file, import.meta.url), 'utf8');
  if (/\b(?:href|src)=["']\/downloads\//.test(html)) throw new Error('Public downloads must use GitHub Releases: ' + file);
}
console.log('Static site verified; SDK downloads remain on GitHub Releases.');
