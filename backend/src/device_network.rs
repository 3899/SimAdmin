//! Device-side network features: DDNS and WLAN client management.

use std::collections::{BTreeMap, VecDeque};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose, Engine as _};
use chrono::{Duration as ChronoDuration, SecondsFormat, Utc};
use reqwest::Client;
use ring::{digest, hmac};
use serde_json::{json, Value as JsonValue};
use tokio::sync::Mutex;

use crate::config::{ConfigManager, DdnsConfig, DdnsIpConfig};
use crate::models::{
    DdnsEvent, DdnsLogEntry, DdnsLogsResponse, DdnsRecordSyncResult, DdnsStatusResponse,
    DdnsSyncResponse, WlanConnectRequest, WlanEnabledRequest, WlanForgetRequest, WlanNetwork,
    WlanProfileRequest, WlanProfilesResponse, WlanSavedNetwork, WlanScanResponse,
    WlanStatusResponse,
};
use crate::notification::NotificationSender;
use crate::utils::{
    interface_addresses_for_family, preferred_interface_for_family, read_network_interfaces,
    NetworkAddressFamily,
};

const DDNS_LOG_LIMIT: usize = 50;
const WLAN_ROUTE_METRIC: &str = "100";

#[derive(Debug, Default)]
struct DdnsRuntimeState {
    running: bool,
    last_sync_at: Option<String>,
    last_ipv4: Option<String>,
    last_ipv6: Option<String>,
    last_message: Option<String>,
    logs: VecDeque<DdnsLogEntry>,
    failure_counts: BTreeMap<String, u32>,
}

#[derive(Clone)]
pub struct DdnsManager {
    client: Client,
    state: Arc<Mutex<DdnsRuntimeState>>,
}

