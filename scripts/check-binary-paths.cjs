// Fails when a shipped file (binary, archive member or text) still embeds a path of the
// machine that built it. `--remap-path-prefix` only covers Rust: C sources compiled by
// build scripts (tree-sitter, SQLite, oniguruma, ring) bake their own `__FILE__` paths.
//
//   node scripts/check-binary-paths.cjs <file|archive.tar.gz|archive.zip>...
//
// A match on any of these is a leak: the builder's home directory, the checkout,
// CARGO_HOME, RUSTUP_HOME, a `/home/<user>/` or `/Users/<user>/` of the builder, or an
// unremapped `.cargo/registry/` (the remapped form is `/build/cargo/registry/`, which is fine).
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

function forbiddenStrings(env, root, home, user) {
  const list = new Set();
  for (const value of [
    home,
    root,
    env.CARGO_HOME,
    env.RUSTUP_HOME,
    env.CARGO_TARGET_DIR,
    env.TMPDIR && env.TMPDIR !== '/tmp' ? env.TMPDIR : undefined,
  ]) {
    // `/` or a one-segment path such as `/root` would match any binary: ignore them.
    if (value && value.length > 6) list.add(value);
  }
  if (user && user !== 'root') {
    list.add(`/home/${user}/`);
    list.add(`/Users/${user}/`);
  }
  list.add('.cargo/registry/');
  return [...list];
}

function scan(name, bytes, needles) {
  const found = [];
  for (const needle of needles) {
    const at = bytes.indexOf(needle);
    if (at < 0) continue;
    // Show the whole printable run around the first hit, to point at the source.
    let start = at;
    let end = at + needle.length;
    const printable = byte => byte >= 0x20 && byte < 0x7f;
    while (start > 0 && printable(bytes[start - 1]) && at - start < 80) start -= 1;
    while (end < bytes.length && printable(bytes[end]) && end - at < 160) end += 1;
    found.push({ file: name, needle, context: bytes.subarray(start, end).toString('latin1') });
  }
  return found;
}

function members(file) {
  if (/\.(tar\.gz|tgz|tar)$/.test(file)) {
    const list = spawnSync('tar', ['-tf', file], { encoding: 'utf8', maxBuffer: 64 << 20 });
    if (list.status !== 0) throw new Error(`tar -tf ${file}: ${list.stderr}`);
    return list.stdout.split('\n').filter(name => name && !name.endsWith('/')).map(name => {
      const body = spawnSync('tar', ['-xOf', file, name], { maxBuffer: 1 << 30 });
      if (body.status !== 0) throw new Error(`tar -xOf ${file} ${name}: ${body.stderr}`);
      return { name: `${path.basename(file)}:${name}`, bytes: body.stdout };
    });
  }
  if (/\.zip$/.test(file)) {
    const list = spawnSync('unzip', ['-Z1', file], { encoding: 'utf8', maxBuffer: 64 << 20 });
    if (list.status !== 0) throw new Error(`unzip -Z1 ${file}: ${list.stderr}`);
    return list.stdout.split('\n').filter(name => name && !name.endsWith('/')).map(name => {
      const body = spawnSync('unzip', ['-p', file, name], { maxBuffer: 1 << 30 });
      if (body.status !== 0) throw new Error(`unzip -p ${file} ${name}: ${body.stderr}`);
      return { name: `${path.basename(file)}:${name}`, bytes: body.stdout };
    });
  }
  return [{ name: file, bytes: fs.readFileSync(file) }];
}

function check(files, env = process.env, root = path.resolve(__dirname, '..'), home = os.homedir(), user = os.userInfo().username) {
  const needles = forbiddenStrings(env, root, home, user);
  const leaks = [];
  for (const file of files) {
    for (const member of members(file)) leaks.push(...scan(member.name, member.bytes, needles));
  }
  return leaks;
}

module.exports = { check, forbiddenStrings };

if (require.main === module) {
  const files = process.argv.slice(2);
  if (files.length === 0) {
    console.error('usage: node scripts/check-binary-paths.cjs <file|archive>...');
    process.exit(2);
  }
  const leaks = check(files);
  for (const leak of leaks) {
    console.error(`${leak.file}: contains ${JSON.stringify(leak.needle)}: ...${JSON.stringify(leak.context)}`);
  }
  if (leaks.length > 0) {
    console.error(`${leaks.length} builder path(s) embedded in shipped files`);
    process.exit(1);
  }
  console.log(`No builder path in ${files.length} shipped file(s)`);
}
