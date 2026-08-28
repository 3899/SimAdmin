//! SMS Listener Module (ModemManager 版)
//!
//! 通过 D-Bus 信号监听 ModemManager 的短信接收事件，并增加轮询兜底，
//! 以便在部分 eSIM/国际运营商场景下尽量减少漏收。
use crate::db::{
    beijing_sms_now_string, normalize_sms_timestamp_for_display, Database, SmsMessage,
};
use crate::modem_manager::{cache_smsc_for_identity, current_sim_identity, find_modem_path};
use crate::notification::NotificationSender;
use futures_util::StreamExt;
use simadmin_device_runtime::{ModemContext, ReceivedSms as IncomingSms};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{Duration, MissedTickBehavior};
use tracing::{info, warn};
use zbus::{Connection, MessageStream, Proxy};

/// ModemManager 常量
const MM_SERVICE: &str = "org.freedesktop.ModemManager1";
const MM_MESSAGING: &str = "org.freedesktop.ModemManager1.Modem.Messaging";
const SMS_DELETE_DELAY_SECS: u64 = 5;
const MODEM_RETRY_DELAY_SECS: u64 = 5;
const SMS_POLL_INTERVAL_SECS: u64 = 15;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SmsIngestMode {
    Live,
    Reconcile,
}

#[derive(Clone)]
pub struct SmsResyncHandle {
    sender: mpsc::UnboundedSender<SmsResyncRequest>,
}

#[derive(Debug)]
pub struct SmsResyncRequest {
    reason: String,
}

pub type SmsResyncReceiver = mpsc::UnboundedReceiver<SmsResyncRequest>;

pub fn sms_resync_channel() -> (SmsResyncHandle, SmsResyncReceiver) {
    let (sender, receiver) = mpsc::unbounded_channel();
    (SmsResyncHandle { sender }, receiver)
}

impl SmsResyncHandle {
    pub fn request_scan(&self, reason: impl Into<String>) -> bool {
        self.sender
            .send(SmsResyncRequest {
                reason: reason.into(),
            })
            .is_ok()
    }
}

fn sms_marker(incoming: &IncomingSms) -> String {
    let raw = if incoming.timestamp.is_empty() {
        format!(
            "{}\n{}\n{}",
            incoming.path, incoming.number, incoming.content
        )
    } else {
        format!(
            "{}\n{}\n{}",
            incoming.number, incoming.timestamp, incoming.content
        )
    };
    format!("mmfp:{:x}", md5::compute(raw))
}

fn sms_timestamp(incoming: &IncomingSms, mode: SmsIngestMode) -> String {
    match mode {
        SmsIngestMode::Live => beijing_sms_now_string(),
        SmsIngestMode::Reconcile => normalize_sms_timestamp_for_display(&incoming.timestamp)
            .unwrap_or_else(beijing_sms_now_string),
    }
}

fn should_forward_after_insert(mode: SmsIngestMode, forward_reconciled_new_sms: bool) -> bool {
    mode == SmsIngestMode::Live || forward_reconciled_new_sms
}

/// 从 SMS 对象路径读取短信内容
async fn read_sms_content(
    conn: &Connection,
    modem_path: &str,
    sms_path: &str,
) -> Option<IncomingSms> {
    ModemContext::new(conn, modem_path)
        .ok()?
        .received_sms_at(sms_path)
        .await
        .ok()?
}

fn schedule_sms_delete(conn: &Connection, modem_path: &str, sms_path: String) {
    let conn_clone = conn.clone();
    let modem_path = modem_path.to_string();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(SMS_DELETE_DELAY_SECS)).await;
        let result = match ModemContext::new(&conn_clone, &modem_path) {
            Ok(context) => context.delete_sms(&sms_path).await,
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            warn!(%error, path = %sms_path, "Failed to delete processed SMS from ModemManager");
        }
    });
}

