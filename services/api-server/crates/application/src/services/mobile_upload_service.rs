//! 移动端上传服务。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{Datelike, Utc};
use sha2::{Digest, Sha256};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use fms_domain::error::DomainError;
use fms_domain::models::mobile::MobileUploadAsset;
use fms_domain::ports::mobile_repository::MobileUploadRepository;

/// Unified upload source — route layer chooses which variant to pass.
/// For large files the route should write to a temp file and pass `TempFile`,
/// keeping the application layer memory-free for large uploads.
#[derive(Debug)]
pub enum UploadSource {
    InMemory(Vec<u8>),
    TempFile(PathBuf),
}

impl UploadSource {
    pub async fn into_bytes(self) -> Result<Vec<u8>, DomainError> {
        match self {
            UploadSource::InMemory(bytes) => Ok(bytes),
            UploadSource::TempFile(path) => fs::read(&path)
                .await
                .map_err(|error| DomainError::Internal(error.to_string())),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            UploadSource::InMemory(bytes) => bytes.is_empty(),
            UploadSource::TempFile(_) => false,
        }
    }

    pub fn len(&self) -> Option<usize> {
        match self {
            UploadSource::InMemory(bytes) => Some(bytes.len()),
            UploadSource::TempFile(_) => None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MobileUploadResponse {
    pub upload_id: String,
    pub original_filename: String,
    pub content_type: Option<String>,
    pub file_size: i64,
    pub checksum_sha256: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub attachment_url: String,
    pub metadata: HashMap<String, serde_json::Value>,
}

pub struct MobileUploadService {
    repo: Arc<dyn MobileUploadRepository + Send + Sync>,
    storage_root: PathBuf,
    max_bytes: usize,
}

const HEADER_READ_BYTES: usize = 16;
const CONTENT_TYPE_MISMATCH_MESSAGE: &str = "file content does not match declared type";

impl MobileUploadService {
    pub fn new(
        repo: Arc<dyn MobileUploadRepository + Send + Sync>,
        storage_root: impl Into<PathBuf>,
        max_file_size_mb: usize,
    ) -> Self {
        Self {
            repo,
            storage_root: storage_root.into(),
            max_bytes: max_file_size_mb.max(1) * 1024 * 1024,
        }
    }

    pub async fn save_upload(
        &self,
        user_id: &str,
        file_name: &str,
        content_type: Option<&str>,
        source: UploadSource,
        category: &str,
        metadata: HashMap<String, serde_json::Value>,
    ) -> Result<MobileUploadResponse, DomainError> {
        let normalized_user_id = normalize_required(user_id, "user_id")?;
        if source.is_empty() {
            return Err(DomainError::ValidationError("upload file is required".into()));
        }

        let original_filename = sanitize_filename(file_name);
        let extension = pick_extension(&original_filename, content_type);
        let now = Utc::now();
        let upload_id = ulid::Ulid::new().to_string();
        let relative_storage_path = PathBuf::from(format!(
            "{}/{:02}/{:02}/{}{}",
            now.year(),
            now.month(),
            now.day(),
            upload_id,
            extension
        ));
        let absolute_storage_path = self.storage_root.join(&relative_storage_path);
        if let Some(parent) = absolute_storage_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|error| DomainError::Internal(error.to_string()))?;
        }

        let (file_size, checksum_sha256) = match source {
            UploadSource::InMemory(bytes) => {
                if bytes.len() > self.max_bytes {
                    return Err(DomainError::ValidationError(format!(
                        "file exceeds max size limit ({} bytes)",
                        self.max_bytes
                    )));
                }
                validate_upload_magic(&extension, &bytes)?;
                let mut file = fs::File::create(&absolute_storage_path)
                    .await
                    .map_err(|error| DomainError::Internal(error.to_string()))?;
                if let Err(error) = file.write_all(&bytes).await {
                    let _ = fs::remove_file(&absolute_storage_path).await;
                    return Err(DomainError::Internal(error.to_string()));
                }
                let mut hasher = Sha256::new();
                hasher.update(&bytes);
                let checksum = format!("{:x}", hasher.finalize());
                (bytes.len() as i64, Some(checksum))
            }
            UploadSource::TempFile(temp_path) => {
                let temp_metadata = fs::metadata(&temp_path)
                    .await
                    .map_err(|error| DomainError::Internal(error.to_string()))?;
                if !temp_metadata.is_file() {
                    return Err(DomainError::ValidationError("upload file is required".into()));
                }
                if temp_metadata.len() == 0 {
                    return Err(DomainError::ValidationError("upload file is required".into()));
                }
                if temp_metadata.len() > self.max_bytes as u64 {
                    return Err(DomainError::ValidationError(format!(
                        "file exceeds max size limit ({} bytes)",
                        self.max_bytes
                    )));
                }
                let header = read_file_header(&temp_path).await?;
                validate_upload_magic(&extension, &header)?;
                // Atomically move temp file to storage location
                if let Err(error) = fs::rename(&temp_path, &absolute_storage_path).await {
                    return Err(DomainError::Internal(error.to_string()));
                }
                match checksum_file_with_limit(&absolute_storage_path, self.max_bytes).await {
                    Ok((file_size, checksum)) => (file_size, Some(checksum)),
                    Err(error) => {
                        let _ = fs::remove_file(&absolute_storage_path).await;
                        return Err(error);
                    }
                }
            }
        };

        let mut persisted_metadata = HashMap::from([(
            "category".to_string(),
            serde_json::Value::String(
                normalize_optional(Some(category), 64).unwrap_or_else(|| "dispatch_issue".to_string()),
            ),
        )]);
        persisted_metadata.extend(metadata);

        let asset = MobileUploadAsset {
            upload_id: upload_id.clone(),
            user_id: normalized_user_id,
            storage_key: relative_storage_path.to_string_lossy().replace('\\', "/"),
            original_filename,
            content_type: normalize_optional(content_type, 128),
            file_size,
            checksum_sha256,
            created_at: now,
            metadata: persisted_metadata,
        };
        let saved = match self.repo.create(&asset).await {
            Ok(saved) => saved,
            Err(error) => {
                let _ = fs::remove_file(&absolute_storage_path).await;
                return Err(error);
            }
        };
        Ok(MobileUploadResponse {
            upload_id: saved.upload_id.clone(),
            original_filename: saved.original_filename.clone(),
            content_type: saved.content_type.clone(),
            file_size: saved.file_size,
            checksum_sha256: saved.checksum_sha256.clone(),
            created_at: saved.created_at,
            attachment_url: format!("/api/v2/mobile/uploads/{}/content", saved.upload_id),
            metadata: saved.metadata.clone(),
        })
    }

    pub async fn resolve_content_path(
        &self,
        upload_id: &str,
    ) -> Result<Option<(MobileUploadAsset, PathBuf)>, DomainError> {
        let normalized_upload_id = normalize_required(upload_id, "upload_id")?;
        let Some(asset) = self.repo.get_by_id(&normalized_upload_id).await? else {
            return Ok(None);
        };
        let path = self.storage_root.join(&asset.storage_key);
        let canonical_root = self
            .storage_root
            .canonicalize()
            .unwrap_or_else(|_| self.storage_root.clone());
        let canonical_candidate = path.canonicalize().unwrap_or(path.clone());
        if !canonical_candidate.starts_with(&canonical_root) {
            return Ok(None);
        }
        match fs::metadata(&canonical_candidate).await {
            Ok(metadata) if metadata.is_file() => Ok(Some((asset, canonical_candidate))),
            _ => Ok(None),
        }
    }

    pub fn build_download_filename(&self, asset: &MobileUploadAsset) -> String {
        let candidate = asset.original_filename.trim();
        if !candidate.is_empty() {
            return candidate.to_string();
        }
        let extension = Path::new(&asset.storage_key)
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| format!(".{value}"))
            .unwrap_or_else(|| ".bin".to_string());
        format!("{}{}", asset.upload_id, extension)
    }
}

