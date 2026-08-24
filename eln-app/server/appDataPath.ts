import { copyFileSync, existsSync, mkdirSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'

export interface AppDataPathProvider {
  getAppDataDir(): string
}

/**
 * The only OS-aware path adapter. Replace this provider with Tauri's
 * app_data_dir() bridge when the desktop shell is introduced.
 */
export class NodeAppDataPathProvider implements AppDataPathProvider {
  getAppDataDir(): string {
    if (process.env.LABFLOW_APP_DATA_DIR) return process.env.LABFLOW_APP_DATA_DIR
    return join(homedir(), 'Library', 'Application Support', 'LabFlow')
  }
}

export const appDataPaths = (provider: AppDataPathProvider = new NodeAppDataPathProvider()) => {
  const getAppDataDir = () => provider.getAppDataDir()
  const getDatabasePath = () => join(getAppDataDir(), 'labflow.sqlite')
  const getAttachmentsDir = () => join(getAppDataDir(), 'files')
  const getAttachmentRelativePath = (attachmentId: string, filename: string) => join('files', attachmentId, filename)
  const ensureUserDataDirectories = () => {
    mkdirSync(getAppDataDir(), { recursive: true })
    mkdirSync(getAttachmentsDir(), { recursive: true })
  }
  return { getAppDataDir, getDatabasePath, getAttachmentsDir, getAttachmentRelativePath, ensureUserDataDirectories }
}

export interface LegacyMigrationResult { migrated: boolean; databasePath: string }

/** Copies a legacy project-local database only when the user-data target is absent. */
export function migrateLegacyDatabase(
  paths: ReturnType<typeof appDataPaths>,
  legacyDatabasePath: string,
  validate: (databasePath: string) => void,
): LegacyMigrationResult {
  paths.ensureUserDataDirectories()
  const target = paths.getDatabasePath()
  if (existsSync(target)) { validate(target); return { migrated: false, databasePath: target } }
  if (existsSync(legacyDatabasePath)) {
    copyFileSync(legacyDatabasePath, target, 0)
    try { validate(target) } catch (error) { throw new Error(`Legacy database copy could not be validated: ${String(error)}`) }
    return { migrated: true, databasePath: target }
  }
  return { migrated: false, databasePath: target }
}

export function assertRelativeAttachmentPath(relativePath: string) {
  if (relativePath.startsWith('/') || relativePath.includes('..') || !relativePath.startsWith(`files${String.fromCharCode(47)}`)) {
    throw new Error('Attachment paths must be relative paths under files/')
  }
}

export function getLegacyDatabasePath(projectRoot: string) { return join(projectRoot, 'data', 'labflow.sqlite') }
export function getAttachmentAbsolutePath(paths: ReturnType<typeof appDataPaths>, relativePath: string) {
  assertRelativeAttachmentPath(relativePath)
  return join(paths.getAppDataDir(), relativePath)
}
