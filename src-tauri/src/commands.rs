use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
    time::{Duration as StdDuration, Instant},
};

use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;
use zeroize::Zeroize;

use crate::{
    capture::{CaptureError, PermissionState, PlatformCapture, ScreenCapture},
    capture_pipeline,
    error::AppError,
    privacy::{self, CaptureDecision, PrivacyContext, PrivacyReason},
    scheduler::{next_capture_at, DEFAULT_CAPTURE_INTERVAL_MINUTES, FIRST_CAPTURE_DELAY},
    system_monitor::SystemEvent,
    timeline, upload,
};

const INVENTORY_MAX_AGE: StdDuration = StdDuration::from_secs(60);

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingState {
    #[default]
    Stopped,
    Running,
    Paused,
    Suspended,
    Degraded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SuspensionReason {
    ScreenLocked,
    SystemSleeping,
    UserIdle,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSettings {
    pub interval_minutes: u16,
    pub idle_pause_minutes: u16,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            interval_minutes: DEFAULT_CAPTURE_INTERVAL_MINUTES,
            idle_pause_minutes: 10,
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
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    state: RecordingState,
    suspension_reason: Option<SuspensionReason>,
    next_capture_at: Option<String>,
    today_count: u32,
    local_storage_bytes: u64,
    pending_uploads: u32,
    permission_granted: bool,
    permission_state: PermissionState,
    last_error: Option<String>,
    settings: CaptureSettings,
}

impl Default for AppSnapshot {
    fn default() -> Self {
        Self {
            state: RecordingState::Stopped,
            suspension_reason: None,
            next_capture_at: None,
            today_count: 0,
            local_storage_bytes: 0,
            pending_uploads: 0,
            permission_granted: false,
            permission_state: PermissionState::NotDetermined,
            last_error: None,
            settings: CaptureSettings::default(),
        }
    }
}

#[derive(Default)]
struct SystemConditions {
    screen_locked: bool,
    system_sleeping: bool,
    user_idle: bool,
}

#[derive(Default)]
struct InventoryFreshness {
    local_date: Option<NaiveDate>,
    verified_at: Option<Instant>,
}

#[derive(Default)]
pub struct RuntimeState {
    snapshot: Mutex<AppSnapshot>,
    system_conditions: Mutex<SystemConditions>,
    inventory_freshness: Mutex<InventoryFreshness>,
    inventory_refresh: tokio::sync::Mutex<()>,
    schedule_generation: AtomicU64,
    recording_requested: AtomicBool,
    system_monitor_ready: AtomicBool,
    startup_recovery_started: AtomicBool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeSummary {
    pub state: RecordingState,
    pub suspension_reason: Option<SuspensionReason>,
    pub permission_state: PermissionState,
}

impl RuntimeState {
    #[cfg(test)]
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
            system_conditions: Mutex::new(SystemConditions::default()),
            inventory_freshness: Mutex::new(InventoryFreshness::default()),
            inventory_refresh: tokio::sync::Mutex::new(()),
            schedule_generation: AtomicU64::new(0),
            recording_requested: AtomicBool::new(false),
            system_monitor_ready: AtomicBool::new(true),
            startup_recovery_started: AtomicBool::new(false),
        }
    }

    pub(crate) fn system_monitor_ready(&self) {
        self.system_monitor_ready.store(true, Ordering::SeqCst);
    }

    pub(crate) fn system_monitor_failed(&self) {
        self.system_monitor_ready.store(false, Ordering::SeqCst);
        self.recording_requested.store(false, Ordering::SeqCst);
        if let Ok(mut snapshot) = self.snapshot.lock() {
            if matches!(
                snapshot.state,
                RecordingState::Running | RecordingState::Suspended
            ) {
                snapshot.state = RecordingState::Degraded;
            }
            snapshot.suspension_reason = None;
            snapshot.next_capture_at = None;
            snapshot.last_error =
                Some("无法监听系统锁屏、休眠或空闲状态，记录已停止；请重新启动应用。".into());
        }
        self.schedule_generation.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn idle_pause_threshold(&self) -> Option<StdDuration> {
        let minutes = self.snapshot.lock().ok()?.settings.idle_pause_minutes;
        (minutes > 0).then(|| StdDuration::from_secs(u64::from(minutes) * 60))
    }

    fn snapshot(&self) -> Result<AppSnapshot, AppError> {
        self.snapshot
            .lock()
            .map(|snapshot| snapshot.clone())
            .map_err(|_| AppError::StatePoisoned)
    }

    pub(crate) fn summary(&self) -> Result<RuntimeSummary, AppError> {
        self.snapshot
            .lock()
            .map(|snapshot| RuntimeSummary {
                state: snapshot.state,
                suspension_reason: snapshot.suspension_reason,
                permission_state: snapshot.permission_state,
            })
            .map_err(|_| AppError::StatePoisoned)
    }

    fn set_state(
        &self,
        state: RecordingState,
    ) -> Result<(AppSnapshot, u64, Option<StdDuration>), AppError> {
        if matches!(state, RecordingState::Suspended | RecordingState::Degraded) {
            return Err(AppError::InvalidStateTransition);
        }

        if matches!(state, RecordingState::Running)
            && !self.system_monitor_ready.load(Ordering::SeqCst)
        {
            return Err(AppError::SystemMonitorUnavailable);
        }

        let conditions = self
            .system_conditions
            .lock()
            .map_err(|_| AppError::StatePoisoned)?;
        let mut snapshot = self.snapshot.lock().map_err(|_| AppError::StatePoisoned)?;
        if matches!(state, RecordingState::Running) && !snapshot.permission_granted {
            return Err(AppError::CapturePermissionRequired);
        }
        self.recording_requested
            .store(matches!(state, RecordingState::Running), Ordering::SeqCst);
        let suspension_reason = matches!(state, RecordingState::Running)
            .then(|| suspension_reason(&conditions))
            .flatten();
        snapshot.state = if suspension_reason.is_some() {
            RecordingState::Suspended
        } else {
            state
        };
        snapshot.suspension_reason = suspension_reason;
        snapshot.last_error = None;
        snapshot.next_capture_at = match snapshot.state {
            RecordingState::Running => {
                Some(next_capture_at(Utc::now(), FIRST_CAPTURE_DELAY).to_rfc3339())
            }
            _ => None,
        };
        let generation = self.schedule_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let delay =
            matches!(snapshot.state, RecordingState::Running).then_some(FIRST_CAPTURE_DELAY);
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
                    && matches!(
                        snapshot.state,
                        RecordingState::Running | RecordingState::Suspended
                    )
                {
                    self.recording_requested.store(false, Ordering::SeqCst);
                    snapshot.state = RecordingState::Degraded;
                    snapshot.suspension_reason = None;
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
                if matches!(
                    snapshot.state,
                    RecordingState::Running | RecordingState::Suspended
                ) {
                    self.recording_requested.store(false, Ordering::SeqCst);
                    snapshot.state = RecordingState::Degraded;
                    snapshot.suspension_reason = None;
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

    pub(crate) fn restore_settings(&self, settings: CaptureSettings) -> Result<(), AppError> {
        settings.validate()?;
        let mut snapshot = self.snapshot.lock().map_err(|_| AppError::StatePoisoned)?;
        snapshot.settings = settings;
        Ok(())
    }

    pub(crate) fn set_settings_recovery_error(&self) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.last_error =
                Some("无法恢复已保存的截图设置，当前已使用默认设置；请重新保存设置。".into());
        }
    }

    fn inventory_is_fresh(&self, local_date: NaiveDate) -> bool {
        self.inventory_freshness
            .lock()
            .map(|freshness| {
                freshness.local_date == Some(local_date)
                    && freshness
                        .verified_at
                        .is_some_and(|verified_at| verified_at.elapsed() < INVENTORY_MAX_AGE)
            })
            .unwrap_or(false)
    }

    pub fn set_inventory(&self, local_date: NaiveDate, count: u32, bytes: u64) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.today_count = count;
            snapshot.local_storage_bytes = bytes;
        }
        if let Ok(mut freshness) = self.inventory_freshness.lock() {
            freshness.local_date = Some(local_date);
            freshness.verified_at = Some(Instant::now());
        }
    }

    fn invalidate_inventory(&self) {
        if let Ok(mut freshness) = self.inventory_freshness.lock() {
            freshness.local_date = None;
            freshness.verified_at = None;
        }
    }

    pub fn set_startup_recovery_error(&self) {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.last_error =
                Some("本地数据恢复未能完成；现有索引仍可使用，请重新启动应用后重试。".into());
        }
    }

    pub fn begin_startup_recovery(&self) -> bool {
        self.startup_recovery_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    fn schedule_is_active(&self, generation: u64) -> bool {
        self.schedule_generation.load(Ordering::SeqCst) == generation
            && self
                .snapshot
                .lock()
                .map(|snapshot| matches!(snapshot.state, RecordingState::Running))
                .unwrap_or(false)
    }

    fn capture_completed(&self, generation: u64) -> Option<StdDuration> {
        if self.schedule_generation.load(Ordering::SeqCst) != generation {
            return None;
        }
        let mut snapshot = self.snapshot.lock().ok()?;
        if !matches!(snapshot.state, RecordingState::Running) {
            return None;
        }
        snapshot.last_error = None;
        let delay = StdDuration::from_secs(u64::from(snapshot.settings.interval_minutes) * 60);
        snapshot.next_capture_at = Some(next_capture_at(Utc::now(), delay).to_rfc3339());
        Some(delay)
    }

    fn capture_succeeded(&self, generation: u64) -> Option<StdDuration> {
        self.capture_completed(generation)
    }

    fn capture_skipped(&self, generation: u64) -> Option<StdDuration> {
        self.capture_completed(generation)
    }

    fn capture_failed(&self, generation: u64, message: String, permission_denied: bool) {
        if self.schedule_generation.load(Ordering::SeqCst) != generation {
            return;
        }
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.state = RecordingState::Degraded;
            snapshot.suspension_reason = None;
            snapshot.next_capture_at = None;
            snapshot.last_error = Some(message);
            if permission_denied {
                snapshot.permission_granted = false;
                snapshot.permission_state = PermissionState::Denied;
            }
        }
        self.recording_requested.store(false, Ordering::SeqCst);
        self.schedule_generation.fetch_add(1, Ordering::SeqCst);
    }

    fn apply_system_event(
        &self,
        event: SystemEvent,
    ) -> Result<Option<(u64, StdDuration)>, AppError> {
        let mut conditions = self
            .system_conditions
            .lock()
            .map_err(|_| AppError::StatePoisoned)?;
        match event {
            SystemEvent::ScreenLocked => conditions.screen_locked = true,
            SystemEvent::ScreenUnlocked => conditions.screen_locked = false,
            SystemEvent::Sleep => conditions.system_sleeping = true,
            SystemEvent::Wake => conditions.system_sleeping = false,
            SystemEvent::UserIdle(idle) => conditions.user_idle = idle,
        }

        let requested = self.recording_requested.load(Ordering::SeqCst);
        let reason = requested.then(|| suspension_reason(&conditions)).flatten();
        let mut snapshot = self.snapshot.lock().map_err(|_| AppError::StatePoisoned)?;

        if let Some(reason) = reason {
            let changed = snapshot.state != RecordingState::Suspended
                || snapshot.suspension_reason != Some(reason);
            snapshot.state = RecordingState::Suspended;
            snapshot.suspension_reason = Some(reason);
            snapshot.next_capture_at = None;
            if changed {
                self.schedule_generation.fetch_add(1, Ordering::SeqCst);
            }
            return Ok(None);
        }

        snapshot.suspension_reason = None;
        if requested
            && snapshot.permission_granted
            && self.system_monitor_ready.load(Ordering::SeqCst)
            && snapshot.state == RecordingState::Suspended
        {
            snapshot.state = RecordingState::Running;
            snapshot.last_error = None;
            snapshot.next_capture_at =
                Some(next_capture_at(Utc::now(), FIRST_CAPTURE_DELAY).to_rfc3339());
            let generation = self.schedule_generation.fetch_add(1, Ordering::SeqCst) + 1;
            return Ok(Some((generation, FIRST_CAPTURE_DELAY)));
        }
        Ok(None)
    }
}

