use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::Duration as StdDuration,
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
#[cfg(target_os = "macos")]
use zeroize::Zeroize;

use crate::{
    capture::{CaptureError, PermissionState, PlatformCapture, ScreenCapture},
    capture_pipeline,
    error::AppError,
    scheduler::{next_capture_at, FIRST_CAPTURE_DELAY},
    timeline,
};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingState {
    #[default]
    Stopped,
    Running,
    Paused,
    Suspended,
    Degraded,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSettings {
    pub interval_minutes: u16,
    pub idle_pause_minutes: u16,
    pub webp_quality: u8,
    pub max_width: u32,
    pub skip_duplicates: bool,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            interval_minutes: 5,
            idle_pause_minutes: 10,
            webp_quality: 85,
            max_width: 2560,
            skip_duplicates: true,
        }
    }
}

impl CaptureSettings {
    fn validate(&self) -> Result<(), AppError> {
        const INTERVALS: [u16; 7] = [1, 2, 5, 10, 15, 30, 60];

        if !INTERVALS.contains(&self.interval_minutes) {
            return Err(AppError::InvalidSettings(
                "unsupported capture interval".into(),
            ));
        }
        if self.idle_pause_minutes > 240 {
            return Err(AppError::InvalidSettings(
                "idle pause must be between 0 and 240 minutes".into(),
            ));
        }
        if !(1..=100).contains(&self.webp_quality) {
            return Err(AppError::InvalidSettings(
                "WebP quality must be between 1 and 100".into(),
            ));
        }
        if !(640..=7680).contains(&self.max_width) {
            return Err(AppError::InvalidSettings(
                "maximum width must be between 640 and 7680".into(),
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    state: RecordingState,
    next_capture_at: Option<String>,
    today_count: u32,
    local_storage_bytes: u64,
    pending_uploads: u32,
    cloud_enabled: bool,
    permission_granted: bool,
    permission_state: PermissionState,
    last_error: Option<String>,
    settings: CaptureSettings,
}

impl Default for AppSnapshot {
    fn default() -> Self {
        Self {
            state: RecordingState::Stopped,
            next_capture_at: None,
            today_count: 0,
            local_storage_bytes: 0,
            pending_uploads: 0,
            cloud_enabled: false,
            permission_granted: false,
            permission_state: PermissionState::NotDetermined,
            last_error: None,
            settings: CaptureSettings::default(),
        }
    }
}

#[derive(Default)]
pub struct RuntimeState {
    snapshot: Mutex<AppSnapshot>,
    schedule_generation: AtomicU64,
}

impl RuntimeState {
    pub fn from_permission_result(
        permission_result: Result<PermissionState, CaptureError>,
    ) -> Self {
        let mut snapshot = AppSnapshot::default();
        match permission_result {
            Ok(permission_state) => {
                snapshot.permission_state = permission_state;
                snapshot.permission_granted = permission_state == PermissionState::Granted;
            }
            Err(_) => {
                snapshot.last_error = Some("无法检查屏幕录制权限，请重新启动应用后再试。".into());
            }
        }

        Self {
            snapshot: Mutex::new(snapshot),
            schedule_generation: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> Result<AppSnapshot, AppError> {
        self.snapshot
            .lock()
            .map(|snapshot| snapshot.clone())
            .map_err(|_| AppError::StatePoisoned)
    }

    fn set_state(
        &self,
        state: RecordingState,
    ) -> Result<(AppSnapshot, u64, Option<StdDuration>), AppError> {
        if matches!(state, RecordingState::Suspended | RecordingState::Degraded) {
            return Err(AppError::InvalidStateTransition);
        }

        let mut snapshot = self.snapshot.lock().map_err(|_| AppError::StatePoisoned)?;
        if matches!(state, RecordingState::Running) && !snapshot.permission_granted {
            return Err(AppError::CapturePermissionRequired);
        }
        snapshot.state = state;
        snapshot.last_error = None;
        snapshot.next_capture_at = match state {
            RecordingState::Running => {
                Some(next_capture_at(Utc::now(), FIRST_CAPTURE_DELAY).to_rfc3339())
            }
            _ => None,
        };
        let generation = self.schedule_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let delay = matches!(state, RecordingState::Running).then_some(FIRST_CAPTURE_DELAY);
        Ok((snapshot.clone(), generation, delay))
    }

    fn update_permission(
        &self,
        permission_result: Result<PermissionState, CaptureError>,
    ) -> Result<AppSnapshot, AppError> {
        let mut snapshot = self.snapshot.lock().map_err(|_| AppError::StatePoisoned)?;
        match permission_result {
            Ok(permission_state) => {
                snapshot.permission_state = permission_state;
                snapshot.permission_granted = permission_state == PermissionState::Granted;
                snapshot.last_error = if permission_state == PermissionState::Denied {
                    Some("屏幕录制权限未授予，请在系统设置中允许后重试。".into())
                } else {
                    None
                };
                if permission_state != PermissionState::Granted
                    && matches!(snapshot.state, RecordingState::Running)
                {
                    snapshot.state = RecordingState::Degraded;
                    snapshot.next_capture_at = None;
                    snapshot.last_error =
                        Some("屏幕录制权限已失效，记录已停止，请重新授权。".into());
                    self.schedule_generation.fetch_add(1, Ordering::SeqCst);
                }
            }
            Err(_) => {
                snapshot.permission_granted = false;
                snapshot.permission_state = PermissionState::NotDetermined;
                snapshot.last_error =
                    Some("无法完成屏幕录制权限请求，请检查系统设置后重试。".into());
                if matches!(snapshot.state, RecordingState::Running) {
                    snapshot.state = RecordingState::Degraded;
                    snapshot.next_capture_at = None;
                    self.schedule_generation.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
        Ok(snapshot.clone())
    }

    fn update_settings(
        &self,
        settings: CaptureSettings,
    ) -> Result<(AppSnapshot, u64, Option<StdDuration>), AppError> {
        settings.validate()?;
        let mut snapshot = self.snapshot.lock().map_err(|_| AppError::StatePoisoned)?;
        snapshot.settings = settings;
        let delay = matches!(snapshot.state, RecordingState::Running)
            .then(|| StdDuration::from_secs(u64::from(snapshot.settings.interval_minutes) * 60));
        snapshot.next_capture_at =
            delay.map(|delay| next_capture_at(Utc::now(), delay).to_rfc3339());
        let generation = self.schedule_generation.fetch_add(1, Ordering::SeqCst) + 1;
        Ok((snapshot.clone(), generation, delay))
    }

    pub fn set_inventory(&self, count: u32, bytes: u64) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.today_count = count;
            snapshot.local_storage_bytes = bytes;
        }
    }

    fn schedule_is_active(&self, generation: u64) -> bool {
        self.schedule_generation.load(Ordering::SeqCst) == generation
            && self
                .snapshot
                .lock()
                .map(|snapshot| matches!(snapshot.state, RecordingState::Running))
                .unwrap_or(false)
    }

    fn capture_succeeded(&self, generation: u64, cipher_size: u64) -> Option<StdDuration> {
        if self.schedule_generation.load(Ordering::SeqCst) != generation {
            return None;
        }
        let mut snapshot = self.snapshot.lock().ok()?;
        if !matches!(snapshot.state, RecordingState::Running) {
            return None;
        }
        snapshot.today_count = snapshot.today_count.saturating_add(1);
        snapshot.local_storage_bytes = snapshot.local_storage_bytes.saturating_add(cipher_size);
        snapshot.last_error = None;
        let delay = StdDuration::from_secs(u64::from(snapshot.settings.interval_minutes) * 60);
        snapshot.next_capture_at = Some(next_capture_at(Utc::now(), delay).to_rfc3339());
        Some(delay)
    }

    fn capture_failed(&self, generation: u64, message: String, permission_denied: bool) {
        if self.schedule_generation.load(Ordering::SeqCst) != generation {
            return;
        }
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.state = RecordingState::Degraded;
            snapshot.next_capture_at = None;
            snapshot.last_error = Some(message);
            if permission_denied {
                snapshot.permission_granted = false;
                snapshot.permission_state = PermissionState::Denied;
            }
        }
        self.schedule_generation.fetch_add(1, Ordering::SeqCst);
    }
}

fn spawn_capture_loop(app: AppHandle, generation: u64, first_delay: StdDuration) {
    tauri::async_runtime::spawn(async move {
        let mut delay = first_delay;
        loop {
            tokio::time::sleep(delay).await;
            let runtime = app.state::<RuntimeState>();
            if !runtime.schedule_is_active(generation) {
                return;
            }

            let capture_result = async {
                let displays = PlatformCapture.list_displays().await?;
                let display = displays
                    .iter()
                    .find(|display| display.is_primary)
                    .or_else(|| displays.first())
                    .ok_or_else(|| CaptureError::DisplayUnavailable("primary".into()))?;
                let captured = PlatformCapture.capture(&display.id).await?;
                Ok::<_, CaptureError>(captured)
            }
            .await;

            let captured = match capture_result {
                Ok(captured) => captured,
                Err(CaptureError::PermissionDenied) => {
                    runtime.capture_failed(
                        generation,
                        "屏幕录制权限已失效，请在系统设置中重新允许。".into(),
                        true,
                    );
                    return;
                }
                Err(_) => {
                    runtime.capture_failed(
                        generation,
                        "系统截图失败，记录已停止；请检查屏幕录制权限和显示器状态。".into(),
                        false,
                    );
                    return;
                }
            };

            if !runtime.schedule_is_active(generation) {
                return;
            }
            let settings = match runtime.snapshot() {
                Ok(snapshot) => snapshot.settings,
                Err(_) => return,
            };
            match capture_pipeline::persist_capture(&app, captured, &settings).await {
                Ok(stored) => {
                    let Some(next_delay) =
                        runtime.capture_succeeded(generation, stored.cipher_size)
                    else {
                        return;
                    };
                    delay = next_delay;
                }
                Err(_) => {
                    runtime.capture_failed(
                        generation,
                        "截图未能加密写入本地保险箱，记录已停止。".into(),
                        false,
                    );
                    return;
                }
            }
        }
    });
}

#[tauri::command]
pub async fn get_app_snapshot(state: State<'_, RuntimeState>) -> Result<AppSnapshot, String> {
    let permission_result = PlatformCapture.permission_state().await;
    state
        .update_permission(permission_result)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_recording_state(
    state: RecordingState,
    runtime: State<'_, RuntimeState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    let (snapshot, generation, delay) = runtime
        .set_state(state)
        .map_err(|error| error.to_string())?;
    if let Some(delay) = delay {
        spawn_capture_loop(app, generation, delay);
    }
    Ok(snapshot)
}

#[tauri::command]
pub fn update_capture_settings(
    settings: CaptureSettings,
    runtime: State<'_, RuntimeState>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    let (snapshot, generation, delay) = runtime
        .update_settings(settings)
        .map_err(|error| error.to_string())?;
    if let Some(delay) = delay {
        spawn_capture_loop(app, generation, delay);
    }
    Ok(snapshot)
}

#[tauri::command]
pub async fn request_screen_capture_permission(
    runtime: State<'_, RuntimeState>,
) -> Result<AppSnapshot, String> {
    let permission_result = PlatformCapture.request_permission().await;

    // On current macOS versions, the separate warning for bypassing the
    // private window picker may not appear until the first direct pixel
    // access. Exercise that access during the consent flow, discard the
    // image immediately, and never let the warning surprise the user when
    // scheduled recording starts later.
    #[cfg(target_os = "macos")]
    let permission_result = match permission_result {
        Ok(PermissionState::Granted) => {
            let verification = async {
                let displays = PlatformCapture.list_displays().await?;
                let display = displays
                    .iter()
                    .find(|display| display.is_primary)
                    .or_else(|| displays.first())
                    .ok_or_else(|| CaptureError::DisplayUnavailable("primary".into()))?;
                let mut captured = PlatformCapture.capture(&display.id).await?;
                captured.rgba.zeroize();
                Ok(PermissionState::Granted)
            }
            .await;
            verification
        }
        other => other,
    };

    runtime
        .update_permission(permission_result)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_timeline_captures(
    app: AppHandle,
    offset: u32,
    limit: Option<u16>,
) -> Result<timeline::TimelinePage, String> {
    timeline::list_captures(&app, offset, limit)
        .map_err(|_| "无法读取本地时间线，请稍后重试。".to_string())
}

#[tauri::command]
pub fn read_timeline_capture(
    app: AppHandle,
    capture_id: String,
) -> Result<tauri::ipc::Response, String> {
    let capture_id =
        uuid::Uuid::parse_str(&capture_id).map_err(|_| "截图标识无效。".to_string())?;
    capture_pipeline::decrypt_saved_capture(&app, capture_id)
        .map(tauri::ipc::Response::new)
        .map_err(|_| "无法解密这张截图；文件可能已损坏或密钥不可用。".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_product_design() {
        let settings = CaptureSettings::default();
        assert_eq!(settings.interval_minutes, 5);
        assert_eq!(settings.idle_pause_minutes, 10);
        assert_eq!(settings.webp_quality, 85);
        assert_eq!(settings.max_width, 2560);
        assert!(settings.skip_duplicates);
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn startup_permission_result_is_reflected_in_snapshot() {
        let runtime = RuntimeState::from_permission_result(Ok(PermissionState::Granted));
        let snapshot = runtime.snapshot().unwrap();

        assert!(snapshot.permission_granted);
        assert_eq!(snapshot.permission_state, PermissionState::Granted);
        assert!(snapshot.last_error.is_none());
    }

    #[test]
    fn recording_cannot_start_without_screen_capture_permission() {
        let runtime = RuntimeState::from_permission_result(Ok(PermissionState::Denied));

        assert!(matches!(
            runtime.set_state(RecordingState::Running),
            Err(AppError::CapturePermissionRequired)
        ));
        assert!(matches!(
            runtime.snapshot().unwrap().state,
            RecordingState::Stopped
        ));
    }

    #[test]
    fn recording_can_start_after_screen_capture_permission_is_granted() {
        let runtime = RuntimeState::from_permission_result(Ok(PermissionState::Granted));

        let (snapshot, _, delay) = runtime.set_state(RecordingState::Running).unwrap();

        assert!(matches!(snapshot.state, RecordingState::Running));
        assert!(snapshot.next_capture_at.is_some());
        assert_eq!(delay, Some(FIRST_CAPTURE_DELAY));
        let next = chrono::DateTime::parse_from_rfc3339(snapshot.next_capture_at.as_ref().unwrap())
            .unwrap()
            .with_timezone(&Utc);
        let seconds = (next - Utc::now()).num_seconds();
        assert!((9..=10).contains(&seconds));
    }

    #[test]
    fn stopping_invalidates_the_pending_capture() {
        let runtime = RuntimeState::from_permission_result(Ok(PermissionState::Granted));
        let (_, running_generation, _) = runtime.set_state(RecordingState::Running).unwrap();
        let _ = runtime.set_state(RecordingState::Stopped).unwrap();

        assert!(!runtime.schedule_is_active(running_generation));
    }

    #[test]
    fn permission_refresh_replaces_the_cached_startup_value() {
        let runtime = RuntimeState::from_permission_result(Ok(PermissionState::NotDetermined));

        let snapshot = runtime
            .update_permission(Ok(PermissionState::Granted))
            .unwrap();

        assert!(snapshot.permission_granted);
        assert_eq!(snapshot.permission_state, PermissionState::Granted);
    }

    #[test]
    fn revoked_permission_stops_an_active_schedule() {
        let runtime = RuntimeState::from_permission_result(Ok(PermissionState::Granted));
        let (_, generation, _) = runtime.set_state(RecordingState::Running).unwrap();

        let snapshot = runtime
            .update_permission(Ok(PermissionState::NotDetermined))
            .unwrap();

        assert!(matches!(snapshot.state, RecordingState::Degraded));
        assert!(!snapshot.permission_granted);
        assert!(snapshot.next_capture_at.is_none());
        assert!(!runtime.schedule_is_active(generation));
    }
}
