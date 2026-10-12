const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { check, forbiddenStrings } = require('./check-binary-paths.cjs');

const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'lmr-path-scan-'));
try {
  const env = { CARGO_HOME: '/build/some user/.cargo' };
  const root = '/build/some user/checkout';
  const needles = forbiddenStrings(env, root, '/home/builder', 'builder');
  assert(needles.includes('/home/builder/') && needles.includes(root) && needles.includes(env.CARGO_HOME));
  assert(needles.includes('.cargo/registry/'));
  assert(!forbiddenStrings({}, '/', '/', 'root').includes('/'), 'a bare slash would match everything');

  const clean = path.join(dir, 'clean.bin');
  fs.writeFileSync(clean, Buffer.concat([Buffer.from('\x7fELF'), Buffer.from('/build/cargo/registry/src/x/parser.c\0')]));
  assert.deepEqual(check([clean], env, root, '/home/builder', 'builder'), []);

  // Binary with a C-style leak, as tree-sitter's parser.c left in the 0.2.6 candidate.
  const leaky = path.join(dir, 'leaky.bin');
  fs.writeFileSync(leaky, Buffer.concat([
    Buffer.from([0x7f, 0x45, 0x4c, 0x46, 0, 1, 2]),
    Buffer.from('/home/builder/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tree-sitter-0.25.10/src/./parser.c\0'),
  ]));
  const leaks = check([leaky], env, root, '/home/builder', 'builder');
  assert(leaks.some(leak => leak.needle === '/home/builder/'), JSON.stringify(leaks));
  assert(leaks.some(leak => leak.needle === '.cargo/registry/'));
  assert(leaks[0].context.includes('tree-sitter-0.25.10'));

  // The same leak inside an archive member is found too.
  fs.mkdirSync(path.join(dir, 'pkg'));
  fs.copyFileSync(leaky, path.join(dir, 'pkg', 'lm-resizer'));
  fs.copyFileSync(clean, path.join(dir, 'pkg', 'README'));
  const archive = path.join(dir, 'pkg.tar.gz');
  const packed = spawnSync('tar', ['-czf', archive, '-C', dir, 'pkg']);
  assert.equal(packed.status, 0, String(packed.stderr));
  const inArchive = check([archive], env, root, '/home/builder', 'builder');
  assert(inArchive.some(leak => leak.file.endsWith('pkg.tar.gz:pkg/lm-resizer')), JSON.stringify(inArchive));
  assert(!inArchive.some(leak => leak.file.endsWith(':pkg/README')));

  // Command line: exit status 1 on a leak, 0 otherwise (the real home is `os.homedir()`).
  const cli = spawnSync('node', [path.join(__dirname, 'check-binary-paths.cjs'), clean], { encoding: 'utf8' });
  assert.equal(cli.status, 0, cli.stderr);
  const hit = path.join(dir, 'hit.bin');
  fs.writeFileSync(hit, `${path.resolve(__dirname, '..')}/src/main.rs\0`);
  const failing = spawnSync('node', [path.join(__dirname, 'check-binary-paths.cjs'), hit], { encoding: 'utf8' });
  assert.equal(failing.status, 1, failing.stdout);
  console.log('Builder-path scan: C-style leak, archive member, bare slash and exit codes OK');
} finally {
  fs.rmSync(dir, { recursive: true, force: true });
}
