const fs = require('node:fs');
const en = fs.readFileSync('README.md', 'utf8');
const fr = fs.readFileSync('README.fr.md', 'utf8');
const shared = ['lm-resizer err|test|summary --', 'lm-resizer gain --json', 'docs/CLI-REFERENCE.md'];
for (const item of shared) {
  if (!en.includes(item) || !fr.includes(item)) {
    console.error(`README parity missing: ${item}`);
    process.exitCode = 1;
  }
}
if (!process.exitCode) console.log('README CLI parity: OK');