fn current_local_day_bounds() -> Option<(NaiveDate, DateTime<Utc>, DateTime<Utc>)> {
    let local_date = Local::now().date_naive();
    let tomorrow = local_date.succ_opt()?;
    let start = Local
        .from_local_datetime(&local_date.and_hms_opt(0, 0, 0)?)
        .earliest()?;
    let end = Local
        .from_local_datetime(&tomorrow.and_hms_opt(0, 0, 0)?)
        .earliest()?;
    Some((
        local_date,
        start.with_timezone(&Utc),
        end.with_timezone(&Utc),
    ))
}

pub(crate) async fn refresh_local_inventory(app: &AppHandle, force: bool) -> Result<(), ()> {
    let (local_date, today_start_utc, tomorrow_start_utc) = current_local_day_bounds().ok_or(())?;
    let runtime = app.state::<RuntimeState>();
    if !force && runtime.inventory_is_fresh(local_date) {
        return Ok(());
    }

    let _refresh_guard = runtime.inventory_refresh.lock().await;
    if !force && runtime.inventory_is_fresh(local_date) {
        return Ok(());
    }

    let pool = app.state::<SqlitePool>().inner().clone();
    match capture_pipeline::capture_inventory(app, &pool, today_start_utc, tomorrow_start_utc).await
    {
        Ok((count, bytes)) => {
            runtime.set_inventory(local_date, count, bytes);
            Ok(())
        }
        Err(_) => {
            runtime.invalidate_inventory();
            Err(())
        }
    }
}