fn validate_upload_magic(extension: &str, header: &[u8]) -> Result<(), DomainError> {
    let matches_declared_type = match extension {
        ".jpg" | ".jpeg" => is_jpeg(header),
        ".png" => header.starts_with(b"\x89PNG\r\n\x1a\n"),
        ".webp" => is_riff_container(header, b"WEBP"),
        ".pdf" => header.starts_with(b"%PDF-"),
        ".mp4" | ".m4a" => is_iso_base_media(header),
        ".mp3" => is_mp3(header),
        ".wav" => is_riff_container(header, b"WAVE"),
        ".aac" => is_aac(header),
        ".txt" | ".log" => looks_like_text(header),
        ".json" => looks_like_json(header),
        ".bin" => true,
        _ => false,
    };

    if matches_declared_type {
        Ok(())
    } else {
        Err(DomainError::ValidationError(CONTENT_TYPE_MISMATCH_MESSAGE.to_string()))
    }
}

fn is_jpeg(header: &[u8]) -> bool {
    header.len() >= 3 && header[0] == 0xff && header[1] == 0xd8 && header[2] == 0xff
}

fn is_riff_container(header: &[u8], format: &[u8; 4]) -> bool {
    header.len() >= 12 && header.starts_with(b"RIFF") && &header[8..12] == format
}

