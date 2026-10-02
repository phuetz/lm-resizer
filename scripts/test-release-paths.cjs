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
console.log('Release path flags: spaces, caller flags, custom directories, Windows separators and unchanged parent environment OK');