fn suspension_reason(conditions: &SystemConditions) -> Option<SuspensionReason> {
    match privacy::evaluate(&PrivacyContext {
        recording_enabled: true,
        screen_locked: conditions.screen_locked,
        system_sleeping: conditions.system_sleeping,
        user_idle: conditions.user_idle,
        excluded_application_active: false,
    }) {
        CaptureDecision::Blocked(PrivacyReason::ScreenLocked) => {
            Some(SuspensionReason::ScreenLocked)
        }
        CaptureDecision::Blocked(PrivacyReason::SystemSleeping) => {
            Some(SuspensionReason::SystemSleeping)
        }
        CaptureDecision::Blocked(PrivacyReason::UserIdle) => Some(SuspensionReason::UserIdle),
        CaptureDecision::Allow
        | CaptureDecision::Blocked(PrivacyReason::RecordingDisabled)
        | CaptureDecision::Blocked(PrivacyReason::ExcludedApplication) => None,
    }
}

pub(crate) fn handle_system_event(app: &AppHandle, event: SystemEvent) {
    let runtime = app.state::<RuntimeState>();
    if let Ok(Some((generation, delay))) = runtime.apply_system_event(event) {
        spawn_capture_loop(app.clone(), generation, delay);
    }
    crate::tray::refresh(app);
}

pub(crate) fn apply_recording_state(
    app: &AppHandle,
    state: RecordingState,
) -> Result<AppSnapshot, AppError> {
    let runtime = app.state::<RuntimeState>();
    let (snapshot, generation, delay) = runtime.set_state(state)?;
    if let Some(delay) = delay {
        spawn_capture_loop(app.clone(), generation, delay);
    }
    Ok(snapshot)
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
                let mut captured = PlatformCapture.capture(&display.id).await?;
                captured.comparison_exclusions = PlatformCapture
                    .comparison_exclusions(&app, &display.id, captured.width, captured.height)
                    .await;
                Ok::<_, CaptureError>((captured, display.id.0.clone()))
            }
            .await;

            let (captured, display_id) = match capture_result {
                Ok(result) => result,
                Err(CaptureError::PermissionDenied) => {
                    runtime.capture_failed(
                        generation,
                        "屏幕录制权限已失效，请在系统设置中重新允许。".into(),
                        true,
                    );
                    crate::tray::refresh(&app);
                    return;
                }
                Err(_) => {
                    runtime.capture_failed(
                        generation,
                        "系统截图失败，记录已停止；请检查屏幕录制权限和显示器状态。".into(),
                        false,
                    );
                    crate::tray::refresh(&app);
                    return;
                }
            };

            if !runtime.schedule_is_active(generation) {
                return;
            }
            let pool = app.state::<SqlitePool>();
            let captured_at_utc = Utc::now();
            let timezone = iana_time_zone::get_timezone().unwrap_or_else(|_| "Etc/Unknown".into());
            match capture_pipeline::persist_capture(
                &app,
                pool.inner(),
                captured,
                &display_id,
                captured_at_utc,
                &timezone,
            )
            .await
            {
                Ok(capture_pipeline::PersistCaptureOutcome::Stored) => {
                    let Some(next_delay) = runtime.capture_succeeded(generation) else {
                        return;
                    };
                    delay = next_delay;
                    let _ = refresh_local_inventory(&app, true).await;
                }
                Ok(capture_pipeline::PersistCaptureOutcome::SkippedDuplicate) => {
                    let Some(next_delay) = runtime.capture_skipped(generation) else {
                        return;
                    };
                    delay = next_delay;
                }
                Err(_) => {
                    runtime.capture_failed(
                        generation,
                        "截图未能写入本地存储，记录已停止。".into(),
                        false,
                    );
                    crate::tray::refresh(&app);
                    return;
                }
            }
        }
    });
}

#[tauri::command]
pub async fn get_app_snapshot(
    app: AppHandle,
    state: State<'_, RuntimeState>,
    pool: State<'_, SqlitePool>,
) -> Result<AppSnapshot, String> {
    crate::trace_startup("cached snapshot requested");
    refresh_local_inventory(&app, false)
        .await
        .map_err(|_| "无法验证本地截图统计，请稍后重试。".to_string())?;
    let mut snapshot = state.snapshot().map_err(|error| error.to_string())?;
    snapshot.pending_uploads = crate::database::active_upload_count(pool.inner())
        .await
        .map_err(|_| "无法读取上传任务状态。".to_string())?;
    Ok(snapshot)
}