async fn process_sms_path(
    conn: &Connection,
    db: &Database,
    notification_sender: &Arc<NotificationSender>,
    modem_path: &str,
    sms_path: &str,
    mode: SmsIngestMode,
    forward_reconciled_new_sms: bool,
) {
    let Some(incoming) = read_sms_content(conn, modem_path, sms_path).await else {
        return;
    };

    let marker = sms_marker(&incoming);
    let timestamp = sms_timestamp(&incoming, mode);
    match db.sms_exists_by_pdu(&marker) {
        Ok(true) => {
            schedule_sms_delete(conn, modem_path, incoming.path);
            return;
        }
        Ok(false) => {}
        Err(e) => {
            warn!(error = %e, marker = %marker, "Failed to check SMS dedupe marker");
            return;
        }
    }

    if mode == SmsIngestMode::Reconcile {
        match db.incoming_sms_exists_by_timestamp(&incoming.number, &incoming.content, &timestamp) {
            Ok(true) => {
                schedule_sms_delete(conn, modem_path, incoming.path);
                return;
            }
            Ok(false) => {}
            Err(e) => {
                warn!(error = %e, marker = %marker, "Failed to check SMS timestamp identity");
                return;
            }
        }

        match db.incoming_sms_exists_by_legacy_content(&incoming.number, &incoming.content) {
            Ok(true) => {
                schedule_sms_delete(conn, modem_path, incoming.path);
                return;
            }
            Ok(false) => {}
            Err(e) => {
                warn!(error = %e, marker = %marker, "Failed to check legacy SMS identity");
                return;
            }
        }
    }

    info!(
        path = %incoming.path,
        from = %incoming.number,
        len = incoming.content.len(),
        "SMS content read"
    );

    if !incoming.sms_center.is_empty() {
        if let Some(identity) = current_sim_identity(conn).await {
            cache_smsc_for_identity(db, &identity, &incoming.sms_center, "sms_object");
        }
    }

    match db.insert_sms_at(
        "incoming",
        &incoming.number,
        &incoming.content,
        &timestamp,
        "received",
        Some(&marker),
    ) {
        Ok(id) => {
            let sms = SmsMessage {
                id,
                direction: "incoming".to_string(),
                phone_number: incoming.number,
                content: incoming.content,
                timestamp,
                status: "received".to_string(),
                pdu: Some(marker),
            };
            if should_forward_after_insert(mode, forward_reconciled_new_sms) {
                let notification_sender = Arc::clone(notification_sender);
                tokio::spawn(async move {
                    let _ = notification_sender.forward_sms(&sms).await;
                });
            }

            schedule_sms_delete(conn, modem_path, incoming.path);
        }
        Err(e) => {
            warn!(error = %e, path = %incoming.path, "Failed to store incoming SMS");
        }
    }
}

async fn list_sms_paths(conn: &Connection, modem_path: &str) -> zbus::Result<Vec<String>> {
    ModemContext::new(conn, modem_path)
        .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?
        .sms_paths()
        .await
        .map_err(|error| zbus::fdo::Error::Failed(error.to_string()).into())
}

async fn scan_sms_paths(
    conn: &Connection,
    db: &Database,
    notification_sender: &Arc<NotificationSender>,
    modem_path: &str,
    reason: &str,
    forward_new_sms: bool,
) {
    match list_sms_paths(conn, modem_path).await {
        Ok(paths) => {
            if !paths.is_empty() {
                info!(
                    modem_path = %modem_path,
                    count = paths.len(),
                    reason = %reason,
                    "Scanning ModemManager SMS objects"
                );
            }
            for sms_path in paths {
                process_sms_path(
                    conn,
                    db,
                    notification_sender,
                    modem_path,
                    &sms_path,
                    SmsIngestMode::Reconcile,
                    forward_new_sms,
                )
                .await;
            }
        }
        Err(e) => {
            warn!(
                error = %e,
                modem_path = %modem_path,
                reason = %reason,
                "Failed to scan ModemManager SMS objects"
            );
        }
    }
}

async fn scan_current_modem_or_rebind(
    conn: &Connection,
    db: &Database,
    notification_sender: &Arc<NotificationSender>,
    modem_path: &str,
    reason: &str,
    forward_new_sms: bool,
) -> bool {
    match find_modem_path(conn).await {
        Ok(current_path) if current_path == modem_path => {
            scan_sms_paths(
                conn,
                db,
                notification_sender,
                modem_path,
                reason,
                forward_new_sms,
            )
            .await;
            true
        }
        Ok(current_path) => {
            info!(
                old_modem_path = %modem_path,
                new_modem_path = %current_path,
                reason = %reason,
                "SMS listener detected modem path change"
            );
            scan_sms_paths(
                conn,
                db,
                notification_sender,
                current_path.as_str(),
                reason,
                false,
            )
            .await;
            false
        }
        Err(e) => {
            warn!(
                error = %e,
                reason = %reason,
                "SMS listener lost modem while scanning"
            );
            false
        }
    }
}

