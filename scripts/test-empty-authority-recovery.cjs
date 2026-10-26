const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { DatabaseSync } = require('node:sqlite');
const { inspectEmptyAuthority, policyTables } = require('./inspect-empty-authority.cjs');

function fixture(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'Focuser-Recovery-Test-'));
  t.after(() => {
    const resolved = fs.realpathSync(dir);
    assert.equal(path.dirname(resolved).toLowerCase(), fs.realpathSync(os.tmpdir()).toLowerCase());
    assert.ok(path.basename(resolved).startsWith('Focuser-Recovery-Test-'));
    fs.rmSync(resolved, { recursive: true });
  });
  const file = path.join(dir, 'authority.db');
  const db = new DatabaseSync(file);
  for (const name of policyTables) db.exec(`CREATE TABLE "${name}"(value TEXT)`);
  db.exec('CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT); CREATE TABLE schema_version(version INTEGER);');
  for (let i = 1; i <= 8; i++) db.prepare('INSERT INTO schema_version VALUES (?)').run(i);
  db.prepare('INSERT INTO settings VALUES (?,?)').run('_authority.installation', 'b34a27bf-7714-47f1-9857-48bf08acd11c');
  db.prepare('INSERT INTO settings VALUES (?,?)').run('_authority.browser_grace', '{}');
  db.close();
  return file;
}
function change(file, sql) { const db = new DatabaseSync(file); try { db.exec(sql); } finally { db.close(); } }

test('empty setup inspection is read-only and preserves its installation identity', t => {
  const file = fixture(t), original = fs.readFileSync(file);
  const result = inspectEmptyAuthority(file);
  assert.equal(result.installation, 'b34a27bf-7714-47f1-9857-48bf08acd11c');
  assert.equal(result.empty, true);
  assert.match(result.sha256, /^[0-9A-F]{64}$/);
  assert.deepEqual(fs.readFileSync(file), original);
});
test('retains the original autostart preference without activating it', t => {
  const file = fixture(t);
  change(file, "INSERT INTO settings VALUES ('autostart_enabled','1')");
  const original = fs.readFileSync(file);
  assert.equal(inspectEmptyAuthority(file).empty, true);
  assert.deepEqual(fs.readFileSync(file), original);
});
for (const name of ['block_lists', 'allowances', 'statistics', 'pomodoro_sessions', 'unlock_challenges']) {
  test(`refuses any retained ${name}, including expired or unprotected records`, t => {
    const file = fixture(t);
    change(file, `INSERT INTO "${name}" VALUES ('preserve this row')`);
    const original = fs.readFileSync(file);
    assert.throws(() => inspectEmptyAuthority(file), /Existing policy or history/);
    assert.deepEqual(fs.readFileSync(file), original);
  });
}
for (const [key, value] of [['_authority.maintenance', 'pending'], ['theme', 'dark'], ['_authority.browser_cleanup_required', 'true'], ['_authority.browser_grace', '{"Chrome":123}']]) {
  test(`refuses non-pristine setting ${key}`, t => {
    const file = fixture(t), db = new DatabaseSync(file);
    db.prepare('INSERT OR REPLACE INTO settings VALUES (?,?)').run(key, value); db.close();
    const original = fs.readFileSync(file);
    assert.throws(() => inspectEmptyAuthority(file), /Existing settings/);
    assert.deepEqual(fs.readFileSync(file), original);
  });
}
test('refuses unknown tables, schema versions and SQLite journals', t => {
  const file = fixture(t);
  change(file, 'CREATE TABLE future_history(value TEXT)');
  assert.throws(() => inspectEmptyAuthority(file), /Unknown or incomplete schema/);
  change(file, 'DROP TABLE future_history; INSERT INTO schema_version VALUES (9)');
  assert.throws(() => inspectEmptyAuthority(file), /Unknown schema version/);
  fs.writeFileSync(file + '-wal', 'preserve journal');
  assert.throws(() => inspectEmptyAuthority(file), /SQLite journal/);
  assert.equal(fs.readFileSync(file + '-wal', 'utf8'), 'preserve journal');
});
test('missing and corrupt databases are never created or repaired', t => {
  const file = fixture(t); fs.unlinkSync(file);
  assert.throws(() => inspectEmptyAuthority(file)); assert.equal(fs.existsSync(file), false);
  fs.writeFileSync(file, 'invalid SQLite');
  assert.throws(() => inspectEmptyAuthority(file)); assert.equal(fs.readFileSync(file, 'utf8'), 'invalid SQLite');
});
