use crate::config::{
    BarkConfig, ConfigManager, DingtalkAppConfig, DingtalkRobotConfig, EmailConfig,
    FeishuRobotConfig, LegacyNotificationConfig, MatcherOperator, MessageChannelConfig,
    NotificationChannel, NotificationChannelInstance, NotificationConfig, NotificationEventType,
    NotificationRule, PushPlusConfig, QuietHoursSchedule, ServerChan3Config, TelegramConfig,
    WebhookConfig, WecomAppConfig, WecomRobotConfig,
};
use crate::db::{
    CallRecord, Database, NewNotificationQueueItem, NotificationQueueEntry, SmsMessage,
};
use crate::device_status::DeviceStatusReport;
use crate::models::{DdnsEvent, VersionUpdateEvent};
use crate::modem_manager::get_sim_info_data_with_cache;
use crate::system_event::SystemEvent;
use crate::verification_code::extract_verification_code;
use base64::{engine::general_purpose, Engine as _};
use chrono::{
    DateTime, Datelike, Duration as ChronoDuration, FixedOffset, NaiveDateTime, Timelike, Utc,
};
use lettre::message::{Mailbox, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS, NON_ALPHANUMERIC};
use reqwest::{Client, StatusCode};
use ring::hmac;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::warn;
use zbus::Connection;

const BEIJING_UTC_OFFSET_SECONDS: i32 = 8 * 60 * 60;
const NOTIFICATION_TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
const QUERY_VALUE_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'!')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

/// Notification sender for all configured notification channels.
pub struct NotificationSender {
    client: Client,
    config_manager: Arc<ConfigManager>,
    dbus_conn: Arc<Connection>,
    database: Arc<Database>,
    wecom_token_cache: tokio::sync::Mutex<HashMap<(String, String, String), WecomTokenCacheEntry>>,
}

struct WecomTokenCacheEntry {
    token: String,
    refresh_at: Instant,
}

struct WecomTokenResponse {
    access_token: String,
    expires_in: Option<u64>,
}

enum WecomMessageError {
    InvalidAccessToken(String),
    Other(String),
}

pub struct NotificationFanoutResult {
    pub delivered: bool,
    pub errors: Vec<String>,
}

#[derive(Default)]
struct NotificationTemplateContext {
    own_number: String,
    carrier: String,
}

#[derive(Default)]
struct NotificationRouteResult {
    attempted: bool,
    delivered: bool,
    has_failures: bool,
    errors: Vec<String>,
}

enum ChannelDeliveryResult {
    Sent(String),
    Queued(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationEvent {
    pub task_id: String,
    pub task_name: String,
    pub task_type: String,
    pub status: String,
    pub message: String,
    pub timestamp: String,
}

enum NotificationEvent<'a> {
    Sms {
        message: &'a SmsMessage,
        context: &'a NotificationTemplateContext,
    },
    Ddns(&'a DdnsEvent, &'a NotificationTemplateContext),
    VersionUpdate(&'a VersionUpdateEvent, &'a NotificationTemplateContext),
    SystemEvent(&'a SystemEvent, &'a NotificationTemplateContext),
    DeviceStatus(&'a DeviceStatusReport, &'a NotificationTemplateContext),
    Automation(&'a AutomationEvent, &'a NotificationTemplateContext),
}

impl NotificationEvent<'_> {
    fn event_type(&self) -> NotificationEventType {
        match self {
            NotificationEvent::Sms { .. } => NotificationEventType::Sms,
            NotificationEvent::Ddns(..) => NotificationEventType::Ddns,
            NotificationEvent::VersionUpdate(..) => NotificationEventType::VersionUpdate,
            NotificationEvent::SystemEvent(..) => NotificationEventType::SystemEvent,
            NotificationEvent::DeviceStatus(..) => NotificationEventType::DeviceStatus,
            NotificationEvent::Automation(..) => NotificationEventType::Automation,
        }
    }

    fn title(&self) -> String {
        match self {
            NotificationEvent::Sms { .. } => "SimAdmin 短信通知".to_string(),
            NotificationEvent::Ddns(..) => "SimAdmin DDNS 通知".to_string(),
            NotificationEvent::VersionUpdate(..) => "SimAdmin 版本更新".to_string(),
            NotificationEvent::SystemEvent(event, _) => {
                format!("SimAdmin 系统事件 - {}", event.event_label)
            }
            NotificationEvent::DeviceStatus(..) => "SimAdmin 设备状态".to_string(),
            NotificationEvent::Automation(event, _) => {
                format!("SimAdmin 自动化 - {}", event.task_name)
            }
        }
    }

    fn summary(&self) -> String {
        match self {
            NotificationEvent::Sms { message, .. } => {
                compact_summary(&format!("[{}] {}", message.phone_number, message.content))
            }
            NotificationEvent::Ddns(event, _) => compact_summary(&format!(
                "{} {} {}",
                event.domains.join(", "),
                event.status,
                event.message
            )),
            NotificationEvent::VersionUpdate(event, _) => {
                compact_summary(&format!("{} {}", event.version, event.asset_name))
            }
            NotificationEvent::SystemEvent(event, _) => compact_summary(&format!(
                "{} {} {}",
                event.event_label, event.status_label, event.message
            )),
            NotificationEvent::DeviceStatus(..) => "设备状态定时报表".to_string(),
            NotificationEvent::Automation(event, _) => {
                compact_summary(&format!("[{}] {}", event.task_name, event.message))
            }
        }
    }

    fn field_value(&self, field: &str) -> String {
        match self {
            NotificationEvent::Sms { message, context } => match field {
                "phone_number" => message.phone_number.clone(),
                "content" => message.content.clone(),
                "own_number" => context.own_number.clone(),
                "carrier" | "operator" => context.carrier.clone(),
                "verification_code" => {
                    extract_verification_code(&message.content).unwrap_or_default()
                }
                "direction" => message.direction.clone(),
                "status" => message.status.clone(),
                _ => self.summary(),
            },
            NotificationEvent::Ddns(event, context) => match field {
                "domains" | "domain" => event.domains.join(", "),
                "provider" => event.provider.clone(),
                "record_type" => event.record_type.clone(),
                "status" => event.status.clone(),
                "message" => event.message.clone(),
                "new_ip" => event.new_ip.clone().unwrap_or_default(),
                "old_ip" => event.old_ip.clone().unwrap_or_default(),
                "failure_count" => event.failure_count.to_string(),
                "own_number" => context.own_number.clone(),
                "carrier" | "operator" => context.carrier.clone(),
                _ => self.summary(),
            },
            NotificationEvent::VersionUpdate(event, context) => match field {
                "asset_name" => event.asset_name.clone(),
                "version" => event.version.clone(),
                "build_time" => event.build_time.clone(),
                "own_number" => common_own_number(context, &event.own_number).to_string(),
                "carrier" | "operator" => context.carrier.clone(),
                _ => self.summary(),
            },
            NotificationEvent::SystemEvent(event, context) => match field {
                "category" => event.category.clone(),
                "category_label" => event.category_label.clone(),
                "event_code" => event.event_code.clone(),
                "event_label" => event.event_label.clone(),
                "severity" => event.severity.clone(),
                "severity_label" => event.severity_label.clone(),
                "status" => event.status.clone(),
                "status_label" => event.status_label.clone(),
                "entity" => event.entity.clone(),
                "message" => event.message.clone(),
                "own_number" => context.own_number.clone(),
                "carrier" | "operator" => context.carrier.clone(),
                _ => self.summary(),
            },
            NotificationEvent::DeviceStatus(report, context) => match field {
                "status_content" | "content" => report.text(),
                "timestamp" => report.timestamp.clone(),
                "own_number" => context.own_number.clone(),
                "carrier" | "operator" => context.carrier.clone(),
                _ => self.summary(),
            },
            NotificationEvent::Automation(event, context) => match field {
                "task_id" => event.task_id.clone(),
                "task_name" => event.task_name.clone(),
                "task_type" => event.task_type.clone(),
                "status" => event.status.clone(),
                "message" => event.message.clone(),
                "timestamp" => event.timestamp.clone(),
                "own_number" => context.own_number.clone(),
                "carrier" | "operator" => context.carrier.clone(),
                _ => self.summary(),
            },
        }
    }

    fn render(&self, template: &str) -> String {
        let template = if template.trim().is_empty() {
            crate::config::default_rule_template(self.event_type())
        } else {
            template.to_string()
        };
        match self {
            NotificationEvent::Sms { message, context } => {
                render_sms_template(&template, message, context, false)
            }
            NotificationEvent::Ddns(event, context) => {
                render_ddns_template(&template, event, context, false)
            }
            NotificationEvent::VersionUpdate(event, context) => {
                render_version_update_template(&template, event, context, false)
            }
            NotificationEvent::SystemEvent(event, context) => {
                render_system_event_template(&template, event, context, false)
            }
            NotificationEvent::DeviceStatus(report, context) => {
                render_device_status_template(&template, report, context, false)
            }
            NotificationEvent::Automation(event, context) => {
                render_automation_template(&template, event, context, false)
            }
        }
    }

    fn render_title(&self, title_template: &str) -> String {
        let use_default = title_template.trim().is_empty();
        let default_template = crate::config::default_rule_title_template(self.event_type());
        if let NotificationEvent::Sms { message, .. } = self {
            if (use_default || title_template.trim() == default_template)
                && extract_verification_code(&message.content).is_none()
            {
                return message.phone_number.clone();
            }
        }

        let template = if use_default {
            default_template
        } else {
            title_template.to_string()
        };
        let title = self.render(&template);
        if title.trim().is_empty() {
            self.title()
        } else {
            title
        }
    }
}

#[allow(dead_code)]
impl NotificationSender {
    /// Create a new sender.
    pub fn new(
        config_manager: Arc<ConfigManager>,
        dbus_conn: Arc<Connection>,
        database: Arc<Database>,
    ) -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("Failed to create HTTP client"),
            config_manager,
            dbus_conn,
            database,
            wecom_token_cache: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    fn get_config(&self) -> NotificationConfig {
        self.config_manager.get_notifications()
    }

    pub async fn get_own_number(&self) -> String {
        get_sim_info_data_with_cache(self.dbus_conn.as_ref(), Some(self.database.as_ref()))
            .await
            .ok()
            .map(|sim| format_own_numbers_for_template(&sim.phone_numbers))
            .unwrap_or_default()
    }

    async fn notification_template_context(&self) -> NotificationTemplateContext {
        let own_number = self.get_own_number().await;

        let carrier = crate::modem_manager::get_network_info_data(self.dbus_conn.as_ref())
            .await
            .ok()
            .map(|net| net.operator_name)
            .unwrap_or_default();

        NotificationTemplateContext {
            own_number,
            carrier,
        }
    }

    /// Forward an incoming SMS to all enabled channels.
    pub async fn forward_sms(&self, message: &SmsMessage) -> Result<(), String> {
        let context = self.notification_template_context().await;
        let event = NotificationEvent::Sms {
            message,
            context: &context,
        };
        let result = self.route_event(&event).await;

        let notification_status = if result.delivered {
            "success"
        } else if result.attempted && result.has_failures {
            "failed"
        } else {
            "skipped"
        };

        if message.id > 0 {
            if let Err(err) = self
                .database
                .update_sms_notification_status(message.id, notification_status)
            {
                warn!(
                    error = %err,
                    sms_id = message.id,
                    notification_status = %notification_status,
                    "Failed to update SMS notification status"
                );
            }
        }

        if result.delivered && !result.errors.is_empty() {
            warn!(
                sms_id = message.id,
                errors = %result.errors.join("; "),
                "SMS notification partially failed"
            );
        }

        if result.errors.is_empty() || result.delivered {
            Ok(())
        } else {
            Err(result.errors.join("; "))
        }
    }

    /// Forward a call record to all enabled channels.
    #[allow(dead_code)]
    pub async fn forward_call(&self, _call: &CallRecord) -> Result<(), String> {
        Ok(())
    }

    /// Forward a DDNS update/failure event to all enabled channels.
    pub async fn forward_ddns_event(&self, event: &DdnsEvent) -> Result<(), String> {
        let context = self.notification_template_context().await;
        let event = NotificationEvent::Ddns(event, &context);
        let result = self.route_event(&event).await;

        if result.errors.is_empty() || result.delivered {
            Ok(())
        } else {
            Err(result.errors.join("; "))
        }
    }

    /// Forward an automation task execution event to all enabled channels.
    pub async fn forward_automation_event(&self, event: &AutomationEvent) -> Result<(), String> {
        let context = self.notification_template_context().await;
        let event = NotificationEvent::Automation(event, &context);
        let result = self.route_event(&event).await;

        if result.errors.is_empty() || result.delivered {
            Ok(())
        } else {
            Err(result.errors.join("; "))
        }
    }

    pub fn has_version_update_targets(&self) -> bool {
        let config = self.get_config();
        config.rules.iter().any(|rule| {
            rule.enabled
                && rule.event_type == NotificationEventType::VersionUpdate
                && rule.channel_ids.iter().any(|channel_id| {
                    config
                        .channels
                        .iter()
                        .any(|channel| channel.enabled && channel.id == *channel_id)
                })
        })
    }

    pub fn system_event_enabled(&self, event_code: &str) -> bool {
        let config = self.get_config();
        config.rules.iter().any(|rule| {
            rule.enabled
                && rule.event_type == NotificationEventType::SystemEvent
                && rule
                    .event_codes
                    .iter()
                    .any(|enabled_code| enabled_code == event_code)
        })
    }

    /// Forward a newly available version update to enabled channels.
    pub async fn forward_version_update_event(
        &self,
        event: &VersionUpdateEvent,
    ) -> Result<NotificationFanoutResult, String> {
        let context = self.notification_template_context().await;
        let event = NotificationEvent::VersionUpdate(event, &context);
        let result = self.route_event(&event).await;

        if result.delivered || result.errors.is_empty() {
            Ok(NotificationFanoutResult {
                delivered: result.delivered,
                errors: result.errors,
            })
        } else {
            Err(result.errors.join("; "))
        }
    }

    pub async fn forward_system_event(&self, event: &SystemEvent) -> Result<(), String> {
        let context = self.notification_template_context().await;
        let event = NotificationEvent::SystemEvent(event, &context);
        let result = self.route_event(&event).await;

        if result.errors.is_empty() || result.delivered {
            Ok(())
        } else {
            Err(result.errors.join("; "))
        }
    }

    pub async fn forward_device_status_report(
        &self,
        rule_id: &str,
        report: &DeviceStatusReport,
    ) -> Result<(), String> {
        let context = self.notification_template_context().await;
        let event = NotificationEvent::DeviceStatus(report, &context);
        let result = self.route_event_for_rule(&event, Some(rule_id)).await;

        if result.errors.is_empty() || result.delivered {
            Ok(())
        } else {
            Err(result.errors.join("; "))
        }
    }

    /// Test a specific notification channel with a simulated SMS.
    pub async fn test_channel(&self, target: &str) -> Result<String, String> {
        let config = self.get_config();
        let channel = config
            .channels
            .iter()
            .find(|channel| channel.id == target)
            .or_else(|| {
                serde_json::from_value::<NotificationChannel>(json!(target))
                    .ok()
                    .and_then(|channel_type| {
                        config
                            .channels
                            .iter()
                            .find(|channel| channel.channel_type == channel_type)
                    })
            })
            .ok_or_else(|| "Notification channel is not configured".to_string())?;

        let channel_type = channel.channel_type.label();
        let text = format!(
            "{channel_type} 信使打卡成功✅\n服务支持：SimAdmin 开源项目\n简介：一站式 SIM/eSIM 蜂窝设备管理系统\nGitHub：https://github.com/3899/SimAdmin"
        );

        self.send_text_to_channel(channel, &format!("{channel_type} 信使打卡成功✅"), &text)
            .await
    }

