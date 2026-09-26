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
  bindings: { STABLE_EMAIL: 'test@example.invalid', STABLE_AAS_TOKEN: 'aas_et/test' },
  d1Databases: { HISTORY: 'test-history' },
  queueProducers: ['ARCHIVE_QUEUE'],
  queueConsumers: ['ARCHIVE_QUEUE'],
  resourcePersistencePath: storage,
  outboundService: request => {
    egress++;
    if (request.url === 'https://play.googleapis.com/test-range') {
      assert.equal(request.headers.get('Range'), 'bytes=0-7');
      return new Response(new Uint8Array([80,75,3,4,1,2,3,4]), {
        status: 206,
        headers: { 'Content-Range': 'bytes 0-7/8' },
      });
    }
    throw new Error('Unexpected network request');
  },
};
let mf = new Miniflare(convertV4MiniflareOptions(options));
try {
  const db = await mf.getD1Database('HISTORY');
  for (const name of ['0001_version_history.sql', '0002_archive_jobs.sql']) {
    const migration = await readFile(`migrations/${name}`, 'utf8');
    await db.exec(migration.replaceAll('\n', ' '));
  }
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
  await db.prepare("UPDATE version_history SET lease_until=unixepoch('now')+180 WHERE version_code='125'").run();

  const history = await (await mf.dispatchFetch('https://local/v1/history/test.package/stable')).json();
  assert.equal(history.success, true);
  assert.deepEqual(history.data.map(r => r.version_code).sort(), [123,124,125]);
  assert.equal((await mf.dispatchFetch('https://local/v1/download/test.package/stable/0')).status,400);
  assert.equal(egress,0,'history must not contact Play or Photos');
  const spec = await (await mf.dispatchFetch('https://local/openapi.json')).json();
  assert.ok(spec.paths['/v1/history/{package_name}/{channel}']);
  assert.ok(spec.components.schemas.DownloadInfo.properties.photos);
  await mf.dispose();
  mf = new Miniflare(convertV4MiniflareOptions(options));
  const restored = await (await mf.dispatchFetch('https://local/v1/history/test.package/stable')).json();
  assert.deepEqual(restored.data.find(r => r.version_code === 123).photos, files, 'history survives a Worker restart');
  assert.equal(egress,0);
  const resumedDb = await mf.getD1Database('HISTORY');
  const interrupted = [{name:'base.apk', complete:false, pending_bmp_sha1:null, bytes:8, sha256:null,
    parts:[{index:0,media_key:'confirmed-key',bytes:8,bmp_sha1:'confirmed-hash'}]}];
  const plan = {main_apk_url:'https://play.googleapis.com/test-range',splits:[],additional_files:[],dex_metadata_url:null,photos:[]};
  await resumedDb.prepare('INSERT INTO version_history(account_key,package,channel,version_code,state,manifest,plan) VALUES (?,?,?,?,?,?,?)')
    .bind(account,'test.package','stable','126','uploading',JSON.stringify(interrupted),JSON.stringify(plan)).run();
  const queue = await mf.getQueueProducer('ARCHIVE_QUEUE');
  await queue.send({package:'test.package',channel:'stable',version:126});
  let resumed;
  for (let attempt=0; attempt<30; attempt++) {
    const result = await resumedDb.prepare("SELECT state,manifest FROM version_history WHERE version_code='126'").first();
    if (result.state === 'complete') { resumed = JSON.parse(result.manifest); break; }
    await new Promise(resolve => setTimeout(resolve,50));
  }
  assert.equal(resumed?.[0].complete,true);
  assert.equal(resumed[0].sha256,createHash('sha256').update(new Uint8Array([80,75,3,4,1,2,3,4])).digest('hex'));
  assert.equal(egress,1,'resume reads the missing hash bytes without uploading again');
  console.log('Worker history: persistence, account isolation, atomic claim, cache reuse, incomplete-state safety, validation, OpenAPI passed');
} finally {
  await mf.dispose();
  await rm(storage, {recursive: true, force: true});
}
