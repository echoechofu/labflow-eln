//! Record image attachment service shared by Desktop adapters and future Agent tools.
//!
//! Image bytes stay in the canonical `files/` directory. SQLite stores only
//! metadata and portable relative locators. Large images and TIFF files get a
//! bounded PNG preview; the original is never rewritten.

use image::{ImageFormat, ImageReader};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
};

use crate::record_service::{RecordAttachment, RecordServiceError};

// floor(sqrt(8 MiB / 4 bytes per RGBA8 pixel)); half the former 2048² budget.
const PREVIEW_MAX_EDGE: u32 = 1_448;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertRecordImageRequest {
    pub id: String,
    pub record_id: String,
    pub source_path: String,
    pub rendered_content: String,
    pub change_id: String,
    pub created_at: String,
}

#[derive(Debug)]
pub struct AttachmentBytes {
    pub bytes: Vec<u8>,
    pub mime_type: String,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn image_details(format: ImageFormat) -> Option<(&'static str, &'static str)> {
    match format {
        ImageFormat::Png => Some(("png", "image/png")),
        ImageFormat::Jpeg => Some(("jpg", "image/jpeg")),
        ImageFormat::WebP => Some(("webp", "image/webp")),
        ImageFormat::Tiff => Some(("tif", "image/tiff")),
        _ => None,
    }
}

fn sha256_file(path: &Path) -> Result<String, RecordServiceError> {
    let mut file =
        File::open(path).map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn remove_created_directory(path: &Path) {
    if path.exists() {
        let _ = fs::remove_dir_all(path);
    }
}

pub fn insert_record_image(
    connection: &mut Connection,
    files_root: &Path,
    request: &InsertRecordImageRequest,
) -> Result<RecordAttachment, RecordServiceError> {
    if !valid_id(&request.id) || !valid_id(&request.change_id) {
        return Err(RecordServiceError::Validation(
            "Image or change id contains unsupported characters.".into(),
        ));
    }
    if request.rendered_content.trim().is_empty() {
        return Err(RecordServiceError::Validation(
            "Record body cannot be empty.".into(),
        ));
    }
    let expected_reference = format!("labflow-attachment://{}", request.id);
    if !request.rendered_content.contains(&expected_reference) {
        return Err(RecordServiceError::Validation(
            "Record body must contain the inserted image reference.".into(),
        ));
    }

    let current_json: String = connection
        .query_row(
            "SELECT current_data_json FROM records WHERE id=?1",
            [&request.record_id],
            |row| row.get(0),
        )
        .map_err(|_| RecordServiceError::NotFound("Record not found".into()))?;
    let mut current: Value = serde_json::from_str(&current_json)
        .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
    if !current.is_object() {
        return Err(RecordServiceError::Persistence(
            "Record data is invalid.".into(),
        ));
    }

    let source = PathBuf::from(&request.source_path);
    let metadata = fs::metadata(&source).map_err(|error| {
        RecordServiceError::Validation(format!("Image cannot be read: {error}"))
    })?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(RecordServiceError::Validation(
            "Selected image is empty or is not a file.".into(),
        ));
    }
    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| RecordServiceError::Validation("Image filename is invalid.".into()))?
        .to_owned();

    let reader = ImageReader::open(&source)
        .map_err(|error| {
            RecordServiceError::Validation(format!("Image cannot be opened: {error}"))
        })?
        .with_guessed_format()
        .map_err(|error| {
            RecordServiceError::Validation(format!("Image format is invalid: {error}"))
        })?;
    let format = reader.format().and_then(image_details).ok_or_else(|| {
        RecordServiceError::Validation(
            "Supported image formats are PNG, JPEG, WebP, and TIFF.".into(),
        )
    })?;
    let (width, height) = reader.into_dimensions().map_err(|error| {
        RecordServiceError::Validation(format!("Image cannot be decoded: {error}"))
    })?;

    fs::create_dir_all(files_root)
        .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
    let directory = files_root.join(&request.id);
    fs::create_dir(&directory).map_err(|error| {
        RecordServiceError::Conflict(format!(
            "Image attachment already exists or cannot be created: {error}"
        ))
    })?;

    let original_name = format!("original.{}", format.0);
    let original_path = directory.join(&original_name);
    if let Err(error) = fs::copy(&source, &original_path) {
        remove_created_directory(&directory);
        return Err(RecordServiceError::Persistence(error.to_string()));
    }
    let relative_path = format!("files/{}/{}", request.id, original_name);

    let needs_preview =
        format.1 == "image/tiff" || width > PREVIEW_MAX_EDGE || height > PREVIEW_MAX_EDGE;
    let preview_relative_path = if needs_preview {
        let preview_path = directory.join("preview.png");
        let preview_result = (|| {
            let image = ImageReader::open(&original_path)
                .map_err(|error| error.to_string())?
                .with_guessed_format()
                .map_err(|error| error.to_string())?
                .decode()
                .map_err(|error| error.to_string())?;
            let preview = image
                .thumbnail(PREVIEW_MAX_EDGE, PREVIEW_MAX_EDGE)
                .to_rgba8();
            preview
                .save_with_format(&preview_path, ImageFormat::Png)
                .map_err(|error| error.to_string())
        })();
        if let Err(error) = preview_result {
            remove_created_directory(&directory);
            return Err(RecordServiceError::Validation(format!(
                "Image preview cannot be generated: {error}"
            )));
        }
        format!("files/{}/preview.png", request.id)
    } else {
        relative_path.clone()
    };

