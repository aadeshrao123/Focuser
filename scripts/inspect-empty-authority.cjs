// Administrator recovery preflight only. Never imported by production clients.
const fs = require('node:fs');
const crypto = require('node:crypto');
const { DatabaseSync } = require('node:sqlite');

const policyTables = [
  'active_blocks', 'allowance_usage', 'allowances', 'block_lists', 'blocked_events',
  'pomodoro_phases', 'pomodoro_sessions', 'shared_allowance_occurrences',
  'shared_allowance_usage', 'statistics', 'unlock_challenges',
];
function inspectEmptyAuthority(path) {
  const metadata = fs.lstatSync(path);
  if (!metadata.isFile() || metadata.isSymbolicLink()) throw Error('Unexpected database path');
  for (const suffix of ['-wal', '-shm', '-journal']) {
    if (fs.existsSync(path + suffix)) throw Error('Live or interrupted SQLite journal requires manual recovery');
  }
  const db = new DatabaseSync(path, { readOnly: true });
  let installation;
  try {
    db.exec('PRAGMA query_only=ON; PRAGMA busy_timeout=3000; BEGIN;');
    if (db.prepare('PRAGMA quick_check').get().quick_check !== 'ok') throw Error('Database integrity failed');
    const tables = db.prepare("SELECT name FROM sqlite_master WHERE type='table'").all().map(r => r.name);
    const allowed = [...policyTables, 'settings', 'schema_version', 'sqlite_sequence'];
    if (tables.some(name => !allowed.includes(name)) || allowed.filter(n => n !== 'sqlite_sequence').some(n => !tables.includes(n))) {
      throw Error('Unknown or incomplete schema requires manual recovery');
    }
    const versions = db.prepare('SELECT version FROM schema_version ORDER BY version').all().map(r => r.version);
    if (JSON.stringify(versions) !== '[1,2,3,4,5,6,7,8]') throw Error('Unknown schema version requires manual recovery');
    for (const name of [...policyTables, ...(tables.includes('sqlite_sequence') ? ['sqlite_sequence'] : [])]) {
      if (db.prepare(`SELECT COUNT(*) AS n FROM "${name}"`).get().n !== 0) {
        throw Error(`Existing policy or history in ${name}; empty-setup recovery refused`);
      }
    }
    const settings = db.prepare('SELECT key,value FROM settings').all();
    for (const { key, value } of settings) {
      if (key === '_authority.installation' && /^[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(value)) installation = value;
      else if (key === '_authority.browser_grace' && value === '{}') continue;
      else if (key === '_authority.browser_cleanup_required' && value === 'false') continue;
      // This enrollment preference is preserved verbatim by the later import;
      // it is not a commitment or permission to start anything during recovery.
      else if (key === 'autostart_enabled' && ['1', '0', 'true', 'false'].includes(value)) continue;
      else throw Error(`Existing settings, maintenance or pending coverage require manual recovery (${key})`);
    }
    if (!installation) throw Error('Missing installation identity requires manual recovery');
    db.exec('ROLLBACK;');
  } finally { db.close(); }
  return { installation, sha256: crypto.createHash('sha256').update(fs.readFileSync(path)).digest('hex').toUpperCase(), empty: true };
}
module.exports = { inspectEmptyAuthority, policyTables };
if (require.main === module) {
  try { console.log(JSON.stringify(inspectEmptyAuthority(process.argv[2]))); }
  catch (error) { console.error(error.message); process.exitCode = 1; }
}
