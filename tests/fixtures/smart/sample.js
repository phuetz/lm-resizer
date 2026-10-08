import fs from 'node:fs';
class Reader {
  // Lit un chemin.
  read(path) { return fs.readFileSync(path); }
}
export default function open(path) { return new Reader().read(path); }
export const transform = value => value + 1;