impl DdnsManager {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(20))
                .user_agent("SimAdmin DDNS")
                .build()
                .expect("Failed to create DDNS HTTP client"),
            state: Arc::new(Mutex::new(DdnsRuntimeState::default())),
        }
    }

    pub async fn status(&self, config: &DdnsConfig) -> DdnsStatusResponse {
        let state = self.state.lock().await;
        DdnsStatusResponse {
            enabled: config.enabled,
            running: state.running,
            provider: config.provider.clone(),
            last_sync_at: state.last_sync_at.clone(),
            last_ipv4: state.last_ipv4.clone(),
            last_ipv6: state.last_ipv6.clone(),
            last_message: state.last_message.clone(),
        }
    }

    pub async fn logs(&self) -> DdnsLogsResponse {
        let state = self.state.lock().await;
        DdnsLogsResponse {
            entries: state.logs.iter().cloned().collect(),
        }
    }

    pub async fn clear_logs(&self) {
        let mut state = self.state.lock().await;
        state.logs.clear();
    }

    pub async fn sync_now(
        &self,
        config_manager: Arc<ConfigManager>,
        notification_sender: Arc<NotificationSender>,
    ) -> Result<DdnsSyncResponse, String> {
        {
            let mut state = self.state.lock().await;
            if state.running {
                return Err("DDNS sync is already running".to_string());
            }
            state.running = true;
        }

        let started_at = now_string();
        let config = config_manager.get_ddns_config();
        let result = self.sync_inner(&config, notification_sender).await;
        let finished_at = now_string();

        let mut state = self.state.lock().await;
        state.running = false;
        state.last_sync_at = Some(finished_at.clone());

        match result {
            Ok(records) => {
                for record in &records {
                    if record.record_type == "A" && record.new_ip.is_some() {
                        state.last_ipv4 = record.new_ip.clone();
                    }
                    if record.record_type == "AAAA" && record.new_ip.is_some() {
                        state.last_ipv6 = record.new_ip.clone();
                    }
                }
                let message = records
                    .last()
                    .map(|record| record.message.clone())
                    .unwrap_or_else(|| "DDNS disabled or no enabled records".to_string());
                state.last_message = Some(message);
                Ok(DdnsSyncResponse {
                    started_at,
                    finished_at,
                    records,
                })
            }
            Err(err) => {
                state.last_message = Some(err.clone());
                Err(err)
            }
        }
    }

    async fn sync_inner(
        &self,
        config: &DdnsConfig,
        notification_sender: Arc<NotificationSender>,
    ) -> Result<Vec<DdnsRecordSyncResult>, String> {
        if !config.enabled {
            self.push_log("info", "", Vec::new(), "DDNS is disabled")
                .await;
            return Ok(Vec::new());
        }

        let mut records = Vec::new();
        if config.ipv4.enabled {
            records.push(
                self.sync_record(config, &config.ipv4, "A", notification_sender.clone())
                    .await,
            );
        }
        if config.ipv6.enabled {
            records.push(
                self.sync_record(config, &config.ipv6, "AAAA", notification_sender.clone())
                    .await,
            );
        }
        Ok(records)
    }

    async fn sync_record(
        &self,
        config: &DdnsConfig,
        ip_config: &DdnsIpConfig,
        record_type: &str,
        notification_sender: Arc<NotificationSender>,
    ) -> DdnsRecordSyncResult {
        let domains = normalize_domains(&ip_config.domains);
        if domains.is_empty() {
            let result = DdnsRecordSyncResult {
                record_type: record_type.to_string(),
                domains,
                status: "skipped".to_string(),
                message: format!("{record_type} has no domains configured"),
                ..DdnsRecordSyncResult::default()
            };
            self.push_log("info", record_type, result.domains.clone(), &result.message)
                .await;
            return result;
        }

        let ip = match self.get_ip(ip_config, record_type).await {
            Ok(ip) => ip,
            Err(err) => {
                let result = DdnsRecordSyncResult {
                    record_type: record_type.to_string(),
                    domains: domains.clone(),
                    status: "failed".to_string(),
                    message: format!("Failed to get {record_type} address: {err}"),
                    ..DdnsRecordSyncResult::default()
                };
                self.push_log("error", record_type, domains.clone(), &result.message)
                    .await;
                let failure_count = self
                    .update_failure_count(config, record_type, &domains, true)
                    .await;
                self.notify(config, &result, failure_count, notification_sender)
                    .await;
                return result;
            }
        };

        let update = self
            .update_provider_records(config, record_type, &domains, &ip)
            .await;
        let result = match update {
            Ok(summary) => {
                let status = if summary.changed {
                    "updated"
                } else {
                    "unchanged"
                };
                DdnsRecordSyncResult {
                    record_type: record_type.to_string(),
                    domains: domains.clone(),
                    old_ip: summary.old_ip,
                    new_ip: Some(ip),
                    status: status.to_string(),
                    message: summary.message,
                }
            }
            Err(err) => DdnsRecordSyncResult {
                record_type: record_type.to_string(),
                domains: domains.clone(),
                new_ip: Some(ip),
                status: "failed".to_string(),
                message: err,
                ..DdnsRecordSyncResult::default()
            },
        };

        let log_level = if result.status == "failed" {
            "error"
        } else {
            "info"
        };
        self.push_log(log_level, record_type, domains, &result.message)
            .await;
        let failure_count = self
            .update_failure_count(
                config,
                record_type,
                &result.domains,
                result.status == "failed",
            )
            .await;
        if matches!(result.status.as_str(), "updated" | "failed") {
            self.notify(config, &result, failure_count, notification_sender)
                .await;
        }
        result
    }

    async fn update_failure_count(
        &self,
        config: &DdnsConfig,
        record_type: &str,
        domains: &[String],
        failed: bool,
    ) -> u32 {
        let key = ddns_failure_key(&config.provider, record_type, domains);
        let mut state = self.state.lock().await;
        if failed {
            let count = state.failure_counts.entry(key).or_insert(0);
            *count = count.saturating_add(1);
            *count
        } else {
            state.failure_counts.remove(&key);
            0
        }
    }

    async fn notify(
        &self,
        config: &DdnsConfig,
        result: &DdnsRecordSyncResult,
        failure_count: u32,
        notification_sender: Arc<NotificationSender>,
    ) {
        let event = DdnsEvent {
            provider: config.provider.clone(),
            record_type: result.record_type.clone(),
            domains: result.domains.clone(),
            old_ip: result.old_ip.clone(),
            new_ip: result.new_ip.clone(),
            status: result.status.clone(),
            message: result.message.clone(),
            timestamp: now_string(),
            failure_count,
        };
        if notification_sender.ddns_event_blocked_by_failure_threshold(&event) {
            return;
        }
        if let Err(err) = notification_sender.forward_ddns_event(&event).await {
            self.push_log(
                "warn",
                &result.record_type,
                result.domains.clone(),
                &format!("DDNS notification failed: {err}"),
            )
            .await;
        }
    }

    async fn push_log(&self, level: &str, record_type: &str, domains: Vec<String>, message: &str) {
        let mut state = self.state.lock().await;
        if state.logs.len() >= DDNS_LOG_LIMIT {
            state.logs.pop_front();
        }
        state.logs.push_back(DdnsLogEntry {
            timestamp: beijing_time_string(),
            level: level.to_string(),
            record_type: record_type.to_string(),
            domains,
            message: message.to_string(),
        });
    }

    async fn get_ip(&self, ip_config: &DdnsIpConfig, record_type: &str) -> Result<String, String> {
        match ip_config.get_type.as_str() {
            "interface" => get_ip_from_interface(ip_config, record_type).await,
            "api" | "" => self.get_ip_from_api(ip_config, record_type).await,
            other => Err(format!("unsupported IP source: {other}")),
        }
    }

    async fn get_ip_from_api(
        &self,
        ip_config: &DdnsIpConfig,
        record_type: &str,
    ) -> Result<String, String> {
        let urls = if ip_config.urls.is_empty() {
            default_urls_for_record(record_type)
        } else {
            ip_config.urls.clone()
        };
        let mut last_error = String::new();
        for url in urls {
            match self.client.get(url.trim()).send().await {
                Ok(response) => match response.text().await {
                    Ok(body) => {
                        if let Some(ip) = extract_ip_from_text(&body, record_type) {
                            return Ok(ip);
                        }
                        last_error = "response did not contain a valid IP address".to_string();
                    }
                    Err(err) => last_error = err.to_string(),
                },
                Err(err) => last_error = err.to_string(),
            }
        }
        Err(last_error)
    }

    async fn update_provider_records(
        &self,
        config: &DdnsConfig,
        record_type: &str,
        domains: &[String],
        ip: &str,
    ) -> Result<ProviderUpdateSummary, String> {
        match config.provider.as_str() {
            "cloudflare" => update_cloudflare(&self.client, config, record_type, domains, ip).await,
            "alidns" => update_alidns(&self.client, config, record_type, domains, ip).await,
            "tencentcloud" | "dnspod" | "tencent" => {
                update_tencentcloud(&self.client, config, record_type, domains, ip).await
            }
            other => Err(format!("unsupported DDNS provider: {other}")),
        }
    }
}

impl Default for DdnsManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default)]
struct ProviderUpdateSummary {
    old_ip: Option<String>,
    changed: bool,
    message: String,
}

fn normalize_domains(domains: &[String]) -> Vec<String> {
    domains
        .iter()
        .flat_map(|item| item.lines())
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn ddns_failure_key(provider: &str, record_type: &str, domains: &[String]) -> String {
    format!("{}|{}|{}", provider, record_type, domains.join(","))
}

fn default_urls_for_record(record_type: &str) -> Vec<String> {
    if record_type == "AAAA" {
        vec![
            "https://api6.ipify.org".to_string(),
            "https://speed.neu6.edu.cn/getIP.php".to_string(),
            "https://v6.ident.me".to_string(),
            "https://myip6.ipip.net".to_string(),
            "https://6.ipw.cn".to_string(),
        ]
    } else {
        vec![
            "https://api.ipify.org".to_string(),
            "https://ip.3322.net".to_string(),
            "https://4.ident.me".to_string(),
            "https://ddns.oray.com/checkip".to_string(),
            "https://4.ipw.cn".to_string(),
        ]
    }
}

fn extract_ip_from_text(text: &str, record_type: &str) -> Option<String> {
    for token in
        text.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ';' | '<' | '>'))
    {
        let candidate =
            token.trim_matches(|c: char| !c.is_ascii_hexdigit() && c != '.' && c != ':');
        if let Ok(ip) = candidate.parse::<IpAddr>() {
            match (record_type, ip) {
                ("A", IpAddr::V4(_)) => return Some(candidate.to_string()),
                ("AAAA", IpAddr::V6(_)) => return Some(candidate.to_string()),
                _ => {}
            }
        }
    }
    None
}

