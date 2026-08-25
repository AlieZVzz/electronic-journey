use std::time::Duration;

use chrono::{DateTime, Local, TimeZone, Utc};
use sqlx::SqlitePool;
use tauri::{AppHandle, Manager};

use crate::{commands, database, upload};

const AUTO_SYNC_CHECK_INTERVAL: Duration = Duration::from_secs(30);
const AUTO_SYNC_BATCH_SIZE: usize = 500;

fn local_day_bounds(now: DateTime<Local>) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let date = now.date_naive();
    let next_date = date.succ_opt()?;
    let start = Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0)?)
        .earliest()?;
    let end = Local
        .from_local_datetime(&next_date.and_hms_opt(0, 0, 0)?)
        .earliest()?;
    Some((start.with_timezone(&Utc), end.with_timezone(&Utc)))
}

fn suspends_automatic_sync(error_code: &str) -> bool {
    matches!(
        error_code,
        "invalid_profile"
            | "invalid_key_path"
            | "invalid_key_file"
            | "credential_store"
            | "host_key_mismatch"
            | "authentication"
    )
}

async fn finish_cycle(
    pool: &SqlitePool,
    state: &str,
    uploaded_items: usize,
    failed_items: usize,
    suspended_reason: Option<&str>,
) {
    let _ = database::record_auto_sync_result(
        pool,
        upload::profile_id(),
        state,
        uploaded_items,
        failed_items,
        suspended_reason,
    )
    .await;
}

async fn run_claimed_cycle(
    app: AppHandle,
    pool: SqlitePool,
    profile: database::RemoteProfileRecord,
) {
    if let Err(error) = upload::validate_stored_profile(&app, &profile) {
        let code = match error {
            upload::UploadError::InvalidProfile => "invalid_profile",
            upload::UploadError::InvalidKeyPath => "invalid_key_path",
            upload::UploadError::InvalidKeyFile => "invalid_key_file",
            _ => "invalid_profile",
        };
        finish_cycle(&pool, "suspended", 0, 0, Some(code)).await;
        return;
    }
    if database::active_upload_batch_id(&pool)
        .await
        .ok()
        .flatten()
        .is_some()
    {
        finish_cycle(&pool, "skipped_busy", 0, 0, None).await;
        return;
    }
    let Some((start_utc, end_utc)) = local_day_bounds(Local::now()) else {
        finish_cycle(&pool, "partial_failed", 0, 0, None).await;
        return;
    };

    let mut total_uploaded = 0_usize;
    let mut total_failed = 0_usize;
    loop {
        let capture_ids =
            match database::unsynced_capture_ids(&pool, start_utc, end_utc, AUTO_SYNC_BATCH_SIZE)
                .await
            {
                Ok(ids) => ids,
                Err(_) => {
                    finish_cycle(&pool, "partial_failed", total_uploaded, total_failed, None).await;
                    return;
                }
            };
        if capture_ids.is_empty() {
            let state = if total_uploaded == 0 {
                "empty"
            } else {
                "completed"
            };
            finish_cycle(&pool, state, total_uploaded, total_failed, None).await;
            return;
        }
        let batch = match database::create_upload_batch(
            &pool,
            upload::profile_id(),
            &capture_ids,
            "automatic",
        )
        .await
        {
            Ok(batch) => batch,
            Err(database::DatabaseError::UploadAlreadyInProgress) => {
                finish_cycle(&pool, "skipped_busy", total_uploaded, total_failed, None).await;
                return;
            }
            Err(_) => {
                finish_cycle(&pool, "partial_failed", total_uploaded, total_failed, None).await;
                return;
            }
        };
        let batch_size = capture_ids.len();
        let result =
            commands::run_upload_batch(app.clone(), pool.clone(), profile.clone(), batch.id).await;
        let result = match result {
            Ok(result) => result,
            Err(()) => {
                let _ = database::fail_active_upload_batch(&pool, batch.id, "internal").await;
                total_failed = total_failed.saturating_add(batch_size);
                finish_cycle(&pool, "partial_failed", total_uploaded, total_failed, None).await;
                return;
            }
        };
        total_uploaded = total_uploaded.saturating_add(result.uploaded_items);
        total_failed = total_failed.saturating_add(result.failed_items);
        if let Some(code) = result.fatal_error_code.as_deref() {
            let suspended = suspends_automatic_sync(code);
            finish_cycle(
                &pool,
                if suspended {
                    "suspended"
                } else {
                    "partial_failed"
                },
                total_uploaded,
                total_failed,
                suspended.then_some(code),
            )
            .await;
            return;
        }
        if result.failed_items > 0 {
            finish_cycle(&pool, "partial_failed", total_uploaded, total_failed, None).await;
            return;
        }
    }
}

async fn claim_and_spawn(app: AppHandle, pool: SqlitePool, force: bool) -> Result<bool, String> {
    if app
        .state::<crate::app_update::UpdateRuntimeState>()
        .is_installing()
    {
        return Ok(false);
    }
    let profile = database::claim_auto_sync(&pool, upload::profile_id(), Utc::now(), force)
        .await
        .map_err(|_| "无法读取或更新自动同步计划。".to_string())?;
    let Some(profile) = profile else {
        return Ok(false);
    };
    tauri::async_runtime::spawn(run_claimed_cycle(app, pool, profile));
    Ok(true)
}

pub async fn start_now(app: AppHandle, pool: SqlitePool) -> Result<(), String> {
    let profile = database::remote_profile(&pool, upload::profile_id())
        .await
        .map_err(|_| "无法读取远程服务器配置。".to_string())?
        .ok_or_else(|| "请先保存远程服务器配置。".to_string())?;
    if !profile.auto_sync_enabled {
        return Err("请先启用自动同步并保存配置。".to_string());
    }
    if profile.auto_sync_suspended_reason.is_some() {
        return Err("自动同步已暂停，请重新测试连接或保存配置后再试。".to_string());
    }
    if database::active_upload_batch_id(&pool)
        .await
        .map_err(|_| "无法读取当前上传状态。".to_string())?
        .is_some()
    {
        return Err("已有一个上传批次正在运行。".to_string());
    }
    if !claim_and_spawn(app, pool, true).await? {
        return Err("自动同步计划当前不可用。".to_string());
    }
    Ok(())
}

pub fn spawn_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut timer = tokio::time::interval(AUTO_SYNC_CHECK_INTERVAL);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            timer.tick().await;
            let pool = app.state::<SqlitePool>().inner().clone();
            let _ = claim_and_spawn(app.clone(), pool, false).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_identity_and_credential_failures_suspend_future_runs() {
        assert!(suspends_automatic_sync("host_key_mismatch"));
        assert!(suspends_automatic_sync("authentication"));
        assert!(suspends_automatic_sync("invalid_key_file"));
        assert!(!suspends_automatic_sync("connection"));
        assert!(!suspends_automatic_sync("remote_write"));
    }

    #[test]
    fn local_day_bounds_cover_the_current_calendar_day() {
        let bounds = local_day_bounds(Local::now()).unwrap();
        assert!(bounds.0 < Utc::now());
        assert!(bounds.1 > Utc::now());
        assert!(bounds.1 - bounds.0 >= chrono::Duration::hours(23));
        assert!(bounds.1 - bounds.0 <= chrono::Duration::hours(25));
    }
}