fn is_iso_base_media(header: &[u8]) -> bool {
    header.len() >= 12 && &header[4..8] == b"ftyp"
}

fn is_mp3(header: &[u8]) -> bool {
    header.starts_with(b"ID3") || (header.len() >= 2 && header[0] == 0xff && (header[1] & 0xe0) == 0xe0)
}

fn is_aac(header: &[u8]) -> bool {
    header.len() >= 2 && header[0] == 0xff && matches!(header[1], 0xf1 | 0xf9)
}

fn looks_like_text(header: &[u8]) -> bool {
    std::str::from_utf8(header).is_ok()
        && !header
            .iter()
            .any(|byte| matches!(*byte, 0x00..=0x08 | 0x0b | 0x0c | 0x0e..=0x1f))
}

fn looks_like_json(header: &[u8]) -> bool {
    looks_like_text(header)
        && header
            .iter()
            .copied()
            .find(|byte| !byte.is_ascii_whitespace())
            .is_some_and(|byte| matches!(byte, b'{' | b'['))
}

fn normalize_required(value: &str, field_name: &str) -> Result<String, DomainError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(DomainError::ValidationError(format!("{field_name} is required")));
    }
    Ok(normalized.to_string())
}

fn normalize_optional(value: Option<&str>, max_length: usize) -> Option<String> {
    let text = value.unwrap_or("").trim();
    if text.is_empty() {
        return None;
    }
    Some(text.chars().take(max_length).collect())
}

fn sanitize_filename(filename: &str) -> String {
    let raw = filename.trim();
    let fallback = if raw.is_empty() { "upload.bin" } else { raw };
    let basename = fallback.rsplit(['/', '\\']).next().unwrap_or("upload.bin");
    let sanitized = basename
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches(|ch| ch == '.' || ch == '_')
        .to_string();
    if sanitized.is_empty() {
        "upload.bin".to_string()
    } else {
        sanitized
    }
}

fn pick_extension(filename: &str, content_type: Option<&str>) -> String {
    let suffix = Path::new(filename)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{}", value.to_ascii_lowercase()))
        .unwrap_or_default();
    let allowed = [
        ".jpg", ".jpeg", ".png", ".webp", ".mp4", ".mp3", ".wav", ".aac", ".m4a", ".pdf", ".txt", ".log", ".json",
        ".bin",
    ];
    if allowed.contains(&suffix.as_str()) {
        return suffix;
    }
    match content_type.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "image/jpeg" | "image/jpg" => ".jpg".to_string(),
        "image/png" => ".png".to_string(),
        "image/webp" => ".webp".to_string(),
        "video/mp4" => ".mp4".to_string(),
        "audio/mpeg" | "audio/mp3" => ".mp3".to_string(),
        "audio/wav" => ".wav".to_string(),
        "application/pdf" => ".pdf".to_string(),
        "text/plain" => ".txt".to_string(),
        _ => ".bin".to_string(),
    }
}