    async fn route_event(&self, event: &NotificationEvent<'_>) -> NotificationRouteResult {
        self.route_event_for_rule(event, None).await
    }

    async fn route_event_for_rule(
        &self,
        event: &NotificationEvent<'_>,
        target_rule_id: Option<&str>,
    ) -> NotificationRouteResult {
        let config = self.get_config();
        let mut result = NotificationRouteResult::default();
        let summary = event.summary();
        let mut matched_rules = 0usize;

        for rule in config.rules.iter().filter(|rule| {
            rule.enabled
                && rule.event_type == event.event_type()
                && target_rule_id
                    .map(|target| rule.id == target)
                    .unwrap_or(true)
        }) {
            if !rule_matches(rule, event) {
                continue;
            }
            if let NotificationEvent::Automation(auto_event, _) = event {
                let match_code = format!("{}:{}", auto_event.task_type, auto_event.status);
                if !rule.event_codes.contains(&match_code) {
                    continue;
                }
            }
            matched_rules += 1;

            if ddns_failure_threshold_pending(rule, event) {
                continue;
            }

            let text = event.render(&rule.template);
            let log_summary = match event.event_type() {
                NotificationEventType::SystemEvent | NotificationEventType::DeviceStatus => {
                    text.as_str()
                }
                _ => summary.as_str(),
            };

            if rule.channel_ids.is_empty() {
                self.record_notification_log(
                    event.event_type(),
                    "no_available_channel",
                    log_summary,
                    Some(rule),
                    None,
                    "规则未选择通知通道",
                );
                continue;
            }

            let quiet = quiet_hours_active(&rule.quiet_hours);
            for channel_id in &rule.channel_ids {
                result.attempted = true;
                let channel = config.channels.iter().find(|item| item.id == *channel_id);
                let Some(channel) = channel else {
                    self.record_notification_log(
                        event.event_type(),
                        "no_available_channel",
                        log_summary,
                        Some(rule),
                        None,
                        "通知通道不存在",
                    );
                    continue;
                };

                if quiet {
                    self.record_notification_log(
                        event.event_type(),
                        "quiet_hours",
                        log_summary,
                        Some(rule),
                        Some(channel),
                        "免打扰时间段内，已跳过发送",
                    );
                    continue;
                }

                if !channel.enabled {
                    self.record_notification_log(
                        event.event_type(),
                        "no_available_channel",
                        log_summary,
                        Some(rule),
                        Some(channel),
                        "通知通道已停用",
                    );
                    continue;
                }

                let title = event.render_title(&rule.title_template);
                match self
                    .send_text_to_channel_with_queue(
                        event,
                        rule,
                        channel,
                        &title,
                        &text,
                        log_summary,
                    )
                    .await
                {
                    Ok(ChannelDeliveryResult::Sent(message)) => {
                        result.delivered = true;
                        self.record_notification_log(
                            event.event_type(),
                            "success",
                            log_summary,
                            Some(rule),
                            Some(channel),
                            &message,
                        );
                    }
                    Ok(ChannelDeliveryResult::Queued(message)) => {
                        result.has_failures = true;
                        result.errors.push(format!("{}: {}", channel.name, message));
                        self.record_notification_log(
                            event.event_type(),
                            "failed",
                            log_summary,
                            Some(rule),
                            Some(channel),
                            &message,
                        );
                    }
                    Err(err) => {
                        result.has_failures = true;
                        result.errors.push(format!("{}: {}", channel.name, err));
                        self.record_notification_log(
                            event.event_type(),
                            "failed",
                            log_summary,
                            Some(rule),
                            Some(channel),
                            &err,
                        );
                    }
                }
            }
        }

        if matched_rules == 0
            && event.event_type() != NotificationEventType::SystemEvent
            && target_rule_id.is_none()
        {
            self.record_notification_log(
                event.event_type(),
                "unmatched",
                &summary,
                None,
                None,
                "没有匹配的启用转发规则",
            );
        }

        result
    }

    fn record_notification_log(
        &self,
        event_type: NotificationEventType,
        status: &str,
        summary: &str,
        rule: Option<&NotificationRule>,
        channel: Option<&NotificationChannelInstance>,
        message: &str,
    ) {
        let (rule_id, rule_name) = rule
            .map(|rule| (rule.id.as_str(), rule.name.as_str()))
            .unwrap_or(("", ""));
        let (channel_id, channel_name) = channel
            .map(|channel| (channel.id.as_str(), channel.name.as_str()))
            .unwrap_or(("", ""));
        self.record_notification_log_raw(
            notification_event_type_key(event_type),
            status,
            summary,
            rule_id,
            rule_name,
            channel_id,
            channel_name,
            message,
        );
    }

    fn record_notification_log_raw(
        &self,
        event_type: &str,
        status: &str,
        summary: &str,
        rule_id: &str,
        rule_name: &str,
        channel_id: &str,
        channel_name: &str,
        message: &str,
    ) {
        if let Err(err) = self
            .database
            .insert_notification_log(crate::db::NewNotificationLog {
                event_type,
                status,
                summary,
                rule_id,
                rule_name,
                channel_id,
                channel_name,
                message,
            })
        {
            warn!(error = %err, "Failed to insert notification log");
            return;
        }

        let config = self.get_config();
        let retention_days = config
            .log_cleanup
            .retention_days_enabled
            .then_some(config.log_cleanup.retention_days);
        let max_entries = config
            .log_cleanup
            .max_entries_enabled
            .then_some(config.log_cleanup.max_entries);
        if retention_days.is_some() || max_entries.is_some() {
            if let Err(err) = self
                .database
                .cleanup_notification_logs(retention_days, max_entries)
            {
                warn!(error = %err, "Failed to auto cleanup notification logs");
            }
        }
    }

    pub fn ddns_event_blocked_by_failure_threshold(&self, event: &DdnsEvent) -> bool {
        let config = self.get_config();
        let context = NotificationTemplateContext::default();
        let event = NotificationEvent::Ddns(event, &context);
        let mut matched_rules = 0usize;

        for rule in config
            .rules
            .iter()
            .filter(|rule| rule.enabled && rule.event_type == NotificationEventType::Ddns)
        {
            if !rule_matches(rule, &event) {
                continue;
            }
            matched_rules += 1;
            if !ddns_failure_threshold_pending(rule, &event) {
                return false;
            }
        }

        matched_rules > 0
    }

    async fn send_text_to_channel_with_queue(
        &self,
        event: &NotificationEvent<'_>,
        rule: &NotificationRule,
        channel: &NotificationChannelInstance,
        title: &str,
        text: &str,
        summary: &str,
    ) -> Result<ChannelDeliveryResult, String> {
        if let Some(reason) = self.rate_limit_reason(channel)? {
            let next_attempt_at =
                beijing_time_after_seconds(i64::from(channel.rate_limit.window_seconds.max(1)));
            self.enqueue_notification(
                event,
                rule,
                channel,
                title,
                text,
                summary,
                "scheduled",
                &reason,
                &next_attempt_at,
            )?;
            return Ok(ChannelDeliveryResult::Queued(reason));
        }

        match self.send_text_to_channel(channel, title, text).await {
            Ok(message) => Ok(ChannelDeliveryResult::Sent(message)),
            Err(err) => {
                let next_attempt_at = beijing_time_after_seconds(60);
                let reason = format!("发送失败，已加入通知队列：{err}");
                self.enqueue_notification(
                    event,
                    rule,
                    channel,
                    title,
                    text,
                    summary,
                    "retrying",
                    &reason,
                    &next_attempt_at,
                )?;
                Ok(ChannelDeliveryResult::Queued(reason))
            }
        }
    }

    fn rate_limit_reason(
        &self,
        channel: &NotificationChannelInstance,
    ) -> Result<Option<String>, String> {
        let limit = &channel.rate_limit;
        if !limit.enabled {
            return Ok(None);
        }

        let max_messages = limit.max_messages.max(1);
        let window_seconds = limit.window_seconds.max(1);
        let since = Utc::now()
            .with_timezone(&beijing_offset())
            .checked_sub_signed(ChronoDuration::seconds(i64::from(window_seconds)))
            .unwrap_or_else(|| Utc::now().with_timezone(&beijing_offset()))
            .format(NOTIFICATION_TIME_FORMAT)
            .to_string();
        let count = self
            .database
            .notification_channel_success_count_since(&channel.id, &since)
            .map_err(|err| format!("读取通道发送频率失败：{err}"))?;

        if count >= i64::from(max_messages) {
            Ok(Some(format!(
                "触发队列保护：{} 秒内最多发送 {} 条",
                window_seconds, max_messages
            )))
        } else {
            Ok(None)
        }
    }

    fn enqueue_notification(
        &self,
        event: &NotificationEvent<'_>,
        rule: &NotificationRule,
        channel: &NotificationChannelInstance,
        title: &str,
        body: &str,
        summary: &str,
        status: &str,
        reason: &str,
        next_attempt_at: &str,
    ) -> Result<i64, String> {
        self.database
            .insert_notification_queue_item(NewNotificationQueueItem {
                status,
                event_type: notification_event_type_key(event.event_type()),
                event_label: event.event_type().label(),
                summary,
                reason,
                rule_id: &rule.id,
                rule_name: &rule.name,
                channel_id: &channel.id,
                channel_name: &channel.name,
                channel_type: channel.channel_type.key(),
                title,
                body,
                next_attempt_at,
                max_attempts: 5,
            })
            .map_err(|err| format!("写入通知队列失败：{err}"))
    }