async fn call_dbus_match(conn: &Connection, method: &str, rule: &str) -> zbus::Result<()> {
    let dbus_proxy = Proxy::new(
        conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .await?;
    dbus_proxy.call::<_, _, ()>(method, &(rule,)).await
}

/// Start SMS listener (ModemManager 版)
///
/// 监听 ModemManager 的 Messaging.Added 信号。
pub async fn start_sms_listener(
    conn: Connection,
    db: Arc<Database>,
    notification_sender: Arc<NotificationSender>,
    mut resync_receiver: SmsResyncReceiver,
) -> zbus::Result<()> {
    info!("Starting SMS listener (ModemManager mode)");
    loop {
        let modem_path = loop {
            match find_modem_path(&conn).await {
                Ok(path) => break path,
                Err(e) => {
                    warn!(
                        error = %e,
                        retry_after_secs = MODEM_RETRY_DELAY_SECS,
                        "SMS listener waiting for modem"
                    );
                    tokio::time::sleep(Duration::from_secs(MODEM_RETRY_DELAY_SECS)).await;
                }
            }
        };

        let rule = format!(
            "type='signal',sender='{}',interface='{}',member='Added',path='{}'",
            MM_SERVICE, MM_MESSAGING, modem_path
        );
        if let Err(e) = call_dbus_match(&conn, "AddMatch", rule.as_str()).await {
            warn!(
                error = %e,
                retry_after_secs = MODEM_RETRY_DELAY_SECS,
                "Failed to register SMS listener match"
            );
            tokio::time::sleep(Duration::from_secs(MODEM_RETRY_DELAY_SECS)).await;
            continue;
        }

        info!(modem_path = %modem_path, "SMS listener registered, waiting for messages...");

        scan_sms_paths(
            &conn,
            &db,
            &notification_sender,
            modem_path.as_str(),
            "initial",
            false,
        )
        .await;

        let mut stream = MessageStream::from(&conn);
        let mut poll_interval = tokio::time::interval(Duration::from_secs(SMS_POLL_INTERVAL_SECS));
        poll_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        poll_interval.tick().await;

        loop {
            tokio::select! {
                maybe_msg = stream.next() => {
                    let msg = match maybe_msg {
                        Some(Ok(msg)) => msg,
                        Some(Err(e)) => {
                            warn!(error = %e, "SMS listener stream error");
                            break;
                        }
                        None => break,
                    };

                    if let Some(member) = msg.header().member() {
                        if member.as_str() == "Added" {
                            if let Ok((sms_path, received)) = msg
                                .body()
                                .deserialize::<(zbus::zvariant::ObjectPath, bool)>()
                            {
                                if !received {
                                    continue;
                                }

                                let sms_path_str = sms_path.to_string();
                                info!(path = %sms_path_str, "New SMS received");

                                // Give ModemManager a short moment to assemble multipart SMS content.
                                tokio::time::sleep(Duration::from_millis(500)).await;
                                process_sms_path(
                                    &conn,
                                    &db,
                                    &notification_sender,
                                    modem_path.as_str(),
                                    &sms_path_str,
                                    SmsIngestMode::Live,
                                    false,
                                )
                                .await;
                            }
                        }
                    }
                }
                _ = poll_interval.tick() => {
                    if !scan_current_modem_or_rebind(
                        &conn,
                        &db,
                        &notification_sender,
                        modem_path.as_str(),
                        "poll",
                        true,
                    ).await {
                        break;
                    }
                }
                Some(request) = resync_receiver.recv() => {
                    info!(reason = %request.reason, "SMS resync requested");
                    if !scan_current_modem_or_rebind(
                        &conn,
                        &db,
                        &notification_sender,
                        modem_path.as_str(),
                        request.reason.as_str(),
                        false,
                    ).await {
                        break;
                    }
                }
            }
        }

        if let Err(e) = call_dbus_match(&conn, "RemoveMatch", rule.as_str()).await {
            warn!(error = %e, "Failed to remove SMS listener match");
        }

        warn!(
            retry_after_secs = MODEM_RETRY_DELAY_SECS,
            "SMS listener stream ended, re-registering after delay"
        );
        tokio::time::sleep(Duration::from_secs(MODEM_RETRY_DELAY_SECS)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_sms_timestamp_with_short_timezone() {
        assert_eq!(
            normalize_sms_timestamp_for_display("2026-05-19 20:17:25+08").as_deref(),
            Some("2026-05-19 20:17:25")
        );
    }

    #[test]
    fn keeps_naive_sms_timestamp_frontend_parseable() {
        assert_eq!(
            normalize_sms_timestamp_for_display("2026-05-19 16:50:26").as_deref(),
            Some("2026-05-19 16:50:26")
        );
    }

    #[test]
    fn rejects_unparseable_sms_timestamp() {
        assert_eq!(normalize_sms_timestamp_for_display("not-a-date"), None);
    }

    #[test]
    fn forwards_live_sms_after_insert() {
        assert!(should_forward_after_insert(SmsIngestMode::Live, false));
    }

    #[test]
    fn forwards_reconciled_sms_only_when_enabled_for_scan() {
        assert!(should_forward_after_insert(SmsIngestMode::Reconcile, true));
        assert!(!should_forward_after_insert(
            SmsIngestMode::Reconcile,
            false
        ));
    }
}
