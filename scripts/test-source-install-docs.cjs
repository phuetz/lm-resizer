// Guard the source install paths for both shells; no downloads or builds.
const assert = require('node:assert/strict');
const fs = require('node:fs');
for (const file of ['README.md', 'README.fr.md', 'docs/CLAUDE_CODEX.md']) {
  const doc = fs.readFileSync(file, 'utf8');
  const blocks = [...doc.matchAll(/(?:```|~~~)powershell\r?\n([\s\S]*?)(?:```|~~~)/g)].map(m => m[1]);
  const source = blocks.find(b => b.includes('cargo install'));
  assert.ok(source, `${file}: missing PowerShell source install`);
  assert.match(source, /Join-Path \$env:USERPROFILE '\.local'/, `${file}: explicit user prefix`);
  assert.match(source, /cargo install --quiet --path \. --locked --root "\$installRoot"/);
  assert.match(source, /\$env:Path = "\$installRoot\\bin;\$env:Path"/);
  assert.match(source, /lm-resizer --version/);
  assert.doesNotMatch(source, /export |\$HOME/);
  assert.match(doc, /MSVC/);
}
console.log('Source install documentation: Bash and PowerShell checked');
