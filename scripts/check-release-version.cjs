const fs = require('node:fs');
const path = require('node:path');

const root = path.resolve(__dirname, '..');
const read = (file) => fs.readFileSync(path.join(root, file), 'utf8');
const cargoVersion = (file) => {
  const match = read(file).match(/^version = "([^"]+)"/m);
  if (!match) throw new Error(`Missing version in ${file}`);
  return match[1];
};
const version = cargoVersion('Cargo.toml');
if (process.env.RELEASE_TAG && process.env.CONFIRM !== 'PREPARE_DRAFT') {
  throw new Error('Expected PREPARE_DRAFT confirmation');
}
const versions = {
  core: cargoVersion('crates/lm-resizer-core/Cargo.toml'),
  wasm: cargoVersion('crates/lm-resizer-wasm/Cargo.toml'),
  npm: JSON.parse(read('packages/wasm/package.json')).version,
  plugin: JSON.parse(read('.claude-plugin/plugin.json')).version,
  marketplace: JSON.parse(read('.claude-plugin/marketplace.json')).plugins.find(
    (plugin) => plugin.name === 'lm-resizer',
  )?.version,
};
for (const [name, actual] of Object.entries(versions)) {
  if (actual !== version) throw new Error(`${name} version ${actual} differs from ${version}`);
}
if (process.env.RELEASE_TAG && process.env.RELEASE_TAG !== `v${version}`) {
  throw new Error(`Tag ${process.env.RELEASE_TAG} differs from v${version}`);
}
console.log(`Release versions aligned: ${version}`);