    pub async fn run_queue_worker(self: Arc<Self>) {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            interval.tick().await;
            let items = match self.database.get_due_notification_queue_items(20) {
                Ok(items) => items,
                Err(err) => {
                    warn!(error = %err, "Failed to load due notification queue items");
                    continue;
                }
            };

            for item in items {
                let self_clone = Arc::clone(&self);
                tokio::spawn(async move {
                    self_clone.process_notification_queue_item(item).await;
                });
            }
        }
    }

    async fn process_notification_queue_item(&self, item: NotificationQueueEntry) {
        if self
            .database
            .mark_notification_queue_sending(item.id)
            .unwrap_or(0)
            == 0
        {
            return;
        }

        let config = self.get_config();
        let Some(channel) = config
            .channels
            .iter()
            .find(|channel| channel.id == item.channel_id && channel.enabled)
        else {
            let err = "通知通道不存在或已停用";
            self.finish_queue_item_failed(item, err);
            return;
        };

        if let Ok(Some(reason)) = self.rate_limit_reason(channel) {
            let next_attempt_at =
                beijing_time_after_seconds(i64::from(channel.rate_limit.window_seconds.max(1)));
            if let Err(err) =
                self.database
                    .mark_notification_queue_scheduled(item.id, &reason, &next_attempt_at)
            {
                warn!(error = %err, id = item.id, "Failed to reschedule notification queue item");
            }
            return;
        }

        match self
            .send_text_to_channel(channel, &item.title, &item.body)
            .await
        {
            Ok(message) => {
                if let Err(err) = self.database.mark_notification_queue_sent(item.id) {
                    warn!(error = %err, id = item.id, "Failed to mark notification queue item sent");
                }
                self.record_notification_log_raw(
                    &item.event_type,
                    "success",
                    &item.summary,
                    &item.rule_id,
                    &item.rule_name,
                    &channel.id,
                    &channel.name,
                    &message,
                );
            }
            Err(err) => {
                let next_attempt = item.attempt_count + 1;
                if next_attempt >= item.max_attempts {
                    self.finish_queue_item_failed(item, &err);
                } else {
                    let backoff = retry_backoff_seconds(next_attempt);
                    let next_attempt_at = beijing_time_after_seconds(backoff);
                    if let Err(db_err) =
                        self.database
                            .mark_notification_queue_retry(item.id, &err, &next_attempt_at)
                    {
                        warn!(error = %db_err, id = item.id, "Failed to mark notification queue item retrying");
                    }
                }
            }
        }
    }

    fn finish_queue_item_failed(&self, item: NotificationQueueEntry, err: &str) {
        if let Err(db_err) = self.database.mark_notification_queue_failed(item.id, err) {
            warn!(error = %db_err, id = item.id, "Failed to mark notification queue item failed");
        }
        self.record_notification_log_raw(
            &item.event_type,
            "failed",
            &item.summary,
            &item.rule_id,
            &item.rule_name,
            &item.channel_id,
            &item.channel_name,
            err,
        );
    }

    async fn send_text_to_channel(
        &self,
        channel: &NotificationChannelInstance,
        title: &str,
        text: &str,
    ) -> Result<String, String> {
        match channel.channel_type {
            NotificationChannel::Webhook => {
                let config = parse_instance_config::<WebhookConfig>(channel)?;
                self.send_webhook_text(&config, text).await
            }
            NotificationChannel::Bark => {
                let config = parse_instance_config::<BarkConfig>(channel)?;
                self.send_bark_message(&config, title.to_string(), text.to_string())
                    .await
            }
            NotificationChannel::PushPlus => {
                let config = parse_instance_config::<PushPlusConfig>(channel)?;
                self.send_pushplus_message(&config, title.to_string(), text.to_string())
                    .await
            }
            NotificationChannel::WecomApp => {
                let config = parse_instance_config::<WecomAppConfig>(channel)?;
                self.send_wecom_app_text(&config, text.to_string()).await
            }
            NotificationChannel::WecomRobot => {
                let config = parse_instance_config::<WecomRobotConfig>(channel)?;
                self.send_wecom_robot_text(&config, text.to_string()).await
            }
            NotificationChannel::DingtalkRobot => {
                let config = parse_instance_config::<DingtalkRobotConfig>(channel)?;
                self.send_dingtalk_robot_text(&config, text.to_string())
                    .await
            }
            NotificationChannel::DingtalkApp => {
                let config = parse_instance_config::<DingtalkAppConfig>(channel)?;
                self.send_dingtalk_app_text(&config, text.to_string()).await
            }
            NotificationChannel::FeishuRobot => {
                let config = parse_instance_config::<FeishuRobotConfig>(channel)?;
                self.send_feishu_robot_text(&config, text.to_string()).await
            }
            NotificationChannel::Telegram => {
                let config = parse_instance_config::<TelegramConfig>(channel)?;
                self.send_telegram_text(&config, text.to_string()).await
            }
            NotificationChannel::Email => {
                let config = parse_instance_config::<EmailConfig>(channel)?;
                self.send_email_message(&config, title.to_string(), text.to_string())
                    .await
            }
            NotificationChannel::ServerChan3 => {
                let config = parse_instance_config::<ServerChan3Config>(channel)?;
                self.send_serverchan3_message(&config, title.to_string(), text.to_string())
                    .await
            }
        }
    }

    async fn send_call_to_channel(
        &self,
        channel: NotificationChannel,
        config: &LegacyNotificationConfig,
        call: &CallRecord,
        force: bool,
    ) -> Result<String, String> {
        match channel {
            NotificationChannel::Webhook => {
                self.send_webhook_call(&config.webhook, call, force).await
            }
            NotificationChannel::Bark => self.send_bark_call(&config.bark, call, force).await,
            NotificationChannel::PushPlus => {
                self.send_pushplus_call(&config.pushplus, call, force).await
            }
            NotificationChannel::WecomApp => {
                self.send_wecom_app_call(&config.wecom_app, call, force)
                    .await
            }
            NotificationChannel::WecomRobot => {
                self.send_wecom_robot_call(&config.wecom_robot, call, force)
                    .await
            }
            NotificationChannel::DingtalkRobot => {
                self.send_dingtalk_robot_call(&config.dingtalk_robot, call, force)
                    .await
            }
            NotificationChannel::DingtalkApp => {
                self.send_dingtalk_app_call(&config.dingtalk_app, call, force)
                    .await
            }
            NotificationChannel::FeishuRobot => {
                self.send_feishu_robot_call(&config.feishu_robot, call, force)
                    .await
            }
            NotificationChannel::Telegram => {
                self.send_telegram_call(&config.telegram, call, force).await
            }
            NotificationChannel::Email => Ok("Email skipped".to_string()),
            NotificationChannel::ServerChan3 => Ok("Server酱3 skipped".to_string()),
        }
    }

    async fn send_ddns_to_channel(
        &self,
        channel: NotificationChannel,
        config: &LegacyNotificationConfig,
        event: &DdnsEvent,
    ) -> Result<String, String> {
        match channel {
            NotificationChannel::Webhook => self.send_webhook_ddns(&config.webhook, event).await,
            NotificationChannel::Bark => self.send_bark_ddns(&config.bark, event).await,
            NotificationChannel::PushPlus => self.send_pushplus_ddns(&config.pushplus, event).await,
            NotificationChannel::WecomApp => {
                self.send_wecom_app_ddns(&config.wecom_app, event).await
            }
            NotificationChannel::WecomRobot => {
                self.send_wecom_robot_ddns(&config.wecom_robot, event).await
            }
            NotificationChannel::DingtalkRobot => {
                self.send_dingtalk_robot_ddns(&config.dingtalk_robot, event)
                    .await
            }
            NotificationChannel::DingtalkApp => {
                self.send_dingtalk_app_ddns(&config.dingtalk_app, event)
                    .await
            }
            NotificationChannel::FeishuRobot => {
                self.send_feishu_robot_ddns(&config.feishu_robot, event)
                    .await
            }
            NotificationChannel::Telegram => self.send_telegram_ddns(&config.telegram, event).await,
            NotificationChannel::Email => Ok("Email skipped".to_string()),
            NotificationChannel::ServerChan3 => Ok("Server酱3 skipped".to_string()),
        }
    }

    async fn send_version_update_to_channel(
        &self,
        channel: NotificationChannel,
        config: &LegacyNotificationConfig,
        event: &VersionUpdateEvent,
    ) -> Result<String, String> {
        match channel {
            NotificationChannel::Webhook => {
                self.send_webhook_version_update(&config.webhook, event)
                    .await
            }
            NotificationChannel::Bark => self.send_bark_version_update(&config.bark, event).await,
            NotificationChannel::PushPlus => {
                self.send_pushplus_version_update(&config.pushplus, event)
                    .await
            }
            NotificationChannel::WecomApp => {
                self.send_wecom_app_version_update(&config.wecom_app, event)
                    .await
            }
            NotificationChannel::WecomRobot => {
                self.send_wecom_robot_version_update(&config.wecom_robot, event)
                    .await
            }
            NotificationChannel::DingtalkRobot => {
                self.send_dingtalk_robot_version_update(&config.dingtalk_robot, event)
                    .await
            }
            NotificationChannel::DingtalkApp => {
                self.send_dingtalk_app_version_update(&config.dingtalk_app, event)
                    .await
            }
            NotificationChannel::FeishuRobot => {
                self.send_feishu_robot_version_update(&config.feishu_robot, event)
                    .await
            }
            NotificationChannel::Telegram => {
                self.send_telegram_version_update(&config.telegram, event)
                    .await
            }
            NotificationChannel::Email => Ok("Email skipped".to_string()),
            NotificationChannel::ServerChan3 => Ok("Server酱3 skipped".to_string()),
        }
    }

    async fn send_webhook_sms(
        &self,
        config: &WebhookConfig,
        message: &SmsMessage,
        context: &NotificationTemplateContext,
        force: bool,
    ) -> Result<String, String> {
        if !force && (!config.enabled || !config.forward_sms) {
            return Ok("Webhook skipped".to_string());
        }
        if config.url.trim().is_empty() {
            return Err("Webhook URL is not configured".to_string());
        }

        let payload = render_sms_template(&config.sms_template, message, context, true);
        self.send_webhook_raw(config, &payload).await
    }

    async fn send_webhook_call(
        &self,
        config: &WebhookConfig,
        call: &CallRecord,
        force: bool,
    ) -> Result<String, String> {
        if !force && (!config.enabled || !config.forward_calls) {
            return Ok("Webhook skipped".to_string());
        }
        if config.url.trim().is_empty() {
            return Err("Webhook URL is not configured".to_string());
        }

        let payload = render_call_template(&config.call_template, call, true);
        self.send_webhook_raw(config, &payload).await
    }

    async fn send_webhook_ddns(
        &self,
        config: &WebhookConfig,
        event: &DdnsEvent,
    ) -> Result<String, String> {
        if !config.enabled || !config.forward_ddns {
            return Ok("Webhook skipped".to_string());
        }
        if config.url.trim().is_empty() {
            return Err("Webhook URL is not configured".to_string());
        }

        let payload = render_ddns_template(
            &config.ddns_template,
            event,
            &NotificationTemplateContext::default(),
            true,
        );
        self.send_webhook_raw(config, &payload).await
    }

    async fn send_webhook_version_update(
        &self,
        config: &WebhookConfig,
        event: &VersionUpdateEvent,
    ) -> Result<String, String> {
        if !config.enabled || !config.forward_updates {
            return Ok("Webhook skipped".to_string());
        }
        if config.url.trim().is_empty() {
            return Err("Webhook URL is not configured".to_string());
        }

        let payload = render_version_update_template(
            &config.update_template,
            event,
            &NotificationTemplateContext::default(),
            true,
        );
        self.send_webhook_raw(config, &payload).await
    }

    async fn send_webhook_raw(
        &self,
        config: &WebhookConfig,
        payload: &str,
    ) -> Result<String, String> {
        let mut request = self.client.post(config.url.trim());
        let mut has_content_type = false;

        for (key, value) in &config.headers {
            if key.eq_ignore_ascii_case("content-type") {
                has_content_type = true;
            }
            request = request.header(key, value);
        }

        if !has_content_type {
            request = request.header("Content-Type", "application/json");
        }

        if !config.secret.trim().is_empty() {
            let signature = compute_legacy_signature(config.secret.trim(), payload);
            request = request.header("X-Webhook-Signature", signature);
        }

        let response = request
            .body(payload.to_string())
            .send()
            .await
            .map_err(|e| format!("Failed to send webhook: {}", e))?;
        response_result(
            "Webhook",
            response.status(),
            response.text().await.unwrap_or_default(),
        )
    }

    async fn send_webhook_text(
        &self,
        config: &WebhookConfig,
        text: &str,
    ) -> Result<String, String> {
        if config.url.trim().is_empty() {
            return Err("Webhook URL is not configured".to_string());
        }

        let mut request = self.client.post(config.url.trim());
        let mut has_content_type = false;

        for (key, value) in &config.headers {
            if key.eq_ignore_ascii_case("content-type") {
                has_content_type = true;
            }
            request = request.header(key, value);
        }

        if !has_content_type {
            request = request.header("Content-Type", "text/plain; charset=utf-8");
        }

        if !config.secret.trim().is_empty() {
            let signature = compute_legacy_signature(config.secret.trim(), text);
            request = request.header("X-Webhook-Signature", signature);
        }

        let response = request
            .body(text.to_string())
            .send()
            .await
            .map_err(|e| format!("Failed to send webhook: {}", e))?;
        response_result(
            "Webhook",
            response.status(),
            response.text().await.unwrap_or_default(),
        )
    }

    async fn send_bark_sms(
        &self,
        config: &BarkConfig,
        message: &SmsMessage,
        context: &NotificationTemplateContext,
        force: bool,
    ) -> Result<String, String> {
        if !should_send_sms(&config.common, force) {
            return Ok("Bark skipped".to_string());
        }
        if config.device_key.trim().is_empty() {
            return Err("Bark device key is not configured".to_string());
        }

        let title = render_sms_template(&config.title_template, message, context, false);
        let body = render_sms_template(&config.common.sms_template, message, context, false);
        self.send_bark_message(config, title, body).await
    }

    async fn send_bark_call(
        &self,
        config: &BarkConfig,
        call: &CallRecord,
        force: bool,
    ) -> Result<String, String> {
        if !should_send_call(&config.common, force) {
            return Ok("Bark skipped".to_string());
        }
        if config.device_key.trim().is_empty() {
            return Err("Bark device key is not configured".to_string());
        }

        let title = "SimAdmin 来电通知".to_string();
        let body = render_call_template(&config.common.call_template, call, false);
        self.send_bark_message(config, title, body).await
    }

    async fn send_bark_ddns(
        &self,
        config: &BarkConfig,
        event: &DdnsEvent,
    ) -> Result<String, String> {
        if !should_send_ddns(&config.common) {
            return Ok("Bark skipped".to_string());
        }
        if config.device_key.trim().is_empty() {
            return Err("Bark device key is not configured".to_string());
        }
        self.send_bark_message(
            config,
            "SimAdmin DDNS 通知".to_string(),
            render_ddns_template(
                &config.common.ddns_template,
                event,
                &NotificationTemplateContext::default(),
                false,
            ),
        )
        .await
    }

    async fn send_bark_version_update(
        &self,
        config: &BarkConfig,
        event: &VersionUpdateEvent,
    ) -> Result<String, String> {
        if !should_send_update(&config.common) {
            return Ok("Bark skipped".to_string());
        }
        if config.device_key.trim().is_empty() {
            return Err("Bark device key is not configured".to_string());
        }
        self.send_bark_message(
            config,
            "SimAdmin 版本更新".to_string(),
            render_version_update_template(
                &config.common.update_template,
                event,
                &NotificationTemplateContext::default(),
                false,
            ),
        )
        .await
    }

    async fn send_bark_message(
        &self,
        config: &BarkConfig,
        title: String,
        body: String,
    ) -> Result<String, String> {
        let url = format!(
            "{}/{}",
            config.server_url.trim().trim_end_matches('/'),
            encode_path_segment(config.device_key.trim())
        );
        let mut payload = Map::new();
        payload.insert("title".to_string(), json!(title));
        payload.insert("body".to_string(), json!(body));
        insert_non_empty(&mut payload, "group", &config.group);
        insert_non_empty(&mut payload, "sound", &config.sound);
        insert_non_empty(&mut payload, "level", &config.level);
        insert_non_empty(&mut payload, "icon", &config.icon);
        insert_non_empty(&mut payload, "url", &config.click_url);
        if config.auto_copy {
            payload.insert("automaticallyCopy".to_string(), json!(1));
            payload.insert(
                "copy".to_string(),
                json!(if config.copy.trim().is_empty() {
                    body.as_str()
                } else {
                    config.copy.trim()
                }),
            );
        }
        payload.insert(
            "isArchive".to_string(),
            json!(if config.save_history { 1 } else { 0 }),
        );

        self.post_json("Bark", &url, Value::Object(payload)).await
    }

    async fn send_pushplus_sms(
        &self,
        config: &PushPlusConfig,
        message: &SmsMessage,
        context: &NotificationTemplateContext,
        force: bool,
    ) -> Result<String, String> {
        if !should_send_sms(&config.common, force) {
            return Ok("PushPlus skipped".to_string());
        }

        let title = render_sms_template(&config.title_template, message, context, false);
        let content = render_sms_template(&config.common.sms_template, message, context, false);
        self.send_pushplus_message(config, title, content).await
    }

    async fn send_pushplus_call(
        &self,
        config: &PushPlusConfig,
        call: &CallRecord,
        force: bool,
    ) -> Result<String, String> {
        if !should_send_call(&config.common, force) {
            return Ok("PushPlus skipped".to_string());
        }

        let content = render_call_template(&config.common.call_template, call, false);
        self.send_pushplus_message(config, "SimAdmin 来电通知".to_string(), content)
            .await
    }

    async fn send_pushplus_ddns(
        &self,
        config: &PushPlusConfig,
        event: &DdnsEvent,
    ) -> Result<String, String> {
        if !should_send_ddns(&config.common) {
            return Ok("PushPlus skipped".to_string());
        }

        let content = render_ddns_template(
            &config.common.ddns_template,
            event,
            &NotificationTemplateContext::default(),
            false,
        );
        self.send_pushplus_message(config, "SimAdmin DDNS 通知".to_string(), content)
            .await
    }

    async fn send_pushplus_version_update(
        &self,
        config: &PushPlusConfig,
        event: &VersionUpdateEvent,
    ) -> Result<String, String> {
        if !should_send_update(&config.common) {
            return Ok("PushPlus skipped".to_string());
        }

        let content = render_version_update_template(
            &config.common.update_template,
            event,
            &NotificationTemplateContext::default(),
            false,
        );
        self.send_pushplus_message(config, "SimAdmin 版本更新".to_string(), content)
            .await
    }

    async fn send_pushplus_message(
        &self,
        config: &PushPlusConfig,
        title: String,
        content: String,
    ) -> Result<String, String> {
        if config.token.trim().is_empty() {
            return Err("PushPlus token is not configured".to_string());
        }

        let mut payload = Map::new();
        payload.insert("token".to_string(), json!(config.token.trim()));
        payload.insert("title".to_string(), json!(title));
        payload.insert("content".to_string(), json!(content));
        insert_non_empty(&mut payload, "topic", &config.topic);
        insert_non_empty(&mut payload, "template", &config.template);
        insert_non_empty(&mut payload, "channel", &config.channel);
        insert_non_empty(&mut payload, "option", &config.option);
        insert_non_empty(&mut payload, "callbackUrl", &config.callback_url);

        self.post_json(
            "PushPlus",
            "https://www.pushplus.plus/send",
            Value::Object(payload),
        )
        .await
    }

    async fn send_wecom_app_sms(
        &self,
        config: &WecomAppConfig,
        message: &SmsMessage,
        context: &NotificationTemplateContext,
        force: bool,
    ) -> Result<String, String> {
        if !should_send_sms(&config.common, force) {
            return Ok("企业微信应用消息 skipped".to_string());
        }
        let text = render_sms_template(&config.common.sms_template, message, context, false);
        self.send_wecom_app_text(config, text).await
    }

    async fn send_wecom_app_call(
        &self,
        config: &WecomAppConfig,
        call: &CallRecord,
        force: bool,
    ) -> Result<String, String> {
        if !should_send_call(&config.common, force) {
            return Ok("企业微信应用消息 skipped".to_string());
        }
        let text = render_call_template(&config.common.call_template, call, false);
        self.send_wecom_app_text(config, text).await
    }

    async fn send_wecom_app_ddns(
        &self,
        config: &WecomAppConfig,
        event: &DdnsEvent,
    ) -> Result<String, String> {
        if !should_send_ddns(&config.common) {
            return Ok("企业微信应用消息 skipped".to_string());
        }
        let text = render_ddns_template(
            &config.common.ddns_template,
            event,
            &NotificationTemplateContext::default(),
            false,
        );
        self.send_wecom_app_text(config, text).await
    }

    async fn send_wecom_app_version_update(
        &self,
        config: &WecomAppConfig,
        event: &VersionUpdateEvent,
    ) -> Result<String, String> {
        if !should_send_update(&config.common) {
            return Ok("企业微信应用消息 skipped".to_string());
        }
        let text = render_version_update_template(
            &config.common.update_template,
            event,
            &NotificationTemplateContext::default(),
            false,
        );
        self.send_wecom_app_text(config, text).await
    }

    async fn send_wecom_app_text(
        &self,
        config: &WecomAppConfig,
        text: String,
    ) -> Result<String, String> {
        if config.corp_id.trim().is_empty()
            || config.secret.trim().is_empty()
            || config.agent_id.trim().is_empty()
        {
            return Err("企业微信 CorpID、AgentID 或 Secret 未配置".to_string());
        }

        let agent_id = config
            .agent_id
            .trim()
            .parse::<i64>()
            .map_err(|_| "企业微信 AgentID 必须为数字".to_string())?;
        let payload = json!({
            "touser": if config.to_user.trim().is_empty() { "@all" } else { config.to_user.trim() },
            "toparty": config.to_party.trim(),
            "totag": config.to_tag.trim(),
            "msgtype": "text",
            "agentid": agent_id,
            "text": { "content": text },
            "safe": if config.safe { 1 } else { 0 },
        });

        self.post_wecom_app_message(config, payload).await
    }

    async fn post_wecom_app_message(
        &self,
        config: &WecomAppConfig,
        payload: Value,
    ) -> Result<String, String> {
        let api_base_url = wecom_api_base_url(config);
        let corp_id = config.corp_id.trim();
        let secret = config.secret.trim();
        let mut retried = false;

        loop {
            let token = self
                .fetch_wecom_access_token(&api_base_url, corp_id, secret)
                .await?;
            match self
                .post_wecom_app_payload(&api_base_url, token.as_str(), payload.clone())
                .await
            {
                Ok(result) => return Ok(result),
                Err(WecomMessageError::InvalidAccessToken(_)) if !retried => {
                    retried = true;
                    self.invalidate_wecom_access_token(&api_base_url, corp_id, secret)
                        .await;
                    continue;
                }
                Err(WecomMessageError::InvalidAccessToken(err)) => return Err(err),
                Err(WecomMessageError::Other(err)) => return Err(err),
            }
        }
    }

    async fn post_wecom_app_payload(
        &self,
        api_base_url: &str,
        access_token: &str,
        payload: Value,
    ) -> Result<String, WecomMessageError> {
        let url = format!(
            "{}/cgi-bin/message/send?access_token={}",
            api_base_url,
            encode_query_value(access_token)
        );
        let response = self
            .client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                WecomMessageError::Other(format!("Failed to send 企业微信应用消息 message: {}", e))
            })?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if is_wecom_access_token_error(&body) {
            return Err(WecomMessageError::InvalidAccessToken(
                response_result("企业微信应用消息", status, body).unwrap_or_else(|err| err),
            ));
        }

        response_result("企业微信应用消息", status, body).map_err(WecomMessageError::Other)
    }

    async fn fetch_wecom_access_token(
        &self,
        api_base_url: &str,
        corp_id: &str,
        secret: &str,
    ) -> Result<String, String> {
        let cache_key = (
            api_base_url.to_string(),
            corp_id.to_string(),
            secret.to_string(),
        );
        let mut cache = self.wecom_token_cache.lock().await;
        if let Some(entry) = cache.get(&cache_key) {
            if Instant::now() < entry.refresh_at {
                return Ok(entry.token.clone());
            }
        }

        let parsed = self
            .request_wecom_access_token(api_base_url, corp_id, secret)
            .await?;
        let expires_in = parsed.expires_in.unwrap_or(7200).max(1);
        let refresh_after = if expires_in > 600 {
            expires_in - 300
        } else {
            (expires_in / 2).max(1)
        };
        let token = parsed.access_token;
        cache.insert(
            cache_key,
            WecomTokenCacheEntry {
                token: token.clone(),
                refresh_at: Instant::now() + Duration::from_secs(refresh_after),
            },
        );

        Ok(token)
    }

    async fn invalidate_wecom_access_token(&self, api_base_url: &str, corp_id: &str, secret: &str) {
        let mut cache = self.wecom_token_cache.lock().await;
        cache.remove(&(
            api_base_url.to_string(),
            corp_id.to_string(),
            secret.to_string(),
        ));
    }

    async fn request_wecom_access_token(
        &self,
        api_base_url: &str,
        corp_id: &str,
        secret: &str,
    ) -> Result<WecomTokenResponse, String> {
        #[derive(Debug, Deserialize)]
        struct RawWecomTokenResponse {
            #[serde(default)]
            errcode: i64,
            #[serde(default)]
            errmsg: String,
            #[serde(default)]
            access_token: String,
            #[serde(default)]
            expires_in: Option<u64>,
        }

        let url = format!(
            "{}/cgi-bin/gettoken?corpid={}&corpsecret={}",
            api_base_url,
            encode_query_value(corp_id),
            encode_query_value(secret)
        );
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to get WeCom access token: {}", e))?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!("WeCom token request failed ({}): {}", status, body));
        }
        let parsed: RawWecomTokenResponse = serde_json::from_str(&body)
            .map_err(|e| format!("Failed to parse WeCom token response: {}", e))?;
        if parsed.errcode != 0 {
            return Err(format_wecom_errcode(
                "WeCom token",
                parsed.errcode,
                &parsed.errmsg,
            ));
        }
        if parsed.access_token.is_empty() {
            return Err("WeCom token response did not include access_token".to_string());
        }
        Ok(WecomTokenResponse {
            access_token: parsed.access_token,
            expires_in: parsed.expires_in,
        })
    }

    async fn send_wecom_robot_sms(
        &self,
        config: &WecomRobotConfig,
        message: &SmsMessage,
        context: &NotificationTemplateContext,
        force: bool,
    ) -> Result<String, String> {
        if !should_send_sms(&config.common, force) {
            return Ok("企业微信群机器人 skipped".to_string());
        }
        let text = render_sms_template(&config.common.sms_template, message, context, false);
        self.send_wecom_robot_text(config, text).await
    }

    async fn send_wecom_robot_call(
        &self,
        config: &WecomRobotConfig,
        call: &CallRecord,
        force: bool,
    ) -> Result<String, String> {
        if !should_send_call(&config.common, force) {
            return Ok("企业微信群机器人 skipped".to_string());
        }
        let text = render_call_template(&config.common.call_template, call, false);
        self.send_wecom_robot_text(config, text).await
    }

    async fn send_wecom_robot_ddns(
        &self,
        config: &WecomRobotConfig,
        event: &DdnsEvent,
    ) -> Result<String, String> {
        if !should_send_ddns(&config.common) {
            return Ok("企业微信群机器人 skipped".to_string());
        }
        let text = render_ddns_template(
            &config.common.ddns_template,
            event,
            &NotificationTemplateContext::default(),
            false,
        );
        self.send_wecom_robot_text(config, text).await
    }

    async fn send_wecom_robot_version_update(
        &self,
        config: &WecomRobotConfig,
        event: &VersionUpdateEvent,
    ) -> Result<String, String> {
        if !should_send_update(&config.common) {
            return Ok("企业微信群机器人 skipped".to_string());
        }
        let text = render_version_update_template(
            &config.common.update_template,
            event,
            &NotificationTemplateContext::default(),
            false,
        );
        self.send_wecom_robot_text(config, text).await
    }

    async fn send_wecom_robot_text(
        &self,
        config: &WecomRobotConfig,
        text: String,
    ) -> Result<String, String> {
        let url = robot_webhook_url(
            &config.webhook_url,
            &config.key,
            "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=",
        )?;
        let payload = json!({
            "msgtype": "text",
            "text": { "content": text },
        });

        self.post_json("企业微信群机器人", &url, payload).await
    }

    async fn send_dingtalk_robot_sms(
        &self,
        config: &DingtalkRobotConfig,
        message: &SmsMessage,
        context: &NotificationTemplateContext,
        force: bool,
    ) -> Result<String, String> {
        if !should_send_sms(&config.common, force) {
            return Ok("钉钉群自定义机器人 skipped".to_string());
        }
        let text = render_sms_template(&config.common.sms_template, message, context, false);
        self.send_dingtalk_robot_text(config, text).await
    }

    async fn send_dingtalk_robot_call(
        &self,
        config: &DingtalkRobotConfig,
        call: &CallRecord,
        force: bool,
    ) -> Result<String, String> {
        if !should_send_call(&config.common, force) {
            return Ok("钉钉群自定义机器人 skipped".to_string());
        }
        let text = render_call_template(&config.common.call_template, call, false);
        self.send_dingtalk_robot_text(config, text).await
    }

    async fn send_dingtalk_robot_ddns(
        &self,
        config: &DingtalkRobotConfig,
        event: &DdnsEvent,
    ) -> Result<String, String> {
        if !should_send_ddns(&config.common) {
            return Ok("钉钉群自定义机器人 skipped".to_string());
        }
        let text = render_ddns_template(
            &config.common.ddns_template,
            event,
            &NotificationTemplateContext::default(),
            false,
        );
        self.send_dingtalk_robot_text(config, text).await
    }

    async fn send_dingtalk_robot_version_update(
        &self,
        config: &DingtalkRobotConfig,
        event: &VersionUpdateEvent,
    ) -> Result<String, String> {
        if !should_send_update(&config.common) {
            return Ok("钉钉群自定义机器人 skipped".to_string());
        }
        let text = render_version_update_template(
            &config.common.update_template,
            event,
            &NotificationTemplateContext::default(),
            false,
        );
        self.send_dingtalk_robot_text(config, text).await
    }

    async fn send_dingtalk_robot_text(
        &self,
        config: &DingtalkRobotConfig,
        text: String,
    ) -> Result<String, String> {
        let mut url = robot_webhook_url(
            &config.webhook_url,
            &config.access_token,
            "https://oapi.dingtalk.com/robot/send?access_token=",
        )?;
        if !config.secret.trim().is_empty() {
            let timestamp = current_timestamp_millis();
            let to_sign = format!("{}\n{}", timestamp, config.secret.trim());
            let sign = hmac_sha256_base64(config.secret.trim().as_bytes(), to_sign.as_bytes());
            let separator = if url.contains('?') { '&' } else { '?' };
            url.push_str(&format!(
                "{}timestamp={}&sign={}",
                separator,
                timestamp,
                encode_query_value(&sign)
            ));
        }

        let at_mobiles = split_csv(&config.at_mobiles);
        let payload = json!({
            "msgtype": "text",
            "text": { "content": text },
            "at": {
                "atMobiles": at_mobiles,
                "isAtAll": config.at_all,
            },
        });

        self.post_json("钉钉群自定义机器人", &url, payload).await
    }

    async fn send_dingtalk_app_sms(
        &self,
        config: &DingtalkAppConfig,
        message: &SmsMessage,
        context: &NotificationTemplateContext,
        force: bool,
    ) -> Result<String, String> {
        if !should_send_sms(&config.common, force) {
            return Ok("钉钉企业内机器人 skipped".to_string());
        }
        let text = render_sms_template(&config.common.sms_template, message, context, false);
        self.send_dingtalk_app_text(config, text).await
    }

    async fn send_dingtalk_app_call(
        &self,
        config: &DingtalkAppConfig,
        call: &CallRecord,
        force: bool,
    ) -> Result<String, String> {
        if !should_send_call(&config.common, force) {
            return Ok("钉钉企业内机器人 skipped".to_string());
        }
        let text = render_call_template(&config.common.call_template, call, false);
        self.send_dingtalk_app_text(config, text).await
    }

    async fn send_dingtalk_app_ddns(
        &self,
        config: &DingtalkAppConfig,
        event: &DdnsEvent,
    ) -> Result<String, String> {
        if !should_send_ddns(&config.common) {
            return Ok("钉钉企业内部机器人 skipped".to_string());
        }
        let text = render_ddns_template(
            &config.common.ddns_template,
            event,
            &NotificationTemplateContext::default(),
            false,
        );
        self.send_dingtalk_app_text(config, text).await
    }

    async fn send_dingtalk_app_version_update(
        &self,
        config: &DingtalkAppConfig,
        event: &VersionUpdateEvent,
    ) -> Result<String, String> {
        if !should_send_update(&config.common) {
            return Ok("钉钉企业内部机器人 skipped".to_string());
        }
        let text = render_version_update_template(
            &config.common.update_template,
            event,
            &NotificationTemplateContext::default(),
            false,
        );
        self.send_dingtalk_app_text(config, text).await
    }

    async fn send_dingtalk_app_text(
        &self,
        config: &DingtalkAppConfig,
        text: String,
    ) -> Result<String, String> {
        if config.app_key.trim().is_empty()
            || config.app_secret.trim().is_empty()
            || config.open_conversation_id.trim().is_empty()
        {
            return Err("钉钉 AppKey、AppSecret 或 OpenConversationId 未配置".to_string());
        }
        let token = self
            .fetch_dingtalk_access_token(config.app_key.trim(), config.app_secret.trim())
            .await?;
        let robot_code = if config.robot_code.trim().is_empty() {
            config.app_key.trim()
        } else {
            config.robot_code.trim()
        };
        let msg_key = if config.msg_key.trim().is_empty() {
            "sampleText"
        } else {
            config.msg_key.trim()
        };
        let msg_param = json!({ "content": text }).to_string();
        let payload = json!({
            "robotCode": robot_code,
            "openConversationId": config.open_conversation_id.trim(),
            "msgKey": msg_key,
            "msgParam": msg_param,
        });

        let response = self
            .client
            .post("https://api.dingtalk.com/v1.0/robot/groupMessages/send")
            .header("x-acs-dingtalk-access-token", token)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Failed to send DingTalk app robot message: {}", e))?;
        response_result(
            "钉钉企业内机器人",
            response.status(),
            response.text().await.unwrap_or_default(),
        )
    }

    async fn fetch_dingtalk_access_token(
        &self,
        app_key: &str,
        app_secret: &str,
    ) -> Result<String, String> {
        #[derive(Debug, Deserialize)]
        struct DingtalkTokenResponse {
            #[serde(default, rename = "accessToken")]
            access_token: String,
            #[serde(default)]
            code: String,
            #[serde(default)]
            message: String,
        }

        let payload = json!({
            "appKey": app_key,
            "appSecret": app_secret,
        });
        let response = self
            .client
            .post("https://api.dingtalk.com/v1.0/oauth2/accessToken")
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Failed to get DingTalk access token: {}", e))?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!(
                "DingTalk token request failed ({}): {}",
                status, body
            ));
        }
        let parsed: DingtalkTokenResponse = serde_json::from_str(&body)
            .map_err(|e| format!("Failed to parse DingTalk token response: {}", e))?;
        if !parsed.access_token.is_empty() {
            return Ok(parsed.access_token);
        }
        Err(format!(
            "DingTalk token response did not include accessToken: {} {}",
            parsed.code, parsed.message
        ))
    }

    async fn send_feishu_robot_sms(
        &self,
        config: &FeishuRobotConfig,
        message: &SmsMessage,
        context: &NotificationTemplateContext,
        force: bool,
    ) -> Result<String, String> {
        if !should_send_sms(&config.common, force) {
            return Ok("飞书机器人 skipped".to_string());
        }
        let text = render_sms_template(&config.common.sms_template, message, context, false);
        self.send_feishu_robot_text(config, text).await
    }

    async fn send_feishu_robot_call(
        &self,
        config: &FeishuRobotConfig,
        call: &CallRecord,
        force: bool,
    ) -> Result<String, String> {
        if !should_send_call(&config.common, force) {
            return Ok("飞书机器人 skipped".to_string());
        }
        let text = render_call_template(&config.common.call_template, call, false);
        self.send_feishu_robot_text(config, text).await
    }

    async fn send_feishu_robot_ddns(
        &self,
        config: &FeishuRobotConfig,
        event: &DdnsEvent,
    ) -> Result<String, String> {
        if !should_send_ddns(&config.common) {
            return Ok("飞书机器人 skipped".to_string());
        }
        let text = render_ddns_template(
            &config.common.ddns_template,
            event,
            &NotificationTemplateContext::default(),
            false,
        );
        self.send_feishu_robot_text(config, text).await
    }

    async fn send_feishu_robot_version_update(
        &self,
        config: &FeishuRobotConfig,
        event: &VersionUpdateEvent,
    ) -> Result<String, String> {
        if !should_send_update(&config.common) {
            return Ok("飞书机器人 skipped".to_string());
        }
        let text = render_version_update_template(
            &config.common.update_template,
            event,
            &NotificationTemplateContext::default(),
            false,
        );
        self.send_feishu_robot_text(config, text).await
    }

    async fn send_feishu_robot_text(
        &self,
        config: &FeishuRobotConfig,
        text: String,
    ) -> Result<String, String> {
        let url = robot_webhook_url(
            &config.webhook_url,
            &config.token,
            "https://open.feishu.cn/open-apis/bot/v2/hook/",
        )?;
        let mut payload = Map::new();
        payload.insert("msg_type".to_string(), json!("text"));
        payload.insert("content".to_string(), json!({ "text": text }));
        if !config.secret.trim().is_empty() {
            let timestamp = current_timestamp_secs().to_string();
            let sign_key = format!("{}\n{}", timestamp, config.secret.trim());
            let sign = hmac_sha256_base64(sign_key.as_bytes(), b"");
            payload.insert("timestamp".to_string(), json!(timestamp));
            payload.insert("sign".to_string(), json!(sign));
        }

        self.post_json("飞书机器人", &url, Value::Object(payload))
            .await
    }

    async fn send_telegram_sms(
        &self,
        config: &TelegramConfig,
        message: &SmsMessage,
        context: &NotificationTemplateContext,
        force: bool,
    ) -> Result<String, String> {
        if !should_send_sms(&config.common, force) {
            return Ok("Telegram skipped".to_string());
        }
        let text = render_sms_template(&config.common.sms_template, message, context, false);
        self.send_telegram_text(config, text).await
    }

    async fn send_telegram_call(
        &self,
        config: &TelegramConfig,
        call: &CallRecord,
        force: bool,
    ) -> Result<String, String> {
        if !should_send_call(&config.common, force) {
            return Ok("Telegram skipped".to_string());
        }
        let text = render_call_template(&config.common.call_template, call, false);
        self.send_telegram_text(config, text).await
    }

    async fn send_telegram_ddns(
        &self,
        config: &TelegramConfig,
        event: &DdnsEvent,
    ) -> Result<String, String> {
        if !should_send_ddns(&config.common) {
            return Ok("Telegram skipped".to_string());
        }
        let text = render_ddns_template(
            &config.common.ddns_template,
            event,
            &NotificationTemplateContext::default(),
            false,
        );
        self.send_telegram_text(config, text).await
    }

    async fn send_telegram_version_update(
        &self,
        config: &TelegramConfig,
        event: &VersionUpdateEvent,
    ) -> Result<String, String> {
        if !should_send_update(&config.common) {
            return Ok("Telegram skipped".to_string());
        }
        let text = render_version_update_template(
            &config.common.update_template,
            event,
            &NotificationTemplateContext::default(),
            false,
        );
        self.send_telegram_text(config, text).await
    }

    async fn send_telegram_text(
        &self,
        config: &TelegramConfig,
        text: String,
    ) -> Result<String, String> {
        if config.bot_token.trim().is_empty() || config.chat_id.trim().is_empty() {
            return Err("Telegram Bot Token 或 Chat ID 未配置".to_string());
        }
        let url = telegram_send_message_url(config);
        let mut payload = Map::new();
        payload.insert("chat_id".to_string(), json!(config.chat_id.trim()));
        payload.insert("text".to_string(), json!(text));
        payload.insert(
            "disable_web_page_preview".to_string(),
            json!(config.disable_web_page_preview),
        );
        insert_non_empty(&mut payload, "parse_mode", &config.parse_mode);

        self.post_json("Telegram", &url, Value::Object(payload))
            .await
    }

    async fn send_serverchan3_message(
        &self,
        config: &ServerChan3Config,
        title: String,
        desp: String,
    ) -> Result<String, String> {
        let url = serverchan3_url(config)?;
        let form = serverchan3_form_payload(config, &title, &desp);
        let response = self
            .client
            .post(&url)
            .form(&form)
            .send()
            .await
            .map_err(|e| format!("Failed to send Server酱3 message: {}", e))?;

        serverchan3_response_result(response.status(), response.text().await.unwrap_or_default())
    }

    async fn send_email_message(
        &self,
        config: &EmailConfig,
        subject: String,
        body: String,
    ) -> Result<String, String> {
        if config.smtp_host.trim().is_empty() {
            return Err("SMTP 服务器未配置".to_string());
        }
        if config.sender_address.trim().is_empty() {
            return Err("发件人邮箱未配置".to_string());
        }

        let sender = mailbox_from_config(&config.sender_address, &config.sender_name, "发件人")?;
        let receivers = email_receivers_from_config(&config.receiver_addresses)?;
        let message = build_email_message(config, sender, receivers, &subject, &body)?;
        let mailer = build_email_transport(config)?;

        mailer
            .send(message)
            .await
            .map_err(|err| format!("Email 发送失败：{err}"))?;

        Ok("Email test successful".to_string())
    }

    async fn post_json(&self, label: &str, url: &str, payload: Value) -> Result<String, String> {
        let response = self
            .client
            .post(url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Failed to send {} message: {}", label, e))?;
        response_result(
            label,
            response.status(),
            response.text().await.unwrap_or_default(),
        )
    }
}

