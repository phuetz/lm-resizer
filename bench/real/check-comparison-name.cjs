// The comparison tool is a measurement reference: its name may appear under bench/ only, never in
// the product's tracked files or in the shipped archive (README, CHANGELOG, notices, docs).
//
//   node bench/real/check-comparison-name.cjs                  tracked text files outside bench/
//   node bench/real/check-comparison-name.cjs <archive.tar.gz>...  text members of shipped archives
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const NAME = /rtk/i;

function isText(bytes) {
  return !bytes.subarray(0, 8000).includes(0);
}

function scanText(label, bytes, hits) {
  if (!isText(bytes)) return;
  bytes.toString('utf8').split('\n').forEach((line, index) => {
    if (NAME.test(line)) hits.push(`${label}:${index + 1}: ${line.trim().slice(0, 120)}`);
  });
}

function trackedOutsideBench(root) {
  const listing = spawnSync('git', ['-c', 'core.fsmonitor=false', 'ls-files', '-z'], { cwd: root, maxBuffer: 64 << 20 });
  if (listing.status !== 0) throw new Error(`git ls-files: ${listing.stderr}`);
  const hits = [];
  for (const name of listing.stdout.toString('utf8').split('\0').filter(Boolean)) {
    if (name.startsWith('bench/')) continue;
    if (NAME.test(name)) hits.push(`${name}: file name`);
    const file = path.join(root, name);
    if (fs.existsSync(file) && fs.statSync(file).isFile()) scanText(name, fs.readFileSync(file), hits);
  }
  return hits;
}

function inArchive(archive) {
  const list = spawnSync('tar', ['-tf', archive], { encoding: 'utf8', maxBuffer: 64 << 20 });
  if (list.status !== 0) throw new Error(`tar -tf ${archive}: ${list.stderr}`);
  const hits = [];
  for (const name of list.stdout.split('\n').filter(entry => entry && !entry.endsWith('/'))) {
    if (NAME.test(name)) hits.push(`${path.basename(archive)}:${name}: file name`);
    const body = spawnSync('tar', ['-xOf', archive, name], { maxBuffer: 1 << 30 });
    if (body.status !== 0) throw new Error(`tar -xOf ${archive} ${name}: ${body.stderr}`);
    scanText(`${path.basename(archive)}:${name}`, body.stdout, hits);
  }
  return hits;
}

module.exports = { trackedOutsideBench, inArchive };

if (require.main === module) {
  const archives = process.argv.slice(2);
  const hits = archives.length > 0
    ? archives.flatMap(inArchive)
    : trackedOutsideBench(path.resolve(__dirname, '..', '..'));
  for (const hit of hits) console.error(hit);
  if (hits.length > 0) {
    console.error(`${hits.length} mention(s) of the comparison tool outside bench/`);
    process.exit(1);
  }
  console.log(archives.length > 0
    ? `No comparison tool name in ${archives.length} shipped archive(s)`
    : 'No comparison tool name outside bench/');
}
