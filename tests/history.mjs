// Run after worker-build --release. Uses an isolated in-memory D1 database,
// dummy credentials, and no external network access.
import assert from 'node:assert/strict';
import { readFile, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { Miniflare, convertV4MiniflareOptions } from 'miniflare';

let egress = 0;
const storage = await mkdtemp(join(tmpdir(), 'sniff-history-test-'));
const options = {
  modules: [{type:'ESModule',path:'build/index.js'}, {type:'CompiledWasm',path:'build/index_bg.wasm'}],
  compatibilityDate: '2025-04-25',
  bindings: { STABLE_EMAIL: 'test@example.invalid', STABLE_AAS_TOKEN: 'dummy' },
  d1Databases: { HISTORY: 'test-history' },
  resourcePersistencePath: storage,
  outboundService: () => { egress++; throw new Error('Unexpected network request'); },
};
let mf = new Miniflare(convertV4MiniflareOptions(options));
try {
  const db = await mf.getD1Database('HISTORY');
  const migration = await readFile('migrations/0001_version_history.sql', 'utf8');
  await db.exec(migration.replaceAll('\n', ' '));
  const account = createHash('sha256').update('test@example.invalid').digest('hex');
  const files = [{ name: 'base.apk', complete: true, pending_bmp_sha1: null, bytes: 4,
    sha256: 'fixture-hash', parts: [{ index: 0, media_key: 'fixture-media-key', bytes: 4, bmp_sha1: 'fixture-bmp-hash' }] }];
  const insert = (key, version, state, manifest, error = null) => db.prepare(
    'INSERT INTO version_history(account_key,package,channel,version_code,state,manifest,error) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT DO NOTHING'
  ).bind(key, 'test.package', 'stable', String(version), state, JSON.stringify(manifest), error).run();
  await insert(account, 123, 'complete', files);
  await insert('other-account', 999, 'complete', []);
  await insert(account, 124, 'failed', [{...files[0], complete:false, pending_bmp_sha1:'uncertain-hash'}], 'commit completion uncertain');
  const claims = await Promise.all([insert(account,125,'uploading',[]), insert(account,125,'uploading',[])]);
  assert.equal(claims.reduce((sum,r) => sum + r.meta.changes, 0), 1, 'one concurrent owner');

  const history = await (await mf.dispatchFetch('https://local/v1/history/test.package/stable')).json();
  assert.equal(history.success, true);
  assert.deepEqual(history.data.map(r => r.version_code).sort(), [123,124,125]);
  for (let attempt = 0; attempt < 2; attempt++) {
    const response = await mf.dispatchFetch('https://local/v1/download/test.package/stable/123');
    assert.equal(response.status, 200);
    assert.equal(response.headers.get('cache-control'), 'no-store');
    const body = await response.json();
    assert.equal(body.success,true);
    assert.deepEqual(body.data.photos,files);
    assert.equal(body.data.main_apk_url,null);
  }
  for (const version of [124,125]) {
    const response = await mf.dispatchFetch(`https://local/v1/download/test.package/stable/${version}`);
    assert.equal(response.status,502);
    assert.equal((await response.json()).success,false);
  }
  assert.equal((await mf.dispatchFetch('https://local/v1/download/test.package/stable/0')).status,400);
  assert.equal(egress,0,'cached and incomplete history must not repeat Play/Photos requests');
  const spec = await (await mf.dispatchFetch('https://local/openapi.json')).json();
  assert.ok(spec.paths['/v1/history/{package_name}/{channel}']);
  assert.ok(spec.components.schemas.DownloadInfo.properties.photos);
  await mf.dispose();
  mf = new Miniflare(convertV4MiniflareOptions(options));
  const restored = await (await mf.dispatchFetch('https://local/v1/download/test.package/stable/123')).json();
  assert.deepEqual(restored.data.photos, files, 'history survives a Worker restart');
  assert.equal(egress,0);
  console.log('Worker history: persistence, account isolation, atomic claim, cache reuse, incomplete-state safety, validation, OpenAPI passed');
} finally {
  await mf.dispose();
  await rm(storage, {recursive: true, force: true});
}