#[tauri::command]
pub async fn refresh_screen_capture_permission(
    app: AppHandle,
    state: State<'_, RuntimeState>,
) -> Result<AppSnapshot, String> {
    crate::trace_startup("permission refresh started");
    let permission_result = PlatformCapture.permission_state().await;
    let snapshot = state
        .update_permission(permission_result)
        .map_err(|error| error.to_string())?;
    crate::tray::refresh(&app);
    crate::trace_startup("permission refresh finished");
    Ok(snapshot)
}

#[tauri::command]
pub fn set_recording_state(state: RecordingState, app: AppHandle) -> Result<AppSnapshot, String> {
    let snapshot = apply_recording_state(&app, state).map_err(|error| error.to_string())?;
    crate::tray::refresh(&app);
    Ok(snapshot)
}

#[tauri::command]
pub async fn update_capture_settings(
    settings: CaptureSettings,
    runtime: State<'_, RuntimeState>,
    pool: State<'_, SqlitePool>,
    app: AppHandle,
) -> Result<AppSnapshot, String> {
    settings.validate().map_err(|error| error.to_string())?;
    crate::database::save_capture_settings(
        pool.inner(),
        &crate::database::CaptureSettingsRecord {
            interval_minutes: settings.interval_minutes,
            idle_pause_minutes: settings.idle_pause_minutes,
        },
    )
    .await
    .map_err(|_| "无法保存截图设置。".to_string())?;
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
    app: AppHandle,
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

    let snapshot = runtime
        .update_permission(permission_result)
        .map_err(|error| error.to_string())?;
    crate::tray::refresh(&app);
    Ok(snapshot)
}

#[tauri::command]
pub async fn list_timeline_captures(
    pool: State<'_, SqlitePool>,
    offset: u32,
    limit: Option<u16>,
) -> Result<timeline::TimelinePage, String> {
    timeline::list_captures(pool.inner(), offset, limit)
        .await
        .map_err(|_| "无法读取本地时间线，请稍后重试。".to_string())
}

#[tauri::command]
pub async fn list_timeline_day_selection(
    pool: State<'_, SqlitePool>,
    date_key: String,
) -> Result<Vec<timeline::TimelineSelectionItem>, String> {
    let date = NaiveDate::parse_from_str(&date_key, "%Y-%m-%d")
        .map_err(|_| "时间线日期无效。".to_string())?;
    let next_date = date
        .succ_opt()
        .ok_or_else(|| "时间线日期无效。".to_string())?;
    let start = Local
        .from_local_datetime(
            &date
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| "时间线日期无效。".to_string())?,
        )
        .earliest()
        .ok_or_else(|| "无法确定当天的系统时区边界。".to_string())?;
    let end = Local
        .from_local_datetime(
            &next_date
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| "时间线日期无效。".to_string())?,
        )
        .earliest()
        .ok_or_else(|| "无法确定当天的系统时区边界。".to_string())?;
    timeline::list_selection_between(
        pool.inner(),
        start.with_timezone(&Utc),
        end.with_timezone(&Utc),
    )
    .await
    .map_err(|_| "无法选择当天的截图，请稍后重试。".to_string())
}

#[tauri::command]
pub async fn read_timeline_capture(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    capture_id: String,
) -> Result<tauri::ipc::Response, String> {
    let capture_id =
        uuid::Uuid::parse_str(&capture_id).map_err(|_| "截图标识无效。".to_string())?;
    let record = crate::database::capture_file(pool.inner(), capture_id)
        .await
        .map_err(|_| "无法读取截图索引。".to_string())?
        .ok_or_else(|| "截图不存在。".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        capture_pipeline::read_saved_capture(&app, capture_id, &record)
    })
    .await
    .map_err(|_| "读取截图的后台任务意外结束。".to_string())?
    .map(tauri::ipc::Response::new)
    .map_err(|_| "无法读取这张截图；文件可能已损坏。".to_string())
}

#[tauri::command]
pub async fn read_timeline_thumbnail(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    capture_id: String,
) -> Result<tauri::ipc::Response, String> {
    let capture_id =
        uuid::Uuid::parse_str(&capture_id).map_err(|_| "截图标识无效。".to_string())?;
    let record = crate::database::capture_file(pool.inner(), capture_id)
        .await
        .map_err(|_| "无法读取截图索引。".to_string())?
        .ok_or_else(|| "截图不存在。".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        capture_pipeline::read_saved_thumbnail(&app, capture_id, &record)
    })
    .await
    .map_err(|_| "读取缩略图的后台任务意外结束。".to_string())?
    .map(tauri::ipc::Response::new)
    .map_err(|_| "无法读取这张截图的缩略图。".to_string())
}

#[tauri::command]
pub async fn delete_timeline_capture(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    capture_id: String,
) -> Result<(), String> {
    let capture_id =
        uuid::Uuid::parse_str(&capture_id).map_err(|_| "截图标识无效。".to_string())?;
    let record = crate::database::capture_file(pool.inner(), capture_id)
        .await
        .map_err(|_| "无法读取截图索引。".to_string())?
        .ok_or_else(|| "截图不存在或已经被删除。".to_string())?;
    let deletion =
        capture_pipeline::delete_saved_capture(&app, pool.inner(), capture_id, &record).await;
    let _ = refresh_local_inventory(&app, true).await;
    match deletion {
        Ok(()) => Ok(()),
        Err(capture_pipeline::CapturePipelineError::CaptureUploadInProgress) => {
            Err("这张截图正在上传，请等待上传结束后再删除。".to_string())
        }
        Err(capture_pipeline::CapturePipelineError::CaptureNotFound) => {
            Err("截图不存在或已经被删除。".to_string())
        }
        Err(capture_pipeline::CapturePipelineError::DeleteIncomplete) => {
            Err("删除记录已提交，但文件清理未能完整验证；请检查本地数据目录。".to_string())
        }
        Err(_) => Err("无法删除这张截图，本地文件和索引未确认全部移除。".to_string()),
    }
}

