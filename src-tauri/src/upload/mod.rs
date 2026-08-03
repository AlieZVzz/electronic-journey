use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use russh::{
    client,
    keys::{HashAlg, PrivateKeyWithHashAlg, PublicKey},
    Disconnect,
};
use russh_sftp::{
    client::{error::Error as SftpError, fs::Metadata, SftpSession},
    protocol::StatusCode,
};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;
use zeroize::Zeroize;

use crate::database::RemoteProfileRecord;

const PROFILE_ID: &str = "primary";
const KEYRING_SERVICE: &str = "com.electronicjourney.app.ssh";
const MAX_PRIVATE_KEY_BYTES: u64 = 64 * 1024;
const MAX_DIAGNOSTIC_BATCHES: usize = 32;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveRemoteProfileInput {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub private_key_path: String,
    pub private_key_passphrase: Option<String>,
    pub host_key_fingerprint: String,
    pub remote_root: String,
    pub auto_sync_enabled: bool,
    pub sync_interval_minutes: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteProfile {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub private_key_path: String,
    pub host_key_fingerprint: String,
    pub remote_root: String,
    pub has_passphrase: bool,
    pub auto_sync_enabled: bool,
    pub sync_interval_minutes: u16,
    pub next_auto_sync_at_utc: Option<String>,
    pub last_auto_sync_attempt_at_utc: Option<String>,
    pub last_auto_sync_state: Option<String>,
    pub last_auto_sync_completed_items: usize,
    pub last_auto_sync_failed_items: usize,
    pub auto_sync_suspended_reason: Option<String>,
}

impl TryFrom<RemoteProfileRecord> for RemoteProfile {
    type Error = UploadError;

    fn try_from(value: RemoteProfileRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            name: value.name,
            host: value.host,
            port: u16::try_from(value.port).map_err(|_| UploadError::InvalidProfile)?,
            username: value.username,
            private_key_path: value.private_key_path,
            host_key_fingerprint: value.host_key_fingerprint,
            remote_root: value.remote_root,
            has_passphrase: value.has_passphrase,
            auto_sync_enabled: value.auto_sync_enabled,
            sync_interval_minutes: u16::try_from(value.sync_interval_minutes)
                .map_err(|_| UploadError::InvalidProfile)?,
            next_auto_sync_at_utc: value.next_auto_sync_at_utc.map(|date| date.to_rfc3339()),
            last_auto_sync_attempt_at_utc: value
                .last_auto_sync_attempt_at_utc
                .map(|date| date.to_rfc3339()),
            last_auto_sync_state: value.last_auto_sync_state,
            last_auto_sync_completed_items: usize::try_from(value.last_auto_sync_completed_items)
                .map_err(|_| UploadError::InvalidProfile)?,
            last_auto_sync_failed_items: usize::try_from(value.last_auto_sync_failed_items)
                .map_err(|_| UploadError::InvalidProfile)?,
            auto_sync_suspended_reason: value.auto_sync_suspended_reason,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteConnectionTest {
    pub remote_root: String,
    pub writable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadItemProgress {
    pub capture_id: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadBatchProgress {
    pub batch_id: String,
    pub state: String,
    pub total_items: usize,
    pub total_bytes: u64,
    pub uploaded_items: usize,
    pub failed_items: usize,
    pub items: Vec<UploadItemProgress>,
    pub last_error: Option<String>,
    pub performance: UploadPerformanceSnapshot,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadPerformanceSnapshot {
    pub phase: String,
    pub uploaded_bytes: u64,
    pub bytes_per_second: u64,
    pub estimated_remaining_seconds: Option<u64>,
    pub connection_ms: u64,
    pub authentication_ms: u64,
    pub sftp_initialization_ms: u64,
    pub local_validation_ms: u64,
    pub transfer_bytes: u64,
    pub transfer_ms: u64,
    pub remote_metadata_operations: u64,
    pub remote_metadata_ms: u64,
}

#[derive(Debug, Clone, Copy)]
enum UploadPhase {
    Pending,
    Connecting,
    Authenticating,
    InitializingSftp,
    ValidatingLocal,
    PreparingRemote,
    Transferring,
    VerifyingRemote,
    Completed,
    Failed,
}

impl UploadPhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Connecting => "connecting",
            Self::Authenticating => "authenticating",
            Self::InitializingSftp => "initializing_sftp",
            Self::ValidatingLocal => "validating_local",
            Self::PreparingRemote => "preparing_remote",
            Self::Transferring => "transferring",
            Self::VerifyingRemote => "verifying_remote",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug)]
struct BatchPerformance {
    total_bytes: u64,
    phase: UploadPhase,
    completed_bytes: u64,
    current_item_bytes: u64,
    connection: Duration,
    authentication: Duration,
    sftp_initialization: Duration,
    local_validation: Duration,
    transfer_bytes: u64,
    transfer: Duration,
    remote_metadata_operations: u64,
    remote_metadata: Duration,
}

impl BatchPerformance {
    fn new(total_bytes: u64) -> Self {
        Self {
            total_bytes,
            phase: UploadPhase::Pending,
            completed_bytes: 0,
            current_item_bytes: 0,
            connection: Duration::ZERO,
            authentication: Duration::ZERO,
            sftp_initialization: Duration::ZERO,
            local_validation: Duration::ZERO,
            transfer_bytes: 0,
            transfer: Duration::ZERO,
            remote_metadata_operations: 0,
            remote_metadata: Duration::ZERO,
        }
    }

    fn snapshot(&self) -> UploadPerformanceSnapshot {
        let uploaded_bytes = self
            .completed_bytes
            .saturating_add(self.current_item_bytes)
            .min(self.total_bytes);
        let bytes_per_second = if self.transfer.is_zero() {
            0
        } else {
            (self.transfer_bytes as f64 / self.transfer.as_secs_f64()).round() as u64
        };
        let estimated_remaining_seconds = if bytes_per_second == 0
            || matches!(self.phase, UploadPhase::Completed | UploadPhase::Failed)
        {
            None
        } else {
            Some(
                self.total_bytes
                    .saturating_sub(uploaded_bytes)
                    .div_ceil(bytes_per_second),
            )
        };
        UploadPerformanceSnapshot {
            phase: self.phase.as_str().to_string(),
            uploaded_bytes,
            bytes_per_second,
            estimated_remaining_seconds,
            connection_ms: duration_millis(self.connection),
            authentication_ms: duration_millis(self.authentication),
            sftp_initialization_ms: duration_millis(self.sftp_initialization),
            local_validation_ms: duration_millis(self.local_validation),
            transfer_bytes: self.transfer_bytes,
            transfer_ms: duration_millis(self.transfer),
            remote_metadata_operations: self.remote_metadata_operations,
            remote_metadata_ms: duration_millis(self.remote_metadata),
        }
    }
}

#[derive(Default)]
struct DiagnosticsInner {
    batches: HashMap<Uuid, BatchPerformance>,
    order: VecDeque<Uuid>,
}

#[derive(Default)]
pub struct UploadDiagnosticsRegistry {
    inner: Mutex<DiagnosticsInner>,
}

impl UploadDiagnosticsRegistry {
    pub fn begin(&self, batch_id: Uuid, total_bytes: u64) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };
        if inner.batches.contains_key(&batch_id) {
            return;
        }
        while inner.order.len() >= MAX_DIAGNOSTIC_BATCHES {
            if let Some(expired) = inner.order.pop_front() {
                inner.batches.remove(&expired);
            }
        }
        inner.order.push_back(batch_id);
        inner
            .batches
            .insert(batch_id, BatchPerformance::new(total_bytes));
    }

    fn update(&self, batch_id: Uuid, update: impl FnOnce(&mut BatchPerformance)) {
        if let Ok(mut inner) = self.inner.lock() {
            if let Some(batch) = inner.batches.get_mut(&batch_id) {
                update(batch);
            }
        }
    }

    pub fn set_phase(&self, batch_id: Uuid, phase: &'static str) {
        self.update(batch_id, |batch| {
            batch.phase = match phase {
                "connecting" => UploadPhase::Connecting,
                "authenticating" => UploadPhase::Authenticating,
                "initializing_sftp" => UploadPhase::InitializingSftp,
                "validating_local" => UploadPhase::ValidatingLocal,
                "preparing_remote" => UploadPhase::PreparingRemote,
                "transferring" => UploadPhase::Transferring,
                "verifying_remote" => UploadPhase::VerifyingRemote,
                "completed" => UploadPhase::Completed,
                "failed" => UploadPhase::Failed,
                _ => UploadPhase::Pending,
            };
        });
    }

    pub fn record_connection(&self, batch_id: Uuid, progress: ConnectionProgress) {
        self.update(batch_id, |batch| match progress {
            ConnectionProgress::Connected(duration) => {
                batch.connection = batch.connection.saturating_add(duration);
                batch.phase = UploadPhase::Authenticating;
            }
            ConnectionProgress::Authenticated(duration) => {
                batch.authentication = batch.authentication.saturating_add(duration);
                batch.phase = UploadPhase::InitializingSftp;
            }
            ConnectionProgress::SftpInitialized(duration) => {
                batch.sftp_initialization = batch.sftp_initialization.saturating_add(duration);
            }
        });
    }

    pub fn record_local_validation(&self, batch_id: Uuid, duration: Duration) {
        self.update(batch_id, |batch| {
            batch.local_validation = batch.local_validation.saturating_add(duration);
        });
    }

    pub fn record_upload_event(
        &self,
        batch_id: Uuid,
        event: UploadProgressEvent,
        previous_transfer_bytes: u64,
        previous_transfer: Duration,
    ) {
        self.update(batch_id, |batch| match event {
            UploadProgressEvent::PreparingRemote => {
                batch.phase = UploadPhase::PreparingRemote;
                batch.current_item_bytes = 0;
            }
            UploadProgressEvent::Transferred { bytes, elapsed } => {
                batch.phase = UploadPhase::Transferring;
                batch.current_item_bytes = bytes;
                batch.transfer_bytes = previous_transfer_bytes.saturating_add(bytes);
                batch.transfer = previous_transfer.saturating_add(elapsed);
            }
            UploadProgressEvent::VerifyingRemote => {
                batch.phase = UploadPhase::VerifyingRemote;
            }
        });
    }

    pub fn record_session(&self, batch_id: Uuid, performance: SessionPerformance) {
        self.update(batch_id, |batch| {
            batch.transfer_bytes = performance.transfer_bytes;
            batch.transfer = performance.transfer;
            batch.remote_metadata_operations = performance.remote_metadata_operations;
            batch.remote_metadata = performance.remote_metadata;
        });
    }

    pub fn complete_item(&self, batch_id: Uuid, file_size: u64) {
        self.update(batch_id, |batch| {
            batch.completed_bytes = batch.completed_bytes.saturating_add(file_size);
            batch.current_item_bytes = 0;
        });
    }

    pub fn fail_item(&self, batch_id: Uuid) {
        self.update(batch_id, |batch| {
            batch.current_item_bytes = 0;
        });
    }

    pub fn finish(&self, batch_id: Uuid, failed: bool) {
        self.update(batch_id, |batch| {
            batch.current_item_bytes = 0;
            batch.phase = if failed {
                UploadPhase::Failed
            } else {
                UploadPhase::Completed
            };
        });
    }

    pub fn snapshot(
        &self,
        batch_id: Uuid,
        total_bytes: u64,
        uploaded_bytes: u64,
        state: &str,
    ) -> UploadPerformanceSnapshot {
        if let Ok(inner) = self.inner.lock() {
            if let Some(batch) = inner.batches.get(&batch_id) {
                return batch.snapshot();
            }
        }
        let mut fallback = BatchPerformance::new(total_bytes);
        fallback.completed_bytes = uploaded_bytes;
        fallback.phase = match state {
            "completed" => UploadPhase::Completed,
            "partial_failed" | "cancelled" => UploadPhase::Failed,
            _ => UploadPhase::Pending,
        };
        fallback.snapshot()
    }
}

fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[derive(Debug, Clone, Copy)]
pub enum ConnectionProgress {
    Connected(Duration),
    Authenticated(Duration),
    SftpInitialized(Duration),
}

#[derive(Debug, Clone, Copy)]
pub enum UploadProgressEvent {
    PreparingRemote,
    Transferred { bytes: u64, elapsed: Duration },
    VerifyingRemote,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SessionPerformance {
    pub transfer_bytes: u64,
    pub transfer: Duration,
    pub remote_metadata_operations: u64,
    pub remote_metadata: Duration,
}

#[derive(Debug, Error)]
pub enum UploadError {
    #[error("remote profile is invalid")]
    InvalidProfile,
    #[error("private key path is outside the allowed directory")]
    InvalidKeyPath,
    #[error("private key file is invalid")]
    InvalidKeyFile,
    #[error("credential store is unavailable")]
    CredentialStore,
    #[error("remote connection failed")]
    Connection,
    #[error("remote host key does not match")]
    HostKeyMismatch,
    #[error("public key authentication failed")]
    Authentication,
    #[error("SFTP subsystem is unavailable")]
    Sftp,
    #[error("remote root is unavailable")]
    RemoteRoot,
    #[error("remote file could not be created")]
    RemoteCreate,
    #[error("remote file write failed")]
    RemoteWrite,
    #[error("remote file flush failed")]
    RemoteFlush,
    #[error("remote file close failed")]
    RemoteClose,
    #[error("remote file could not be inspected")]
    RemoteInspect,
    #[error("remote file could not be deleted")]
    RemoteDelete,
    #[error("remote file could not be renamed")]
    RemoteRename,
    #[error("remote directory could not be created")]
    RemoteCreateDirectory,
    #[error("remote file conflicts with this upload")]
    RemoteConflict,
    #[error("local capture is invalid")]
    InvalidCapture,
}

pub fn profile_id() -> &'static str {
    PROFILE_ID
}

pub fn validate_profile_input(
    app: &AppHandle,
    input: &SaveRemoteProfileInput,
) -> Result<(PathBuf, String), UploadError> {
    if input.name.trim().is_empty()
        || input.name.chars().count() > 64
        || !valid_host(&input.host)
        || !valid_username(&input.username)
        || !valid_fingerprint(&input.host_key_fingerprint)
        || !matches!(input.sync_interval_minutes, 15 | 30 | 60 | 120 | 240)
    {
        return Err(UploadError::InvalidProfile);
    }
    let remote_root = normalize_remote_root(&input.remote_root)?;
    let key_path = validate_private_key_path(app, Path::new(&input.private_key_path))?;
    Ok((key_path, remote_root))
}

pub fn validate_stored_profile(
    app: &AppHandle,
    profile: &RemoteProfileRecord,
) -> Result<(), UploadError> {
    validate_endpoint(
        &profile.host,
        u16::try_from(profile.port).map_err(|_| UploadError::InvalidProfile)?,
    )?;
    if !valid_username(&profile.username)
        || !valid_fingerprint(&profile.host_key_fingerprint)
        || normalize_remote_root(&profile.remote_root)? != profile.remote_root
    {
        return Err(UploadError::InvalidProfile);
    }
    let validated = validate_private_key_path(app, Path::new(&profile.private_key_path))?;
    if validated != PathBuf::from(&profile.private_key_path) {
        return Err(UploadError::InvalidKeyPath);
    }
    Ok(())
}

pub fn validate_endpoint(host: &str, port: u16) -> Result<(), UploadError> {
    if port == 0 || !valid_host(host) {
        return Err(UploadError::InvalidProfile);
    }
    Ok(())
}

pub fn validate_selected_private_key(app: &AppHandle, path: &Path) -> Result<PathBuf, UploadError> {
    validate_private_key_path(app, path)
}

fn valid_host(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && !value
            .chars()
            .any(|character| character.is_whitespace() || "/@\\\0".contains(character))
}

fn valid_username(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
}

fn valid_fingerprint(value: &str) -> bool {
    value.starts_with("SHA256:")
        && value.len() >= 16
        && value.len() <= 128
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "+/=:".contains(character))
}

fn normalize_remote_root(value: &str) -> Result<String, UploadError> {
    if !value.starts_with('/') || value.contains('\0') || value.contains('\\') {
        return Err(UploadError::InvalidProfile);
    }
    let components: Vec<_> = value.split('/').filter(|part| !part.is_empty()).collect();
    if components
        .iter()
        .any(|component| *component == "." || *component == "..")
    {
        return Err(UploadError::InvalidProfile);
    }
    if components.is_empty() {
        Ok("/".to_string())
    } else {
        Ok(format!("/{}", components.join("/")))
    }
}

fn validate_private_key_path(_app: &AppHandle, path: &Path) -> Result<PathBuf, UploadError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| UploadError::InvalidKeyFile)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_PRIVATE_KEY_BYTES
    {
        return Err(UploadError::InvalidKeyFile);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(UploadError::InvalidKeyFile);
        }
    }
    let canonical_path = path
        .canonicalize()
        .map_err(|_| UploadError::InvalidKeyFile)?;
    Ok(canonical_path)
}