fn parse_instance_config<T>(channel: &NotificationChannelInstance) -> Result<T, String>
where
    T: DeserializeOwned + Default,
{
    if channel.config.is_null() {
        return Ok(T::default());
    }
    serde_json::from_value(channel.config.clone())
        .map_err(|err| format!("Failed to parse {} channel config: {}", channel.name, err))
}

fn serverchan3_url(config: &ServerChan3Config) -> Result<String, String> {
    let send_key = config.send_key.trim();
    if send_key.is_empty() {
        return Err("Server酱3 SendKey 未配置".to_string());
    }
    let uid = serverchan3_uid(config)
        .ok_or_else(|| "Server酱3 UID 未配置，且无法从 SendKey 自动解析".to_string())?;
    if !is_valid_serverchan3_uid(&uid) {
        return Err("Server酱3 UID 只能包含字母、数字或短横线".to_string());
    }

    Ok(format!(
        "https://{}.push.ft07.com/send/{}.send",
        uid,
        encode_path_segment(send_key)
    ))
}

fn serverchan3_uid(config: &ServerChan3Config) -> Option<String> {
    let uid = config.uid.trim();
    if !uid.is_empty() {
        return Some(uid.to_string());
    }
    serverchan3_uid_from_send_key(&config.send_key)
}

