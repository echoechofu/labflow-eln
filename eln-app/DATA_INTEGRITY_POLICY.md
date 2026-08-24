# User data isolation

LabFlow source/build directories must never contain runtime user data. The database and attachments are resolved only through the `AppDataPathProvider` abstraction.

On macOS the Node fallback uses `~/Library/Application Support/LabFlow/`; a Tauri adapter will replace this provider with Tauri `app_data_dir` without exposing OS-specific paths to the UI, repository, or domain modules.

Any database or attachment stored below the source/project directory is a **P0 data-integrity failure**. Attachment records retain only `files/<attachment_id>/<filename>` relative paths.
