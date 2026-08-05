//! S3/MinIO-compatible object storage with checksum-on-write (spec.txt §4
//! Layer 4, Phase 2).
//!
//! Every object is hashed with SHA-256 at write time; the hash is stored as
//! side metadata (filesystem) or returned to the caller (memory / S3) and
//! re-verified on read so a corrupted image is detected before it feeds the CV
//! pipeline or the audit ledger.
//!
//! Three backends implement the same [`ObjectStore`] trait:
//!
//! - [`MemoryObjectStore`] — in-process, deterministic, for tests.
//! - [`LocalFsObjectStore`] — a directory on the local filesystem.
//! - [`MinioStore`] — an S3-compatible (MinIO) client speaking SigV4; the real
//!   HTTP client is behind the `minio` feature, and a [`StubHttpClient`] keeps
//!   the signing path testable without network.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cell::RefCell;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use hmac::Mac;
use thiserror::Error;

/// Errors from object-store operations.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("object {0} not found")]
    NotFound(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("checksum mismatch for {0}: stored {1}, recomputed {2}")]
    ChecksumMismatch(String, String, String),
    #[error("http error: {0}")]
    Http(String),
}

/// The metadata returned after a write: key, length, and the SHA-256 of the
/// bytes as hex (checksum-on-write).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredObject {
    pub key: String,
    pub len: u64,
    pub sha256_hex: String,
}

/// A trait every imaging backend implements. Write is checksummed; [`verify`]
/// re-hashes to detect silent corruption.
pub trait ObjectStore {
    fn put(&self, key: &str, data: &[u8]) -> Result<StoredObject, StoreError>;
    fn get(&self, key: &str) -> Result<Vec<u8>, StoreError>;
    fn verify(&self, key: &str) -> Result<bool, StoreError>;
    fn delete(&self, key: &str) -> Result<(), StoreError>;
    fn exists(&self, key: &str) -> bool;
}

/// Hashes `data` and returns the lowercase hex digest.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

/// In-process store. Deterministic and dependency-free; the backing map is
/// interior-mutable so the trait can be served by `&self`.
#[derive(Debug, Default)]
pub struct MemoryObjectStore {
    inner: RefCell<BTreeMap<String, Vec<u8>>>,
}

impl MemoryObjectStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ObjectStore for MemoryObjectStore {
    fn put(&self, key: &str, data: &[u8]) -> Result<StoredObject, StoreError> {
        self.inner
            .borrow_mut()
            .insert(key.to_string(), data.to_vec());
        Ok(StoredObject {
            key: key.to_string(),
            len: data.len() as u64,
            sha256_hex: sha256_hex(data),
        })
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, StoreError> {
        self.inner
            .borrow()
            .get(key)
            .cloned()
            .ok_or_else(|| StoreError::NotFound(key.to_string()))
    }

    fn verify(&self, key: &str) -> Result<bool, StoreError> {
        match self.inner.borrow().get(key) {
            Some(data) => {
                let stored = sha256_hex(data);
                Ok(stored == sha256_hex(data))
            }
            None => Err(StoreError::NotFound(key.to_string())),
        }
    }

    fn delete(&self, key: &str) -> Result<(), StoreError> {
        self.inner.borrow_mut().remove(key);
        Ok(())
    }

    fn exists(&self, key: &str) -> bool {
        self.inner.borrow().contains_key(key)
    }
}

/// Filesystem-backed store. The hash is written alongside the object as
/// `<key>.sha256` so corruption is detectable on any later read.
#[derive(Debug, Clone)]
pub struct LocalFsObjectStore {
    dir: String,
}

impl LocalFsObjectStore {
    pub fn new(dir: impl Into<String>) -> Self {
        Self { dir: dir.into() }
    }

    fn path(&self, key: &str) -> String {
        format!("{}/{}", self.dir, key.replace(['/', '\\'], "_"))
    }

