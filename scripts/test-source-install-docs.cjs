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

// A newcomer with the distribution Cargo must be able to select our MSRV.
const pin = fs.readFileSync('rust-toolchain.toml', 'utf8');
const channel = pin.match(/channel\s*=\s*"([0-9.]+)"/)?.[1];
assert.ok(channel, "numeric pinned toolchain required");
assert.match(pin, /profile\s*=\s*"minimal"/);
for (const component of ['rustfmt', 'clippy']) assert.ok(pin.includes(`"${component}"`));
for (const file of ['README.md', 'README.fr.md']) {
  const doc = fs.readFileSync(file, 'utf8');
  assert.match(doc, /https:\/\/sh\.rustup\.rs/);
  assert.match(doc, /https:\/\/static\.rust-lang\.org\/rustup\/dist\/x86_64-pc-windows-msvc\/rustup-init\.exe/);
  assert.match(doc, /Join-Path \$env:TEMP 'rustup-init\.exe'/, `${file}: rustup installer requires its original basename`);
  const installers = doc.split(/\r?\n/).filter(line => line.includes('--default-toolchain'));
  assert.ok(installers.length >= 2, `${file}: both installers must pin their toolchain`);
  for (const line of installers) {
    assert.equal(line.match(/--default-toolchain\s+([0-9.]+)/)?.[1], channel,
      `${file}: every installer must match rust-toolchain.toml`);
  }
  assert.match(doc, /\. "\$HOME\/\.cargo\/env"/);
  assert.match(doc, /git clone --branch v0\.2\.5 https:\/\/github\.com\/phuetz\/lm-resizer\.git/);
  assert.match(doc, /cd lm-resizer/);
  assert.match(doc, /ca-certificates curl git gcc g\+\+ libc6-dev/);
  assert.match(doc, /rust-toolchain\.toml/);
  assert.match(doc, /lm-resizer 0\.2\.5/);
  assert.match(doc, /\[raw: [a-f0-9]{12}\]/);
  assert.match(doc, /\.log/);
  for (const m of doc.matchAll(/(?:```|~~~)(?:bash|sh)\r?\n([\s\S]*?)(?:```|~~~)/g)) {
    if (process.platform !== 'win32') {
      const check = require('node:child_process').spawnSync('bash', ['-n'], { input: m[1], encoding: 'utf8' });
      assert.equal(check.status, 0, `${file}: invalid shell example: ${check.stderr}`);
    }
  }
}
console.log('Source install documentation: Rustup, clone, prerequisites, samples, Bash and PowerShell checked');
