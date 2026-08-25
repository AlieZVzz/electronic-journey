use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::UpdaterExt;

use crate::commands::{self, RecordingState};

const MAX_RELEASE_NOTES_CHARS: usize = 4_000;

#[derive(Default)]
pub struct UpdateRuntimeState {
    installing: AtomicBool,
}

impl UpdateRuntimeState {
    fn begin_install(&self) -> bool {
        self.installing
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    fn finish_install(&self) {
        self.installing.store(false, Ordering::SeqCst);
    }

    pub fn is_installing(&self) -> bool {
        self.installing.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    current_version: String,
    version: String,
    notes: Option<String>,
    published_at: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProgress {
    phase: &'static str,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
}

fn bounded_notes(notes: Option<String>) -> Option<String> {
    notes.map(|notes| notes.chars().take(MAX_RELEASE_NOTES_CHARS).collect())
}

fn updater_error(action: &str) -> String {
    format!("无法{action}。请确认网络可以访问 GitHub Releases 后重试。")
}

#[tauri::command]
pub fn get_app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
pub async fn check_for_app_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let current_version = app.package_info().version.to_string();
    let updater = app.updater().map_err(|_| updater_error("初始化更新检查"))?;
    let update = updater
        .check()
        .await
        .map_err(|_| updater_error("检查更新"))?;

    Ok(update.map(|update| UpdateInfo {
        current_version,
        version: update.version,
        notes: bounded_notes(update.body),
        published_at: update.date.map(|date| date.to_string()),
    }))
}

async fn install_checked_update(app: &AppHandle, expected_version: &str) -> Result<(), String> {
    let pool = app.state::<sqlx::SqlitePool>().inner().clone();
    if crate::database::active_upload_batch_id(&pool)
        .await
        .map_err(|_| "无法确认当前上传状态，更新未开始。".to_string())?
        .is_some()
    {
        return Err("当前仍有上传任务。请等待上传完成或明确取消后再安装更新。".to_string());
    }

    commands::apply_recording_state(app, RecordingState::Stopped)
        .map_err(|_| "无法安全停止截图，更新未开始。".to_string())?;
    crate::tray::refresh(app);

    let updater = app.updater().map_err(|_| updater_error("初始化更新安装"))?;
    let update = updater
        .check()
        .await
        .map_err(|_| updater_error("重新确认更新"))?
        .ok_or_else(|| "当前已经是最新版本，无需安装。".to_string())?;
    if update.version != expected_version {
        return Err("可用版本已经变化，请重新检查并确认更新。".to_string());
    }

    let progress_app = app.clone();
    let finished_app = app.clone();
    let downloaded_bytes = Arc::new(AtomicU64::new(0));
    let progress_bytes = downloaded_bytes.clone();
    let finished_bytes = downloaded_bytes;
    update
        .download_and_install(
            move |chunk_length, total_bytes| {
                let chunk_length = u64::try_from(chunk_length).unwrap_or(0);
                let downloaded_bytes = progress_bytes
                    .fetch_add(chunk_length, Ordering::SeqCst)
                    .saturating_add(chunk_length);
                let _ = progress_app.emit(
                    "app-update-progress",
                    UpdateProgress {
                        phase: "downloading",
                        downloaded_bytes,
                        total_bytes,
                    },
                );
            },
            move || {
                let downloaded_bytes = finished_bytes.load(Ordering::SeqCst);
                let _ = finished_app.emit(
                    "app-update-progress",
                    UpdateProgress {
                        phase: "installing",
                        downloaded_bytes,
                        total_bytes: Some(downloaded_bytes),
                    },
                );
            },
        )
        .await
        .map_err(|_| {
            "更新包下载、签名验证或安装失败。应用数据没有被修改，请重试或使用 GitHub Release 安装包。"
                .to_string()
        })?;

    app.request_restart();
    Ok(())
}

#[tauri::command]
pub async fn install_app_update(
    app: AppHandle,
    state: State<'_, UpdateRuntimeState>,
    expected_version: String,
) -> Result<(), String> {
    if expected_version.trim().is_empty() {
        return Err("目标版本无效，请重新检查更新。".to_string());
    }
    if !state.begin_install() {
        return Err("更新安装已经在进行中。".to_string());
    }

    let result = install_checked_update(&app, expected_version.trim()).await;
    if result.is_err() {
        state.finish_install();
    }
    result
}

pub fn ensure_not_installing(app: &AppHandle) -> Result<(), String> {
    if app.state::<UpdateRuntimeState>().is_installing() {
        Err("应用正在准备安装更新，暂时不能开始截图或上传。".to_string())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_install_guard_is_exclusive_and_recoverable() {
        let state = UpdateRuntimeState::default();
        assert!(state.begin_install());
        assert!(state.is_installing());
        assert!(!state.begin_install());
        state.finish_install();
        assert!(!state.is_installing());
        assert!(state.begin_install());
    }

    #[test]
    fn release_notes_are_bounded_before_reaching_the_webview() {
        let notes = "a".repeat(MAX_RELEASE_NOTES_CHARS + 20);
        assert_eq!(
            bounded_notes(Some(notes)).unwrap().chars().count(),
            MAX_RELEASE_NOTES_CHARS
        );
    }
}
