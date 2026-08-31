//! Incremental image-page PDF writer. Only one compressed page is accepted at a
//! time; completed pages live on disk, not in a document-sized byte buffer.
use image::{ImageDecoder, ImageReader};
use std::{
    fs::{self, File, OpenOptions},
    io::{Cursor, Seek, Write},
    path::{Path, PathBuf},
};

pub const PAGE_WIDTH: u32 = 1240;
pub const PAGE_HEIGHT: u32 = 1754;
const MAX_PAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_PAGES: usize = 10_000;
// Keep classic PDF xref offsets below ten digits, including the final index.
const MAX_DOCUMENT_BYTES: u64 = 8 * 1024 * 1024 * 1024;

pub struct PdfExport {
    pub id: String,
    file: Option<File>,
    temporary: PathBuf,
    destination: PathBuf,
    offsets: Vec<u64>,
    pages: usize,
    poisoned: bool,
}

impl PdfExport {
    pub fn begin(destination: &Path) -> Result<Self, String> {
        if !destination.is_absolute()
            || destination
                .extension()
                .and_then(|s| s.to_str())
                .map(str::to_lowercase)
                .as_deref()
                != Some("pdf")
        {
            return Err("请选择绝对路径的 .pdf 文件".into());
        }
        // Never replace a user file, including one created during generation.
        if destination.exists() {
            return Err("目标文件已存在，请选择新的 PDF 文件名".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let temporary = destination.with_file_name(format!(".labflow-{id}.pdf-part"));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        let mut export = Self {
            id,
            file: Some(file),
            temporary,
            destination: destination.into(),
            offsets: vec![0; 3],
            pages: 0,
            poisoned: false,
        };
        export
            .file
            .as_mut()
            .unwrap()
            .write_all(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
            .map_err(|e| e.to_string())?;
        Ok(export)
    }

    fn object(&mut self, id: usize, body: &[u8]) -> std::io::Result<()> {
        let file = self.file.as_mut().unwrap();
        self.offsets.resize(self.offsets.len().max(id + 1), 0);
        self.offsets[id] = file.stream_position()?;
        writeln!(file, "{id} 0 obj")?;
        file.write_all(body)?;
        file.write_all(b"\nendobj\n")
    }

    pub fn append(&mut self, sequence: usize, jpeg: &[u8]) -> Result<(), String> {
        if self.poisoned || sequence != self.pages || self.pages >= MAX_PAGES {
            return Err("PDF 会话不可写、页码不连续或超过 10000 页限制".into());
        }
        if jpeg.len() > MAX_PAGE_BYTES || jpeg.is_empty() {
            return Err("单页压缩数据超过 8 MiB 或为空".into());
        }
        let position = self
            .file
            .as_mut()
            .unwrap()
            .stream_position()
            .map_err(|e| e.to_string())?;
        if position + jpeg.len() as u64 + 2 * 1024 * 1024 > MAX_DOCUMENT_BYTES {
            return Err("PDF 将超过 8 GiB，请拆分导出".into());
        }
        let reader = ImageReader::with_format(Cursor::new(jpeg), image::ImageFormat::Jpeg);
        let decoder = reader.into_decoder().map_err(|e| e.to_string())?;
        if decoder.dimensions() != (PAGE_WIDTH, PAGE_HEIGHT)
            || decoder.color_type() != image::ColorType::Rgb8
        {
            return Err("PDF 页必须为 1240 × 1754 的 RGB JPEG".into());
        }
        drop(decoder);
        let page = 3 + self.pages * 3;
        let result = (|| -> std::io::Result<()> {
            self.object(page, format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595.28 841.89] /Resources << /XObject << /Im {} 0 R >> >> /Contents {} 0 R >>", page + 1, page + 2).as_bytes())?;
            let file = self.file.as_mut().unwrap();
            self.offsets.resize(page + 3, 0);
            self.offsets[page + 1] = file.stream_position()?;
            write!(file, "{} 0 obj\n<< /Type /XObject /Subtype /Image /Width {PAGE_WIDTH} /Height {PAGE_HEIGHT} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n", page + 1, jpeg.len())?;
            file.write_all(jpeg)?;
            file.write_all(b"\nendstream\nendobj\n")?;
            let drawing = "q\n595.28 0 0 841.89 0 0 cm\n/Im Do\nQ\n";
            self.object(
                page + 2,
                format!(
                    "<< /Length {} >>\nstream\n{}endstream",
                    drawing.len(),
                    drawing
                )
                .as_bytes(),
            )?;
            self.file.as_mut().unwrap().flush()
        })();
        if let Err(error) = result {
            self.poisoned = true;
            return Err(error.to_string());
        }
        self.pages += 1;
        Ok(())
    }

    pub fn finish(mut self) -> Result<String, String> {
        if self.pages == 0 || self.poisoned {
            return Err("不能保存空白或失败的 PDF 会话".into());
        }
        self.object(1, b"<< /Type /Catalog /Pages 2 0 R >>")
            .map_err(|e| e.to_string())?;
        let kids = (0..self.pages)
            .map(|i| format!("{} 0 R", 3 + i * 3))
            .collect::<Vec<_>>()
            .join(" ");
        self.object(
            2,
            format!("<< /Type /Pages /Count {} /Kids [{}] >>", self.pages, kids).as_bytes(),
        )
        .map_err(|e| e.to_string())?;
        let file = self.file.as_mut().unwrap();
        let xref = file.stream_position().map_err(|e| e.to_string())?;
        write!(
            file,
            "xref\n0 {}\n0000000000 65535 f \n",
            self.offsets.len()
        )
        .map_err(|e| e.to_string())?;
        for offset in &self.offsets[1..] {
            writeln!(file, "{offset:010} 00000 n ").map_err(|e| e.to_string())?;
        }
        write!(
            file,
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            self.offsets.len()
        )
        .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        self.file.take();
        // Same-directory hard link publishes the fully written file atomically,
        // with create-new semantics on macOS and Windows (no overwrite race).
        fs::hard_link(&self.temporary, &self.destination).map_err(|e| e.to_string())?;
        Ok(self.destination.to_string_lossy().into_owned())
    }

    pub fn cancel(mut self) -> Result<(), String> {
        self.file.take();
        match fs::remove_file(&self.temporary) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

impl Drop for PdfExport {
    fn drop(&mut self) {
        self.file.take();
        let _ = fs::remove_file(&self.temporary);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Run with LABFLOW_PDF_QA_DIR from scripts/pdf-export-smoke.ts"]
    fn render_visual_fixture() {
        let root = PathBuf::from(std::env::var("LABFLOW_PDF_QA_DIR").unwrap());
        let mut pages = fs::read_dir(&root)
            .unwrap()
            .map(|item| item.unwrap().path())
            .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("jpg"))
            .collect::<Vec<_>>();
        pages.sort();
        let path = root.join(format!("fixture-{}.pdf", pages.len()));
        let mut job = PdfExport::begin(&path).unwrap();
        for (sequence, page) in pages.iter().enumerate() {
            job.append(sequence, &fs::read(page).unwrap()).unwrap();
        }
        job.finish().unwrap();
        println!("Verified fixture: {}", path.display());
    }
    fn page() -> Vec<u8> {
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
            .encode(
                &vec![255; (PAGE_WIDTH * PAGE_HEIGHT * 3) as usize],
                PAGE_WIDTH,
                PAGE_HEIGHT,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
        jpeg
    }
    fn target() -> PathBuf {
        std::env::temp_dir().join(format!("labflow-pdf-{}.pdf", uuid::Uuid::new_v4()))
    }
    #[test]
    fn streams_pages_and_writes_correct_xref() {
        let path = target();
        let mut job = PdfExport::begin(&path).unwrap();
        let jpeg = page();
        for i in 0..100 {
            job.append(i, &jpeg).unwrap();
        }
        // Only three offsets per page remain in memory, never JPEG page data.
        assert_eq!(job.offsets.len(), 303);
        let offsets = job.offsets.clone();
        let temporary = job.temporary.clone();
        job.finish().unwrap();
        assert!(!temporary.exists());
        let bytes = fs::read(&path).unwrap();
        for (id, offset) in offsets.iter().enumerate().skip(3) {
            assert!(bytes[*offset as usize..].starts_with(format!("{id} 0 obj").as_bytes()));
        }
        assert!(bytes.ends_with(b"%%EOF\n"));
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn cancel_failure_and_destination_race_preserve_user_files() {
        let path = target();
        let mut job = PdfExport::begin(&path).unwrap();
        let temporary = job.temporary.clone();
        assert!(job.append(0, b"not a jpeg").is_err());
        assert!(job.append(1, &page()).is_err());
        drop(job);
        assert!(!temporary.exists());
        assert!(!path.exists());
        let mut job = PdfExport::begin(&path).unwrap();
        job.append(0, &page()).unwrap();
        fs::write(&path, b"user file").unwrap();
        assert!(job.finish().is_err());
        assert_eq!(fs::read(&path).unwrap(), b"user file");
        assert!(PdfExport::begin(&path).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_oversize_wrong_dimensions_page_and_document_limits() {
        let path = target();
        let mut job = PdfExport::begin(&path).unwrap();
        assert!(job.append(0, &vec![0; MAX_PAGE_BYTES + 1]).is_err());
        let mut tiny = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut tiny)
            .encode(&[0, 0, 0], 1, 1, image::ExtendedColorType::Rgb8)
            .unwrap();
        assert!(job.append(0, &tiny).is_err());
        job.pages = MAX_PAGES;
        assert!(job.append(MAX_PAGES, &page()).is_err());
        job.pages = 0;
        job.file
            .as_mut()
            .unwrap()
            .seek(std::io::SeekFrom::Start(MAX_DOCUMENT_BYTES))
            .unwrap();
        assert!(job.append(0, &page()).is_err());
        let temporary = job.temporary.clone();
        job.cancel().unwrap();
        assert!(!temporary.exists());
        assert!(!path.exists());
    }
}