    let content_sha256 = match sha256_file(&original_path) {
        Ok(hash) => hash,
        Err(error) => {
            remove_created_directory(&directory);
            return Err(error);
        }
    };
    let old_content = current
        .get("renderedContent")
        .cloned()
        .unwrap_or(Value::Null);
    let new_content = json!(request.rendered_content);
    current["renderedContent"] = new_content.clone();

    let transaction_result = (|| {
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO attachments (id,record_id,file_name,relative_path,mime_type,size,created_at,content_sha256,preview_relative_path,width_px,height_px) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                request.id,
                request.record_id,
                file_name,
                relative_path,
                format.1,
                metadata.len() as i64,
                request.created_at,
                content_sha256,
                preview_relative_path,
                width as i64,
                height as i64,
            ],
        )?;
        transaction.execute(
            "UPDATE records SET current_data_json=?2,updated_at=?3 WHERE id=?1",
            params![request.record_id, current.to_string(), request.created_at],
        )?;
        transaction.execute(
            "INSERT INTO record_changes (id,record_id,field_path,old_value_json,new_value_json,actor_id,changed_at) VALUES (?1,?2,'renderedContent',?3,?4,'local_user',?5)",
            params![
                request.change_id,
                request.record_id,
                old_content.to_string(),
                new_content.to_string(),
                request.created_at,
            ],
        )?;
        transaction.commit()?;
        Ok::<(), RecordServiceError>(())
    })();
    if let Err(error) = transaction_result {
        remove_created_directory(&directory);
        return Err(error);
    }

    Ok(RecordAttachment {
        id: request.id.clone(),
        file_name,
        relative_path,
        mime_type: Some(format.1.into()),
        size: Some(metadata.len() as i64),
        content_sha256: Some(content_sha256),
        preview_relative_path: Some(preview_relative_path),
        width_px: Some(width as i64),
        height_px: Some(height as i64),
    })
}

fn portable_file_path(app_data_root: &Path, relative_path: &str) -> Option<PathBuf> {
    let path = Path::new(relative_path);
    let mut components = path.components();
    if components.next() != Some(Component::Normal("files".as_ref())) {
        return None;
    }
    if components.any(|component| !matches!(component, Component::Normal(_))) {
        return None;
    }
    Some(app_data_root.join(path))
}

