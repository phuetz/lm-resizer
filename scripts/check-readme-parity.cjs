#!/usr/bin/env node
// Échoue si README.md et README.fr.md ne portent pas les mêmes nombres,
// pourcentages, liens, codes en ligne et blocs de code, dans le même ordre.
// Les nombres sont normalisés (virgule/point décimal, espaces insécables).
'use strict';
const fs = require('fs');
const path = require('path');

const root = path.resolve(__dirname, '..');
const read = (name) => fs.readFileSync(path.join(root, name), 'utf8');

function tokens(text) {
  text = text.replace(/[  ]/g, '');
  text = text.replace(/(?<=\d) (?=\d{3}\b)/g, '');
  text = text.replace(/(\d),(\d)/g, '$1.$2');
  const out = [];
  const re = /`[^`\n]+`|\]\([^)\s]+\)|\d+(?:\.\d+)?[ ]?%?|\b\d+(?:\.\d+)*\b/g;
  for (const m of text.matchAll(re)) {
    const t = m[0];
    if (t === '](README.md)' || t === '](README.fr.md)') continue;
    out.push(t.startsWith('`') || t.startsWith('](') ? t : t.replace(/ /g, ''));
  }
  return out;
}

function fences(text) {
  return [...text.matchAll(/^(?:~~~|```)[^\n]*\n([\s\S]*?)^(?:~~~|```)/gm)].map((m) => m[1]);
}

const en = read('README.md');
const fr = read('README.fr.md');
let bad = 0;
const a = tokens(en);
const b = tokens(fr);
// Alignement par plus longue sous-suite commune, pour ne signaler que les écarts.
const n = a.length, m = b.length;
const dp = Array.from({ length: n + 1 }, () => new Int32Array(m + 1));
for (let i = n - 1; i >= 0; i--)
  for (let j = m - 1; j >= 0; j--)
    dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
let i = 0, j = 0;
while (i < n || j < m) {
  if (i < n && j < m && a[i] === b[j]) { i++; j++; continue; }
  const onlyEn = [], onlyFr = [];
  while ((i < n || j < m) && !(i < n && j < m && a[i] === b[j])) {
    if (j >= m || (i < n && dp[i + 1][j] >= dp[i][j + 1])) onlyEn.push(a[i++]);
    else onlyFr.push(b[j++]);
  }
  bad++;
  console.log(`DIFFÉRENCE : EN ${JSON.stringify(onlyEn)} / FR ${JSON.stringify(onlyFr)}`);
}
const fa = fences(en), fb = fences(fr);
if (fa.length !== fb.length) {
  bad++;
  console.log(`DIFFÉRENCE : ${fa.length} blocs de code en EN, ${fb.length} en FR`);
}
fa.forEach((x, k) => {
  if (fb[k] !== undefined && x !== fb[k]) {
    bad++;
    console.log(`DIFFÉRENCE : bloc de code n°${k + 1}\nEN: ${x}FR: ${fb[k]}`);
  }
});
console.log(`check-readme-parity : ${n} nombres/liens/codes et ${fa.length} blocs comparés, ${bad} différence(s)`);
process.exit(bad ? 1 : 0);