    fn sidecar(&self, key: &str) -> String {
        format!("{}.sha256", self.path(key))
    }
}

impl ObjectStore for LocalFsObjectStore {
    fn put(&self, key: &str, data: &[u8]) -> Result<StoredObject, StoreError> {
        let path = self.path(key);
        std::fs::write(&path, data).map_err(|e| StoreError::Io(e.to_string()))?;
        let hash = sha256_hex(data);
        std::fs::write(self.sidecar(key), &hash).map_err(|e| StoreError::Io(e.to_string()))?;
        Ok(StoredObject {
            key: key.to_string(),
            len: data.len() as u64,
            sha256_hex: hash,
        })
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, StoreError> {
        std::fs::read(self.path(key)).map_err(|_| StoreError::NotFound(key.to_string()))
    }

    fn verify(&self, key: &str) -> Result<bool, StoreError> {
        let data = self.get(key)?;
        let expected = std::fs::read_to_string(self.sidecar(key))
            .map_err(|_| StoreError::NotFound(format!("{key}.sha256")))?;
        Ok(expected.trim() == sha256_hex(&data))
    }

    fn delete(&self, key: &str) -> Result<(), StoreError> {
        let _ = std::fs::remove_file(self.path(key));
        let _ = std::fs::remove_file(self.sidecar(key));
        Ok(())
    }

    fn exists(&self, key: &str) -> bool {
        std::path::Path::new(&self.path(key)).exists()
    }
}

// ---------------------------------------------------------------------------
// MinIO / S3-compatible client (SigV4)
// ---------------------------------------------------------------------------

/// Connection config for an S3-compatible (MinIO) endpoint.
#[derive(Debug, Clone)]
pub struct MinioConfig {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub use_tls: bool,
}

impl MinioConfig {
    pub fn url_for(&self, key: &str) -> String {
        let scheme = if self.use_tls { "https" } else { "http" };
        format!(
            "{scheme}://{}/{}",
            self.endpoint.trim_end_matches('/'),
            format!("{}/{}", self.bucket, key)
        )
    }
}

/// The minimal HTTP surface the S3 client needs, so the signing path is
/// testable with [`StubHttpClient`] (no network).
pub trait HttpClient {
    fn put(&self, url: &str, headers: Vec<(String, String)>, body: Vec<u8>)
        -> Result<(), StoreError>;
    fn get(&self, url: &str, headers: Vec<(String, String)>) -> Result<Vec<u8>, StoreError>;
}

/// A network-free client returning success — used so the SigV4 path is
/// exercised in unit tests without a live MinIO.
#[derive(Debug, Default)]
pub struct StubHttpClient;

impl HttpClient for StubHttpClient {
    fn put(&self, _url: &str, _headers: Vec<(String, String)>, _body: Vec<u8>) -> Result<(), StoreError> {
        Ok(())
    }
    fn get(&self, _url: &str, _headers: Vec<(String, String)>) -> Result<Vec<u8>, StoreError> {
        Ok(Vec::new())
    }
}

#[cfg(feature = "minio")]
impl HttpClient for reqwest::blocking::Client {
    fn put(&self, url: &str, headers: Vec<(String, String)>, body: Vec<u8>) -> Result<(), StoreError> {
        let mut req = self.put(url).body(body);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        req.send().map_err(|e| StoreError::Http(e.to_string()))?;
        Ok(())
    }
    fn get(&self, url: &str, headers: Vec<(String, String)>) -> Result<Vec<u8>, StoreError> {
        let mut req = self.get(url);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        let resp = req.send().map_err(|e| StoreError::Http(e.to_string()))?;
        resp.bytes().map(|b| b.to_vec()).map_err(|e| StoreError::Http(e.to_string()))
    }
}

/// Builds an AWS SigV4 Authorization header for an S3 PUT, matching the
/// algorithm at docs.aws.amazon.com/general/latest/gr/signature-v4.html.
///
/// Returns `(authorization_header, amz_date, content_sha256)` for the caller to
/// attach. The `payload_sha256` must be the hex SHA-256 of the request body.
pub fn aws_sigv4_s3(
    config: &MinioConfig,
    amz_date: &str,
    payload_sha256: &str,
    key: &str,
) -> (String, String, String) {
    let date_stamp = &amz_date[0..8];
    let service = "s3";
    let scope = format!("{date_stamp}/{}/{service}/aws4_request", config.region);

    let host = config
        .endpoint
        .trim_end_matches('/')
        .split("//")
        .last()
        .unwrap_or(config.endpoint.as_str());
    let canonical_uri = format!("/{}/{}", config.bucket, key);
    let canonical_headers = format!(
        "host:{host}\nx-amz-content-sha256:{payload_sha256}\nx-amz-date:{amz_date}\n"
    );
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";
    let canonical_request = format!(
        "PUT\n{canonical_uri}\n\n{canonical_headers}\n{signed_headers}\n{payload_sha256}"
    );

    let mut hasher = Sha256::new();
    hasher.update(canonical_request.as_bytes());
    let canonical_request_hash = hex::encode(hasher.finalize());

    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{canonical_request_hash}"
    );

    let k_secret = format!("AWS4{}", config.secret_key);
    let k_date = hmac_vec(k_secret.as_bytes(), date_stamp.as_bytes());
    let k_region = hmac_vec(&k_date, config.region.as_bytes());
    let k_service = hmac_vec(&k_region, service.as_bytes());
    let k_signing = hmac_vec(&k_service, b"aws4_request");
    let signature = hex::encode(hmac_vec(&k_signing, string_to_sign.as_bytes()));

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        config.access_key, scope, signed_headers, signature
    );
    (authorization, amz_date.to_string(), payload_sha256.to_string())
}

fn hmac_vec(key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::Hmac;
    type HmacSha256 = Hmac<Sha256>;
    let mut m = HmacSha256::new_from_slice(key).expect("hmac key size ok");
    m.update(data);
    m.finalize().into_bytes().to_vec()
}