async fn get_ip_from_interface(
    ip_config: &DdnsIpConfig,
    record_type: &str,
) -> Result<String, String> {
    let interfaces = read_network_interfaces(None).await?;
    let family = NetworkAddressFamily::from_ddns_record_type(record_type)
        .ok_or_else(|| format!("unsupported DDNS record type: {record_type}"))?;
    let iface = if ip_config.interface_name.trim().is_empty() {
        preferred_interface_for_family(&interfaces, family)
    } else {
        interfaces.iter().find(|iface| {
            iface.name == ip_config.interface_name
                && !interface_addresses_for_family(&iface.ip_addresses, family).is_empty()
        })
    }
    .ok_or_else(|| "no matching network interface found".to_string())?;

    interface_addresses_for_family(&iface.ip_addresses, family)
        .first()
        .map(|addr| addr.address.clone())
        .ok_or_else(|| format!("no {record_type} address found on {}", iface.name))
}

async fn update_cloudflare(
    client: &Client,
    config: &DdnsConfig,
    record_type: &str,
    domains: &[String],
    ip: &str,
) -> Result<ProviderUpdateSummary, String> {
    let zone_id = config.access_id.trim();
    let token = config.access_secret.trim();
    if zone_id.is_empty() || token.is_empty() {
        return Err("Cloudflare Zone ID or API Token is not configured".to_string());
    }

    let mut changed = false;
    let mut old_ip = None;
    for domain in domains {
        let list_url = format!(
            "https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records?type={record_type}&name={domain}"
        );
        let listed = client
            .get(&list_url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|err| format!("Cloudflare list request failed: {err}"))?
            .json::<JsonValue>()
            .await
            .map_err(|err| format!("Cloudflare list response parse failed: {err}"))?;
        ensure_cloudflare_success(&listed)?;
        let first = listed
            .get("result")
            .and_then(JsonValue::as_array)
            .and_then(|items| items.first())
            .cloned();
        let current = first
            .as_ref()
            .and_then(|record| record.get("content"))
            .and_then(JsonValue::as_str)
            .map(ToString::to_string);
        if old_ip.is_none() {
            old_ip = current.clone();
        }
        if current.as_deref() == Some(ip) {
            continue;
        }

        let payload = json!({
            "type": record_type,
            "name": domain,
            "content": ip,
            "ttl": config.ttl,
            "proxied": false,
        });
        let response = if let Some(record) = first {
            let record_id = record
                .get("id")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| "Cloudflare record id missing".to_string())?;
            client
                .put(format!(
                    "https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records/{record_id}"
                ))
                .bearer_auth(token)
                .json(&payload)
                .send()
                .await
        } else {
            client
                .post(format!(
                    "https://api.cloudflare.com/client/v4/zones/{zone_id}/dns_records"
                ))
                .bearer_auth(token)
                .json(&payload)
                .send()
                .await
        }
        .map_err(|err| format!("Cloudflare update request failed: {err}"))?
        .json::<JsonValue>()
        .await
        .map_err(|err| format!("Cloudflare update response parse failed: {err}"))?;
        ensure_cloudflare_success(&response)?;
        changed = true;
    }

    Ok(ProviderUpdateSummary {
        old_ip,
        changed,
        message: if changed {
            format!("Cloudflare {record_type} records updated to {ip}")
        } else {
            format!("Cloudflare {record_type} records unchanged")
        },
    })
}

fn ensure_cloudflare_success(value: &JsonValue) -> Result<(), String> {
    if value
        .get("success")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false)
    {
        return Ok(());
    }
    Err(format!(
        "Cloudflare returned error: {}",
        value
            .get("errors")
            .cloned()
            .unwrap_or_else(|| json!("unknown error"))
    ))
}

async fn update_alidns(
    client: &Client,
    config: &DdnsConfig,
    record_type: &str,
    domains: &[String],
    ip: &str,
) -> Result<ProviderUpdateSummary, String> {
    if config.access_id.trim().is_empty() || config.access_secret.trim().is_empty() {
        return Err("AliDNS AccessKey ID or Secret is not configured".to_string());
    }

    let mut changed = false;
    let mut old_ip = None;
    for fqdn in domains {
        let parsed = split_domain(fqdn)?;
        let mut describe = BTreeMap::new();
        describe.insert("DomainName".to_string(), parsed.domain.clone());
        describe.insert("RRKeyWord".to_string(), parsed.sub_domain.clone());
        describe.insert("Type".to_string(), record_type.to_string());
        let listed = alidns_request(client, config, "DescribeDomainRecords", describe).await?;
        let records = listed
            .get("DomainRecords")
            .and_then(|v| v.get("Record"))
            .and_then(JsonValue::as_array)
            .cloned()
            .unwrap_or_default();
        let current = records.iter().find(|record| {
            record.get("RR").and_then(JsonValue::as_str) == Some(parsed.sub_domain.as_str())
                && record.get("Type").and_then(JsonValue::as_str) == Some(record_type)
        });
        let current_ip = current
            .and_then(|record| record.get("Value"))
            .and_then(JsonValue::as_str)
            .map(ToString::to_string);
        if old_ip.is_none() {
            old_ip = current_ip.clone();
        }
        if current_ip.as_deref() == Some(ip) {
            continue;
        }

        let mut params = BTreeMap::new();
        params.insert("RR".to_string(), parsed.sub_domain.clone());
        params.insert("Type".to_string(), record_type.to_string());
        params.insert("Value".to_string(), ip.to_string());
        params.insert("TTL".to_string(), config.ttl.to_string());
        if let Some(record) = current {
            let record_id = record
                .get("RecordId")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| "AliDNS RecordId missing".to_string())?;
            params.insert("RecordId".to_string(), record_id.to_string());
            alidns_request(client, config, "UpdateDomainRecord", params).await?;
        } else {
            params.insert("DomainName".to_string(), parsed.domain.clone());
            alidns_request(client, config, "AddDomainRecord", params).await?;
        }
        changed = true;
    }

    Ok(ProviderUpdateSummary {
        old_ip,
        changed,
        message: if changed {
            format!("AliDNS {record_type} records updated to {ip}")
        } else {
            format!("AliDNS {record_type} records unchanged")
        },
    })
}