fn is_valid_serverchan3_uid(uid: &str) -> bool {
    !uid.is_empty()
        && uid
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
}

fn serverchan3_uid_from_send_key(send_key: &str) -> Option<String> {
    let send_key = send_key.trim();
    let lower = send_key.to_ascii_lowercase();
    let rest = lower.strip_prefix("sctp")?;
    let digits_len = rest
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .map(char::len_utf8)
        .sum::<usize>();
    if digits_len == 0 || rest.get(digits_len..=digits_len)? != "t" {
        return None;
    }
    Some(rest[..digits_len].to_string())
}

fn serverchan3_form_payload(
    config: &ServerChan3Config,
    title: &str,
    desp: &str,
) -> Vec<(String, String)> {
    let mut form = vec![
        ("title".to_string(), title.to_string()),
        ("desp".to_string(), desp.to_string()),
    ];
    if !config.channel.trim().is_empty() {
        form.push(("channel".to_string(), config.channel.trim().to_string()));
    }
    if !config.openid.trim().is_empty() {
        form.push(("group".to_string(), config.openid.trim().to_string()));
    }
    form
}

fn serverchan3_response_result(status: StatusCode, body: String) -> Result<String, String> {
    if !status.is_success() {
        return Err(format!("Server酱3 returned HTTP {}: {}", status, body));
    }

    let value = serde_json::from_str::<Value>(&body)
        .map_err(|err| format!("Server酱3 返回内容不是合法 JSON：{}；{}", err, body))?;
    let code = value
        .get("code")
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("Server酱3 返回缺少 code 字段：{}", body))?;
    if code != 0 {
        let message = value
            .get("message")
            .or_else(|| value.get("msg"))
            .and_then(Value::as_str)
            .unwrap_or(&body);
        return Err(format!("Server酱3 returned code {}: {}", code, message));
    }

    Ok(format!("Server酱3 test successful (status: {})", status))
}

fn split_receiver_addresses(value: &str) -> Vec<String> {
    value
        .split(|ch| matches!(ch, ',' | ';' | '\n' | '\r' | '，' | '；'))
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn mailbox_from_config(address: &str, name: &str, label: &str) -> Result<Mailbox, String> {
    let address = address.trim();
    if address.is_empty() {
        return Err(format!("{label}邮箱未配置"));
    }
    let address = address
        .parse::<Address>()
        .map_err(|err| format!("{label}邮箱格式无效：{err}"))?;
    let name = name.trim();
    Ok(Mailbox::new(
        if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        },
        address,
    ))
}

fn email_receivers_from_config(value: &str) -> Result<Vec<Mailbox>, String> {
    let addresses = split_receiver_addresses(value);
    if addresses.is_empty() {
        return Err("收件人邮箱未配置".to_string());
    }
    addresses
        .iter()
        .map(|address| mailbox_from_config(address, "", "收件人"))
        .collect()
}

fn build_email_message(
    config: &EmailConfig,
    sender: Mailbox,
    receivers: Vec<Mailbox>,
    subject: &str,
    body: &str,
) -> Result<Message, String> {
    let mut builder = Message::builder().from(sender).subject(subject);
    for receiver in receivers {
        builder = builder.to(receiver);
    }

    let part = match config.message_format.trim().to_ascii_lowercase().as_str() {
        "" | "plain" | "text" => Ok(SinglePart::plain(body.to_string())),
        "html" => Ok(SinglePart::html(body.to_string())),
        other => Err(format!("不支持的 Email 消息格式：{other}")),
    }?;

    builder
        .singlepart(part)
        .map_err(|err| format!("构建 Email 消息失败：{err}"))
}

fn build_email_transport(
    config: &EmailConfig,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, String> {
    let host = config.smtp_host.trim();
    let port = config.smtp_port.max(1);
    let tls = match config.smtp_security.trim().to_ascii_lowercase().as_str() {
        "" | "implicit_tls" | "tls" => {
            let tls_parameters = email_tls_parameters(host, config.allow_insecure_tls)?;
            Tls::Wrapper(tls_parameters)
        }
        "starttls" => {
            let tls_parameters = email_tls_parameters(host, config.allow_insecure_tls)?;
            Tls::Required(tls_parameters)
        }
        "none" => Tls::None,
        other => return Err(format!("不支持的 SMTP 安全模式：{other}")),
    };
    let mut builder = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
        .port(port)
        .tls(tls);

    if !config.username.trim().is_empty() || !config.password.is_empty() {
        builder = builder.credentials(Credentials::new(
            config.username.trim().to_string(),
            config.password.clone(),
        ));
    }

    Ok(builder.build())
}

fn email_tls_parameters(host: &str, allow_insecure: bool) -> Result<TlsParameters, String> {
    TlsParameters::builder(host.to_string())
        .dangerous_accept_invalid_certs(allow_insecure)
        .dangerous_accept_invalid_hostnames(allow_insecure)
        .build()
        .map_err(|err| format!("构建 SMTP TLS 参数失败：{err}"))
}

fn rule_matches(rule: &NotificationRule, event: &NotificationEvent<'_>) -> bool {
    if let NotificationEvent::SystemEvent(system_event, _) = event {
        return rule
            .event_codes
            .iter()
            .any(|event_code| event_code == &system_event.event_code);
    }

    let value = event.field_value(rule.matcher.field.as_str());
    let expected = rule.matcher.value.trim();
    match rule.matcher.operator {
        MatcherOperator::Always => true,
        MatcherOperator::Contains => {
            expected.is_empty() || value.to_lowercase().contains(&expected.to_lowercase())
        }
        MatcherOperator::NotContains => {
            expected.is_empty() || !value.to_lowercase().contains(&expected.to_lowercase())
        }
        MatcherOperator::Equals => value.trim() == expected,
        MatcherOperator::Regex => {
            if expected.is_empty() {
                true
            } else {
                regex_automata::meta::Regex::new(expected)
                    .map(|regex| regex.is_match(value.as_bytes()))
                    .unwrap_or(false)
            }
        }
    }
}

fn ddns_failure_threshold_pending(rule: &NotificationRule, event: &NotificationEvent<'_>) -> bool {
    let NotificationEvent::Ddns(ddns, _) = event else {
        return false;
    };
    if ddns.status != "failed" {
        return false;
    }

    let threshold = rule.ddns_failure_threshold.max(1);
    if threshold <= 1 {
        return false;
    }

    let failure_count = ddns.failure_count;
    failure_count == 0 || failure_count % threshold != 0
}

pub(crate) fn quiet_hours_active(schedules: &[QuietHoursSchedule]) -> bool {
    let now = Utc::now().with_timezone(&beijing_offset());
    let weekday = now.weekday().number_from_monday() as u8;
    let minutes = now.hour() as u16 * 60 + now.minute() as u16;

    schedules
        .iter()
        .filter(|schedule| schedule.enabled)
        .any(|schedule| quiet_schedule_matches(schedule, weekday, minutes))
}

fn quiet_schedule_matches(schedule: &QuietHoursSchedule, weekday: u8, minutes: u16) -> bool {
    let weekdays = if schedule.weekdays.is_empty() {
        vec![1, 2, 3, 4, 5, 6, 7]
    } else {
        schedule.weekdays.clone()
    };
    let Some(start) = parse_hhmm(&schedule.start) else {
        return false;
    };
    let Some(end) = parse_hhmm(&schedule.end) else {
        return false;
    };

    if start == end {
        return weekdays.contains(&weekday);
    }
    if start < end {
        return weekdays.contains(&weekday) && minutes >= start && minutes < end;
    }

    let previous_weekday = if weekday == 1 { 7 } else { weekday - 1 };
    (weekdays.contains(&weekday) && minutes >= start)
        || (weekdays.contains(&previous_weekday) && minutes < end)
}

fn parse_hhmm(value: &str) -> Option<u16> {
    let (hour, minute) = value.split_once(':')?;
    let hour = hour.parse::<u16>().ok()?;
    let minute = minute.parse::<u16>().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(hour * 60 + minute)
}

fn notification_event_type_key(event_type: NotificationEventType) -> &'static str {
    match event_type {
        NotificationEventType::Sms => "sms",
        NotificationEventType::Ddns => "ddns",
        NotificationEventType::VersionUpdate => "version_update",
        NotificationEventType::SystemEvent => "system_event",
        NotificationEventType::DeviceStatus => "device_status",
        NotificationEventType::Automation => "automation",
    }
}

impl NotificationEventType {
    fn label(self) -> &'static str {
        match self {
            NotificationEventType::Sms => "短信",
            NotificationEventType::Ddns => "DDNS",
            NotificationEventType::VersionUpdate => "版本更新",
            NotificationEventType::SystemEvent => "系统事件",
            NotificationEventType::DeviceStatus => "设备状态",
            NotificationEventType::Automation => "自动化中心",
        }
    }
}