pub async fn store_passphrase(mut passphrase: String) -> Result<(), UploadError> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        let entry = keyring::Entry::new(KEYRING_SERVICE, PROFILE_ID)
            .map_err(|_| UploadError::CredentialStore)?;
        let result = entry
            .set_password(&passphrase)
            .map_err(|_| UploadError::CredentialStore);
        passphrase.zeroize();
        result
    })
    .await
    .map_err(|_| UploadError::CredentialStore)?;
    result
}

async fn load_passphrase(has_passphrase: bool) -> Result<Option<String>, UploadError> {
    if !has_passphrase {
        return Ok(None);
    }
    tauri::async_runtime::spawn_blocking(|| {
        keyring::Entry::new(KEYRING_SERVICE, PROFILE_ID)
            .and_then(|entry| entry.get_password())
            .map(Some)
            .map_err(|_| UploadError::CredentialStore)
    })
    .await
    .map_err(|_| UploadError::CredentialStore)?
}

struct ProbeHandler {
    observed: Arc<Mutex<Option<String>>>,
}

impl client::Handler for ProbeHandler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKey) -> Result<bool, Self::Error> {
        if let Ok(mut observed) = self.observed.lock() {
            *observed = Some(key.fingerprint(HashAlg::Sha256).to_string());
        }
        Ok(true)
    }
}