fn upload_error_message(error: &upload::UploadError) -> String {
    match error {
        upload::UploadError::InvalidProfile => "远程服务器配置无效。".to_string(),
        upload::UploadError::InvalidKeyPath => {
            "已保存的私钥路径发生变化，请重新选择私钥文件。".to_string()
        }
        upload::UploadError::InvalidKeyFile => {
            "私钥文件无效、权限过宽，或无法使用提供的口令解锁。".to_string()
        }
        upload::UploadError::CredentialStore => "无法访问系统钥匙串。".to_string(),
        upload::UploadError::Connection => "无法连接远程服务器。".to_string(),
        upload::UploadError::HostKeyMismatch => {
            "服务器主机指纹与已保存值不一致，已拒绝连接。".to_string()
        }
        upload::UploadError::Authentication => "SSH 私钥认证失败。".to_string(),
        upload::UploadError::Sftp => "远程服务器的 SFTP 操作失败。".to_string(),
        upload::UploadError::RemoteRoot => "远程文件夹不存在、不是目录或不可写。".to_string(),
        upload::UploadError::RemoteCreate => {
            "无法在远程文件夹创建临时文件，请检查目录属主和写权限。".to_string()
        }
        upload::UploadError::RemoteWrite => "远程临时文件写入失败。".to_string(),
        upload::UploadError::RemoteFlush => {
            "远程服务器未能确认文件写入，请检查 SFTP 服务兼容性。".to_string()
        }
        upload::UploadError::RemoteClose => {
            "远程文件写入后无法正常关闭，请检查 SFTP 服务状态。".to_string()
        }
        upload::UploadError::RemoteInspect => {
            "无法读取远程文件状态或文件长度验证失败。".to_string()
        }
        upload::UploadError::RemoteDelete => {
            "测试文件已创建，但无法删除；请检查远程目录删除权限。".to_string()
        }
        upload::UploadError::RemoteRename => "临时文件已写入，但无法改名为最终文件。".to_string(),
        upload::UploadError::RemoteCreateDirectory => {
            "无法创建远程日期目录，请检查目标目录写权限。".to_string()
        }
        upload::UploadError::RemoteConflict => {
            "远端已存在同名但大小不同的文件，未覆盖。".to_string()
        }
        upload::UploadError::InvalidCapture => "本地截图完整性校验失败，未上传。".to_string(),
    }
}

fn upload_error_code(error: &upload::UploadError) -> &'static str {
    match error {
        upload::UploadError::InvalidProfile => "invalid_profile",
        upload::UploadError::InvalidKeyPath => "invalid_key_path",
        upload::UploadError::InvalidKeyFile => "invalid_key_file",
        upload::UploadError::CredentialStore => "credential_store",
        upload::UploadError::Connection => "connection",
        upload::UploadError::HostKeyMismatch => "host_key_mismatch",
        upload::UploadError::Authentication => "authentication",
        upload::UploadError::Sftp => "sftp",
        upload::UploadError::RemoteRoot => "remote_root",
        upload::UploadError::RemoteCreate => "remote_create",
        upload::UploadError::RemoteWrite => "remote_write",
        upload::UploadError::RemoteFlush => "remote_flush",
        upload::UploadError::RemoteClose => "remote_close",
        upload::UploadError::RemoteInspect => "remote_inspect",
        upload::UploadError::RemoteDelete => "remote_delete",
        upload::UploadError::RemoteRename => "remote_rename",
        upload::UploadError::RemoteCreateDirectory => "remote_create_directory",
        upload::UploadError::RemoteConflict => "remote_conflict",
        upload::UploadError::InvalidCapture => "invalid_capture",
    }
}

#[tauri::command]
pub async fn pick_private_key_file(app: AppHandle) -> Result<Option<String>, String> {
    let selected = app
        .dialog()
        .file()
        .set_title("选择 SSH 私钥文件")
        .blocking_pick_file();
    let Some(selected) = selected else {
        return Ok(None);
    };
    let path = selected
        .into_path()
        .map_err(|_| "选择的项目不是可用的本地文件。".to_string())?;
    let canonical = upload::validate_selected_private_key(&app, &path)
        .map_err(|error| upload_error_message(&error))?;
    canonical
        .to_str()
        .map(|value| Some(value.to_string()))
        .ok_or_else(|| "私钥路径无法显示。".to_string())
}

#[tauri::command]
pub async fn get_remote_profile(
    pool: State<'_, SqlitePool>,
) -> Result<Option<upload::RemoteProfile>, String> {
    crate::database::remote_profile(pool.inner(), upload::profile_id())
        .await
        .map_err(|_| "无法读取远程服务器配置。".to_string())?
        .map(upload::RemoteProfile::try_from)
        .transpose()
        .map_err(|error| upload_error_message(&error))
}

#[tauri::command]
pub async fn probe_remote_host_key(host: String, port: u16) -> Result<String, String> {
    upload::probe_host_key(&host, port)
        .await
        .map_err(|error| upload_error_message(&error))
}