fn beijing_time_after_seconds(seconds: i64) -> String {
    Utc::now()
        .with_timezone(&beijing_offset())
        .checked_add_signed(ChronoDuration::seconds(seconds.max(1)))
        .unwrap_or_else(|| Utc::now().with_timezone(&beijing_offset()))
        .format(NOTIFICATION_TIME_FORMAT)
        .to_string()
}

fn retry_backoff_seconds(attempt_count: i64) -> i64 {
    let exponent = attempt_count.saturating_sub(1).clamp(0, 5) as u32;
    (60_i64 * 2_i64.pow(exponent)).min(3600)
}

fn compact_summary(value: &str) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = collapsed.chars();
    let summary = chars.by_ref().take(120).collect::<String>();
    if chars.next().is_some() {
        format!("{}...", summary)
    } else {
        summary
    }
}

#[allow(dead_code)]
impl NotificationChannel {
    fn key(self) -> &'static str {
        match self {
            NotificationChannel::Webhook => "webhook",
            NotificationChannel::Bark => "bark",
            NotificationChannel::PushPlus => "pushplus",
            NotificationChannel::WecomApp => "wecom_app",
            NotificationChannel::WecomRobot => "wecom_robot",
            NotificationChannel::DingtalkRobot => "dingtalk_robot",
            NotificationChannel::DingtalkApp => "dingtalk_app",
            NotificationChannel::FeishuRobot => "feishu_robot",
            NotificationChannel::Telegram => "telegram",
            NotificationChannel::Email => "email",
            NotificationChannel::ServerChan3 => "serverchan3",
        }
    }

    fn label(self) -> &'static str {
        match self {
            NotificationChannel::Webhook => "Webhook",
            NotificationChannel::Bark => "Bark",
            NotificationChannel::PushPlus => "PushPlus",
            NotificationChannel::WecomApp => "企业微信应用消息",
            NotificationChannel::WecomRobot => "企业微信群机器人",
            NotificationChannel::DingtalkRobot => "钉钉群自定义机器人",
            NotificationChannel::DingtalkApp => "钉钉企业内机器人",
            NotificationChannel::FeishuRobot => "飞书机器人",
            NotificationChannel::Telegram => "Telegram机器人",
            NotificationChannel::Email => "Email",
            NotificationChannel::ServerChan3 => "Server酱3",
        }
    }
}

#[allow(dead_code)]
fn all_channels() -> [NotificationChannel; 11] {
    [
        NotificationChannel::Webhook,
        NotificationChannel::Bark,
        NotificationChannel::PushPlus,
        NotificationChannel::WecomApp,
        NotificationChannel::WecomRobot,
        NotificationChannel::DingtalkRobot,
        NotificationChannel::DingtalkApp,
        NotificationChannel::FeishuRobot,
        NotificationChannel::Telegram,
        NotificationChannel::Email,
        NotificationChannel::ServerChan3,
    ]
}

#[allow(dead_code)]
fn should_send_sms(config: &MessageChannelConfig, force: bool) -> bool {
    force || (config.enabled && config.forward_sms)
}

#[allow(dead_code)]
fn should_send_sms_to_channel(
    channel: NotificationChannel,
    config: &LegacyNotificationConfig,
) -> bool {
    match channel {
        NotificationChannel::Webhook => config.webhook.enabled && config.webhook.forward_sms,
        NotificationChannel::Bark => should_send_sms(&config.bark.common, false),
        NotificationChannel::PushPlus => should_send_sms(&config.pushplus.common, false),
        NotificationChannel::WecomApp => should_send_sms(&config.wecom_app.common, false),
        NotificationChannel::WecomRobot => should_send_sms(&config.wecom_robot.common, false),
        NotificationChannel::DingtalkRobot => should_send_sms(&config.dingtalk_robot.common, false),
        NotificationChannel::DingtalkApp => should_send_sms(&config.dingtalk_app.common, false),
        NotificationChannel::FeishuRobot => should_send_sms(&config.feishu_robot.common, false),
        NotificationChannel::Telegram => should_send_sms(&config.telegram.common, false),
        NotificationChannel::Email => should_send_sms(&config.email.common, false),
        NotificationChannel::ServerChan3 => should_send_sms(&config.serverchan3.common, false),
    }
}

#[allow(dead_code)]
fn should_send_call(config: &MessageChannelConfig, force: bool) -> bool {
    force || (config.enabled && config.forward_calls)
}

#[allow(dead_code)]
fn should_send_ddns(config: &MessageChannelConfig) -> bool {
    config.enabled && config.forward_ddns
}

#[allow(dead_code)]
fn should_send_update(config: &MessageChannelConfig) -> bool {
    config.enabled && config.forward_updates
}

#[allow(dead_code)]
fn should_send_update_to_channel(
    channel: NotificationChannel,
    config: &LegacyNotificationConfig,
) -> bool {
    match channel {
        NotificationChannel::Webhook => config.webhook.enabled && config.webhook.forward_updates,
        NotificationChannel::Bark => should_send_update(&config.bark.common),
        NotificationChannel::PushPlus => should_send_update(&config.pushplus.common),
        NotificationChannel::WecomApp => should_send_update(&config.wecom_app.common),
        NotificationChannel::WecomRobot => should_send_update(&config.wecom_robot.common),
        NotificationChannel::DingtalkRobot => should_send_update(&config.dingtalk_robot.common),
        NotificationChannel::DingtalkApp => should_send_update(&config.dingtalk_app.common),
        NotificationChannel::FeishuRobot => should_send_update(&config.feishu_robot.common),
        NotificationChannel::Telegram => should_send_update(&config.telegram.common),
        NotificationChannel::Email => should_send_update(&config.email.common),
        NotificationChannel::ServerChan3 => should_send_update(&config.serverchan3.common),
    }
}

const DEFAULT_DDNS_TEXT_TEMPLATE: &str = "SimAdmin DDNS 通知\n域名: {{域名}}\nIP类型: {{IP类型}}\n新IP: {{新IP}}\n旧IP: {{旧IP}}\n服务商: {{服务商}}\n记录类型: {{记录类型}}\n状态: {{状态}}\n消息: {{消息}}\n更新时间: {{更新时间}}";
const DEFAULT_DDNS_JSON_TEMPLATE: &str = r#"{
  "msg_type": "text",
  "content": {
    "text": "SimAdmin DDNS 通知\n域名: {{domains}}\nIP类型: {{ip_type}}\n新IP: {{new_ip}}\n旧IP: {{old_ip}}\n服务商: {{provider}}\n记录类型: {{record_type}}\n状态: {{status}}\n消息: {{message}}\n更新时间: {{timestamp}}"
  }
}"#;
const DEFAULT_UPDATE_TEXT_TEMPLATE: &str = "🚀 SimAdmin 发现新版本\n固件包: {{固件包}}\n版本号: {{版本号}}\n时间: {{时间}}\n来源: {{本机号码}}\n\n请前往 OTA 更新页面的在线更新模块检查更新，可一键下载并升级。";
const DEFAULT_UPDATE_JSON_TEMPLATE: &str = r#"{
  "msg_type": "text",
  "content": {
    "text": "🚀 SimAdmin 发现新版本\n固件包: {{asset_name}}\n版本号: {{version}}\n时间: {{time}}\n来源: {{own_number}}\n\n请前往 OTA 更新页面的在线更新模块检查更新，可一键下载并升级。"
  }
}"#;

fn render_ddns_template(
    template: &str,
    event: &DdnsEvent,
    context: &NotificationTemplateContext,
    escape_json: bool,
) -> String {
    let domains = if event.domains.is_empty() {
        "-".to_string()
    } else {
        event.domains.join(", ")
    };
    let ip_type = match event.record_type.as_str() {
        "A" => "IPv4",
        "AAAA" => "IPv6",
        other => other,
    };
    let old_ip = event.old_ip.as_deref().unwrap_or("-").to_string();
    let new_ip = event.new_ip.as_deref().unwrap_or("-").to_string();
    let template = if template.trim().is_empty() && escape_json {
        DEFAULT_DDNS_JSON_TEMPLATE
    } else if template.trim().is_empty() {
        DEFAULT_DDNS_TEXT_TEMPLATE
    } else {
        template
    };

    let maybe_escape = |value: &str| {
        if escape_json {
            escape_json_string(value)
        } else {
            value.to_string()
        }
    };
    let domains = maybe_escape(&domains);
    let ip_type = maybe_escape(ip_type);
    let old_ip = maybe_escape(&old_ip);
    let new_ip = maybe_escape(&new_ip);
    let provider = maybe_escape(&event.provider);
    let record_type = maybe_escape(&event.record_type);
    let status = maybe_escape(&event.status);
    let message = maybe_escape(&event.message);
    let timestamp_value = format_notification_time(&event.timestamp);
    let timestamp = maybe_escape(&timestamp_value);
    let failure_count_value = event.failure_count.to_string();
    let failure_count = maybe_escape(&failure_count_value);

    let rendered = template
        .replace("{{domains}}", &domains)
        .replace("{{domain}}", &domains)
        .replace("{{ip_type}}", &ip_type)
        .replace("{{new_ip}}", &new_ip)
        .replace("{{old_ip}}", &old_ip)
        .replace("{{provider}}", &provider)
        .replace("{{record_type}}", &record_type)
        .replace("{{status}}", &status)
        .replace("{{message}}", &message)
        .replace("{{failure_count}}", &failure_count)
        .replace("{{timestamp}}", &timestamp)
        .replace("{{time}}", &timestamp)
        .replace("{{域名}}", &domains)
        .replace("{{IP类型}}", &ip_type)
        .replace("{{新IP}}", &new_ip)
        .replace("{{旧IP}}", &old_ip)
        .replace("{{服务商}}", &provider)
        .replace("{{记录类型}}", &record_type)
        .replace("{{状态}}", &status)
        .replace("{{消息}}", &message)
        .replace("{{失败次数}}", &failure_count)
        .replace("{{更新时间}}", &timestamp);
    replace_common_variables(rendered, context, escape_json)
}

fn replace_own_number(template: String, own_number: &str) -> String {
    template
        .replace("{{own_number}}", own_number)
        .replace("{{local_phone_number}}", own_number)
        .replace("{{self_phone_number}}", own_number)
        .replace("{{本机号码}}", own_number)
}

fn common_own_number<'a>(context: &'a NotificationTemplateContext, fallback: &'a str) -> &'a str {
    if context.own_number.trim().is_empty() {
        fallback
    } else {
        context.own_number.as_str()
    }
}

fn replace_common_variables(
    template: String,
    context: &NotificationTemplateContext,
    escape_json: bool,
) -> String {
    let own_number = if escape_json {
        escape_json_string(&context.own_number)
    } else {
        context.own_number.clone()
    };
    let carrier = if escape_json {
        escape_json_string(&context.carrier)
    } else {
        context.carrier.clone()
    };
    replace_own_number(template, &own_number)
        .replace("{{carrier}}", &carrier)
        .replace("{{operator}}", &carrier)
        .replace("{{运营商}}", &carrier)
}

fn render_version_update_template(
    template: &str,
    event: &VersionUpdateEvent,
    context: &NotificationTemplateContext,
    escape_json: bool,
) -> String {
    let template = if template.trim().is_empty() && escape_json {
        DEFAULT_UPDATE_JSON_TEMPLATE
    } else if template.trim().is_empty() {
        DEFAULT_UPDATE_TEXT_TEMPLATE
    } else {
        template
    };

    let maybe_escape = |value: &str| {
        if escape_json {
            escape_json_string(value)
        } else {
            value.to_string()
        }
    };
    let asset_name = maybe_escape(&event.asset_name);
    let version = maybe_escape(&event.version);
    let build_time_value = format_notification_time(&event.build_time);
    let build_time = maybe_escape(&build_time_value);
    let release_url = maybe_escape(&event.release_url);
    let timestamp_value = format_notification_time(&event.timestamp);
    let timestamp = maybe_escape(&timestamp_value);
    let own_number = maybe_escape(common_own_number(context, &event.own_number));

    let rendered = template
        .replace("{{asset_name}}", &asset_name)
        .replace("{{file_name}}", &asset_name)
        .replace("{{firmware_name}}", &asset_name)
        .replace("{{version}}", &version)
        .replace("{{build_time}}", &build_time)
        .replace("{{release_url}}", &release_url)
        .replace("{{timestamp}}", &timestamp)
        .replace("{{time}}", &timestamp)
        .replace("{{时间}}", &timestamp)
        .replace("{{固件包}}", &asset_name)
        .replace("{{文件名}}", &asset_name)
        .replace("{{版本号}}", &version)
        .replace("{{构建时间}}", &build_time)
        .replace("{{发布地址}}", &release_url)
        .replace("{{发布时间}}", &timestamp);
    replace_common_variables(
        replace_own_number(rendered, &own_number),
        context,
        escape_json,
    )
}

fn render_system_event_template(
    template: &str,
    event: &SystemEvent,
    context: &NotificationTemplateContext,
    escape_json: bool,
) -> String {
    let maybe_escape = |value: &str| {
        if escape_json {
            escape_json_string(value)
        } else {
            value.to_string()
        }
    };
    let category = maybe_escape(&event.category);
    let category_label = maybe_escape(&event.category_label);
    let event_code = maybe_escape(&event.event_code);
    let event_label = maybe_escape(&event.event_label);
    let severity = maybe_escape(&event.severity);
    let severity_label = maybe_escape(&event.severity_label);
    let status = maybe_escape(&event.status);
    let status_label = maybe_escape(&event.status_label);
    let entity = maybe_escape(&event.entity);
    let message = maybe_escape(&event.message);
    let timestamp_value = format_notification_time(&event.timestamp);
    let timestamp = maybe_escape(&timestamp_value);

    let rendered = template
        .replace("{{category}}", &category)
        .replace("{{category_label}}", &category_label)
        .replace("{{event_code}}", &event_code)
        .replace("{{event_label}}", &event_label)
        .replace("{{severity}}", &severity)
        .replace("{{severity_label}}", &severity_label)
        .replace("{{status}}", &status)
        .replace("{{status_label}}", &status_label)
        .replace("{{entity}}", &entity)
        .replace("{{message}}", &message)
        .replace("{{timestamp}}", &timestamp)
        .replace("{{time}}", &timestamp)
        .replace("{{分类}}", &category_label)
        .replace("{{分类编码}}", &category)
        .replace("{{事件}}", &event_label)
        .replace("{{事件编码}}", &event_code)
        .replace("{{等级}}", &severity_label)
        .replace("{{等级编码}}", &severity)
        .replace("{{状态}}", &status_label)
        .replace("{{状态编码}}", &status)
        .replace("{{对象}}", &entity)
        .replace("{{消息}}", &message)
        .replace("{{时间}}", &timestamp);
    replace_common_variables(rendered, context, escape_json)
}