pub async fn probe_host_key(host: &str, port: u16) -> Result<String, UploadError> {
    validate_endpoint(host, port)?;
    let observed = Arc::new(Mutex::new(None));
    let handler = ProbeHandler {
        observed: observed.clone(),
    };
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(15)),
        ..Default::default()
    });
    let session = tokio::time::timeout(
        Duration::from_secs(15),
        client::connect(config, (host, port), handler),
    )
    .await
    .map_err(|_| UploadError::Connection)?
    .map_err(|_| UploadError::Connection)?;
    let _ = session
        .disconnect(Disconnect::ByApplication, "fingerprint read complete", "")
        .await;
    let fingerprint = observed
        .lock()
        .map_err(|_| UploadError::Connection)?
        .clone()
        .ok_or(UploadError::Connection)?;
    Ok(fingerprint)
}

struct PinnedHandler {
    expected: String,
    mismatch: Arc<Mutex<bool>>,
}

impl client::Handler for PinnedHandler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKey) -> Result<bool, Self::Error> {
        let matches = key.fingerprint(HashAlg::Sha256).to_string() == self.expected;
        if !matches {
            if let Ok(mut mismatch) = self.mismatch.lock() {
                *mismatch = true;
            }
        }
        Ok(matches)
    }
}

pub struct RemoteSession {
    handle: client::Handle<PinnedHandler>,
    sftp: SftpSession,
    remote_root: String,
    verified_directories: HashSet<String>,
    performance: SessionPerformance,
}