#[tauri::command]
pub async fn save_remote_profile(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    mut input: upload::SaveRemoteProfileInput,
) -> Result<upload::RemoteProfile, String> {
    let (private_key_path, remote_root) = upload::validate_profile_input(&app, &input)
        .map_err(|error| upload_error_message(&error))?;
    let previous = crate::database::remote_profile(pool.inner(), upload::profile_id())
        .await
        .map_err(|_| "无法读取现有远程服务器配置。".to_string())?;
    let supplied_passphrase = input
        .private_key_passphrase
        .take()
        .filter(|value| !value.is_empty());
    let has_passphrase = supplied_passphrase.is_some()
        || previous
            .as_ref()
            .is_some_and(|profile| profile.has_passphrase);
    if let Some(passphrase) = supplied_passphrase {
        upload::store_passphrase(passphrase)
            .await
            .map_err(|error| upload_error_message(&error))?;
    }

    crate::database::save_remote_profile(
        pool.inner(),
        &crate::database::SaveRemoteProfile {
            id: upload::profile_id(),
            name: input.name.trim(),
            host: input.host.trim(),
            port: input.port,
            username: input.username.trim(),
            private_key_path: private_key_path
                .to_str()
                .ok_or_else(|| "私钥路径无法保存。".to_string())?,
            host_key_fingerprint: input.host_key_fingerprint.trim(),
            remote_root: &remote_root,
            has_passphrase,
        },
    )
    .await
    .map_err(|_| "无法保存远程服务器配置。".to_string())?;
    let next_auto_sync_at = input
        .auto_sync_enabled
        .then(|| Utc::now() + chrono::Duration::minutes(i64::from(input.sync_interval_minutes)));
    crate::database::save_auto_sync_settings(
        pool.inner(),
        upload::profile_id(),
        input.auto_sync_enabled,
        input.sync_interval_minutes,
        next_auto_sync_at,
    )
    .await
    .map_err(|_| "服务器配置已保存，但自动同步设置未能确认保存。".to_string())?;
    let stored = crate::database::remote_profile(pool.inner(), upload::profile_id())
        .await
        .map_err(|_| "无法读取刚保存的配置。".to_string())?
        .ok_or_else(|| "远程服务器配置没有保存成功。".to_string())?;
    upload::RemoteProfile::try_from(stored).map_err(|error| upload_error_message(&error))
}

#[tauri::command]
pub async fn test_remote_profile(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
) -> Result<upload::RemoteConnectionTest, String> {
    let profile = crate::database::remote_profile(pool.inner(), upload::profile_id())
        .await
        .map_err(|_| "无法读取远程服务器配置。".to_string())?
        .ok_or_else(|| "请先保存远程服务器配置。".to_string())?;
    upload::validate_stored_profile(&app, &profile)
        .map_err(|error| upload_error_message(&error))?;
    let session = upload::RemoteSession::connect(&profile)
        .await
        .map_err(|error| upload_error_message(&error))?;
    let result = session
        .test_writable()
        .await
        .map_err(|error| upload_error_message(&error));
    session.disconnect().await;
    if result.is_ok() && profile.auto_sync_enabled {
        let next_run = Utc::now()
            + chrono::Duration::minutes(
                u16::try_from(profile.sync_interval_minutes)
                    .map(i64::from)
                    .unwrap_or(30),
            );
        crate::database::save_auto_sync_settings(
            pool.inner(),
            upload::profile_id(),
            true,
            u16::try_from(profile.sync_interval_minutes).unwrap_or(30),
            Some(next_run),
        )
        .await
        .map_err(|_| "连接验证通过，但自动同步暂停状态未能更新。".to_string())?;
    }
    result
}

fn upload_error_message_from_code(code: &str) -> String {
    match code {
        "invalid_profile" => "远程服务器配置无效。",
        "invalid_key_path" => "已保存的私钥路径发生变化，请重新选择私钥文件。",
        "invalid_key_file" => "私钥文件无效、权限过宽或无法解锁。",
        "credential_store" => "无法访问系统钥匙串。",
        "connection" => "无法连接远程服务器。",
        "host_key_mismatch" => "服务器主机指纹与已保存值不一致。",
        "authentication" => "SSH 私钥认证失败。",
        "sftp" => "远程服务器未能启动 SFTP。",
        "remote_root" => "远程文件夹不存在、不是目录或不可写。",
        "remote_create" => "无法在远程文件夹创建临时文件。",
        "remote_write" => "远程临时文件写入失败。",
        "remote_flush" => "远程服务器未确认文件写入。",
        "remote_close" => "远程文件写入后无法正常关闭。",
        "remote_inspect" => "无法读取远程文件状态或长度验证失败。",
        "remote_delete" => "测试文件无法删除。",
        "remote_rename" => "临时文件无法改名为最终文件。",
        "remote_create_directory" => "无法创建远程日期目录。",
        "remote_conflict" => "远端存在同名但大小不同的文件。",
        "invalid_capture" => "本地截图完整性校验失败。",
        "interrupted" => "应用上次退出时中断了上传。",
        _ => "后台上传发生内部错误。",
    }
    .to_string()
}

fn upload_progress_from_record(
    status: crate::database::UploadBatchStatus,
) -> Result<upload::UploadBatchProgress, String> {
    let last_error = status
        .items
        .iter()
        .find_map(|item| item.last_error_code.as_deref())
        .map(upload_error_message_from_code);
    Ok(upload::UploadBatchProgress {
        batch_id: status.batch.id,
        state: status.batch.state,
        total_items: usize::try_from(status.batch.total_items)
            .map_err(|_| "上传批次数量无效。".to_string())?,
        total_bytes: u64::try_from(status.batch.total_bytes)
            .map_err(|_| "上传批次大小无效。".to_string())?,
        uploaded_items: usize::try_from(status.batch.completed_items)
            .map_err(|_| "上传完成数量无效。".to_string())?,
        failed_items: usize::try_from(status.batch.failed_items)
            .map_err(|_| "上传失败数量无效。".to_string())?,
        items: status
            .items
            .into_iter()
            .map(|item| upload::UploadItemProgress {
                capture_id: item.capture_id,
                state: item.state,
            })
            .collect(),
        last_error,
    })
}

async fn load_upload_progress(
    pool: &SqlitePool,
    batch_id: uuid::Uuid,
) -> Result<upload::UploadBatchProgress, String> {
    let status = crate::database::upload_batch_status(pool, batch_id)
        .await
        .map_err(|_| "无法读取后台上传状态。".to_string())?
        .ok_or_else(|| "上传批次不存在。".to_string())?;
    upload_progress_from_record(status)
}

pub(crate) struct UploadRunResult {
    pub uploaded_items: usize,
    pub failed_items: usize,
    pub fatal_error_code: Option<String>,
}