fn render_automation_template(
    template: &str,
    event: &AutomationEvent,
    context: &NotificationTemplateContext,
    escape_json: bool,
) -> String {
    let maybe_escape = |value: &str| {
        if escape_json {
            escape_json_string(value)
        } else {
            value.to_string()
        }
    };

    let task_id = maybe_escape(&event.task_id);
    let task_name = maybe_escape(&event.task_name);

    let task_type_label = match event.task_type.as_str() {
        "restart_baseband" => "重启基带",
        "reboot_device" => "重启设备",
        "send_sms" => "发送短信",
        other => other,
    };
    let task_type = maybe_escape(task_type_label);

    let status_label = match event.status.as_str() {
        "success" => "成功",
        "failed" => "失败",
        other => other,
    };
    let status = maybe_escape(status_label);

    let message = maybe_escape(&event.message);
    let timestamp = maybe_escape(&event.timestamp);
    let own_number = maybe_escape(&context.own_number);

    let rendered = template
        .replace("{{task_id}}", &task_id)
        .replace("{{task_name}}", &task_name)
        .replace("{{任务名称}}", &task_name)
        .replace("{{task_type}}", &task_type)
        .replace("{{任务类型}}", &task_type)
        .replace("{{status}}", &status)
        .replace("{{任务状态}}", &status)
        .replace("{{执行状态}}", &status)
        .replace("{{message}}", &message)
        .replace("{{任务详情}}", &message)
        .replace("{{详情}}", &message)
        .replace("{{timestamp}}", &timestamp)
        .replace("{{触发时间}}", &timestamp)
        .replace("{{时间}}", &timestamp);
    replace_common_variables(
        replace_own_number(rendered, &own_number),
        context,
        escape_json,
    )
}

fn render_device_status_template(
    template: &str,
    report: &DeviceStatusReport,
    context: &NotificationTemplateContext,
    escape_json: bool,
) -> String {
    let maybe_escape = |value: &str| {
        if escape_json {
            escape_json_string(value)
        } else {
            value.to_string()
        }
    };
    let timestamp = maybe_escape(&report.timestamp);
    if template.contains("{{状态分类}}") || template.contains("{{status_category}}") {
        let category_token = template
            .find("{{状态分类}}")
            .or_else(|| template.find("{{status_category}}"));
        let content_token = template
            .find("{{状态内容}}")
            .or_else(|| template.find("{{status_content}}"))
            .or_else(|| template.find("{{content}}"));
        if let (Some(category_index), Some(content_index)) = (category_token, content_token) {
            let section_start = template[..category_index]
                .rfind('\n')
                .map(|index| index + 1)
                .unwrap_or(0);
            let section_end = template[content_index..]
                .find('\n')
                .map(|offset| content_index + offset + 1)
                .unwrap_or(template.len());
            let header = &template[..section_start];
            let section_template = &template[section_start..section_end];
            let footer = &template[section_end..];
            let sections = report
                .sections()
                .into_iter()
                .map(|section| {
                    let category = maybe_escape(&section.category);
                    let content = maybe_escape(&section.lines.join("\n"));
                    section_template
                        .replace("{{status_category}}", &category)
                        .replace("{{状态分类}}", &category)
                        .replace("{{status_content}}", &content)
                        .replace("{{content}}", &content)
                        .replace("{{状态内容}}", &content)
                        .replace("{{timestamp}}", &timestamp)
                        .replace("{{time}}", &timestamp)
                        .replace("{{时间}}", &timestamp)
                })
                .collect::<Vec<_>>()
                .join("\n");
            let rendered = format!("{header}{sections}{footer}")
                .replace("{{timestamp}}", &timestamp)
                .replace("{{time}}", &timestamp)
                .replace("{{时间}}", &timestamp);
            return replace_common_variables(rendered, context, escape_json);
        }
    }

    let content = maybe_escape(&report.text());
    let rendered = template
        .replace("{{status_content}}", &content)
        .replace("{{content}}", &content)
        .replace("{{timestamp}}", &timestamp)
        .replace("{{time}}", &timestamp)
        .replace("{{状态内容}}", &content)
        .replace("{{时间}}", &timestamp);
    replace_common_variables(rendered, context, escape_json)
}

fn robot_webhook_url(webhook_url: &str, key: &str, prefix: &str) -> Result<String, String> {
    let webhook_url = webhook_url.trim();
    if !webhook_url.is_empty() {
        return Ok(webhook_url.to_string());
    }
    let key = key.trim();
    if key.is_empty() {
        return Err("Webhook URL 或 Key/Token 未配置".to_string());
    }
    Ok(format!("{}{}", prefix, encode_path_segment(key)))
}

fn split_csv(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn insert_non_empty(payload: &mut Map<String, Value>, key: &str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        payload.insert(key.to_string(), json!(value));
    }
}

fn encode_query_value(value: &str) -> String {
    utf8_percent_encode(value, QUERY_VALUE_ENCODE_SET).to_string()
}

fn encode_path_segment(value: &str) -> String {
    utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}

fn current_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn current_timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn hmac_sha256_base64(key: &[u8], data: &[u8]) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, key);
    let tag = hmac::sign(&key, data);
    general_purpose::STANDARD.encode(tag.as_ref())
}

fn is_wecom_access_token_error(body: &str) -> bool {
    json_errcode(body)
        .map(|(errcode, _)| matches!(errcode, 40014 | 42001))
        .unwrap_or(false)
}

fn wecom_api_base_url(config: &WecomAppConfig) -> String {
    let configured = config.api_base_url.trim();
    let base = if configured.is_empty() {
        "https://qyapi.weixin.qq.com"
    } else {
        configured
    };
    base.trim_end_matches('/').to_string()
}

fn telegram_api_base_url(config: &TelegramConfig) -> String {
    let configured = config.api_base_url.trim();
    let base = if configured.is_empty() {
        "https://api.telegram.org"
    } else {
        configured
    };
    base.trim_end_matches('/').to_string()
}

fn telegram_send_message_url(config: &TelegramConfig) -> String {
    format!(
        "{}/bot{}/sendMessage",
        telegram_api_base_url(config),
        config.bot_token.trim()
    )
}

fn format_channel_errcode(label: &str, errcode: i64, message: &str) -> String {
    if errcode == 60020 {
        return format!(
            "{} returned errcode 60020: {}. 企业微信应用请求出口 IP 未加入企业可信 IP，请在企业微信后台添加当前出口 IP，或将 API 地址设置为固定公网 IP 的企业微信中继/反代地址。",
            label, message
        );
    }

    format!("{} returned errcode {}: {}", label, errcode, message)
}

fn format_wecom_errcode(label: &str, errcode: i64, message: &str) -> String {
    format_channel_errcode(label, errcode, message)
}

fn json_errcode(body: &str) -> Option<(i64, String)> {
    let value = serde_json::from_str::<Value>(body).ok()?;
    let errcode = value.get("errcode").and_then(Value::as_i64)?;
    let message = value
        .get("errmsg")
        .or_else(|| value.get("err_msg"))
        .and_then(Value::as_str)
        .unwrap_or(body)
        .to_string();
    Some((errcode, message))
}