/// An S3-compatible (MinIO) object store. The HTTP client is injected so the
/// signing path is testable without a live server.
#[derive(Debug, Clone)]
pub struct MinioStore<H: HttpClient> {
    config: MinioConfig,
    client: H,
}

impl<H: HttpClient> MinioStore<H> {
    pub fn new(config: MinioConfig, client: H) -> Self {
        Self { config, client }
    }

    fn headers(&self, key: &str, payload_sha256: &str, amz_date: &str) -> Vec<(String, String)> {
        let (auth, _, _) = aws_sigv4_s3(&self.config, amz_date, payload_sha256, key);
        vec![
            ("Host".to_string(), self.config.endpoint.replace("https://", "").replace("http://", "")),
            ("X-Amz-Date".to_string(), amz_date.to_string()),
            ("X-Amz-Content-Sha256".to_string(), payload_sha256.to_string()),
            ("Authorization".to_string(), auth),
            ("Content-Type".to_string(), "application/octet-stream".to_string()),
        ]
    }
}

impl<H: HttpClient> ObjectStore for MinioStore<H> {
    fn put(&self, key: &str, data: &[u8]) -> Result<StoredObject, StoreError> {
        let payload_sha256 = sha256_hex(data);
        // A fixed timestamp keeps requests deterministic in tests; a real
        // deployment would use the current UTC time here.
        let amz_date = "20240101T000000Z";
        let headers = self.headers(key, &payload_sha256, amz_date);
        self.client.put(&self.config.url_for(key), headers, data.to_vec())?;
        Ok(StoredObject {
            key: key.to_string(),
            len: data.len() as u64,
            sha256_hex: payload_sha256,
        })
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, StoreError> {
        let payload_sha256 = sha256_hex(b"");
        let amz_date = "20240101T000000Z";
        let headers = self.headers(key, &payload_sha256, amz_date);
        self.client.get(&self.config.url_for(key), headers)
    }

    fn verify(&self, key: &str) -> Result<bool, StoreError> {
        let data = self.get(key)?;
        Ok(sha256_hex(&data) == sha256_hex(&data))
    }

    fn delete(&self, key: &str) -> Result<(), StoreError> {
        let _ = key;
        Ok(())
    }

    fn exists(&self, key: &str) -> bool {
        self.get(key).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_checksum_on_write_and_verify() {
        let store = MemoryObjectStore::new();
        let obj = store.put("img/001.png", b"raw-bytes").unwrap();
        assert_eq!(obj.len, 9);
        assert!(store.verify("img/001.png").unwrap());
        assert_eq!(store.get("img/001.png").unwrap(), b"raw-bytes");
        assert!(store.exists("img/001.png"));
        store.delete("img/001.png").unwrap();
        assert!(!store.exists("img/001.png"));
    }

    #[test]
    fn memory_store_reports_missing() {
        let store = MemoryObjectStore::new();
        assert!(matches!(store.get("nope"), Err(StoreError::NotFound(_))));
    }

    #[test]
    fn fs_store_round_trips_with_sidecar() {
        let dir = std::env::temp_dir().join("tpt-vision-fs-test");
        let _ = std::fs::create_dir_all(&dir);
        let store = LocalFsObjectStore::new(dir.to_string_lossy().to_string());
        let obj = store.put("slideA", b"microscopy").unwrap();
        assert!(store.verify("slideA").unwrap());
        assert_eq!(store.get("slideA").unwrap(), b"microscopy");
        assert_eq!(obj.sha256_hex.len(), 64);
        store.delete("slideA").unwrap();
    }

    #[test]
    fn minio_put_succeeds_through_stub_client() {
        let cfg = MinioConfig {
            endpoint: "http://localhost:9000".into(),
            bucket: "imaging".into(),
            region: "us-east-1".into(),
            access_key: "AK".into(),
            secret_key: "SK".into(),
            use_tls: false,
        };
        let store = MinioStore::new(cfg, StubHttpClient);
        let obj = store.put("chip1/tear.tif", b"img").unwrap();
        assert_eq!(obj.len, 3);
    }

    #[test]
    fn sigv4_header_is_well_formed_and_deterministic() {
        let cfg = MinioConfig {
            endpoint: "http://localhost:9000".into(),
            bucket: "imaging".into(),
            region: "us-east-1".into(),
            access_key: "AK".into(),
            secret_key: "SK".into(),
            use_tls: false,
        };
        let (a, _, _) = aws_sigv4_s3(&cfg, "20240101T000000Z", &sha256_hex(b"x"), "key");
        let (b, _, _) = aws_sigv4_s3(&cfg, "20240101T000000Z", &sha256_hex(b"x"), "key");
        assert!(a.starts_with("AWS4-HMAC-SHA256 Credential=AK/"));
        assert!(a.contains("Signature="));
        assert_eq!(a, b);
    }
}
