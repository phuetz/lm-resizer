// Exercise the embedded downloader's protocol without depending on a network.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const vm = require('node:vm');
const { Readable } = require('node:stream');
const { finished } = require('node:stream/promises');
const installer = fs.readFileSync(path.join(__dirname, '../install.ps1'), 'utf8');
const code = installer.match(/\$download = @'\r?\n([\s\S]*?)\r?\n'@/)[1];
const root = fs.mkdtempSync(path.join(os.tmpdir(), 'install fallback '));
async function check(responses, expectedError) {
  const output = path.join(root, 'archive.zip');
  let writer;
  const run = () => vm.runInNewContext(code, {
    URL, process: { argv: ['node', 'https://example.test/asset', output] },
    require(name) {
      if (name === 'fs') return { createWriteStream(p) { writer = fs.createWriteStream(p); return writer; } };
      assert.equal(name, 'https');
      return { get(url, options, callback) {
        assert.equal(options.rejectUnauthorized, true);
        assert.ok(url.startsWith('https://'));
        const response = responses.shift();
        assert.ok(response, 'unexpected request');
        const stream = Readable.from([Buffer.from([0, 130, 255, 13, 10])]);
        stream.statusCode = response.status;
        stream.headers = response.location ? { location: response.location } : {};
        callback(stream);
        return { on() { return this; } };
      } };
    },
  });
  if (expectedError) assert.throws(run, expectedError);
  else {
    run();
    await finished(writer);
    assert.deepEqual(fs.readFileSync(output), Buffer.from([0, 130, 255, 13, 10]));
  }
}
(async () => {
  try {
    await check([{ status: 302, location: '/redirected' }, { status: 200 }]);
    await check([{ status: 404 }], /HTTP 404/);
    await check([{ status: 302, location: 'http://example.test/plain' }], /Only HTTPS/);
    await check(Array.from({ length: 6 }, () => ({ status: 302, location: '/loop' })), /Too many redirects/);
    console.log('Installer fallback: exact binary bytes, HTTPS-only redirects, status failures and redirect bound pass');
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
})().catch(error => { console.error(error); process.exitCode = 1; });