fn response_result(label: &str, status: StatusCode, body: String) -> Result<String, String> {
    if !status.is_success() {
        return Err(format!("{} returned HTTP {}: {}", label, status, body));
    }

    if let Ok(value) = serde_json::from_str::<Value>(&body) {
        if let Some(ok) = value.get("ok").and_then(Value::as_bool) {
            if !ok {
                return Err(format!("{} returned error: {}", label, body));
            }
        }
        if let Some(errcode) = value.get("errcode").and_then(Value::as_i64) {
            if errcode != 0 {
                let message = value
                    .get("errmsg")
                    .or_else(|| value.get("err_msg"))
                    .and_then(Value::as_str)
                    .unwrap_or(&body);
                return Err(format_channel_errcode(label, errcode, message));
            }
        }
        if let Some(code) = value.get("code").and_then(Value::as_i64) {
            if code != 0 && code != 200 {
                let message = value
                    .get("msg")
                    .or_else(|| value.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or(&body);
                return Err(format!("{} returned code {}: {}", label, code, message));
            }
        }
        if let Some(status_code) = value.get("StatusCode").and_then(Value::as_i64) {
            if status_code != 0 {
                let message = value
                    .get("StatusMessage")
                    .and_then(Value::as_str)
                    .unwrap_or(&body);
                return Err(format!(
                    "{} returned StatusCode {}: {}",
                    label, status_code, message
                ));
            }
        }
    }

    Ok(format!("{} test successful (status: {})", label, status))
}

fn render_sms_template(
    template: &str,
    message: &SmsMessage,
    context: &NotificationTemplateContext,
    escape_json: bool,
) -> String {
    let content = if escape_json {
        escape_json_string(&message.content)
    } else {
        message.content.clone()
    };
    let own_number = if escape_json {
        escape_json_string(&context.own_number)
    } else {
        context.own_number.clone()
    };
    let carrier = if escape_json {
        escape_json_string(&context.carrier)
    } else {
        context.carrier.clone()
    };
    let timestamp = render_time_value(&message.timestamp, escape_json);
    let verification_code = extract_verification_code(&message.content).unwrap_or_default();

    let rendered = template
        .replace("{{id}}", &message.id.to_string())
        .replace("{{phone_number}}", &message.phone_number)
        .replace("{{发送方号码}}", &message.phone_number)
        .replace("{{发送方}}", &message.phone_number)
        .replace("{{发件人}}", &message.phone_number)
        .replace("{{content}}", &content)
        .replace("{{内容}}", &content)
        .replace("{{短信内容}}", &content)
        .replace("{{verification_code}}", &verification_code)
        .replace("{{验证码}}", &verification_code)
        .replace("{{direction}}", &message.direction)
        .replace("{{短信方向}}", &message.direction)
        .replace("{{方向}}", &message.direction)
        .replace("{{timestamp}}", &timestamp)
        .replace("{{时间}}", &timestamp)
        .replace("{{status}}", &message.status)
        .replace("{{短信状态}}", &message.status)
        .replace("{{状态}}", &message.status)
        .replace("{{sender}}", &message.phone_number)
        .replace("{{message}}", &content)
        .replace("{{time}}", &timestamp)
        .replace("{{carrier}}", &carrier)
        .replace("{{operator}}", &carrier)
        .replace("{{运营商}}", &carrier);
    replace_own_number(rendered, &own_number)
}

fn format_own_numbers_for_template(numbers: &[String]) -> String {
    numbers
        .iter()
        .map(|number| format_own_number_for_template(number))
        .filter(|number| !number.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_own_number_for_template(number: &str) -> String {
    let value = number
        .trim()
        .trim_matches(|c| matches!(c, '"' | '\'' | ',' | ';'))
        .trim()
        .strip_prefix("tel:")
        .unwrap_or_else(|| number.trim());
    let mut compact = String::new();

    for ch in value.chars() {
        if ch == '+' && compact.is_empty() {
            compact.push(ch);
        } else if ch.is_ascii_digit() {
            compact.push(ch);
        }
    }

    let has_plus = compact.starts_with('+');
    let digits = compact.strip_prefix('+').unwrap_or(&compact);
    if digits.len() == 13 && digits.starts_with("86") {
        return digits[2..].to_string();
    }
    if !has_plus && !(digits.len() == 11 && digits.starts_with('1')) {
        return format!("+{digits}");
    }

    compact
}

fn render_call_template(template: &str, call: &CallRecord, escape_json: bool) -> String {
    let start_time = render_time_value(&call.start_time, escape_json);
    let end_time = call
        .end_time
        .as_deref()
        .map(|value| render_time_value(value, escape_json))
        .unwrap_or_default();
    let answered_str = if call.answered { "是" } else { "否" };
    let answered_value = if escape_json {
        escape_json_string(answered_str)
    } else {
        answered_str.to_string()
    };
    let direction_cn = if call.direction == "incoming" {
        "来电"
    } else {
        "去电"
    };

    template
        .replace("{{id}}", &call.id.to_string())
        .replace("{{phone_number}}", &call.phone_number)
        .replace("{{direction}}", &call.direction)
        .replace("{{direction_cn}}", direction_cn)
        .replace("{{duration}}", &call.duration.to_string())
        .replace("{{start_time}}", &start_time)
        .replace("{{end_time}}", &end_time)
        .replace("{{answered}}", &answered_value)
        .replace("{{answered_bool}}", &call.answered.to_string())
        .replace("{{caller}}", &call.phone_number)
        .replace("{{time}}", &start_time)
}

fn render_time_value(value: &str, escape_json: bool) -> String {
    let formatted = format_notification_time(value);
    if escape_json {
        escape_json_string(&formatted)
    } else {
        formatted
    }
}

#[allow(dead_code)]
fn beijing_now_string() -> String {
    Utc::now()
        .with_timezone(&beijing_offset())
        .format(NOTIFICATION_TIME_FORMAT)
        .to_string()
}

fn format_notification_time(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return String::new();
    }

    if let Ok(datetime) = DateTime::parse_from_rfc3339(value) {
        return datetime
            .with_timezone(&beijing_offset())
            .format(NOTIFICATION_TIME_FORMAT)
            .to_string();
    }

    for format in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S"] {
        if let Ok(datetime) = NaiveDateTime::parse_from_str(value, format) {
            return datetime.format(NOTIFICATION_TIME_FORMAT).to_string();
        }
    }

    value.to_string()
}

fn beijing_offset() -> FixedOffset {
    FixedOffset::east_opt(BEIJING_UTC_OFFSET_SECONDS).expect("valid Beijing UTC offset")
}

fn escape_json_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn compute_legacy_signature(secret: &str, data: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    format!("{}{}", secret, data).hash(&mut hasher);
    let hash = hasher.finish();

    format!("{:016x}", hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RuleMatcher;

    #[test]
    fn quiet_schedule_matches_weekday_and_overnight_range() {
        let schedule = QuietHoursSchedule {
            enabled: true,
            weekdays: vec![1],
            start: "22:00".to_string(),
            end: "08:00".to_string(),
        };

        assert!(quiet_schedule_matches(&schedule, 1, 22 * 60));
        assert!(quiet_schedule_matches(&schedule, 2, 7 * 60 + 59));
        assert!(!quiet_schedule_matches(&schedule, 2, 8 * 60));
        assert!(!quiet_schedule_matches(&schedule, 3, 7 * 60));
    }

    #[test]
    fn rule_matcher_supports_contains_and_regex() {
        let message = SmsMessage {
            id: 1,
            direction: "incoming".to_string(),
            phone_number: "+10086".to_string(),
            content: "Your code is 482910".to_string(),
            timestamp: "2026-05-23 18:30:12".to_string(),
            status: "received".to_string(),
            pdu: None,
        };
        let context = NotificationTemplateContext::default();
        let event = NotificationEvent::Sms {
            message: &message,
            context: &context,
        };

        let contains_rule = NotificationRule {
            id: "rule-1".to_string(),
            event_type: NotificationEventType::Sms,
            name: "验证码".to_string(),
            enabled: true,
            matcher: RuleMatcher {
                field: "content".to_string(),
                operator: MatcherOperator::Contains,
                value: "code".to_string(),
            },
            channel_ids: Vec::new(),
            event_codes: Vec::new(),
            title_template: String::new(),
            template: String::new(),
            quiet_hours: Vec::new(),
            ddns_failure_threshold: 1,
            device_status_items: crate::config::default_device_status_items(),
            device_status_schedule: crate::config::DeviceStatusSchedule::default(),
            device_status_sms_period: "last_24h".to_string(),
        };
        assert!(rule_matches(&contains_rule, &event));

        let regex_rule = NotificationRule {
            matcher: RuleMatcher {
                field: "content".to_string(),
                operator: MatcherOperator::Regex,
                value: r"\d{6}".to_string(),
            },
            ..contains_rule
        };
        assert!(rule_matches(&regex_rule, &event));
    }

    #[test]
    fn ddns_failure_threshold_waits_until_threshold_multiple() {
        let rule = NotificationRule {
            id: "rule-ddns".to_string(),
            event_type: NotificationEventType::Ddns,
            name: "DDNS threshold".to_string(),
            enabled: true,
            matcher: RuleMatcher::default(),
            channel_ids: Vec::new(),
            event_codes: Vec::new(),
            title_template: String::new(),
            template: String::new(),
            quiet_hours: Vec::new(),
            ddns_failure_threshold: 5,
            device_status_items: crate::config::default_device_status_items(),
            device_status_schedule: crate::config::DeviceStatusSchedule::default(),
            device_status_sms_period: "last_24h".to_string(),
        };
        let mut ddns = DdnsEvent {
            status: "failed".to_string(),
            failure_count: 4,
            ..DdnsEvent::default()
        };
        let context = NotificationTemplateContext::default();

        let event = NotificationEvent::Ddns(&ddns, &context);
        assert!(ddns_failure_threshold_pending(&rule, &event));

        ddns.failure_count = 5;
        let event = NotificationEvent::Ddns(&ddns, &context);
        assert!(!ddns_failure_threshold_pending(&rule, &event));

        ddns.failure_count = 6;
        let event = NotificationEvent::Ddns(&ddns, &context);
        assert!(ddns_failure_threshold_pending(&rule, &event));

        ddns.failure_count = 10;
        let event = NotificationEvent::Ddns(&ddns, &context);
        assert!(!ddns_failure_threshold_pending(&rule, &event));

        ddns.status = "updated".to_string();
        ddns.failure_count = 1;
        let event = NotificationEvent::Ddns(&ddns, &context);
        assert!(!ddns_failure_threshold_pending(&rule, &event));
    }

    #[test]
    fn formats_rfc3339_time_as_beijing_time() {
        assert_eq!(
            format_notification_time("2026-05-14T16:30:45Z"),
            "2026-05-15 00:30:45"
        );
        assert_eq!(
            format_notification_time("2026-05-15T08:30:45+08:00"),
            "2026-05-15 08:30:45"
        );
    }

    #[test]
    fn renders_sms_time_variables_as_beijing_time() {
        let message = SmsMessage {
            id: 7,
            direction: "incoming".to_string(),
            phone_number: "+10000".to_string(),
            content: "hello".to_string(),
            timestamp: "2026-05-14T16:30:45Z".to_string(),
            status: "received".to_string(),
            pdu: None,
        };
        let context = NotificationTemplateContext::default();

        assert_eq!(
            render_sms_template("{{timestamp}}|{{time}}", &message, &context, false),
            "2026-05-15 00:30:45|2026-05-15 00:30:45"
        );
    }

    #[test]
    fn renders_sms_own_number_variables() {
        let message = SmsMessage {
            id: 7,
            direction: "incoming".to_string(),
            phone_number: "+10000".to_string(),
            content: "hello".to_string(),
            timestamp: "2026-05-14T16:30:45Z".to_string(),
            status: "received".to_string(),
            pdu: None,
        };
        let context = NotificationTemplateContext {
            own_number: "+10001".to_string(),
            ..Default::default()
        };

        assert_eq!(
            render_sms_template(
                "{{own_number}}|{{local_phone_number}}|{{self_phone_number}}|{{本机号码}}",
                &message,
                &context,
                false
            ),
            "+10001|+10001|+10001|+10001"
        );
    }

    #[test]
    fn builds_serverchan3_url_from_send_key_or_uid() {
        let from_key = ServerChan3Config {
            send_key: "sctp12345tsecret".to_string(),
            ..Default::default()
        };
        assert_eq!(
            serverchan3_url(&from_key).unwrap(),
            "https://12345.push.ft07.com/send/sctp12345tsecret.send"
        );

        let manual_uid = ServerChan3Config {
            uid: "user-1".to_string(),
            send_key: "manual-secret".to_string(),
            ..Default::default()
        };
        assert_eq!(
            serverchan3_url(&manual_uid).unwrap(),
            "https://user-1.push.ft07.com/send/manual%2Dsecret.send"
        );
    }

    #[test]
    fn serverchan3_requires_uid_when_send_key_cannot_be_parsed() {
        let missing_uid = ServerChan3Config {
            send_key: "manual-secret".to_string(),
            ..Default::default()
        };
        assert!(serverchan3_url(&missing_uid).is_err());
    }

    #[test]
    fn serverchan3_form_includes_optional_routing_fields() {
        let config = ServerChan3Config {
            channel: "9".to_string(),
            openid: "openid-1".to_string(),
            ..Default::default()
        };
        let form = serverchan3_form_payload(&config, "title", "content");

        assert!(form.contains(&("title".to_string(), "title".to_string())));
        assert!(form.contains(&("desp".to_string(), "content".to_string())));
        assert!(form.contains(&("channel".to_string(), "9".to_string())));
        assert!(form.contains(&("group".to_string(), "openid-1".to_string())));
    }

    #[test]
    fn serverchan3_requires_zero_response_code() {
        assert!(serverchan3_response_result(StatusCode::OK, r#"{"code":0}"#.to_string()).is_ok());
        assert!(serverchan3_response_result(
            StatusCode::OK,
            r#"{"code":200,"message":"ok"}"#.to_string()
        )
        .is_err());
    }

    #[test]
    fn email_receiver_parser_accepts_common_separators() {
        let receivers = email_receivers_from_config(
            "first@example.com; second@example.com\nthird@example.com，fourth@example.com",
        )
        .unwrap();

        assert_eq!(receivers.len(), 4);
    }

    #[test]
    fn email_receiver_parser_rejects_invalid_address() {
        assert!(email_receivers_from_config("not-an-email").is_err());
    }

    #[test]
    fn email_builders_validate_format_and_security() {
        let mut config = EmailConfig {
            smtp_host: "smtp.example.com".to_string(),
            sender_address: "sender@example.com".to_string(),
            receiver_addresses: "receiver@example.com".to_string(),
            message_format: "plain".to_string(),
            ..Default::default()
        };
        let sender = mailbox_from_config(&config.sender_address, "", "发件人").unwrap();
        let receivers = email_receivers_from_config(&config.receiver_addresses).unwrap();
        assert!(build_email_message(&config, sender, receivers, "subject", "body").is_ok());

        config.message_format = "markdown".to_string();
        let sender = mailbox_from_config(&config.sender_address, "", "发件人").unwrap();
        let receivers = email_receivers_from_config(&config.receiver_addresses).unwrap();
        assert!(build_email_message(&config, sender, receivers, "subject", "body").is_err());

        config.message_format = "plain".to_string();
        config.smtp_security = "starttls".to_string();
        assert!(build_email_transport(&config).is_ok());

        config.smtp_security = "invalid".to_string();
        assert!(build_email_transport(&config).is_err());
    }

    #[test]
    fn renders_sms_carrier_variables() {
        let message = SmsMessage {
            id: 7,
            direction: "incoming".to_string(),
            phone_number: "+10000".to_string(),
            content: "hello".to_string(),
            timestamp: "2026-05-14T16:30:45Z".to_string(),
            status: "received".to_string(),
            pdu: None,
        };
        let context = NotificationTemplateContext {
            own_number: "+10001".to_string(),
            carrier: "中国联通".to_string(),
        };

        assert_eq!(
            render_sms_template(
                "{{运营商}}|{{carrier}}|{{operator}}",
                &message,
                &context,
                false
            ),
            "中国联通|中国联通|中国联通"
        );
    }

    #[test]
    fn renders_sms_verification_code_variables() {
        let message = SmsMessage {
            id: 7,
            direction: "incoming".to_string(),
            phone_number: "+10000".to_string(),
            content: "【谷歌信息】G-248521是您的 Google 验证码".to_string(),
            timestamp: "2026-05-14T16:30:45Z".to_string(),
            status: "received".to_string(),
            pdu: None,
        };
        let context = NotificationTemplateContext::default();

        assert_eq!(
            render_sms_template(
                "{{验证码}}|{{verification_code}}",
                &message,
                &context,
                false
            ),
            "248521|248521"
        );
    }

    #[test]
    fn formats_own_number_variables_for_display() {
        assert_eq!(
            format_own_number_for_template("+8613112345678"),
            "13112345678"
        );
        assert_eq!(
            format_own_number_for_template("8613112345678"),
            "13112345678"
        );
        assert_eq!(format_own_number_for_template("13112345678"), "13112345678");
        assert_eq!(format_own_number_for_template("+4412345678"), "+4412345678");
        assert_eq!(
            format_own_number_for_template("447434452765"),
            "+447434452765"
        );
        assert_eq!(
            format_own_numbers_for_template(&[
                "+8613112345678".to_string(),
                "447434452765".to_string()
            ]),
            "13112345678, +447434452765"
        );
    }

    #[test]
    fn renders_call_time_variables_as_beijing_time() {
        let call = CallRecord {
            id: 9,
            direction: "incoming".to_string(),
            phone_number: "+10000".to_string(),
            duration: 12,
            start_time: "2026-05-14T16:30:45Z".to_string(),
            end_time: Some("2026-05-14T16:31:45Z".to_string()),
            answered: true,
        };

        assert_eq!(
            render_call_template("{{start_time}}|{{end_time}}|{{time}}", &call, false),
            "2026-05-15 00:30:45|2026-05-15 00:31:45|2026-05-15 00:30:45"
        );
    }

    #[test]
    fn renders_ddns_time_variables_as_beijing_time() {
        let event = DdnsEvent {
            timestamp: "2026-05-14T16:30:45Z".to_string(),
            ..DdnsEvent::default()
        };
        let context = NotificationTemplateContext::default();

        assert_eq!(
            render_ddns_template(
                "{{timestamp}}|{{time}}|{{更新时间}}",
                &event,
                &context,
                false
            ),
            "2026-05-15 00:30:45|2026-05-15 00:30:45|2026-05-15 00:30:45"
        );
    }

    #[test]
    fn renders_version_update_build_time_as_beijing_time() {
        let event = VersionUpdateEvent {
            asset_name: "simadmin_1.0.4.tar.gz".to_string(),
            version: "1.0.4".to_string(),
            build_time: "2026-05-14T16:30:45Z".to_string(),
            release_url: "https://github.com/3899/SimAdmin/releases/tag/v1.0.4".to_string(),
            timestamp: "2026-05-14T17:00:00Z".to_string(),
            own_number: "+10001".to_string(),
        };
        let context = NotificationTemplateContext::default();

        assert_eq!(
            render_version_update_template(
                "{{asset_name}}|{{version}}|{{build_time}}|{{时间}}|{{本机号码}}",
                &event,
                &context,
                false
            ),
            "simadmin_1.0.4.tar.gz|1.0.4|2026-05-15 00:30:45|2026-05-15 01:00:00|+10001"
        );
    }

    #[test]
    fn renders_common_variables_for_non_sms_events() {
        let context = NotificationTemplateContext {
            own_number: "18888888888".to_string(),
            carrier: "中国移动".to_string(),
        };
        let ddns = DdnsEvent::default();
        assert_eq!(
            render_ddns_template("{{本机号码}}|{{运营商}}", &ddns, &context, false),
            "18888888888|中国移动"
        );

        let version = VersionUpdateEvent {
            own_number: "+10001".to_string(),
            ..VersionUpdateEvent::default()
        };
        assert_eq!(
            render_version_update_template("{{本机号码}}|{{运营商}}", &version, &context, false),
            "18888888888|中国移动"
        );

        let system = SystemEvent::new("baseband.restart", "info", "triggered", "modem", "ok");
        assert_eq!(
            render_system_event_template("{{本机号码}}|{{运营商}}", &system, &context, false),
            "18888888888|中国移动"
        );

        let report = DeviceStatusReport {
            lines: vec!["设备：在线，上电".to_string()],
            timestamp: "2026-05-14T17:00:00Z".to_string(),
        };
        assert_eq!(
            render_device_status_template("{{本机号码}}|{{运营商}}", &report, &context, false),
            "18888888888|中国移动"
        );

        let automation = AutomationEvent {
            task_id: "task-1".to_string(),
            task_name: "发短信".to_string(),
            task_type: "send_sms".to_string(),
            status: "success".to_string(),
            message: "ok".to_string(),
            timestamp: "2026-05-14T17:00:00Z".to_string(),
        };
        assert_eq!(
            render_automation_template("{{本机号码}}|{{运营商}}", &automation, &context, false),
            "18888888888|中国移动"
        );
    }

    #[test]
    fn renders_rule_title_templates_with_sms_fallback() {
        let context = NotificationTemplateContext {
            own_number: "18888888888".to_string(),
            carrier: "中国移动".to_string(),
        };
        let sms_with_code = SmsMessage {
            id: 1,
            direction: "incoming".to_string(),
            phone_number: "16600001111".to_string(),
            content: "验证码 123456".to_string(),
            timestamp: "2026-05-14T17:00:00Z".to_string(),
            status: "received".to_string(),
            pdu: None,
        };
        let sms_event = NotificationEvent::Sms {
            message: &sms_with_code,
            context: &context,
        };
        assert_eq!(sms_event.render_title(""), "16600001111：验证码123456");

        let sms_without_code = SmsMessage {
            content: "普通短信内容".to_string(),
            ..sms_with_code
        };
        let sms_event = NotificationEvent::Sms {
            message: &sms_without_code,
            context: &context,
        };
        assert_eq!(sms_event.render_title(""), "16600001111");
        assert_eq!(
            sms_event.render_title(&crate::config::default_rule_title_template(
                NotificationEventType::Sms
            )),
            "16600001111"
        );

        let ddns = DdnsEvent::default();
        let ddns_event = NotificationEvent::Ddns(&ddns, &context);
        assert_eq!(ddns_event.render_title(""), "DDNS通知：18888888888");
    }

    #[test]
    fn detects_wecom_access_token_errors() {
        assert!(is_wecom_access_token_error(
            r#"{"errcode":40014,"errmsg":"invalidaccess_token"}"#
        ));
        assert!(is_wecom_access_token_error(
            r#"{"errcode":42001,"errmsg":"access_token expired"}"#
        ));
        assert!(!is_wecom_access_token_error(
            r#"{"errcode":0,"errmsg":"ok"}"#
        ));
    }

    #[test]
    fn normalizes_wecom_api_base_url() {
        let mut config = WecomAppConfig::default();
        assert_eq!(wecom_api_base_url(&config), "https://qyapi.weixin.qq.com");

        config.api_base_url = " https://relay.example.com/wecom/ ".to_string();
        assert_eq!(
            wecom_api_base_url(&config),
            "https://relay.example.com/wecom"
        );

        config.api_base_url = " ".to_string();
        assert_eq!(wecom_api_base_url(&config), "https://qyapi.weixin.qq.com");
    }

    #[test]
    fn builds_telegram_send_message_url_from_api_base_url() {
        let mut config = TelegramConfig::default();
        config.bot_token = "123456:abcdef".to_string();
        assert_eq!(
            telegram_send_message_url(&config),
            "https://api.telegram.org/bot123456:abcdef/sendMessage"
        );

        config.api_base_url = " https://relay.example.com/telegram/ ".to_string();
        assert_eq!(
            telegram_send_message_url(&config),
            "https://relay.example.com/telegram/bot123456:abcdef/sendMessage"
        );

        config.api_base_url = " ".to_string();
        assert_eq!(telegram_api_base_url(&config), "https://api.telegram.org");
    }

    #[test]
    fn query_value_encoding_preserves_unreserved_token_chars() {
        assert_eq!(encode_query_value("abc-DEF_123.~"), "abc-DEF_123.~");
        assert_eq!(encode_query_value("a+b/c=d&x y"), "a%2Bb%2Fc%3Dd%26x%20y");
    }

    #[test]
    fn formats_wecom_trusted_ip_error() {
        let error =
            format_channel_errcode("WeCom token", 60020, "not allow to access from your ip");

        assert!(error.contains("60020"));
        assert!(error.contains("企业可信 IP"));
        assert!(error.contains("中继/反代"));
    }
}
