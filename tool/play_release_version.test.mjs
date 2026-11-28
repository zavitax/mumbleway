// node --test tool/play_release_version.test.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { namesVersion, productionStatus } from './play_release_version.mjs';

test('a version matches only as a whole token', () => {
  assert.ok(namesVersion('1.2.1', '1.2.1'));
  assert.ok(namesVersion('1.2.1 (45)', '1.2.1'));
  assert.ok(namesVersion('45 (1.2.1)', '1.2.1'));
  assert.ok(!namesVersion('1.2.10', '1.2.1'));
  assert.ok(!namesVersion('1.2.10 (46)', '1.2.1'));
  assert.ok(!namesVersion('11.2.1', '1.2.1'));
  assert.ok(!namesVersion('1x2y1', '1.2.1'));
  assert.ok(!namesVersion(undefined, '1.2.1'));
});

test('a staged rollout is reported but does not count as released', () => {
  const releases = [
    { name: '1.2.0', status: 'completed', versionCodes: ['40'] },
    { name: '1.2.1', status: 'inProgress', versionCodes: ['45'] },
    { name: '1.2.2', status: 'draft', versionCodes: ['46'] },
  ];
  const s = productionStatus(releases, '1.2.1');
  assert.equal(s.released, false);
  assert.equal(s.staging, true);
  assert.deepEqual(s.completed, ['1.2.0']);
  assert.deepEqual(s.staged, ['1.2.1']);
  assert.deepEqual(s.other, ['1.2.2 [draft]']);
  assert.equal(productionStatus(releases, '1.2.0').released, true);
});

test('a release without version codes is ignored', () => {
  const s = productionStatus([{ name: '1.2.1', status: 'completed' }], '1.2.1');
  assert.equal(s.released, false);
});
