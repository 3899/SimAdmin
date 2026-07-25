use crate::automation::tasks::TaskRegistry;
use crate::config::{AutomationAction, AutomationTask, AutomationTrigger};
use crate::db::beijing_sms_now_string;
use crate::notification::AutomationEvent;
use crate::state::AppState;
use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDateTime, TimeZone, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info, warn};

fn beijing_offset() -> FixedOffset {
    FixedOffset::east_opt(8 * 60 * 60).unwrap()
}

fn beijing_now() -> DateTime<FixedOffset> {
    Utc::now().with_timezone(&beijing_offset())
}

pub fn spawn_automation_scheduler(app: AppState) {
    tokio::spawn(async move {
        info!("Starting automation center scheduler...");
        let registry = Arc::new(TaskRegistry::new());

        // 用于防止定点定时任务在同一分钟内重复运行
        // 键为 task_id，值为执行时的分钟数字符串，例如 "2026-06-10 04:00"
        let mut fixed_last_run: HashMap<String, String> = HashMap::new();

        loop {
            // 每隔 30 秒执行一次评估
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;

            let config = app.config_manager.get_automation_config();
            if !config.enabled {
                continue;
            }

            for task in config.tasks {
                if !task.enabled {
                    continue;
                }

                // 判断是否应当触发
                let should_trigger = match &task.trigger {
                    AutomationTrigger::Fixed { weekdays, times } => {
                        let now = beijing_now();
                        let day_of_week = now.weekday().number_from_monday() as u8; // 1 to 7
                        let current_minute_str = now.format("%H:%M").to_string();

                        if weekdays.contains(&day_of_week) && times.contains(&current_minute_str) {
                            let unique_minute = now.format("%Y-%m-%d %H:%M").to_string();
                            // 检查是否在此分钟内已经运行过
                            if fixed_last_run.get(&task.id) == Some(&unique_minute) {
                                false
                            } else {
                                fixed_last_run.insert(task.id.clone(), unique_minute);
                                true
                            }
                        } else {
                            false
                        }
                    }
                    AutomationTrigger::Interval {
                        interval_value,
                        interval_unit,
                    } => {
                        // 查询上一次运行历史
                        let last_log = match app.database.get_last_log_for_task(&task.id) {
                            Ok(res) => res,
                            Err(e) => {
                                error!("Failed to query last log for task {}: {:?}", task.id, e);
                                None
                            }
                        };

                        match last_log {
                            Some(log) => {
                                if let Ok(parsed) = NaiveDateTime::parse_from_str(
                                    &log.created_at,
                                    "%Y-%m-%d %H:%M:%S",
                                ) {
                                    let last_run_time =
                                        beijing_offset().from_local_datetime(&parsed).unwrap();
                                    let now = beijing_now();

                                    let duration = match interval_unit.as_str() {
                                        "mins" => Duration::minutes(*interval_value as i64),
                                        "hours" => Duration::hours(*interval_value as i64),
                                        "days" => Duration::days(*interval_value as i64),
                                        _ => Duration::days(180), // 默认 Giffgaff 保号大间隔
                                    };

                                    now.signed_duration_since(last_run_time) >= duration
                                } else {
                                    true
                                }
                            }
                            None => true, // 从无历史记录，触发首次运行
                        }
                    }
                };

                if should_trigger {
                    let registry_clone = registry.clone();
                    let app_clone = app.clone();
                    let task_clone = task.clone();

                    tokio::spawn(async move {
                        if let Err(e) = execute_task(&app_clone, &registry_clone, &task_clone).await
                        {
                            error!("Automation task {} failed: {:?}", task_clone.id, e);
                        }
                    });
                }
            }

            // 定期执行自动清理策略 (清理旧的自动化日志)
            let config_notifications = app.config_manager.get_notifications();
            let cleanup = config_notifications.log_cleanup;
            let retention_days = if cleanup.retention_days_enabled {
                Some(cleanup.retention_days)
            } else {
                None
            };
            let max_entries = if cleanup.max_entries_enabled {
                Some(cleanup.max_entries)
            } else {
                None
            };
            if retention_days.is_some() || max_entries.is_some() {
                let _ = app
                    .database
                    .cleanup_automation_logs(retention_days, max_entries);
            }
        }
    });
}

async fn execute_task(
    app: &AppState,
    registry: &TaskRegistry,
    task: &AutomationTask,
) -> Result<()> {
    info!("Triggering automation task: {} ({})", task.name, task.id);

    let task_type = match &task.action {
        AutomationAction::RestartBaseband => "restart_baseband",
        AutomationAction::RebootDevice { .. } => "reboot_device",
        AutomationAction::BackupData { .. } => "backup_data",
        AutomationAction::SendSms { .. } => "send_sms",
    };

    let handler = match registry.get(task_type) {
        Some(h) => h,
        None => {
            let err_msg = format!("No handler found for task type: {}", task_type);
            let _ = app
                .database
                .insert_automation_log(&task.id, &task.name, task_type, "failed", &err_msg);
            return Err(anyhow::anyhow!(err_msg));
        }
    };

    let mut delay_secs = 0u64;
    // 参数转换
    let params = match &task.action {
        AutomationAction::RestartBaseband => serde_json::Value::Null,
        AutomationAction::RebootDevice { delay_seconds } => {
            serde_json::json!({ "delay_seconds": delay_seconds })
        }
        AutomationAction::BackupData {
            components,
            storage,
        } => {
            serde_json::json!({
                "components": components,
                "storage": storage,
            })
        }
        AutomationAction::SendSms {
            phone_number,
            content,
            random_delay_seconds,
            retry_limit,
        } => {
            delay_secs = u64::from(random_delay_seconds.unwrap_or(0));
            serde_json::json!({
                "phone_number": phone_number,
                "content": content,
                "random_delay_seconds": random_delay_seconds,
                "retry_limit": retry_limit
            })
        }
    };

    // 执行任务并控制超时（基准60秒 + 随机延迟时间）
    let result = tokio::time::timeout(
        tokio::time::Duration::from_secs(60 + delay_secs),
        handler.execute(app, &params),
    )
    .await;

    let (status, detail) = match result {
        Ok(Ok(_)) => ("success", "执行成功".to_string()),
        Ok(Err(e)) => ("failed", format!("执行失败: {}", e)),
        Err(_) => ("failed", "执行超时 (超过60秒限制)".to_string()),
    };

    // 1. 写入 SQLite 日志表
    let _ = app
        .database
        .insert_automation_log(&task.id, &task.name, task_type, status, &detail);

    // 2. 发出通知事件
    let event = AutomationEvent {
        task_id: task.id.clone(),
        task_name: task.name.clone(),
        task_type: task_type.to_string(),
        status: status.to_string(),
        message: detail.clone(),
        timestamp: beijing_sms_now_string(),
    };

    if let Err(e) = app
        .notification_sender
        .forward_automation_event(&event)
        .await
    {
        warn!("Failed to forward automation notification event: {:?}", e);
    }

    Ok(())
}