async fn alidns_request(
    client: &Client,
    config: &DdnsConfig,
    action: &str,
    mut params: BTreeMap<String, String>,
) -> Result<JsonValue, String> {
    params.insert("Action".to_string(), action.to_string());
    params.insert("Version".to_string(), "2015-01-09".to_string());
    params.insert("Format".to_string(), "JSON".to_string());
    params.insert(
        "AccessKeyId".to_string(),
        config.access_id.trim().to_string(),
    );
    params.insert("SignatureMethod".to_string(), "HMAC-SHA1".to_string());
    params.insert("SignatureVersion".to_string(), "1.0".to_string());
    params.insert("SignatureNonce".to_string(), nonce());
    params.insert(
        "Timestamp".to_string(),
        Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
    );

    let canonical = canonical_query(&params, ali_percent_encode);
    let string_to_sign = format!("GET&%2F&{}", ali_percent_encode(&canonical));
    let key = format!("{}&", config.access_secret.trim());
    let signature = hmac_base64(
        hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY,
        key.as_bytes(),
        string_to_sign.as_bytes(),
    );
    let url = format!(
        "https://alidns.aliyuncs.com/?{}&Signature={}",
        canonical,
        ali_percent_encode(&signature)
    );
    let value = client
        .get(url)
        .send()
        .await
        .map_err(|err| format!("AliDNS request failed: {err}"))?
        .json::<JsonValue>()
        .await
        .map_err(|err| format!("AliDNS response parse failed: {err}"))?;
    if let Some(message) = value.get("Message").and_then(JsonValue::as_str) {
        return Err(format!("AliDNS {action} failed: {message}"));
    }
    Ok(value)
}

async fn update_tencentcloud(
    client: &Client,
    config: &DdnsConfig,
    record_type: &str,
    domains: &[String],
    ip: &str,
) -> Result<ProviderUpdateSummary, String> {
    if config.access_id.trim().is_empty() || config.access_secret.trim().is_empty() {
        return Err("Tencent Cloud/DNSPod ID or Token is not configured".to_string());
    }
    if !config.access_id.trim().starts_with("AKID") {
        return update_dnspod_token(client, config, record_type, domains, ip).await;
    }

    let mut changed = false;
    let mut old_ip = None;
    for fqdn in domains {
        let parsed = split_domain(fqdn)?;
        let listed = tencentcloud_request(
            client,
            config,
            "DescribeRecordList",
            json!({
                "Domain": parsed.domain,
                "Subdomain": parsed.sub_domain,
                "RecordType": record_type,
            }),
        )
        .await?;
        let records = listed
            .get("Response")
            .and_then(|v| v.get("RecordList"))
            .and_then(JsonValue::as_array)
            .cloned()
            .unwrap_or_default();
        let current = records.first();
        let current_ip = current
            .and_then(|record| record.get("Value"))
            .and_then(JsonValue::as_str)
            .map(ToString::to_string);
        if old_ip.is_none() {
            old_ip = current_ip.clone();
        }
        if current_ip.as_deref() == Some(ip) {
            continue;
        }

        if let Some(record) = current {
            let record_id = record
                .get("RecordId")
                .or_else(|| record.get("Id"))
                .and_then(JsonValue::as_u64)
                .ok_or_else(|| "Tencent Cloud RecordId missing".to_string())?;
            tencentcloud_request(
                client,
                config,
                "ModifyRecord",
                json!({
                    "Domain": parsed.domain,
                    "SubDomain": parsed.sub_domain,
                    "RecordType": record_type,
                    "RecordLine": "默认",
                    "Value": ip,
                    "RecordId": record_id,
                    "TTL": config.ttl,
                }),
            )
            .await?;
        } else {
            tencentcloud_request(
                client,
                config,
                "CreateRecord",
                json!({
                    "Domain": parsed.domain,
                    "SubDomain": parsed.sub_domain,
                    "RecordType": record_type,
                    "RecordLine": "默认",
                    "Value": ip,
                    "TTL": config.ttl,
                }),
            )
            .await?;
        }
        changed = true;
    }

    Ok(ProviderUpdateSummary {
        old_ip,
        changed,
        message: if changed {
            format!("Tencent Cloud {record_type} records updated to {ip}")
        } else {
            format!("Tencent Cloud {record_type} records unchanged")
        },
    })
}