pub(crate) async fn run_upload_batch(
    app: AppHandle,
    pool: SqlitePool,
    profile: crate::database::RemoteProfileRecord,
    batch_id: uuid::Uuid,
) -> Result<UploadRunResult, ()> {
    crate::database::start_upload_batch(&pool, batch_id)
        .await
        .map_err(|_| ())?;
    let items = crate::database::upload_batch_items(&pool, batch_id)
        .await
        .map_err(|_| ())?;
    let session = match upload::RemoteSession::connect(&profile).await {
        Ok(session) => session,
        Err(error) => {
            let error_code = upload_error_code(&error);
            crate::database::fail_active_upload_batch(&pool, batch_id, upload_error_code(&error))
                .await
                .map_err(|_| ())?;
            return Ok(UploadRunResult {
                uploaded_items: 0,
                failed_items: items.len(),
                fatal_error_code: Some(error_code.to_string()),
            });
        }
    };

    let mut uploaded_items = 0_usize;
    let mut failed_items = 0_usize;
    for item in items {
        crate::database::set_upload_item_state(&pool, &item.id, "uploading", None)
            .await
            .map_err(|_| ())?;
        let capture_id = uuid::Uuid::parse_str(&item.capture_id).map_err(|_| ())?;
        let record = crate::database::capture_file(&pool, capture_id)
            .await
            .map_err(|_| ())?;
        let item_result = if let Some(record) = record {
            if record.file_size != u64::try_from(item.file_size).unwrap_or(u64::MAX)
                || record.content_sha256 != item.content_sha256
                || record.local_path != item.local_path
            {
                Err(upload::UploadError::InvalidCapture)
            } else {
                let read_app = app.clone();
                let read_result = tauri::async_runtime::spawn_blocking(move || {
                    capture_pipeline::read_saved_capture(&read_app, capture_id, &record)
                })
                .await;
                match read_result {
                    Ok(Ok(mut bytes)) => {
                        let result = session.upload(&item.remote_path, &bytes).await;
                        bytes.zeroize();
                        result
                    }
                    _ => Err(upload::UploadError::InvalidCapture),
                }
            }
        } else {
            Err(upload::UploadError::InvalidCapture)
        };
        match item_result {
            Ok(()) => {
                uploaded_items += 1;
                crate::database::set_upload_item_state(&pool, &item.id, "uploaded", None)
                    .await
                    .map_err(|_| ())?;
            }
            Err(error) => {
                failed_items += 1;
                crate::database::set_upload_item_state(
                    &pool,
                    &item.id,
                    "failed",
                    Some(upload_error_code(&error)),
                )
                .await
                .map_err(|_| ())?;
            }
        }
    }
    session.disconnect().await;
    crate::database::finish_upload_batch(&pool, batch_id, uploaded_items, failed_items)
        .await
        .map_err(|_| ())?;
    Ok(UploadRunResult {
        uploaded_items,
        failed_items,
        fatal_error_code: None,
    })
}