pub fn load_image_preview(
    connection: &Connection,
    app_data_root: &Path,
    attachment_id: &str,
) -> Result<AttachmentBytes, RecordServiceError> {
    if !valid_id(attachment_id) {
        return Err(RecordServiceError::Validation(
            "Attachment id is invalid.".into(),
        ));
    }
    let row: Option<(String, String)> = connection
        .query_row(
            "SELECT coalesce(preview_relative_path,relative_path), CASE WHEN preview_relative_path LIKE '%.png' THEN 'image/png' ELSE coalesce(mime_type,'application/octet-stream') END FROM attachments WHERE id=?1 AND mime_type LIKE 'image/%'",
            [attachment_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (relative_path, mime_type) =
        row.ok_or_else(|| RecordServiceError::NotFound("Image attachment not found.".into()))?;
    let path = portable_file_path(app_data_root, &relative_path).ok_or_else(|| {
        RecordServiceError::Persistence("Image attachment locator is invalid.".into())
    })?;
    // Bound encoded input before IPC/browser decode, including legacy locators.
    const MAX_PREVIEW_BYTES: u64 = 16 * 1024 * 1024;
    let file =
        File::open(path).map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
    let mut bytes = Vec::new();
    file.take(MAX_PREVIEW_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| RecordServiceError::Persistence(error.to_string()))?;
    if bytes.len() as u64 > MAX_PREVIEW_BYTES {
        return Err(RecordServiceError::Validation(
            "Preview exceeds 16 MiB; reinsert the image to regenerate it.".into(),
        ));
    }
    let (width, height) = ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|error| RecordServiceError::Validation(error.to_string()))?
        .into_dimensions()
        .map_err(|error| RecordServiceError::Validation(error.to_string()))?;
    if width > 2048 || height > 2048 {
        return Err(RecordServiceError::Validation(
            "Legacy image has no bounded preview; reinsert it to regenerate the preview.".into(),
        ));
    }
    Ok(AttachmentBytes { bytes, mime_type })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply_schema;

    fn workspace(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("labflow-image-{label}-{}", uuid::Uuid::new_v4()))
    }

    fn seeded_connection() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        apply_schema(&connection).unwrap();
        connection.execute_batch(
            "INSERT INTO experiments VALUES ('e','EXP','Main','','#000');
             INSERT INTO tasks (id,experiment_id,title,start_time,end_time,status,updated_at) VALUES ('t','e','Task','2026-08-31T09:00','2026-08-31T10:00','completed','now');
             INSERT INTO records (id,task_id,experiment_id,protocol_id,protocol_snapshot_json,current_data_json,updated_at) VALUES ('r','t','e','p','{}','{\"renderedContent\":\"before\"}','now');",
        ).unwrap();
        connection
    }

    #[test]
    fn image_insert_copies_original_and_updates_record_atomically() {
        let root = workspace("insert");
        let files = root.join("files");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.png");
        image::RgbaImage::from_pixel(4, 3, image::Rgba([10, 20, 30, 255]))
            .save(&source)
            .unwrap();
        let mut connection = seeded_connection();
        let request = InsertRecordImageRequest {
            id: "attachment-1".into(),
            record_id: "r".into(),
            source_path: source.to_string_lossy().into_owned(),
            rendered_content: "before\n\n![Image](labflow-attachment://attachment-1)".into(),
            change_id: "change-1".into(),
            created_at: "2026-08-31T09:30:00".into(),
        };
        let attachment = insert_record_image(&mut connection, &files, &request).unwrap();
        assert_eq!(attachment.width_px, Some(4));
        assert_eq!(attachment.height_px, Some(3));
        assert!(root.join(&attachment.relative_path).is_file());
        let current: String = connection
            .query_row(
                "SELECT current_data_json FROM records WHERE id='r'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(current.contains("labflow-attachment://attachment-1"));
        let loaded = load_image_preview(&connection, &root, "attachment-1").unwrap();
        assert_eq!(loaded.mime_type, "image/png");
        assert!(!loaded.bytes.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn image_insert_rejects_body_without_new_reference() {
        let root = workspace("missing-reference");
        fs::create_dir_all(&root).unwrap();
        let mut connection = seeded_connection();
        let request = InsertRecordImageRequest {
            id: "attachment-1".into(),
            record_id: "r".into(),
            source_path: root.join("missing.png").to_string_lossy().into_owned(),
            rendered_content: "unchanged".into(),
            change_id: "change-1".into(),
            created_at: "2026-08-31T09:30:00".into(),
        };
        let error =
            insert_record_image(&mut connection, &root.join("files"), &request).unwrap_err();
        assert_eq!(error.code(), "validation_error");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn square_preview_fits_eight_mib_and_preserves_high_bit_depth_original() {
        let root = workspace("preview-budget");
        let files = root.join("files");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("square.png");
        // Previously this image reused the original: it is below the old 2048px cap.
        image::ImageBuffer::<image::Rgba<u16>, Vec<u16>>::from_pixel(
            1_600,
            1_600,
            image::Rgba([10_000, 20_000, 30_000, 65_535]),
        )
        .save(&source)
        .unwrap();
        let mut connection = seeded_connection();
        let request = InsertRecordImageRequest {
            id: "attachment-square".into(),
            record_id: "r".into(),
            source_path: source.to_string_lossy().into_owned(),
            rendered_content: "![Square](labflow-attachment://attachment-square)".into(),
            change_id: "change-square".into(),
            created_at: "2026-08-31T09:30:00".into(),
        };
        let attachment = insert_record_image(&mut connection, &files, &request).unwrap();
        assert_eq!(
            fs::read(root.join(&attachment.relative_path)).unwrap(),
            fs::read(&source).unwrap()
        );
        let loaded = load_image_preview(&connection, &root, "attachment-square").unwrap();
        let preview = image::load_from_memory(&loaded.bytes).unwrap();
        assert_eq!((preview.width(), preview.height()), (1_448, 1_448));
        assert_eq!(preview.color(), image::ColorType::Rgba8);
        assert!(preview.as_bytes().len() <= 8 * 1_024 * 1_024);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn oversized_image_keeps_original_and_serves_bounded_preview() {
        let root = workspace("preview");
        let files = root.join("files");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("wide.png");
        image::RgbaImage::from_pixel(3_000, 10, image::Rgba([40, 50, 60, 255]))
            .save(&source)
            .unwrap();
        let mut connection = seeded_connection();
        let request = InsertRecordImageRequest {
            id: "attachment-wide".into(),
            record_id: "r".into(),
            source_path: source.to_string_lossy().into_owned(),
            rendered_content: "![Wide](labflow-attachment://attachment-wide)".into(),
            change_id: "change-wide".into(),
            created_at: "2026-08-31T09:30:00".into(),
        };
        let attachment = insert_record_image(&mut connection, &files, &request).unwrap();
        assert_ne!(
            attachment.preview_relative_path,
            Some(attachment.relative_path.clone())
        );
        assert!(root.join(&attachment.relative_path).is_file());
        let loaded = load_image_preview(&connection, &root, "attachment-wide").unwrap();
        let preview = image::load_from_memory(&loaded.bytes).unwrap();
        assert_eq!(preview.width(), PREVIEW_MAX_EDGE);
        assert!(preview.height() <= PREVIEW_MAX_EDGE);
        fs::remove_dir_all(root).unwrap();
    }
}
