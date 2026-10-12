// Build shipped artifacts without embedding the builder's home/checkout paths.
const path = require('node:path');
const os = require('node:os');
const { spawnSync } = require('node:child_process');

function buildEnvironment(env, root, home) {
  // Cargo gives encoded flags precedence over RUSTFLAGS. Preserve that rule,
  // and encode each new flag separately so paths containing spaces stay intact.
  const flags = env.CARGO_ENCODED_RUSTFLAGS !== undefined
    ? env.CARGO_ENCODED_RUSTFLAGS.split('\x1f').filter(Boolean)
    : (env.RUSTFLAGS || '').split(/\s+/).filter(Boolean);
  const prefixes = [
    [home, '/build-home'],
    [root, '/build/lm-resizer'],
    [env.CARGO_HOME || path.join(home, '.cargo'), '/build/cargo'],
    [env.RUSTUP_HOME || path.join(home, '.rustup'), '/build/rustup'],
  ];
  for (const [from, to] of prefixes) {
    flags.push(`--remap-path-prefix=${from}=${to}`);
    // Rust dependency paths can use forward slashes on Windows too.
    if (from.includes('\\')) flags.push(`--remap-path-prefix=${from.replaceAll('\\', '/')}=${to}`);
  }
  const result = { ...env, CARGO_ENCODED_RUSTFLAGS: flags.join('\x1f') };
  if (process.platform !== 'win32') {
    // `--remap-path-prefix` only reaches rustc. C sources compiled by build scripts (tree-sitter,
    // SQLite, oniguruma, ring) bake `__FILE__` paths of the registry checkout into the binary:
    // 0.2.6's first candidate carried seven `/home/<user>/.cargo/registry/...` paths. The same
    // prefixes go to the C compiler, last match first, so the cargo and rustup homes keep winning
    // over the enclosing home directory. `cc` splits CFLAGS like a shell: quote paths with spaces.
    const quote = flag => (/\s/.test(flag) ? `"${flag}"` : flag);
    const cFlags = prefixes
      .map(([from, to]) => `-ffile-prefix-map=${from}=${to}`)
      .map(quote);
    for (const name of ['CFLAGS', 'CXXFLAGS']) {
      result[name] = [env[name], ...cFlags].filter(Boolean).join(' ');
    }
  }
  return result;
}

function build(mode) {
  if (!['native', 'wasm'].includes(mode)) throw new Error('Expected native or wasm');
  const root = path.resolve(__dirname, '..');
  const env = buildEnvironment(process.env, root, os.homedir());
  const args = mode === 'native'
    ? ['build', '--release', '--locked']
    : ['build', '-p', 'lm-resizer-wasm', '--release', '--locked', '--target', 'wasm32-unknown-unknown'];
  if (mode === 'wasm') env.CARGO_ENCODED_RUSTFLAGS += '\x1f--cfg\x1fgetrandom_backend="wasm_js"';
  const child = spawnSync('cargo', args, { cwd: root, env, stdio: 'inherit' });
  if (child.error) throw child.error;
  if (child.signal) throw new Error(`cargo terminated by ${child.signal}`);
  process.exitCode = child.status;
}

module.exports = { buildEnvironment };
if (require.main === module) build(process.argv[2]);