async fn checksum_file_with_limit(path: &Path, max_bytes: usize) -> Result<(i64, String), DomainError> {
    let mut file = fs::File::open(path)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
    let mut hasher = Sha256::new();
    let mut total_size = 0_u64;
    let max_bytes = max_bytes as u64;
    let mut buffer = vec![0_u8; 64 * 1024];

    loop {
        let read_size = file
            .read(&mut buffer)
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        if read_size == 0 {
            break;
        }
        total_size += read_size as u64;
        if total_size > max_bytes {
            return Err(DomainError::ValidationError(format!(
                "file exceeds max size limit ({max_bytes} bytes)"
            )));
        }
        hasher.update(&buffer[..read_size]);
    }

    Ok((total_size as i64, format!("{:x}", hasher.finalize())))
}

async fn read_file_header(path: &Path) -> Result<Vec<u8>, DomainError> {
    let mut file = fs::File::open(path)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
    let mut header = vec![0_u8; HEADER_READ_BYTES];
    let read_size = file
        .read(&mut header)
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))?;
    header.truncate(read_size);
    Ok(header)
}

#[cfg(test)]
mod tests {
    use super::{MobileUploadService, UploadSource};
    use async_trait::async_trait;
    use fms_domain::error::DomainError;
    use fms_domain::models::mobile::MobileUploadAsset;
    use fms_domain::ports::mobile_repository::MobileUploadRepository;
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::fs;
    use tokio::io::AsyncWriteExt;

    struct FakeUploadRepository {
        fail_create: bool,
        create_calls: AtomicUsize,
    }

    impl FakeUploadRepository {
        fn new(fail_create: bool) -> Self {
            Self {
                fail_create,
                create_calls: AtomicUsize::new(0),
            }
        }