async fn update_dnspod_token(
    client: &Client,
    config: &DdnsConfig,
    record_type: &str,
    domains: &[String],
    ip: &str,
) -> Result<ProviderUpdateSummary, String> {
    let mut changed = false;
    let mut old_ip = None;
    for fqdn in domains {
        let parsed = split_domain(fqdn)?;
        let mut list_params = BTreeMap::new();
        list_params.insert("domain".to_string(), parsed.domain.clone());
        list_params.insert("sub_domain".to_string(), parsed.sub_domain.clone());
        list_params.insert("record_type".to_string(), record_type.to_string());
        let listed = dnspod_token_request(client, config, "Record.List", list_params).await?;
        let records = listed
            .get("records")
            .and_then(JsonValue::as_array)
            .cloned()
            .unwrap_or_default();
        let current = records.iter().find(|record| {
            record.get("type").and_then(JsonValue::as_str) == Some(record_type)
                && record.get("name").and_then(JsonValue::as_str)
                    == Some(parsed.sub_domain.as_str())
        });
        let current_ip = current
            .and_then(|record| record.get("value"))
            .and_then(JsonValue::as_str)
            .map(ToString::to_string);
        if old_ip.is_none() {
            old_ip = current_ip.clone();
        }
        if current_ip.as_deref() == Some(ip) {
            continue;
        }

        let mut params = BTreeMap::new();
        params.insert("domain".to_string(), parsed.domain.clone());
        params.insert("sub_domain".to_string(), parsed.sub_domain.clone());
        params.insert("record_type".to_string(), record_type.to_string());
        params.insert("record_line".to_string(), "默认".to_string());
        params.insert("value".to_string(), ip.to_string());
        params.insert("ttl".to_string(), config.ttl.to_string());
        if let Some(record) = current {
            let record_id = record
                .get("id")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| "DNSPod RecordId missing".to_string())?;
            params.insert("record_id".to_string(), record_id.to_string());
            dnspod_token_request(client, config, "Record.Modify", params).await?;
        } else {
            dnspod_token_request(client, config, "Record.Create", params).await?;
        }
        changed = true;
    }

    Ok(ProviderUpdateSummary {
        old_ip,
        changed,
        message: if changed {
            format!("Tencent Cloud {record_type} records updated to {ip}")
        } else {
            format!("Tencent Cloud {record_type} records unchanged")
        },
    })
}

async fn dnspod_token_request(
    client: &Client,
    config: &DdnsConfig,
    action: &str,
    mut params: BTreeMap<String, String>,
) -> Result<JsonValue, String> {
    params.insert(
        "login_token".to_string(),
        format!(
            "{},{}",
            config.access_id.trim(),
            config.access_secret.trim()
        ),
    );
    params.insert("format".to_string(), "json".to_string());
    let value = client
        .post(format!("https://dnsapi.cn/{action}"))
        .form(&params)
        .send()
        .await
        .map_err(|err| format!("DNSPod {action} request failed: {err}"))?
        .json::<JsonValue>()
        .await
        .map_err(|err| format!("DNSPod {action} response parse failed: {err}"))?;
    let status = value.get("status").cloned().unwrap_or_else(|| json!({}));
    let code = status
        .get("code")
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    if code != "1" {
        let message = status
            .get("message")
            .and_then(JsonValue::as_str)
            .unwrap_or("unknown error");
        return Err(format!("DNSPod {action} failed: {message}"));
    }
    Ok(value)
}

