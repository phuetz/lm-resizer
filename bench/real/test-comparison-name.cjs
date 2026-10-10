const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { trackedOutsideBench, inArchive } = require('./check-comparison-name.cjs');

const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'lmr-name-scan-'));
const git = (...args) => {
  const result = spawnSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@example.test', ...args], { cwd: dir });
  assert.equal(result.status, 0, String(result.stderr));
};
try {
  git('init', '-q');
  fs.mkdirSync(path.join(dir, 'bench'));
  fs.writeFileSync(path.join(dir, 'bench', 'NOTICES.md'), 'Reference tool: RTK 0.50.0\n');
  fs.writeFileSync(path.join(dir, 'README.md'), 'Compared with a pinned reference tool.\n');
  fs.writeFileSync(path.join(dir, 'blob.bin'), Buffer.from([0, 1, 2, 0x72, 0x74, 0x6b]));
  git('add', '-A');
  assert.deepEqual(trackedOutsideBench(dir), [], 'bench/ and binary files may carry the name');

  fs.writeFileSync(path.join(dir, 'CHANGELOG.md'), 'ok\nMeasured against RTK 0.50.0\n');
  git('add', 'CHANGELOG.md');
  assert.deepEqual(trackedOutsideBench(dir).map(hit => hit.split(':').slice(0, 2).join(':')), ['CHANGELOG.md:2']);
  fs.writeFileSync(path.join(dir, 'docs-rtk-notes.md'), 'x\n');
  git('add', 'docs-rtk-notes.md');
  assert(trackedOutsideBench(dir).some(hit => hit === 'docs-rtk-notes.md: file name'));

  fs.mkdirSync(path.join(dir, 'pkg'));
  fs.writeFileSync(path.join(dir, 'pkg', 'README.md'), 'Inspired by rtk.\n');
  fs.writeFileSync(path.join(dir, 'pkg', 'lm-resizer'), Buffer.from([0x7f, 0, 0x72, 0x74, 0x6b]));
  const archive = path.join(dir, 'pkg.tar.gz');
  assert.equal(spawnSync('tar', ['-czf', archive, '-C', dir, 'pkg']).status, 0);
  const hits = inArchive(archive);
  assert.equal(hits.length, 1, hits.join('\n'));
  assert(hits[0].startsWith('pkg.tar.gz:pkg/README.md:1'));

  const cli = spawnSync('node', [path.join(__dirname, 'check-comparison-name.cjs'), archive], { encoding: 'utf8' });
  assert.equal(cli.status, 1);
  console.log('Comparison-name scan: bench exemption, tracked files, file names, archive text members and exit code OK');
} finally {
  fs.rmSync(dir, { recursive: true, force: true });
}