        fn create_calls(&self) -> usize {
            self.create_calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl MobileUploadRepository for FakeUploadRepository {
        async fn create(&self, item: &MobileUploadAsset) -> Result<MobileUploadAsset, DomainError> {
            self.create_calls.fetch_add(1, Ordering::SeqCst);
            if self.fail_create {
                return Err(DomainError::Internal("database insert failed".into()));
            }
            Ok(item.clone())
        }

        async fn get_by_id(&self, _upload_id: &str) -> Result<Option<MobileUploadAsset>, DomainError> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn save_upload_rejects_jpeg_claim_with_non_jpeg_bytes_before_repo_create() {
        let storage_root = unique_temp_path("mobile-upload-storage");
        let repo = Arc::new(FakeUploadRepository::new(false));
        let service = MobileUploadService::new(repo.clone(), &storage_root, 1);

        let result = service
            .save_upload(
                "user-1",
                "photo.jpg",
                Some("image/jpeg"),
                UploadSource::InMemory(b"<script>alert('not an image')</script>".to_vec()),
                "dispatch_issue",
                HashMap::new(),
            )
            .await;

        assert!(
            matches!(result, Err(DomainError::ValidationError(message)) if message == "file content does not match declared type")
        );
        assert_eq!(repo.create_calls(), 0);
        assert_eq!(count_files(&storage_root), 0);

        let _ = fs::remove_dir_all(&storage_root).await;
    }

    #[tokio::test]
    async fn save_upload_rejects_temp_jpeg_claim_with_non_jpeg_bytes_before_repo_create() {
        let storage_root = unique_temp_path("mobile-upload-storage");
        let temp_path = unique_temp_path("mobile-upload-source.jpg");
        write_file(&temp_path, b"not really a jpeg").await;
        let repo = Arc::new(FakeUploadRepository::new(false));
        let service = MobileUploadService::new(repo.clone(), &storage_root, 1);

        let result = service
            .save_upload(
                "user-1",
                "photo.jpg",
                Some("image/jpeg"),
                UploadSource::TempFile(temp_path.clone()),
                "dispatch_issue",
                HashMap::new(),
            )
            .await;

        assert!(
            matches!(result, Err(DomainError::ValidationError(message)) if message == "file content does not match declared type")
        );
        assert_eq!(repo.create_calls(), 0);
        assert!(temp_path.exists());
        assert_eq!(count_files(&storage_root), 0);

        let _ = fs::remove_file(&temp_path).await;
        let _ = fs::remove_dir_all(&storage_root).await;
    }

    #[tokio::test]
    async fn save_upload_accepts_jpeg_when_magic_bytes_match() {
        let storage_root = unique_temp_path("mobile-upload-storage");
        let repo = Arc::new(FakeUploadRepository::new(false));
        let service = MobileUploadService::new(repo.clone(), &storage_root, 1);

        let result = service
            .save_upload(
                "user-1",
                "photo.jpg",
                Some("image/jpeg"),
                UploadSource::InMemory(vec![0xff, 0xd8, 0xff, 0xe0, b'f', b'm', b's']),
                "dispatch_issue",
                HashMap::new(),
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(repo.create_calls(), 1);
        assert_eq!(count_files(&storage_root), 1);

        let _ = fs::remove_dir_all(&storage_root).await;
    }

    #[tokio::test]
    async fn save_upload_removes_persisted_temp_file_when_repo_create_fails() {
        let storage_root = unique_temp_path("mobile-upload-storage");
        let temp_path = unique_temp_path("mobile-upload-source.txt");
        write_file(&temp_path, b"mobile upload payload").await;
        let repo = Arc::new(FakeUploadRepository::new(true));
        let service = MobileUploadService::new(repo.clone(), &storage_root, 1);

        let result = service
            .save_upload(
                "user-1",
                "note.txt",
                Some("text/plain"),
                UploadSource::TempFile(temp_path.clone()),
                "dispatch_issue",
                HashMap::new(),
            )
            .await;

        assert!(matches!(result, Err(DomainError::Internal(_))));
        assert_eq!(repo.create_calls(), 1);
        assert!(!temp_path.exists());
        assert_eq!(count_files(&storage_root), 0);

        let _ = fs::remove_dir_all(&storage_root).await;
    }

    #[tokio::test]
    async fn save_upload_rejects_oversized_temp_file_before_repo_create() {
        let storage_root = unique_temp_path("mobile-upload-storage");
        let temp_path = unique_temp_path("mobile-upload-source.bin");
        let oversized = vec![b'x'; (1024 * 1024) + 1];
        write_file(&temp_path, &oversized).await;
        let repo = Arc::new(FakeUploadRepository::new(false));
        let service = MobileUploadService::new(repo.clone(), &storage_root, 1);

        let result = service
            .save_upload(
                "user-1",
                "large.bin",
                Some("application/octet-stream"),
                UploadSource::TempFile(temp_path.clone()),
                "dispatch_issue",
                HashMap::new(),
            )
            .await;

        assert!(matches!(result, Err(DomainError::ValidationError(_))));
        assert_eq!(repo.create_calls(), 0);
        assert!(temp_path.exists());
        assert_eq!(count_files(&storage_root), 0);

        let _ = fs::remove_file(&temp_path).await;
        let _ = fs::remove_dir_all(&storage_root).await;
    }

    async fn write_file(path: &Path, contents: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.unwrap();
        }
        let mut file = fs::File::create(path).await.unwrap();
        file.write_all(contents).await.unwrap();
    }

    fn unique_temp_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("{label}-{}", ulid::Ulid::new()))
    }

    fn count_files(path: &Path) -> usize {
        let Ok(entries) = std::fs::read_dir(path) else {
            return 0;
        };
        entries
            .filter_map(Result::ok)
            .map(|entry| {
                let entry_path = entry.path();
                if entry_path.is_dir() {
                    count_files(&entry_path)
                } else if entry_path.is_file() {
                    1
                } else {
                    0
                }
            })
            .sum()
    }
}
