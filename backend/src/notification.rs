use crate::config::{
    BarkConfig, ConfigManager, DingtalkAppConfig, DingtalkRobotConfig, EmailConfig,
    FeishuRobotConfig, LegacyNotificationConfig, MatcherOperator, MessageChannelConfig,
    NotificationChannel, NotificationChannelInstance, NotificationConfig, NotificationEventType,
    NotificationRule, PushPlusConfig, QuietHoursSchedule, ServerChan3Config, TelegramConfig,
    WebhookConfig, WecomAppConfig, WecomRobotConfig,
};
use crate::db::{
    CallRecord, Database, NewNotificationLog, NewNotificationQueueItem, NotificationQueueEntry,
    SmsMessage,
};
use crate::device_status::DeviceStatusReport;
use crate::models::{DdnsEvent, VersionUpdateEvent};
use crate::modem_manager::get_sim_info_data_with_cache;
use crate::system_event::SystemEvent;
use crate::verification_code::extract_verification_code;
use chrono::{
    DateTime, Datelike, Duration as ChronoDuration, FixedOffset, NaiveDateTime, Timelike, Utc,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::warn;
use zbus::Connection;

const BEIJING_UTC_OFFSET_SECONDS: i32 = 8 * 60 * 60;
const NOTIFICATION_TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
/// Notification sender for all configured notification channels.
pub struct NotificationSender {
    shared_sender: simadmin_notify::Sender,
    config_manager: Arc<ConfigManager>,
    dbus_conn: Arc<Connection>,
    database: Arc<Database>,
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

struct NotificationDelivery<'a, 'event> {
    event: &'a NotificationEvent<'event>,
    rule: &'a NotificationRule,
    channel: &'a NotificationChannelInstance,
    title: &'a str,
    body: &'a str,
    summary: &'a str,
    use_custom_body: bool,
}

impl NotificationDelivery<'_, '_> {
    fn queue_item<'a>(
        &'a self,
        status: &'a str,
        reason: &'a str,
        next_attempt_at: &'a str,
    ) -> NewNotificationQueueItem<'a> {
        NewNotificationQueueItem {
            status,
            event_type: notification_event_type_key(self.event.event_type()),
            event_label: self.event.event_type().label(),
            summary: self.summary,
            reason,
            rule_id: &self.rule.id,
            rule_name: &self.rule.name,
            channel_id: &self.channel.id,
            channel_name: &self.channel.name,
            channel_type: self.channel.channel_type.key(),
            title: self.title,
            body: self.body,
            next_attempt_at,
            max_attempts: 5,
        }
    }
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

    /// Render a JSON template with all variable values properly JSON-escaped.
    /// Used for custom_body rendering where the output must be valid JSON.
    fn render_json_safe(&self, template: &str) -> String {
        match self {
            NotificationEvent::Sms { message, context } => {
                render_sms_template(template, message, context, true)
            }
            NotificationEvent::Ddns(event, context) => {
                render_ddns_template(template, event, context, true)
            }
            NotificationEvent::VersionUpdate(event, context) => {
                render_version_update_template(template, event, context, true)
            }
            NotificationEvent::SystemEvent(event, context) => {
                render_system_event_template(template, event, context, true)
            }
            NotificationEvent::DeviceStatus(report, context) => {
                render_device_status_template(template, report, context, true)
            }
            NotificationEvent::Automation(event, context) => {
                render_automation_template(template, event, context, true)
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
            shared_sender: simadmin_notify::Sender::new()
                .expect("shared notification client configuration is valid"),
            config_manager,
            dbus_conn,
            database,
        }
    }

    fn get_config(&self) -> NotificationConfig {
        self.config_manager.get_notifications()
    }

    async fn send_shared_config<T: Serialize>(
        &self,
        channel_type: simadmin_notify::ChannelType,
        config: &T,
        title: String,
        body: String,
        custom_body: Option<String>,
    ) -> Result<String, String> {
        let config = serde_json::to_value(config)
            .map_err(|error| format!("Failed to serialize notification config: {error}"))?;
        let receipt = self
            .shared_sender
            .send(
                channel_type,
                &config,
                &simadmin_notify::NotificationMessage {
                    title,
                    body,
                    custom_body,
                },
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(format!(
            "{} returned {}: {}",
            receipt.provider, receipt.status_code, receipt.response_summary
        ))
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
        if crate::hub_agent::queue_notification_event(
            &self.config_manager,
            &self.database,
            "sms",
            "sms.received",
            compact_summary(&format!("[{}] {}", message.phone_number, message.content)),
            serde_json::to_value(message).map_err(|error| error.to_string())?,
        )
        .await?
        {
            return Ok(());
        }
        self.forward_sms_local(message).await
    }

    async fn forward_sms_local(&self, message: &SmsMessage) -> Result<(), String> {
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
        if crate::hub_agent::queue_notification_event(
            &self.config_manager,
            &self.database,
            "ddns",
            &format!("ddns.{}", event.status),
            compact_summary(&format!("{} {}", event.domains.join(", "), event.message)),
            serde_json::to_value(event).map_err(|error| error.to_string())?,
        )
        .await?
        {
            return Ok(());
        }
        self.forward_ddns_event_local(event).await
    }

    async fn forward_ddns_event_local(&self, event: &DdnsEvent) -> Result<(), String> {
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
        if crate::hub_agent::queue_notification_event(
            &self.config_manager,
            &self.database,
            "automation",
            &format!("automation.{}", event.status),
            compact_summary(&format!("[{}] {}", event.task_name, event.message)),
            serde_json::to_value(event).map_err(|error| error.to_string())?,
        )
        .await?
        {
            return Ok(());
        }
        self.forward_automation_event_local(event).await
    }

    async fn forward_automation_event_local(&self, event: &AutomationEvent) -> Result<(), String> {
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
        if crate::hub_agent::queue_notification_event(
            &self.config_manager,
            &self.database,
            "version_update",
            "version_update.available",
            compact_summary(&format!("{} {}", event.version, event.asset_name)),
            serde_json::to_value(event).map_err(|error| error.to_string())?,
        )
        .await?
        {
            return Ok(NotificationFanoutResult {
                delivered: true,
                errors: Vec::new(),
            });
        }
        self.forward_version_update_event_local(event).await
    }

    async fn forward_version_update_event_local(
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
        if crate::hub_agent::queue_notification_event(
            &self.config_manager,
            &self.database,
            "system_event",
            &event.event_code,
            compact_summary(&format!("{} {}", event.event_label, event.message)),
            serde_json::to_value(event).map_err(|error| error.to_string())?,
        )
        .await?
        {
            return Ok(());
        }
        self.forward_system_event_local(event).await
    }

    async fn forward_system_event_local(&self, event: &SystemEvent) -> Result<(), String> {
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
        if crate::hub_agent::queue_notification_event(
            &self.config_manager,
            &self.database,
            "device_status",
            &format!("device_status.{rule_id}"),
            "设备状态定时报表".to_owned(),
            json!({ "rule_id": rule_id, "report": report }),
        )
        .await?
        {
            return Ok(());
        }
        self.forward_device_status_report_local(rule_id, report)
            .await
    }

    async fn forward_device_status_report_local(
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

    pub async fn deliver_queued_hub_event(&self, item_id: &str) -> Result<(), String> {
        if self
            .database
            .mark_hub_event_local_fallback(item_id)
            .map_err(|error| error.to_string())?
            == 0
        {
            return Ok(());
        }
        let event = self
            .database
            .hub_event(item_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Hub event no longer exists".to_string())?;
        match event.event_type.as_str() {
            "sms" => {
                let value: SmsMessage =
                    serde_json::from_value(event.details).map_err(|error| error.to_string())?;
                self.forward_sms_local(&value).await
            }
            "ddns" => {
                let value: DdnsEvent =
                    serde_json::from_value(event.details).map_err(|error| error.to_string())?;
                self.forward_ddns_event_local(&value).await
            }
            "version_update" => {
                let value: VersionUpdateEvent =
                    serde_json::from_value(event.details).map_err(|error| error.to_string())?;
                self.forward_version_update_event_local(&value)
                    .await
                    .map(|_| ())
            }
            "system_event" => {
                let value: SystemEvent =
                    serde_json::from_value(event.details).map_err(|error| error.to_string())?;
                self.forward_system_event_local(&value).await
            }
            "device_status" => {
                let rule_id = event
                    .details
                    .get("rule_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let report: DeviceStatusReport = serde_json::from_value(
                    event.details.get("report").cloned().unwrap_or_default(),
                )
                .map_err(|error| error.to_string())?;
                self.forward_device_status_report_local(rule_id, &report)
                    .await
            }
            "automation" => {
                let value: AutomationEvent =
                    serde_json::from_value(event.details).map_err(|error| error.to_string())?;
                self.forward_automation_event_local(&value).await
            }
            value => Err(format!("unsupported queued Hub event type {value}")),
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
            let use_custom_body = !rule.custom_body.trim().is_empty();
            let custom_body_rendered = if use_custom_body {
                event.render_json_safe(&rule.custom_body)
            } else {
                String::new()
            };
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
                // Branch: custom_body mode vs plain text mode
                let send_body = if use_custom_body {
                    &custom_body_rendered
                } else {
                    &text
                };
                match self
                    .send_text_to_channel_with_queue(NotificationDelivery {
                        event,
                        rule,
                        channel,
                        title: &title,
                        body: send_body,
                        summary: log_summary,
                        use_custom_body,
                    })
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
        self.record_notification_log_raw(NewNotificationLog {
            event_type: notification_event_type_key(event_type),
            status,
            summary,
            rule_id,
            rule_name,
            channel_id,
            channel_name,
            message,
        });
    }

    fn record_notification_log_raw(&self, log: NewNotificationLog<'_>) {
        if let Err(err) = self.database.insert_notification_log(log) {
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
        delivery: NotificationDelivery<'_, '_>,
    ) -> Result<ChannelDeliveryResult, String> {
        if let Some(reason) = self.rate_limit_reason(delivery.channel)? {
            let next_attempt_at = beijing_time_after_seconds(i64::from(
                delivery.channel.rate_limit.window_seconds.max(1),
            ));
            self.enqueue_notification(delivery.queue_item("scheduled", &reason, &next_attempt_at))?;
            return Ok(ChannelDeliveryResult::Queued(reason));
        }

        let send_result = if delivery.use_custom_body {
            self.send_custom_body_to_channel(delivery.channel, delivery.body)
                .await
        } else {
            self.send_text_to_channel(delivery.channel, delivery.title, delivery.body)
                .await
        };
        match send_result {
            Ok(message) => Ok(ChannelDeliveryResult::Sent(message)),
            Err(err) => {
                let next_attempt_at = beijing_time_after_seconds(60);
                let reason = format!("发送失败，已加入通知队列：{err}");
                self.enqueue_notification(delivery.queue_item(
                    "retrying",
                    &reason,
                    &next_attempt_at,
                ))?;
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

    fn enqueue_notification(&self, item: NewNotificationQueueItem<'_>) -> Result<i64, String> {
        self.database
            .insert_notification_queue_item(item)
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

        // Auto-detect custom body: if stored body is valid JSON object/array, use custom body path
        let is_custom_body = {
            let trimmed = item.body.trim();
            (trimmed.starts_with('{') || trimmed.starts_with('['))
                && serde_json::from_str::<Value>(trimmed).is_ok()
        };
        let send_result = if is_custom_body {
            self.send_custom_body_to_channel(channel, &item.body).await
        } else {
            self.send_text_to_channel(channel, &item.title, &item.body)
                .await
        };
        match send_result {
            Ok(message) => {
                if let Err(err) = self.database.mark_notification_queue_sent(item.id) {
                    warn!(error = %err, id = item.id, "Failed to mark notification queue item sent");
                }
                self.record_notification_log_raw(NewNotificationLog {
                    event_type: &item.event_type,
                    status: "success",
                    summary: &item.summary,
                    rule_id: &item.rule_id,
                    rule_name: &item.rule_name,
                    channel_id: &channel.id,
                    channel_name: &channel.name,
                    message: &message,
                });
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
        self.record_notification_log_raw(NewNotificationLog {
            event_type: &item.event_type,
            status: "failed",
            summary: &item.summary,
            rule_id: &item.rule_id,
            rule_name: &item.rule_name,
            channel_id: &item.channel_id,
            channel_name: &item.channel_name,
            message: err,
        });
    }

    async fn send_text_to_channel(
        &self,
        channel: &NotificationChannelInstance,
        title: &str,
        text: &str,
    ) -> Result<String, String> {
        let receipt = self
            .shared_sender
            .send(
                shared_channel_type(channel.channel_type),
                &channel.config,
                &simadmin_notify::NotificationMessage {
                    title: title.to_string(),
                    body: text.to_string(),
                    custom_body: None,
                },
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(format!(
            "{} returned {}: {}",
            receipt.provider, receipt.status_code, receipt.response_summary
        ))
    }

    /// Send a custom JSON body to a channel, reusing each channel's authentication/signing.
    /// The `rendered_body` is expected to be a valid JSON string with all placeholders already rendered.
    async fn send_custom_body_to_channel(
        &self,
        channel: &NotificationChannelInstance,
        rendered_body: &str,
    ) -> Result<String, String> {
        serde_json::from_str::<Value>(rendered_body)
            .map_err(|error| format!("自定义请求体 JSON 格式错误: {error}"))?;
        let receipt = self
            .shared_sender
            .send(
                shared_channel_type(channel.channel_type),
                &channel.config,
                &simadmin_notify::NotificationMessage {
                    title: "自定义通知".into(),
                    body: rendered_body.into(),
                    custom_body: Some(rendered_body.into()),
                },
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(format!(
            "{} returned {}: {}",
            receipt.provider, receipt.status_code, receipt.response_summary
        ))
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
        self.send_shared_config(
            simadmin_notify::ChannelType::Webhook,
            config,
            "SimAdmin 通知".into(),
            payload.into(),
            Some(payload.into()),
        )
        .await
    }

    async fn send_webhook_text(
        &self,
        config: &WebhookConfig,
        text: &str,
    ) -> Result<String, String> {
        self.send_shared_config(
            simadmin_notify::ChannelType::Webhook,
            config,
            "SimAdmin 通知".into(),
            text.into(),
            None,
        )
        .await
    }

    /// Send a custom body to a Webhook channel, supporting both POST and GET methods.
    async fn send_webhook_custom_body(
        &self,
        config: &WebhookConfig,
        body: &str,
    ) -> Result<String, String> {
        self.send_shared_config(
            simadmin_notify::ChannelType::Webhook,
            config,
            "SimAdmin 通知".into(),
            body.into(),
            Some(body.into()),
        )
        .await
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
        self.send_shared_config(
            simadmin_notify::ChannelType::Bark,
            config,
            title,
            body,
            None,
        )
        .await
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
        self.send_shared_config(
            simadmin_notify::ChannelType::Pushplus,
            config,
            title,
            content,
            None,
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
        self.send_shared_config(
            simadmin_notify::ChannelType::WecomApp,
            config,
            "SimAdmin 通知".into(),
            text,
            None,
        )
        .await
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
        self.send_shared_config(
            simadmin_notify::ChannelType::WecomRobot,
            config,
            "SimAdmin 通知".into(),
            text,
            None,
        )
        .await
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
        self.send_shared_config(
            simadmin_notify::ChannelType::DingtalkRobot,
            config,
            "SimAdmin 通知".into(),
            text,
            None,
        )
        .await
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
        self.send_shared_config(
            simadmin_notify::ChannelType::DingtalkApp,
            config,
            "SimAdmin 通知".into(),
            text,
            None,
        )
        .await
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
        self.send_shared_config(
            simadmin_notify::ChannelType::FeishuRobot,
            config,
            "SimAdmin 通知".into(),
            text,
            None,
        )
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
        self.send_shared_config(
            simadmin_notify::ChannelType::Telegram,
            config,
            "SimAdmin 通知".into(),
            text,
            None,
        )
        .await
    }

    async fn send_serverchan3_message(
        &self,
        config: &ServerChan3Config,
        title: String,
        desp: String,
    ) -> Result<String, String> {
        self.send_shared_config(
            simadmin_notify::ChannelType::Serverchan3,
            config,
            title,
            desp,
            None,
        )
        .await
    }

    async fn send_email_message(
        &self,
        config: &EmailConfig,
        subject: String,
        body: String,
    ) -> Result<String, String> {
        self.send_shared_config(
            simadmin_notify::ChannelType::Email,
            config,
            subject,
            body,
            None,
        )
        .await
    }
}

fn shared_channel_type(channel: NotificationChannel) -> simadmin_notify::ChannelType {
    match channel {
        NotificationChannel::Webhook => simadmin_notify::ChannelType::Webhook,
        NotificationChannel::Bark => simadmin_notify::ChannelType::Bark,
        NotificationChannel::PushPlus => simadmin_notify::ChannelType::Pushplus,
        NotificationChannel::WecomApp => simadmin_notify::ChannelType::WecomApp,
        NotificationChannel::WecomRobot => simadmin_notify::ChannelType::WecomRobot,
        NotificationChannel::DingtalkRobot => simadmin_notify::ChannelType::DingtalkRobot,
        NotificationChannel::DingtalkApp => simadmin_notify::ChannelType::DingtalkApp,
        NotificationChannel::FeishuRobot => simadmin_notify::ChannelType::FeishuRobot,
        NotificationChannel::Telegram => simadmin_notify::ChannelType::Telegram,
        NotificationChannel::Email => simadmin_notify::ChannelType::Email,
        NotificationChannel::ServerChan3 => simadmin_notify::ChannelType::Serverchan3,
    }
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

fn contains_verification_code_placeholder(s: &str) -> bool {
    s.contains("{{验证码}}") || s.contains("{{verification_code}}")
}

fn is_standalone_verification_code_line(line: &str) -> bool {
    let rem = line
        .replace("{{验证码}}", "")
        .replace("{{verification_code}}", "");
    let trimmed = rem.trim();

    if trimmed.is_empty() {
        return true;
    }

    let lower = trimmed.to_lowercase();
    let keywords = [
        "验证码",
        "动态验证码",
        "verification code",
        "verification_code",
        "code",
        "otp",
        "captcha",
        "passcode",
    ];

    let mut stripped = lower;
    for kw in keywords {
        stripped = stripped.replace(kw, "");
    }

    stripped.chars().all(|c| {
        c.is_whitespace()
            || matches!(
                c,
                ':' | '：'
                    | '-'
                    | '—'
                    | '|'
                    | ','
                    | '，'
                    | ';'
                    | '；'
                    | '['
                    | ']'
                    | '【'
                    | '】'
                    | '('
                    | ')'
                    | '（'
                    | '）'
                    | '"'
                    | '\''
                    | '📱'
                    | '💬'
                    | '🔑'
                    | '🔒'
            )
    })
}

fn clean_inline_verification_code_placeholder(text: &str) -> String {
    let mut result = text.to_string();

    let bracketed_patterns = [
        "（验证码: {{验证码}}）",
        "（验证码：{{验证码}}）",
        "（验证码{{验证码}}）",
        "(验证码: {{验证码}})",
        "(验证码：{{验证码}})",
        "(验证码{{验证码}})",
        "【验证码: {{验证码}}】",
        "【验证码：{{验证码}}】",
        "[验证码: {{验证码}}]",
        "[验证码：{{验证码}}]",
        "(Code: {{verification_code}})",
        "(Verification Code: {{verification_code}})",
        "（verification_code: {{verification_code}}）",
    ];
    for p in bracketed_patterns {
        result = result.replace(p, "");
    }

    let prefix_patterns = [
        "：验证码: {{验证码}}",
        "：验证码：{{验证码}}",
        "：验证码{{验证码}}",
        ": 验证码: {{验证码}}",
        ": 验证码：{{验证码}}",
        ": 验证码{{验证码}}",
        " - 验证码: {{验证码}}",
        " - 验证码：{{验证码}}",
        " - 验证码{{验证码}}",
        " | 验证码: {{验证码}}",
        " | 验证码：{{验证码}}",
        " | 验证码{{验证码}}",
        "，验证码: {{验证码}}",
        "，验证码：{{验证码}}",
        "，验证码{{验证码}}",
        ", 验证码: {{验证码}}",
        ", 验证码：{{验证码}}",
        ", 验证码{{验证码}}",
        "；验证码: {{验证码}}",
        "；验证码：{{验证码}}",
        "；验证码{{验证码}}",
        "; 验证码: {{验证码}}",
        "; 验证码：{{验证码}}",
        "; 验证码{{验证码}}",
        "：Code: {{verification_code}}",
        ": Code: {{verification_code}}",
        " - Code: {{verification_code}}",
        " | Code: {{verification_code}}",
        ", Code: {{verification_code}}",
        "：{{verification_code}}",
        ": {{verification_code}}",
        " - {{verification_code}}",
        " | {{verification_code}}",
        "：{{验证码}}",
        ": {{验证码}}",
        " - {{验证码}}",
        " | {{验证码}}",
    ];
    for p in prefix_patterns {
        result = result.replace(p, "");
    }

    let suffix_patterns = [
        "验证码: {{验证码}}，",
        "验证码：{{验证码}}，",
        "验证码: {{验证码}}, ",
        "验证码：{{验证码}}, ",
        "验证码: {{验证码}}；",
        "验证码：{{验证码}}；",
        "验证码: {{验证码}}; ",
        "验证码：{{验证码}}; ",
        "验证码: {{验证码}} - ",
        "验证码：{{验证码}} - ",
        "验证码: {{验证码}} | ",
        "验证码：{{验证码}} | ",
        "验证码: {{验证码}}",
        "验证码：{{验证码}}",
        "验证码 {{验证码}}",
        "验证码{{验证码}}",
        "动态验证码: {{验证码}}",
        "动态验证码：{{验证码}}",
        "动态验证码{{验证码}}",
        "Verification Code: {{verification_code}}, ",
        "Verification Code: {{verification_code}}",
        "Code: {{verification_code}}, ",
        "Code: {{verification_code}}",
        "Code {{verification_code}}",
        "code: {{verification_code}}",
    ];
    for p in suffix_patterns {
        result = result.replace(p, "");
    }

    result
        .replace("{{验证码}}", "")
        .replace("{{verification_code}}", "")
}

fn clean_empty_verification_code_template(template: &str) -> String {
    if !contains_verification_code_placeholder(template) {
        return template.to_string();
    }

    let original_lines: Vec<&str> = template.split('\n').collect();
    let mut lines = Vec::new();

    for line in original_lines {
        let trimmed_line = line.trim_end_matches('\r');
        if contains_verification_code_placeholder(trimmed_line) {
            if is_standalone_verification_code_line(trimmed_line) {
                continue;
            } else {
                lines.push(clean_inline_verification_code_placeholder(trimmed_line));
            }
        } else {
            lines.push(trimmed_line.to_string());
        }
    }

    let joined = lines.join("\n");
    clean_inline_verification_code_placeholder(&joined)
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

    let template_to_use = if verification_code.is_empty() {
        clean_empty_verification_code_template(template)
    } else {
        template.to_string()
    };

    let rendered = template_to_use
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
        if (ch == '+' && compact.is_empty()) || ch.is_ascii_digit() {
            compact.push(ch);
        }
    }

    let has_plus = compact.starts_with('+');
    let digits = compact.strip_prefix('+').unwrap_or(&compact);
    if digits.len() == 13 && digits.starts_with("86") {
        return digits[2..].to_string();
    }
    if !(has_plus || digits.len() == 11 && digits.starts_with('1')) {
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
            custom_body: String::new(),
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
            custom_body: String::new(),
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
            content: "xxx气象台2026年07月27日00时25分发布暴雨橙色预警信号".to_string(),
            phone_number: "1063".to_string(),
            ..sms_with_code
        };
        let sms_event = NotificationEvent::Sms {
            message: &sms_without_code,
            context: &context,
        };
        assert_eq!(sms_event.render_title(""), "1063");
        assert_eq!(
            sms_event.render_title(&crate::config::default_rule_title_template(
                NotificationEventType::Sms
            )),
            "1063"
        );

        let multiline_template =
            "📱 短信通知\n号码: {{发送方号码}}\n验证码: {{验证码}}\n内容: {{短信内容}}";
        let rendered_multiline =
            render_sms_template(multiline_template, &sms_without_code, &context, false);
        assert_eq!(
            rendered_multiline,
            "📱 短信通知\n号码: 1063\n内容: xxx气象台2026年07月27日00时25分发布暴雨橙色预警信号"
        );

        let inline_template = "号码: {{发送方号码}}, 验证码: {{验证码}}, 内容: {{短信内容}}";
        let rendered_inline =
            render_sms_template(inline_template, &sms_without_code, &context, false);
        assert_eq!(
            rendered_inline,
            "号码: 1063, 内容: xxx气象台2026年07月27日00时25分发布暴雨橙色预警信号"
        );

        let ddns = DdnsEvent::default();
        let ddns_event = NotificationEvent::Ddns(&ddns, &context);
        assert_eq!(ddns_event.render_title(""), "DDNS通知：18888888888");
    }
}