async fn tencentcloud_request(
    client: &Client,
    config: &DdnsConfig,
    action: &str,
    payload: JsonValue,
) -> Result<JsonValue, String> {
    const HOST: &str = "dnspod.tencentcloudapi.com";
    const SERVICE: &str = "dnspod";
    const VERSION: &str = "2021-03-23";
    let timestamp = current_timestamp_secs();
    let date = chrono::DateTime::<Utc>::from_timestamp(timestamp as i64, 0)
        .unwrap_or_else(Utc::now)
        .format("%Y-%m-%d")
        .to_string();
    let payload_text = payload.to_string();
    let hashed_payload = sha256_hex(payload_text.as_bytes());
    let canonical_request = format!(
        "POST\n/\n\ncontent-type:application/json; charset=utf-8\nhost:{HOST}\n\ncontent-type;host\n{hashed_payload}"
    );
    let credential_scope = format!("{date}/{SERVICE}/tc3_request");
    let string_to_sign = format!(
        "TC3-HMAC-SHA256\n{timestamp}\n{credential_scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let secret_date = hmac_sha256(
        format!("TC3{}", config.access_secret.trim()).as_bytes(),
        date.as_bytes(),
    );
    let secret_service = hmac_sha256(&secret_date, SERVICE.as_bytes());
    let secret_signing = hmac_sha256(&secret_service, b"tc3_request");
    let signature = hex_lower(&hmac_sha256(&secret_signing, string_to_sign.as_bytes()));
    let authorization = format!(
        "TC3-HMAC-SHA256 Credential={}/{}, SignedHeaders=content-type;host, Signature={}",
        config.access_id.trim(),
        credential_scope,
        signature
    );

    let value = client
        .post(format!("https://{HOST}"))
        .header("Authorization", authorization)
        .header("Content-Type", "application/json; charset=utf-8")
        .header("Host", HOST)
        .header("X-TC-Action", action)
        .header("X-TC-Timestamp", timestamp.to_string())
        .header("X-TC-Version", VERSION)
        .json(&payload)
        .send()
        .await
        .map_err(|err| format!("Tencent Cloud request failed: {err}"))?
        .json::<JsonValue>()
        .await
        .map_err(|err| format!("Tencent Cloud response parse failed: {err}"))?;
    if let Some(error) = value.get("Response").and_then(|v| v.get("Error")) {
        return Err(format!("Tencent Cloud {action} failed: {error}"));
    }
    Ok(value)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedDomain {
    domain: String,
    sub_domain: String,
}

fn split_domain(fqdn: &str) -> Result<ParsedDomain, String> {
    let trimmed = fqdn.trim().trim_end_matches('.');
    let labels: Vec<&str> = trimmed.split('.').filter(|s| !s.is_empty()).collect();
    if labels.len() < 2 {
        return Err(format!("invalid domain: {fqdn}"));
    }
    let public_suffix_len = if labels.len() >= 3
        && matches!(
            format!("{}.{}", labels[labels.len() - 2], labels[labels.len() - 1]).as_str(),
            "com.cn" | "net.cn" | "org.cn" | "gov.cn" | "co.uk" | "org.uk" | "com.au"
        ) {
        2
    } else {
        1
    };
    let root_start = labels.len().saturating_sub(public_suffix_len + 1);
    let domain = labels[root_start..].join(".");
    let sub_domain = if root_start == 0 {
        "@".to_string()
    } else {
        labels[..root_start].join(".")
    };
    Ok(ParsedDomain { domain, sub_domain })
}

fn canonical_query(params: &BTreeMap<String, String>, encoder: fn(&str) -> String) -> String {
    params
        .iter()
        .map(|(key, value)| format!("{}={}", encoder(key), encoder(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn ali_percent_encode(input: &str) -> String {
    let mut out = String::new();
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn hmac_base64(algorithm: hmac::Algorithm, key: &[u8], data: &[u8]) -> String {
    let key = hmac::Key::new(algorithm, key);
    let tag = hmac::sign(&key, data);
    general_purpose::STANDARD.encode(tag.as_ref())
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let key = hmac::Key::new(hmac::HMAC_SHA256, key);
    hmac::sign(&key, data).as_ref().to_vec()
}

fn sha256_hex(data: &[u8]) -> String {
    hex_lower(digest::digest(&digest::SHA256, data).as_ref())
}

fn hex_lower(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}

fn current_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn nonce() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

fn now_string() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn beijing_time_string() -> String {
    (Utc::now() + ChronoDuration::hours(8))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

// WLAN client management. These handlers use NetworkManager's command-line
// frontend for compatibility with minimal Debian images where nmcli is the
// stable administrative interface already present with NetworkManager.

pub async fn wlan_status() -> Result<WlanStatusResponse, String> {
    let radio = run_nmcli(&["-t", "-f", "WIFI,WIFI-HW", "general", "status"]).await?;
    let fields = split_nmcli_fields(radio.lines().next().unwrap_or_default());
    let enabled = fields.first().map(|v| v == "enabled").unwrap_or(false);
    let hardware_enabled = fields.get(1).map(|v| v == "enabled").unwrap_or(false);
    let devices = run_nmcli(&[
        "-t",
        "-f",
        "DEVICE,TYPE,STATE,CONNECTION",
        "device",
        "status",
    ])
    .await?;
    let wifi = devices.lines().find_map(|line| {
        let fields = split_nmcli_fields(line);
        if fields.get(1).map(String::as_str) == Some("wifi") {
            Some(fields)
        } else {
            None
        }
    });
    let Some(wifi) = wifi else {
        return Ok(WlanStatusResponse {
            available: false,
            enabled,
            hardware_enabled,
            ..WlanStatusResponse::default()
        });
    };
    let iface = wifi.first().cloned().unwrap_or_default();
    let connected = wifi.get(2).map(|v| v == "connected").unwrap_or(false);
    let connection_id = wifi.get(3).filter(|v| !v.is_empty() && *v != "--").cloned();
    let details = run_nmcli(&[
        "-t",
        "-f",
        "GENERAL.CONNECTION,IP4.ADDRESS,IP4.GATEWAY,IP6.ADDRESS",
        "device",
        "show",
        &iface,
    ])
    .await
    .unwrap_or_default();
    let mut ipv4_addresses = Vec::new();
    let mut ipv4_gateway = None;
    let mut ipv6_addresses = Vec::new();
    for line in details.lines() {
        if let Some(value) = line.strip_prefix("IP4.ADDRESS") {
            if let Some((_, address)) = value.split_once(':') {
                ipv4_addresses.push(unescape_nmcli(address));
            }
        } else if let Some(value) = line.strip_prefix("IP4.GATEWAY:") {
            let gateway = unescape_nmcli(value);
            if !gateway.is_empty() {
                ipv4_gateway = Some(gateway);
            }
        } else if let Some(value) = line.strip_prefix("IP6.ADDRESS") {
            if let Some((_, address)) = value.split_once(':') {
                ipv6_addresses.push(unescape_nmcli(address));
            }
        }
    }

    Ok(WlanStatusResponse {
        available: true,
        enabled,
        hardware_enabled,
        interface_name: Some(iface),
        connected,
        ssid: connection_id.clone(),
        connection_id,
        ipv4_addresses,
        ipv4_gateway,
        ipv6_addresses,
    })
}

pub async fn wlan_set_enabled(req: WlanEnabledRequest) -> Result<WlanStatusResponse, String> {
    run_nmcli(&["radio", "wifi", if req.enabled { "on" } else { "off" }]).await?;
    wlan_status().await
}

pub async fn wlan_scan() -> Result<WlanScanResponse, String> {
    let output = run_nmcli(&[
        "-t",
        "-f",
        "SSID,BSSID,SIGNAL,SECURITY,ACTIVE",
        "device",
        "wifi",
        "list",
        "--rescan",
        "yes",
    ])
    .await?;
    let mut networks = Vec::new();
    for line in output.lines() {
        let fields = split_nmcli_fields(line);
        let ssid = fields.first().cloned().unwrap_or_default();
        if ssid.is_empty() {
            continue;
        }
        let security = fields.get(3).cloned().unwrap_or_default();
        networks.push(WlanNetwork {
            ssid,
            bssid: fields.get(1).cloned().unwrap_or_default(),
            signal: fields
                .get(2)
                .and_then(|v| v.parse::<u8>().ok())
                .unwrap_or_default(),
            secure: !security.trim().is_empty() && security.trim() != "--",
            security,
            connected: fields.get(4).map(|v| v == "yes").unwrap_or(false),
        });
    }
    Ok(WlanScanResponse { networks })
}

pub async fn wlan_profiles() -> Result<WlanProfilesResponse, String> {
    let output = run_nmcli(&[
        "-t",
        "-f",
        "NAME,UUID,TYPE,DEVICE,AUTOCONNECT",
        "connection",
        "show",
    ])
    .await?;
    let mut profiles = Vec::new();
    for line in output.lines() {
        let fields = split_nmcli_fields(line);
        let connection_type = fields.get(2).map(String::as_str).unwrap_or_default();
        if connection_type != "wifi" && connection_type != "802-11-wireless" {
            continue;
        }
        let id = fields.first().cloned().unwrap_or_default();
        let uuid = fields.get(1).cloned().unwrap_or_default();
        if id.is_empty() || uuid.is_empty() {
            continue;
        }
        let device = fields.get(3).cloned().unwrap_or_default();
        let ssid = wlan_profile_ssid(&uuid).await.unwrap_or_else(|| id.clone());
        profiles.push(WlanSavedNetwork {
            id,
            uuid,
            ssid,
            interface_name: if device.is_empty() || device == "--" {
                None
            } else {
                Some(device)
            },
            active: fields
                .get(3)
                .map(|v| !v.is_empty() && v != "--")
                .unwrap_or(false),
            auto_join: fields.get(4).map(|v| v == "yes").unwrap_or(false),
        });
    }
    profiles.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then_with(|| b.auto_join.cmp(&a.auto_join))
            .then_with(|| a.ssid.cmp(&b.ssid))
    });
    Ok(WlanProfilesResponse { profiles })
}

pub async fn wlan_forget(req: WlanForgetRequest) -> Result<WlanProfilesResponse, String> {
    if !req.uuid.trim().is_empty() {
        run_nmcli(&["connection", "delete", "uuid", req.uuid.trim()]).await?;
    } else if !req.connection_id.trim().is_empty() {
        run_nmcli(&["connection", "delete", req.connection_id.trim()]).await?;
    } else {
        return Err("connection id or uuid is required".to_string());
    }
    wlan_profiles().await
}

pub async fn wlan_connect(req: WlanConnectRequest) -> Result<WlanStatusResponse, String> {
    let status = wlan_status().await?;
    let iface = status
        .interface_name
        .ok_or_else(|| "No WLAN interface found".to_string())?;
    let mut args = vec!["device", "wifi", "connect", req.ssid.as_str()];
    if !req.password.is_empty() {
        args.push("password");
        args.push(req.password.as_str());
    }
    args.push("ifname");
    args.push(iface.as_str());
    run_nmcli(&args).await?;
    let connection = wlan_status()
        .await?
        .connection_id
        .unwrap_or_else(|| req.ssid.clone());
    let auto = if req.auto_join { "yes" } else { "no" };
    let _ = run_nmcli(&[
        "connection",
        "modify",
        connection.as_str(),
        "connection.autoconnect",
        auto,
        "ipv4.route-metric",
        WLAN_ROUTE_METRIC,
        "ipv6.route-metric",
        WLAN_ROUTE_METRIC,
    ])
    .await;
    wlan_status().await
}

pub async fn wlan_disconnect() -> Result<WlanStatusResponse, String> {
    let status = wlan_status().await?;
    if let Some(iface) = status.interface_name {
        run_nmcli(&["device", "disconnect", iface.as_str()]).await?;
    }
    wlan_status().await
}

pub async fn wlan_save_profile(req: WlanProfileRequest) -> Result<WlanStatusResponse, String> {
    run_nmcli(&[
        "connection",
        "modify",
        req.connection_id.as_str(),
        "ipv4.route-metric",
        WLAN_ROUTE_METRIC,
        "ipv6.route-metric",
        WLAN_ROUTE_METRIC,
    ])
    .await?;
    if let Some(auto_join) = req.auto_join {
        run_nmcli(&[
            "connection",
            "modify",
            req.connection_id.as_str(),
            "connection.autoconnect",
            if auto_join { "yes" } else { "no" },
        ])
        .await?;
    }
    match req.ipv4_mode.as_deref() {
        Some("dhcp") | Some("auto") => {
            run_nmcli(&[
                "connection",
                "modify",
                req.connection_id.as_str(),
                "ipv4.method",
                "auto",
                "ipv4.addresses",
                "",
                "ipv4.gateway",
                "",
            ])
            .await?;
        }
        Some("manual") => {
            let address = req
                .ipv4_address
                .as_deref()
                .ok_or_else(|| "IPv4 address is required for manual mode".to_string())?;
            let prefix = req.ipv4_prefix.unwrap_or(24);
            let gateway = req
                .ipv4_gateway
                .as_deref()
                .ok_or_else(|| "IPv4 gateway is required for manual mode".to_string())?;
            let cidr = format!("{address}/{prefix}");
            run_nmcli(&[
                "connection",
                "modify",
                req.connection_id.as_str(),
                "ipv4.method",
                "manual",
                "ipv4.addresses",
                cidr.as_str(),
                "ipv4.gateway",
                gateway,
            ])
            .await?;
        }
        _ => {}
    }
    wlan_status().await
}

async fn wlan_profile_ssid(uuid: &str) -> Option<String> {
    let output = run_nmcli(&[
        "-t",
        "-f",
        "802-11-wireless.ssid",
        "connection",
        "show",
        uuid,
    ])
    .await
    .ok()?;
    output.lines().find_map(|line| {
        line.strip_prefix("802-11-wireless.ssid:")
            .map(unescape_nmcli)
            .filter(|value| !value.is_empty())
    })
}

async fn run_nmcli(args: &[&str]) -> Result<String, String> {
    let output = tokio::process::Command::new("nmcli")
        .env("LANG", "C.UTF-8")
        .env("LC_ALL", "C.UTF-8")
        .env("LC_MESSAGES", "C")
        .args(args)
        .output()
        .await
        .map_err(|err| format!("Failed to execute nmcli: {err}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("nmcli exited with {}", output.status)
        } else {
            stderr
        })
    }
}

fn split_nmcli_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for ch in line.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == ':' {
            fields.push(current);
            current = String::new();
        } else {
            current.push(ch);
        }
    }
    fields.push(current);
    fields
}

fn unescape_nmcli(value: &str) -> String {
    split_nmcli_fields(value).join(":")
}

#[cfg(test)]
mod tests {
    use crate::models::IpAddress;

    use super::*;

    #[test]
    fn splits_regular_domain() {
        assert_eq!(
            split_domain("home.example.com").unwrap(),
            ParsedDomain {
                domain: "example.com".to_string(),
                sub_domain: "home".to_string(),
            }
        );
        assert_eq!(
            split_domain("example.com").unwrap(),
            ParsedDomain {
                domain: "example.com".to_string(),
                sub_domain: "@".to_string(),
            }
        );
    }

    #[test]
    fn splits_common_two_part_public_suffix() {
        assert_eq!(
            split_domain("router.example.com.cn").unwrap(),
            ParsedDomain {
                domain: "example.com.cn".to_string(),
                sub_domain: "router".to_string(),
            }
        );
    }

    #[test]
    fn parses_ip_from_api_text() {
        assert_eq!(
            extract_ip_from_text("ip=203.0.113.10", "A").as_deref(),
            Some("203.0.113.10")
        );
        assert_eq!(
            extract_ip_from_text("2001:db8::1\n", "AAAA").as_deref(),
            Some("2001:db8::1")
        );
    }

    #[test]
    fn ddns_ipv6_interface_addresses_match_dashboard_public_scope() {
        let addresses = vec![
            IpAddress {
                address: "fe80::1".to_string(),
                prefix_len: 64,
                ip_type: "ipv6".to_string(),
                scope: "link-local".to_string(),
            },
            IpAddress {
                address: "fd00::1".to_string(),
                prefix_len: 64,
                ip_type: "ipv6".to_string(),
                scope: "private".to_string(),
            },
            IpAddress {
                address: "2408::64".to_string(),
                prefix_len: 64,
                ip_type: "ipv6".to_string(),
                scope: "public".to_string(),
            },
            IpAddress {
                address: "2408::128".to_string(),
                prefix_len: 128,
                ip_type: "ipv6".to_string(),
                scope: "public".to_string(),
            },
        ];

        let candidates = interface_addresses_for_family(&addresses, NetworkAddressFamily::Ipv6);
        let candidate_addresses: Vec<&str> = candidates
            .iter()
            .map(|addr| addr.address.as_str())
            .collect();

        assert_eq!(candidate_addresses, vec!["2408::128", "2408::64"]);
    }

    fn test_interface(
        name: &str,
        is_wireless: bool,
        is_cellular: bool,
        is_default_ipv4: bool,
        is_default_ipv6: bool,
        ip_addresses: Vec<IpAddress>,
    ) -> crate::models::NetworkInterfaceInfo {
        crate::models::NetworkInterfaceInfo {
            name: name.to_string(),
            status: "up".to_string(),
            is_wireless,
            is_cellular,
            is_default_ipv4,
            is_default_ipv6,
            mac_address: None,
            mtu: 1500,
            ip_addresses,
            rx_bytes: 0,
            tx_bytes: 0,
            rx_packets: 0,
            tx_packets: 0,
            rx_errors: 0,
            tx_errors: 0,
        }
    }

    fn ipv4(address: &str) -> IpAddress {
        IpAddress {
            address: address.to_string(),
            prefix_len: 24,
            ip_type: "ipv4".to_string(),
            scope: "private".to_string(),
        }
    }

    fn public_ipv6(address: &str) -> IpAddress {
        IpAddress {
            address: address.to_string(),
            prefix_len: 64,
            ip_type: "ipv6".to_string(),
            scope: "public".to_string(),
        }
    }

    #[test]
    fn ddns_interface_selection_prefers_wireless_default_route() {
        let interfaces = vec![
            test_interface("wwan0", false, true, true, true, vec![ipv4("10.11.242.67")]),
            test_interface(
                "wlp1s0",
                true,
                false,
                true,
                false,
                vec![ipv4("192.168.1.10")],
            ),
        ];

        let selected =
            preferred_interface_for_family(&interfaces, NetworkAddressFamily::Ipv4).unwrap();

        assert_eq!(selected.name, "wlp1s0");
    }

    #[test]
    fn ddns_interface_selection_uses_default_route_before_bridge_or_loopback() {
        let interfaces = vec![
            test_interface("lo", false, false, false, false, vec![ipv4("127.0.0.1")]),
            test_interface(
                "nm-bridge",
                false,
                false,
                false,
                false,
                vec![ipv4("192.168.68.1")],
            ),
            test_interface("wwan0", false, true, true, true, vec![ipv4("10.11.242.67")]),
        ];

        let selected =
            preferred_interface_for_family(&interfaces, NetworkAddressFamily::Ipv4).unwrap();

        assert_eq!(selected.name, "wwan0");
    }

    #[test]
    fn ddns_interface_selection_uses_ipv6_default_route_per_record_family() {
        let interfaces = vec![
            test_interface(
                "nm-bridge",
                false,
                false,
                false,
                false,
                vec![public_ipv6("2408::68")],
            ),
            test_interface(
                "wwan0",
                false,
                true,
                false,
                true,
                vec![public_ipv6("2408::cell")],
            ),
        ];

        let selected =
            preferred_interface_for_family(&interfaces, NetworkAddressFamily::Ipv6).unwrap();

        assert_eq!(selected.name, "wwan0");
    }

    #[test]
    fn ddns_interface_selection_prefers_wireless_ipv6_over_cellular_default_route() {
        let interfaces = vec![
            test_interface(
                "wwan0",
                false,
                true,
                false,
                true,
                vec![public_ipv6("2408::cell")],
            ),
            test_interface(
                "wlan0",
                true,
                false,
                false,
                false,
                vec![public_ipv6("240e::wifi")],
            ),
        ];

        let selected =
            preferred_interface_for_family(&interfaces, NetworkAddressFamily::Ipv6).unwrap();

        assert_eq!(selected.name, "wlan0");
    }

    #[test]
    fn ddns_interface_selection_excludes_volte_ims_interface() {
        let interfaces = vec![
            test_interface(
                "rmnet_data1",
                false,
                true,
                false,
                true,
                vec![public_ipv6("2409::ims")],
            ),
            test_interface(
                "wwan0",
                false,
                true,
                false,
                false,
                vec![public_ipv6("2408::cell")],
            ),
        ];

        let selected =
            preferred_interface_for_family(&interfaces, NetworkAddressFamily::Ipv6).unwrap();

        assert_eq!(selected.name, "wwan0");
    }

    #[test]
    fn splits_nmcli_escaped_fields() {
        assert_eq!(
            split_nmcli_fields(r"SSID:AA\:BB\:CC:90:WPA2:yes"),
            vec!["SSID", "AA:BB:CC", "90", "WPA2", "yes"]
        );
    }
}