impl RemoteSession {
    pub async fn connect(
        profile: &RemoteProfileRecord,
        mut on_progress: impl FnMut(ConnectionProgress),
    ) -> Result<Self, UploadError> {
        let passphrase = load_passphrase(profile.has_passphrase).await?;
        let key_path = PathBuf::from(&profile.private_key_path);
        let mut passphrase_for_key = passphrase;
        let key = tauri::async_runtime::spawn_blocking(move || {
            let result = russh::keys::load_secret_key(key_path, passphrase_for_key.as_deref())
                .map_err(|_| UploadError::InvalidKeyFile);
            if let Some(value) = passphrase_for_key.as_mut() {
                value.zeroize();
            }
            result
        })
        .await
        .map_err(|_| UploadError::InvalidKeyFile)??;

        let mismatch = Arc::new(Mutex::new(false));
        let handler = PinnedHandler {
            expected: profile.host_key_fingerprint.clone(),
            mismatch: mismatch.clone(),
        };
        let config = Arc::new(client::Config {
            inactivity_timeout: Some(Duration::from_secs(20)),
            ..Default::default()
        });
        let connection_started = Instant::now();
        let connection = tokio::time::timeout(
            Duration::from_secs(20),
            client::connect(
                config,
                (
                    profile.host.as_str(),
                    u16::try_from(profile.port).map_err(|_| UploadError::InvalidProfile)?,
                ),
                handler,
            ),
        )
        .await
        .map_err(|_| UploadError::Connection)?;
        let mut handle = match connection {
            Ok(handle) => handle,
            Err(_) if mismatch.lock().map(|value| *value).unwrap_or(false) => {
                return Err(UploadError::HostKeyMismatch)
            }
            Err(_) => return Err(UploadError::Connection),
        };
        on_progress(ConnectionProgress::Connected(connection_started.elapsed()));
        let authentication_started = Instant::now();
        let rsa_hash = handle
            .best_supported_rsa_hash()
            .await
            .map_err(|_| UploadError::Authentication)?
            .flatten();
        let auth = handle
            .authenticate_publickey(
                profile.username.clone(),
                PrivateKeyWithHashAlg::new(Arc::new(key), rsa_hash),
            )
            .await
            .map_err(|_| UploadError::Authentication)?;
        if !auth.success() {
            return Err(UploadError::Authentication);
        }
        on_progress(ConnectionProgress::Authenticated(
            authentication_started.elapsed(),
        ));
        let sftp_started = Instant::now();
        let channel = handle
            .channel_open_session()
            .await
            .map_err(|_| UploadError::Sftp)?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|_| UploadError::Sftp)?;
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|_| UploadError::Sftp)?;
        on_progress(ConnectionProgress::SftpInitialized(sftp_started.elapsed()));
        Ok(Self {
            handle,
            sftp,
            remote_root: profile.remote_root.clone(),
            verified_directories: HashSet::new(),
            performance: SessionPerformance::default(),
        })
    }

    pub async fn test_writable(&self) -> Result<RemoteConnectionTest, UploadError> {
        let metadata = self
            .sftp
            .metadata(&self.remote_root)
            .await
            .map_err(|_| UploadError::RemoteRoot)?;
        if !metadata.is_dir() {
            return Err(UploadError::RemoteRoot);
        }
        let path = join_remote(
            &self.remote_root,
            &format!(".electronic-journey-write-test-{}.part", Uuid::new_v4()),
        )?;
        self.write_file(&path, &[]).await?;
        let metadata = self
            .sftp
            .metadata(&path)
            .await
            .map_err(|_| UploadError::RemoteInspect)?;
        if metadata.size != Some(0) {
            let _ = self.sftp.remove_file(&path).await;
            return Err(UploadError::RemoteInspect);
        }
        self.sftp
            .remove_file(&path)
            .await
            .map_err(|_| UploadError::RemoteDelete)?;
        if self
            .sftp
            .try_exists(&path)
            .await
            .map_err(|_| UploadError::RemoteInspect)?
        {
            return Err(UploadError::RemoteDelete);
        }
        Ok(RemoteConnectionTest {
            remote_root: self.remote_root.clone(),
            writable: true,
        })
    }

    pub fn performance(&self) -> SessionPerformance {
        self.performance
    }

    pub async fn upload(
        &mut self,
        relative_path: &str,
        bytes: &[u8],
        mut on_progress: impl FnMut(UploadProgressEvent),
    ) -> Result<(), UploadError> {
        validate_relative_remote_path(relative_path)?;
        on_progress(UploadProgressEvent::PreparingRemote);
        let final_path = join_remote(&self.remote_root, relative_path)?;
        if let Some(metadata) = self.inspect_optional(&final_path).await? {
            return if metadata.size == Some(bytes.len() as u64) {
                Ok(())
            } else {
                Err(UploadError::RemoteConflict)
            };
        }
        self.ensure_parent_directories(relative_path).await?;
        let temporary_path = format!("{final_path}.{}.part", Uuid::new_v4());
        let transfer = self
            .write_upload_file(&temporary_path, bytes, |bytes, elapsed| {
                on_progress(UploadProgressEvent::Transferred { bytes, elapsed });
            })
            .await;
        self.performance.transfer_bytes = self
            .performance
            .transfer_bytes
            .saturating_add(transfer.bytes);
        self.performance.transfer = self.performance.transfer.saturating_add(transfer.elapsed);
        if let Err(error) = transfer.result {
            let _ = self.sftp.remove_file(&temporary_path).await;
            return Err(error);
        }
        on_progress(UploadProgressEvent::VerifyingRemote);
        let result = async {
            let metadata = self.inspect_required(&temporary_path).await?;
            if metadata.size != Some(bytes.len() as u64) {
                return Err(UploadError::RemoteInspect);
            }
            if self.inspect_optional(&final_path).await?.is_some() {
                return Err(UploadError::RemoteConflict);
            }
            self.sftp
                .rename(&temporary_path, &final_path)
                .await
                .map_err(|_| UploadError::RemoteRename)?;
            let final_metadata = self.inspect_required(&final_path).await?;
            if final_metadata.size != Some(bytes.len() as u64) {
                return Err(UploadError::RemoteInspect);
            }
            Ok(())
        }
        .await;
        if result.is_err() {
            let _ = self.sftp.remove_file(&temporary_path).await;
        }
        result
    }

    async fn inspect_optional(&mut self, path: &str) -> Result<Option<Metadata>, UploadError> {
        let started = Instant::now();
        let result = self.sftp.metadata(path).await;
        self.performance.remote_metadata_operations = self
            .performance
            .remote_metadata_operations
            .saturating_add(1);
        self.performance.remote_metadata = self
            .performance
            .remote_metadata
            .saturating_add(started.elapsed());
        match result {
            Ok(metadata) => Ok(Some(metadata)),
            Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => {
                Ok(None)
            }
            Err(_) => Err(UploadError::RemoteInspect),
        }
    }

    async fn inspect_required(&mut self, path: &str) -> Result<Metadata, UploadError> {
        self.inspect_optional(path)
            .await?
            .ok_or(UploadError::RemoteInspect)
    }

    async fn write_file(&self, path: &str, bytes: &[u8]) -> Result<(), UploadError> {
        let mut file = self
            .sftp
            .create(path)
            .await
            .map_err(|_| UploadError::RemoteCreate)?;
        file.write_all(bytes)
            .await
            .map_err(|_| UploadError::RemoteWrite)?;
        file.flush().await.map_err(|_| UploadError::RemoteFlush)?;
        file.shutdown().await.map_err(|_| UploadError::RemoteClose)
    }

    async fn write_upload_file(
        &self,
        path: &str,
        bytes: &[u8],
        mut on_progress: impl FnMut(u64, Duration),
    ) -> TransferAttempt {
        let mut file = match self.sftp.create(path).await {
            Ok(file) => file,
            Err(_) => return TransferAttempt::failed(UploadError::RemoteCreate),
        };
        let started = Instant::now();
        let mut transferred = 0_u64;
        while transferred < bytes.len() as u64 {
            let offset = usize::try_from(transferred).unwrap_or(bytes.len());
            let written = match file.write(&bytes[offset..]).await {
                Ok(0) => {
                    return TransferAttempt {
                        result: Err(UploadError::RemoteWrite),
                        bytes: transferred,
                        elapsed: started.elapsed(),
                    }
                }
                Ok(written) => written,
                Err(_) => {
                    return TransferAttempt {
                        result: Err(UploadError::RemoteWrite),
                        bytes: transferred,
                        elapsed: started.elapsed(),
                    }
                }
            };
            transferred = transferred.saturating_add(written as u64);
            on_progress(transferred, started.elapsed());
        }
        if file.flush().await.is_err() {
            return TransferAttempt {
                result: Err(UploadError::RemoteFlush),
                bytes: transferred,
                elapsed: started.elapsed(),
            };
        }
        let result = file.shutdown().await.map_err(|_| UploadError::RemoteClose);
        let elapsed = started.elapsed();
        on_progress(transferred, elapsed);
        TransferAttempt {
            result,
            bytes: transferred,
            elapsed,
        }
    }

    async fn ensure_parent_directories(&mut self, relative_path: &str) -> Result<(), UploadError> {
        let parent = relative_path
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .ok_or(UploadError::InvalidProfile)?;
        let mut current = self.remote_root.clone();
        for component in parent.split('/') {
            current = join_remote(&current, component)?;
            if self.verified_directories.contains(&current) {
                continue;
            }
            match self.inspect_optional(&current).await? {
                Some(metadata) if metadata.is_dir() => {}
                Some(_) => return Err(UploadError::RemoteConflict),
                None => self
                    .sftp
                    .create_dir(&current)
                    .await
                    .map_err(|_| UploadError::RemoteCreateDirectory)?,
            }
            self.verified_directories.insert(current.clone());
        }
        Ok(())
    }

    pub async fn disconnect(self) {
        let _ = self
            .handle
            .disconnect(Disconnect::ByApplication, "upload complete", "")
            .await;
    }
}

