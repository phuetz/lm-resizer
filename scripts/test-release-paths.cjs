const assert = require('node:assert/strict');
const { buildEnvironment } = require('./build-release-artifact.cjs');

const original = { RUSTFLAGS: '-C opt-level=2' };
const result = buildEnvironment(original, '/tmp/checkout with spaces', '/tmp/build home');
const flags = result.CARGO_ENCODED_RUSTFLAGS.split('\x1f');
assert(flags.includes('--remap-path-prefix=/tmp/checkout with spaces=/build/lm-resizer'));
assert(flags.includes('--remap-path-prefix=/tmp/build home=/build-home'));
assert.deepEqual(flags.slice(0, 2), ['-C', 'opt-level=2']);
assert.deepEqual(original, { RUSTFLAGS: '-C opt-level=2' });

const encoded = buildEnvironment({
  CARGO_ENCODED_RUSTFLAGS: '-C\x1flink-arg=path with spaces',
  RUSTFLAGS: '--ignored',
  CARGO_HOME: '/tmp/custom cargo',
  RUSTUP_HOME: '/tmp/custom rustup',
}, '/tmp/project', '/tmp/build-home').CARGO_ENCODED_RUSTFLAGS.split('\x1f');
assert.deepEqual(encoded.slice(0, 2), ['-C', 'link-arg=path with spaces']);
assert(!encoded.includes('--ignored'));
assert(encoded.includes('--remap-path-prefix=/tmp/custom cargo=/build/cargo'));
assert(encoded.includes('--remap-path-prefix=/tmp/custom rustup=/build/rustup'));

const windows = buildEnvironment({}, 'C:\\checkout', 'C:\\Users\\Builder').CARGO_ENCODED_RUSTFLAGS.split('\x1f');
assert(windows.includes('--remap-path-prefix=C:\\Users\\Builder=/build-home'));
assert(windows.includes('--remap-path-prefix=C:/Users/Builder=/build-home'));
if (process.platform !== 'win32') {
  // C compilers need the same prefixes as rustc (tree-sitter's parser.c embedded __FILE__).
  const c = buildEnvironment({ CFLAGS: '-O2' }, '/tmp/checkout with spaces', '/tmp/build home');
  assert(c.CFLAGS.startsWith('-O2 '));
  assert(c.CFLAGS.includes('"-ffile-prefix-map=/tmp/checkout with spaces=/build/lm-resizer"'));
  assert(c.CFLAGS.includes('"-ffile-prefix-map=/tmp/build home=/build-home"'));
  assert(c.CFLAGS.includes('-ffile-prefix-map=/tmp/build home/.cargo=/build/cargo') ||
    c.CFLAGS.includes('"-ffile-prefix-map=/tmp/build home/.cargo=/build/cargo"'));
  // Order matters to GCC: the last matching map wins, so the specific homes come after the user home.
  assert(c.CFLAGS.indexOf('=/build-home') < c.CFLAGS.indexOf('=/build/cargo'));
  assert.equal(c.CXXFLAGS.includes('=/build/cargo'), true);
  assert.equal(buildEnvironment({}, '/p', '/h').CFLAGS.startsWith('-ffile-prefix-map=/h=/build-home'), true);
}
console.log('Release path flags: spaces, caller flags, custom directories, Windows separators and unchanged parent environment OK');