#[tauri::command]
pub async fn upload_selected_captures(
    app: AppHandle,
    pool: State<'_, SqlitePool>,
    capture_ids: Vec<String>,
) -> Result<upload::UploadBatchProgress, String> {
    let capture_ids = capture_ids
        .iter()
        .map(|capture_id| {
            uuid::Uuid::parse_str(capture_id).map_err(|_| "截图选择中包含无效标识。".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let profile = crate::database::remote_profile(pool.inner(), upload::profile_id())
        .await
        .map_err(|_| "无法读取远程服务器配置。".to_string())?
        .ok_or_else(|| "请先在“远程存储”中保存并测试服务器配置。".to_string())?;
    upload::validate_stored_profile(&app, &profile)
        .map_err(|error| upload_error_message(&error))?;
    let batch = crate::database::create_upload_batch(
        pool.inner(),
        upload::profile_id(),
        &capture_ids,
        "manual",
    )
    .await
    .map_err(|error| match error {
        crate::database::DatabaseError::InvalidUploadSelection => {
            "请选择 1 至 500 张不重复的截图。".to_string()
        }
        crate::database::DatabaseError::CaptureNotFound => {
            "选择中包含已经删除的截图，请刷新后重试。".to_string()
        }
        crate::database::DatabaseError::UploadAlreadyInProgress => {
            "已有一个后台上传批次正在运行。".to_string()
        }
        _ => "无法创建上传批次。".to_string(),
    })?;
    let progress = load_upload_progress(pool.inner(), batch.id).await?;
    let background_pool = pool.inner().clone();
    tauri::async_runtime::spawn(async move {
        if run_upload_batch(app, background_pool.clone(), profile, batch.id)
            .await
            .is_err()
        {
            let _ =
                crate::database::fail_active_upload_batch(&background_pool, batch.id, "internal")
                    .await;
        }
    });
    Ok(progress)
}

#[tauri::command]
pub async fn sync_today_now(app: AppHandle, pool: State<'_, SqlitePool>) -> Result<(), String> {
    crate::auto_sync::start_now(app, pool.inner().clone()).await
}

#[tauri::command]
pub async fn get_upload_batch_status(
    pool: State<'_, SqlitePool>,
    batch_id: String,
) -> Result<upload::UploadBatchProgress, String> {
    let batch_id =
        uuid::Uuid::parse_str(&batch_id).map_err(|_| "上传批次标识无效。".to_string())?;
    load_upload_progress(pool.inner(), batch_id).await
}

#[tauri::command]
pub async fn get_active_upload_batch(
    pool: State<'_, SqlitePool>,
) -> Result<Option<upload::UploadBatchProgress>, String> {
    let Some(batch_id) = crate::database::active_upload_batch_id(pool.inner())
        .await
        .map_err(|_| "无法读取后台上传状态。".to_string())?
    else {
        return Ok(None);
    };
    load_upload_progress(pool.inner(), batch_id).await.map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_product_design() {
        let settings = CaptureSettings::default();
        assert_eq!(settings.interval_minutes, 5);
        assert_eq!(settings.idle_pause_minutes, 10);
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
    fn startup_recovery_can_only_begin_once() {
        let runtime = RuntimeState::default();

        assert!(runtime.begin_startup_recovery());
        assert!(!runtime.begin_startup_recovery());
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

    #[test]
    fn skipping_a_duplicate_reschedules_without_changing_inventory() {
        let runtime = RuntimeState::from_permission_result(Ok(PermissionState::Granted));
        runtime.set_inventory(Local::now().date_naive(), 2, 600);
        let (_, generation, _) = runtime.set_state(RecordingState::Running).unwrap();

        assert!(runtime.capture_skipped(generation).is_some());
        let snapshot = runtime.snapshot().unwrap();
        assert_eq!(snapshot.today_count, 2);
        assert_eq!(snapshot.local_storage_bytes, 600);
        assert!(snapshot.next_capture_at.is_some());
        assert!(matches!(snapshot.state, RecordingState::Running));
    }

    #[test]
    fn lock_suspends_and_unlock_resumes_after_the_first_capture_delay() {
        let runtime = RuntimeState::from_permission_result(Ok(PermissionState::Granted));
        let (_, original_generation, _) = runtime.set_state(RecordingState::Running).unwrap();

        assert!(runtime
            .apply_system_event(SystemEvent::ScreenLocked)
            .unwrap()
            .is_none());
        let suspended = runtime.snapshot().unwrap();
        assert_eq!(suspended.state, RecordingState::Suspended);
        assert_eq!(
            suspended.suspension_reason,
            Some(SuspensionReason::ScreenLocked)
        );
        assert!(suspended.next_capture_at.is_none());
        assert!(!runtime.schedule_is_active(original_generation));

        let resumed = runtime
            .apply_system_event(SystemEvent::ScreenUnlocked)
            .unwrap();
        assert_eq!(resumed.map(|(_, delay)| delay), Some(FIRST_CAPTURE_DELAY));
        let snapshot = runtime.snapshot().unwrap();
        assert_eq!(snapshot.state, RecordingState::Running);
        assert!(snapshot.suspension_reason.is_none());
        assert!(snapshot.next_capture_at.is_some());
    }

    #[test]
    fn clearing_one_of_multiple_system_blockers_does_not_resume_recording() {
        let runtime = RuntimeState::from_permission_result(Ok(PermissionState::Granted));
        runtime.set_state(RecordingState::Running).unwrap();
        runtime
            .apply_system_event(SystemEvent::ScreenLocked)
            .unwrap();
        runtime.apply_system_event(SystemEvent::Sleep).unwrap();

        assert!(runtime
            .apply_system_event(SystemEvent::Wake)
            .unwrap()
            .is_none());
        let snapshot = runtime.snapshot().unwrap();
        assert_eq!(snapshot.state, RecordingState::Suspended);
        assert_eq!(
            snapshot.suspension_reason,
            Some(SuspensionReason::ScreenLocked)
        );

        assert!(runtime
            .apply_system_event(SystemEvent::ScreenUnlocked)
            .unwrap()
            .is_some());
        assert_eq!(runtime.snapshot().unwrap().state, RecordingState::Running);
    }

    #[test]
    fn user_pause_while_suspended_prevents_automatic_resume() {
        let runtime = RuntimeState::from_permission_result(Ok(PermissionState::Granted));
        runtime.set_state(RecordingState::Running).unwrap();
        runtime
            .apply_system_event(SystemEvent::UserIdle(true))
            .unwrap();

        let (snapshot, _, delay) = runtime.set_state(RecordingState::Paused).unwrap();
        assert_eq!(snapshot.state, RecordingState::Paused);
        assert!(snapshot.suspension_reason.is_none());
        assert!(delay.is_none());
        assert!(runtime
            .apply_system_event(SystemEvent::UserIdle(false))
            .unwrap()
            .is_none());
        assert_eq!(runtime.snapshot().unwrap().state, RecordingState::Paused);
    }

    #[test]
    fn starting_while_idle_preserves_user_intent_without_scheduling() {
        let runtime = RuntimeState::from_permission_result(Ok(PermissionState::Granted));
        runtime
            .apply_system_event(SystemEvent::UserIdle(true))
            .unwrap();

        let (snapshot, _, delay) = runtime.set_state(RecordingState::Running).unwrap();
        assert_eq!(snapshot.state, RecordingState::Suspended);
        assert_eq!(snapshot.suspension_reason, Some(SuspensionReason::UserIdle));
        assert!(delay.is_none());

        assert!(runtime
            .apply_system_event(SystemEvent::UserIdle(false))
            .unwrap()
            .is_some());
    }

    #[test]
    fn monitor_failure_stops_recording_and_blocks_restart() {
        let runtime = RuntimeState::from_permission_result(Ok(PermissionState::Granted));
        runtime.set_state(RecordingState::Running).unwrap();

        runtime.system_monitor_failed();

        let snapshot = runtime.snapshot().unwrap();
        assert_eq!(snapshot.state, RecordingState::Degraded);
        assert!(snapshot.next_capture_at.is_none());
        assert!(snapshot.last_error.is_some());
        assert!(matches!(
            runtime.set_state(RecordingState::Running),
            Err(AppError::SystemMonitorUnavailable)
        ));
    }

    #[test]
    fn zero_idle_pause_minutes_disables_idle_detection() {
        let runtime = RuntimeState::from_permission_result(Ok(PermissionState::Granted));
        runtime
            .update_settings(CaptureSettings {
                interval_minutes: 5,
                idle_pause_minutes: 0,
            })
            .unwrap();

        assert_eq!(runtime.idle_pause_threshold(), None);
    }

    #[test]
    fn saved_settings_can_be_restored_into_a_fresh_runtime() {
        let runtime = RuntimeState::default();
        runtime
            .restore_settings(CaptureSettings {
                interval_minutes: 30,
                idle_pause_minutes: 45,
            })
            .unwrap();

        let snapshot = runtime.snapshot().unwrap();
        assert_eq!(snapshot.settings.interval_minutes, 30);
        assert_eq!(snapshot.settings.idle_pause_minutes, 45);
    }
}