struct TransferAttempt {
    result: Result<(), UploadError>,
    bytes: u64,
    elapsed: Duration,
}

impl TransferAttempt {
    fn failed(error: UploadError) -> Self {
        Self {
            result: Err(error),
            bytes: 0,
            elapsed: Duration::ZERO,
        }
    }
}

fn validate_relative_remote_path(value: &str) -> Result<(), UploadError> {
    let path = Path::new(value);
    if path.is_absolute()
        || value.contains('\\')
        || value.contains('\0')
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(UploadError::InvalidProfile);
    }
    Ok(())
}

fn join_remote(root: &str, relative: &str) -> Result<String, UploadError> {
    validate_relative_remote_path(relative)?;
    if root == "/" {
        Ok(format!("/{relative}"))
    } else {
        Ok(format!("{root}/{relative}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_root_is_absolute_and_normalized() {
        assert_eq!(
            normalize_remote_root("/srv/journey/").unwrap(),
            "/srv/journey"
        );
        assert_eq!(normalize_remote_root("/").unwrap(), "/");
        assert!(normalize_remote_root("srv/journey").is_err());
        assert!(normalize_remote_root("/srv/../secret").is_err());
    }

    #[test]
    fn relative_upload_paths_cannot_escape_the_remote_root() {
        assert_eq!(
            join_remote("/srv/journey", "2026/07/29/a.webp").unwrap(),
            "/srv/journey/2026/07/29/a.webp"
        );
        assert!(join_remote("/srv/journey", "../secret").is_err());
        assert!(join_remote("/srv/journey", "/etc/passwd").is_err());
    }

    #[test]
    fn endpoint_fields_reject_shell_and_path_syntax() {
        assert!(validate_endpoint("server.example", 22).is_ok());
        assert!(validate_endpoint("user@server", 22).is_err());
        assert!(!valid_username("user name"));
        assert!(valid_username("journey_upload"));
    }

    #[test]
    fn diagnostics_report_bytes_speed_eta_and_reset_failed_item_progress() {
        let registry = UploadDiagnosticsRegistry::default();
        let batch_id = Uuid::new_v4();
        let mebibyte = 1024 * 1024;
        registry.begin(batch_id, 10 * mebibyte);
        registry.record_connection(
            batch_id,
            ConnectionProgress::Connected(Duration::from_millis(120)),
        );
        registry.record_local_validation(batch_id, Duration::from_millis(300));
        registry.record_upload_event(
            batch_id,
            UploadProgressEvent::Transferred {
                bytes: 2 * mebibyte,
                elapsed: Duration::from_secs(1),
            },
            0,
            Duration::ZERO,
        );

        let active = registry.snapshot(batch_id, 10 * mebibyte, 0, "uploading");
        assert_eq!(active.phase, "transferring");
        assert_eq!(active.uploaded_bytes, 2 * mebibyte);
        assert_eq!(active.bytes_per_second, 2 * mebibyte);
        assert_eq!(active.estimated_remaining_seconds, Some(4));
        assert_eq!(active.connection_ms, 120);
        assert_eq!(active.local_validation_ms, 300);

        registry.fail_item(batch_id);
        let failed_item = registry.snapshot(batch_id, 10 * mebibyte, 0, "uploading");
        assert_eq!(failed_item.uploaded_bytes, 0);
        assert_eq!(failed_item.transfer_bytes, 2 * mebibyte);
    }

    #[test]
    fn completed_diagnostics_do_not_report_an_eta() {
        let registry = UploadDiagnosticsRegistry::default();
        let batch_id = Uuid::new_v4();
        registry.begin(batch_id, 1024);
        registry.record_upload_event(
            batch_id,
            UploadProgressEvent::Transferred {
                bytes: 1024,
                elapsed: Duration::from_secs(1),
            },
            0,
            Duration::ZERO,
        );
        registry.complete_item(batch_id, 1024);
        registry.finish(batch_id, false);

        let completed = registry.snapshot(batch_id, 1024, 1024, "completed");
        assert_eq!(completed.phase, "completed");
        assert_eq!(completed.uploaded_bytes, 1024);
        assert_eq!(completed.estimated_remaining_seconds, None);
    }
}
