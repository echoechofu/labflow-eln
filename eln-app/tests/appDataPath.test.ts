import assert from 'node:assert/strict'
import { afterEach, test } from 'node:test'
import { existsSync, mkdirSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import Database from 'better-sqlite3'
import { appDataPaths, getAttachmentAbsolutePath, migrateLegacyDatabase } from '../server/appDataPath.ts'
import { databaseExistsOutsideProject, openDatabase, validateDatabase } from '../server/database.ts'

const roots: string[] = []
const fixture = () => { const root = join(tmpdir(), `labflow-path-${crypto.randomUUID()}`); roots.push(root); return root }
afterEach(() => roots.splice(0).forEach(root => rmSync(root, { recursive: true, force: true })))

test('app startup creates isolated user-data and attachment directories', () => {
  const root = fixture(); const paths = appDataPaths({ getAppDataDir: () => join(root, 'Library', 'Application Support', 'LabFlow') })
  paths.ensureUserDataDirectories()
  assert.equal(existsSync(paths.getAppDataDir()), true)
  assert.equal(existsSync(paths.getAttachmentsDir()), true)
  assert.equal(paths.getDatabasePath().endsWith('/LabFlow/labflow.sqlite'), true)
})

test('fresh migration creates SQLite outside project source directory', () => {
  const root = fixture(); const project = join(root, 'project'); const paths = appDataPaths({ getAppDataDir: () => join(root, 'user-data', 'LabFlow') })
  paths.ensureUserDataDirectories(); const db = openDatabase(paths.getDatabasePath()); db.close()
  validateDatabase(paths.getDatabasePath())
  assert.equal(databaseExistsOutsideProject(paths.getDatabasePath(), project), true)
})

test('legacy project database migrates without losing data', () => {
  const root = fixture(); const legacy = join(root, 'project', 'data', 'labflow.sqlite'); mkdirSync(join(root, 'project', 'data'), { recursive: true })
  const old = openDatabase(legacy); old.prepare('INSERT INTO experiments VALUES (?,?,?,?,?)').run('e1', 'E001', 'Preserved experiment', '', '#000'); old.close()
  const paths = appDataPaths({ getAppDataDir: () => join(root, 'user-data', 'LabFlow') })
  const result = migrateLegacyDatabase(paths, legacy, validateDatabase)
  assert.equal(result.migrated, true); assert.equal(existsSync(paths.getDatabasePath()), true)
  const migrated = new Database(paths.getDatabasePath(), { readonly: true }); assert.equal((migrated.prepare('SELECT title FROM experiments WHERE id=?').get('e1') as { title: string }).title, 'Preserved experiment'); migrated.close()
})

test('restart uses the same user database and source build cleanup cannot remove it', () => {
  const root = fixture(); const project = join(root, 'project'); const build = join(project, 'dist'); mkdirSync(build, { recursive: true })
  const paths = appDataPaths({ getAppDataDir: () => join(root, 'user-data', 'LabFlow') }); paths.ensureUserDataDirectories()
  const first = openDatabase(paths.getDatabasePath()); first.prepare('INSERT INTO experiments VALUES (?,?,?,?,?)').run('e2', 'E002', 'Restart check', '', '#000'); first.close()
  rmSync(build, { recursive: true, force: true })
  const second = new Database(paths.getDatabasePath(), { readonly: true }); assert.equal((second.prepare('SELECT count(*) AS count FROM experiments').get() as { count: number }).count, 1); second.close()
})

test('attachment locators are relative and resolve through the path service', () => {
  const root = fixture(); const paths = appDataPaths({ getAppDataDir: () => join(root, 'user-data', 'LabFlow') })
  const relative = paths.getAttachmentRelativePath('att-1', 'raw.xlsx')
  assert.equal(relative, 'files/att-1/raw.xlsx')
  assert.equal(getAttachmentAbsolutePath(paths, relative), join(paths.getAppDataDir(), relative))
  assert.throws(() => getAttachmentAbsolutePath(paths, '/Users/example/raw.xlsx'))
})
