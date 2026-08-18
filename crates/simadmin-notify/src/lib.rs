use std::{collections::HashMap, time::Duration};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use hmac::{Hmac, Mac};
use lettre::{
    message::{Mailbox, SinglePart},
    transport::smtp::{
        authentication::Credentials,
        client::{Tls, TlsParameters},
    },
    Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use reqwest::{Client, Method, Response};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::Sha256;

const URL_COMPONENT_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    Webhook,
    Bark,
    Pushplus,
    WecomApp,
    WecomRobot,
    DingtalkRobot,
    DingtalkApp,
    FeishuRobot,
    Telegram,
    Email,
    Serverchan3,
}

impl ChannelType {
    pub const ALL: [Self; 11] = [
        Self::Webhook,
        Self::Bark,
        Self::Pushplus,
        Self::WecomApp,
        Self::WecomRobot,
        Self::DingtalkRobot,
        Self::DingtalkApp,
        Self::FeishuRobot,
        Self::Telegram,
        Self::Email,
        Self::Serverchan3,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Webhook => "webhook",
            Self::Bark => "bark",
            Self::Pushplus => "pushplus",
            Self::WecomApp => "wecom_app",
            Self::WecomRobot => "wecom_robot",
            Self::DingtalkRobot => "dingtalk_robot",
            Self::DingtalkApp => "dingtalk_app",
            Self::FeishuRobot => "feishu_robot",
            Self::Telegram => "telegram",
            Self::Email => "email",
            Self::Serverchan3 => "serverchan3",
        }
    }

    pub const fn supported(self) -> bool {
        true
    }

    pub const fn secret_fields(self) -> &'static [&'static str] {
        match self {
            Self::Webhook => &["secret"],
            Self::Bark => &["device_key"],
            Self::Pushplus => &["token"],
            Self::WecomApp => &["secret"],
            Self::WecomRobot => &["key"],
            Self::DingtalkRobot => &["access_token", "secret"],
            Self::DingtalkApp => &["app_secret"],
            Self::FeishuRobot => &["token", "secret"],
            Self::Telegram => &["bot_token"],
            Self::Email => &["password"],
            Self::Serverchan3 => &["send_key"],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebhookConfig {
    pub url: String,
    #[serde(default = "post_method")]
    pub http_method: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BarkConfig {
    #[serde(default = "bark_server")]
    pub server_url: String,
    #[serde(default)]
    pub device_key: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub sound: String,
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub click_url: String,
    #[serde(default)]
    pub copy: String,
    #[serde(default)]
    pub auto_copy: bool,
    #[serde(default = "default_true")]
    pub save_history: bool,
}

impl Default for BarkConfig {
    fn default() -> Self {
        serde_json::from_value(json!({})).expect("Bark defaults are valid")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PushplusConfig {
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub template: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub option: String,
    #[serde(default)]
    pub callback_url: String,
    #[serde(default)]
    pub api_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WecomAppConfig {
    #[serde(default = "wecom_api")]
    pub api_base_url: String,
    #[serde(default)]
    pub corp_id: String,
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub secret: String,
    #[serde(default = "at_all")]
    pub to_user: String,
    #[serde(default)]
    pub to_party: String,
    #[serde(default)]
    pub to_tag: String,
    #[serde(default)]
    pub safe: bool,
}

impl Default for WecomAppConfig {
    fn default() -> Self {
        serde_json::from_value(json!({})).expect("WeCom defaults are valid")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WecomRobotConfig {
    #[serde(default)]
    pub webhook_url: String,
    #[serde(default)]
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DingtalkRobotConfig {
    #[serde(default)]
    pub webhook_url: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub secret: String,
    #[serde(default)]
    pub at_mobiles: String,
    #[serde(default)]
    pub at_all: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DingtalkAppConfig {
    #[serde(default)]
    pub app_key: String,
    #[serde(default)]
    pub app_secret: String,
    #[serde(default)]
    pub robot_code: String,
    #[serde(default)]
    pub open_conversation_id: String,
    #[serde(default)]
    pub msg_key: String,
    #[serde(default)]
    pub api_base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FeishuRobotConfig {
    #[serde(default)]
    pub webhook_url: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    #[serde(default = "telegram_api")]
    pub api_base_url: String,
    #[serde(default)]
    pub bot_token: String,
    #[serde(default)]
    pub chat_id: String,
    #[serde(default)]
    pub parse_mode: String,
    #[serde(default)]
    pub disable_web_page_preview: bool,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        serde_json::from_value(json!({})).expect("Telegram defaults are valid")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmailConfig {
    #[serde(default)]
    pub smtp_host: String,
    #[serde(default = "smtp_port")]
    pub smtp_port: u16,
    #[serde(default = "smtp_security")]
    pub smtp_security: String,
    #[serde(default, alias = "allow_insecure_connections")]
    pub allow_insecure_tls: bool,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub sender_address: String,
    #[serde(default)]
    pub sender_name: String,
    #[serde(default, alias = "receiver_address")]
    pub receiver_addresses: String,
    #[serde(default)]
    pub message_format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Serverchan3Config {
    #[serde(default)]
    pub send_key: String,
    #[serde(default)]
    pub uid: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub openid: String,
    #[serde(default)]
    pub api_url: String,
}

#[derive(Debug, Clone)]
pub struct NotificationMessage {
    pub title: String,
    pub body: String,
    pub custom_body: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DeliveryReceipt {
    pub provider: String,
    pub status_code: u16,
    pub response_summary: String,
}

#[derive(Debug, thiserror::Error)]
pub enum NotifyError {
    #[error("invalid channel configuration: {0}")]
    InvalidConfig(String),
    #[error("notification request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("notification endpoint returned HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("notification provider rejected the request: {0}")]
    Provider(String),
    #[error("email delivery failed: {0}")]
    Email(String),
}

#[derive(Clone)]
pub struct Sender {
    client: Client,
}

impl Sender {
    pub fn new() -> Result<Self, NotifyError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .user_agent("SimAdmin/notification")
            .build()?;
        Ok(Self { client })
    }

    pub async fn send(
        &self,
        channel_type: ChannelType,
        config: &Value,
        message: &NotificationMessage,
    ) -> Result<DeliveryReceipt, NotifyError> {
        validate_config(channel_type, config)?;
        if channel_type != ChannelType::Webhook {
            if let Some(body) = message.custom_body.as_deref() {
                return self.send_custom(channel_type, config, message, body).await;
            }
        }
        match channel_type {
            ChannelType::Webhook => self.send_webhook(parse(config)?, message).await,
            ChannelType::Bark => self.send_bark(parse(config)?, message).await,
            ChannelType::Pushplus => self.send_pushplus(parse(config)?, message).await,
            ChannelType::WecomApp => self.send_wecom_app(parse(config)?, message).await,
            ChannelType::WecomRobot => self.send_wecom_robot(parse(config)?, message).await,
            ChannelType::DingtalkRobot => self.send_dingtalk_robot(parse(config)?, message).await,
            ChannelType::DingtalkApp => self.send_dingtalk_app(parse(config)?, message).await,
            ChannelType::FeishuRobot => self.send_feishu(parse(config)?, message).await,
            ChannelType::Telegram => self.send_telegram(parse(config)?, message).await,
            ChannelType::Email => self.send_email(parse(config)?, message).await,
            ChannelType::Serverchan3 => self.send_serverchan(parse(config)?, message).await,
        }
    }

    async fn send_custom(
        &self,
        channel_type: ChannelType,
        config: &Value,
        message: &NotificationMessage,
        body: &str,
    ) -> Result<DeliveryReceipt, NotifyError> {
        let payload: Value = serde_json::from_str(body)
            .map_err(|error| invalid(format!("custom body must be valid JSON: {error}")))?;
        match channel_type {
            ChannelType::WecomRobot => {
                let config: WecomRobotConfig = parse(config)?;
                let url = robot_url(
                    &config.webhook_url,
                    &config.key,
                    "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=",
                )?;
                self.post_json("wecom_robot", &url, payload, true).await
            }
            ChannelType::DingtalkRobot => {
                let config: DingtalkRobotConfig = parse(config)?;
                let mut url = robot_url(
                    &config.webhook_url,
                    &config.access_token,
                    "https://oapi.dingtalk.com/robot/send?access_token=",
                )?;
                if !config.secret.trim().is_empty() {
                    let timestamp = unix_millis();
                    let to_sign = format!("{timestamp}\n{}", config.secret.trim());
                    let sign = hmac_base64(config.secret.trim(), to_sign.as_bytes())?;
                    url.push_str(&format!("&timestamp={timestamp}&sign={}", encode(&sign)));
                }
                self.post_json("dingtalk_robot", &url, payload, true).await
            }
            ChannelType::FeishuRobot => {
                let config: FeishuRobotConfig = parse(config)?;
                let url = robot_url(
                    &config.webhook_url,
                    &config.token,
                    "https://open.feishu.cn/open-apis/bot/v2/hook/",
                )?;
                let mut payload = payload
                    .as_object()
                    .cloned()
                    .ok_or_else(|| invalid("Feishu custom body must be a JSON object"))?;
                if !config.secret.trim().is_empty() {
                    let timestamp = unix_seconds().to_string();
                    let key = format!("{timestamp}\n{}", config.secret.trim());
                    payload.insert("timestamp".into(), json!(timestamp));
                    payload.insert("sign".into(), json!(hmac_base64(&key, b"")?));
                }
                self.post_json("feishu_robot", &url, Value::Object(payload), true)
                    .await
            }
            ChannelType::Telegram => {
                let config: TelegramConfig = parse(config)?;
                let url = format!(
                    "{}/bot{}/sendMessage",
                    config.api_base_url.trim().trim_end_matches('/'),
                    config.bot_token.trim()
                );
                self.post_json("telegram", &url, payload, true).await
            }
            ChannelType::Bark => {
                let config: BarkConfig = parse(config)?;
                let url = format!("{}/push", config.server_url.trim().trim_end_matches('/'));
                self.post_json("bark", &url, payload, true).await
            }
            _ => {
                let fallback = NotificationMessage {
                    title: message.title.clone(),
                    body: body.to_owned(),
                    custom_body: None,
                };
                match channel_type {
                    ChannelType::Pushplus => self.send_pushplus(parse(config)?, &fallback).await,
                    ChannelType::WecomApp => self.send_wecom_app(parse(config)?, &fallback).await,
                    ChannelType::DingtalkApp => {
                        self.send_dingtalk_app(parse(config)?, &fallback).await
                    }
                    ChannelType::Email => self.send_email(parse(config)?, &fallback).await,
                    ChannelType::Serverchan3 => {
                        self.send_serverchan(parse(config)?, &fallback).await
                    }
                    ChannelType::Webhook
                    | ChannelType::WecomRobot
                    | ChannelType::DingtalkRobot
                    | ChannelType::FeishuRobot
                    | ChannelType::Telegram
                    | ChannelType::Bark => unreachable!(),
                }
            }
        }
    }

    async fn send_webhook(
        &self,
        config: WebhookConfig,
        message: &NotificationMessage,
    ) -> Result<DeliveryReceipt, NotifyError> {
        let method = Method::from_bytes(config.http_method.trim().to_uppercase().as_bytes())
            .map_err(|_| invalid("invalid HTTP method"))?;
        let body = message.custom_body.as_deref().unwrap_or(&message.body);
        let mut request = self.client.request(method.clone(), config.url.trim());
        let mut content_type = false;
        for (name, value) in config.headers {
            content_type |= name.eq_ignore_ascii_case("content-type");
            request = request.header(name, value);
        }
        if method == Method::POST {
            if !content_type {
                request = request.header(
                    "Content-Type",
                    if is_json(body) {
                        "application/json; charset=utf-8"
                    } else {
                        "text/plain; charset=utf-8"
                    },
                );
            }
            request = request.body(body.to_owned());
        }
        if !config.secret.trim().is_empty() {
            request = request.header(
                "X-Webhook-Signature",
                hmac_hex(config.secret.trim(), body.as_bytes())?,
            );
        }
        self.finish(
            "webhook",
            request
                .header("X-SimAdmin-Title", &message.title)
                .send()
                .await?,
            false,
        )
        .await
    }

    async fn send_bark(
        &self,
        config: BarkConfig,
        message: &NotificationMessage,
    ) -> Result<DeliveryReceipt, NotifyError> {
        let url = format!(
            "{}/{}",
            config.server_url.trim().trim_end_matches('/'),
            encode(&config.device_key)
        );
        let mut payload = Map::new();
        payload.insert("title".into(), json!(message.title));
        payload.insert("body".into(), json!(message.body));
        put(&mut payload, "group", &config.group);
        put(&mut payload, "sound", &config.sound);
        put(&mut payload, "level", &config.level);
        put(&mut payload, "icon", &config.icon);
        put(&mut payload, "url", &config.click_url);
        if config.auto_copy {
            payload.insert("automaticallyCopy".into(), json!(1));
            payload.insert(
                "copy".into(),
                json!(if config.copy.trim().is_empty() {
                    &message.body
                } else {
                    &config.copy
                }),
            );
        }
        payload.insert(
            "isArchive".into(),
            json!(if config.save_history { 1 } else { 0 }),
        );
        self.post_json("bark", &url, Value::Object(payload), true)
            .await
    }

    async fn send_pushplus(
        &self,
        config: PushplusConfig,
        message: &NotificationMessage,
    ) -> Result<DeliveryReceipt, NotifyError> {
        let mut payload = Map::new();
        payload.insert("token".into(), json!(config.token.trim()));
        payload.insert("title".into(), json!(message.title));
        payload.insert("content".into(), json!(message.body));
        put(&mut payload, "topic", &config.topic);
        put(&mut payload, "template", &config.template);
        put(&mut payload, "channel", &config.channel);
        put(&mut payload, "option", &config.option);
        put(&mut payload, "callbackUrl", &config.callback_url);
        let url = if config.api_url.trim().is_empty() {
            "https://www.pushplus.plus/send"
        } else {
            config.api_url.trim()
        };
        self.post_json("pushplus", url, Value::Object(payload), true)
            .await
    }

    async fn send_wecom_app(
        &self,
        config: WecomAppConfig,
        message: &NotificationMessage,
    ) -> Result<DeliveryReceipt, NotifyError> {
        let base = config.api_base_url.trim().trim_end_matches('/');
        let token_response = self
            .client
            .get(format!("{base}/cgi-bin/gettoken"))
            .query(&[
                ("corpid", config.corp_id.trim()),
                ("corpsecret", config.secret.trim()),
            ])
            .send()
            .await?;
        let status = token_response.status().as_u16();
        let token_body: Value = token_response.json().await?;
        let token = token_body
            .get("access_token")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                NotifyError::Provider(format!(
                    "WeCom token HTTP {status}: {}",
                    compact(&token_body.to_string(), 240)
                ))
            })?;
        let agent_id = config
            .agent_id
            .trim()
            .parse::<i64>()
            .map_err(|_| invalid("WeCom agent_id must be numeric"))?;
        let payload = json!({
            "touser": if config.to_user.trim().is_empty() { "@all" } else { config.to_user.trim() },
            "toparty": config.to_party.trim(), "totag": config.to_tag.trim(), "msgtype": "text",
            "agentid": agent_id, "text": { "content": message.body }, "safe": if config.safe { 1 } else { 0 }
        });
        let response = self
            .client
            .post(format!("{base}/cgi-bin/message/send"))
            .query(&[("access_token", token)])
            .json(&payload)
            .send()
            .await?;
        self.finish("wecom_app", response, true).await
    }

    async fn send_wecom_robot(
        &self,
        config: WecomRobotConfig,
        message: &NotificationMessage,
    ) -> Result<DeliveryReceipt, NotifyError> {
        let url = robot_url(
            &config.webhook_url,
            &config.key,
            "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=",
        )?;
        self.post_json(
            "wecom_robot",
            &url,
            json!({"msgtype":"text","text":{"content":message.body}}),
            true,
        )
        .await
    }

    async fn send_dingtalk_robot(
        &self,
        config: DingtalkRobotConfig,
        message: &NotificationMessage,
    ) -> Result<DeliveryReceipt, NotifyError> {
        let mut url = robot_url(
            &config.webhook_url,
            &config.access_token,
            "https://oapi.dingtalk.com/robot/send?access_token=",
        )?;
        if !config.secret.trim().is_empty() {
            let timestamp = unix_millis();
            let to_sign = format!("{timestamp}\n{}", config.secret.trim());
            let sign = hmac_base64(config.secret.trim(), to_sign.as_bytes())?;
            url.push_str(&format!("&timestamp={timestamp}&sign={}", encode(&sign)));
        }
        let mobiles = config
            .at_mobiles
            .split([',', ';', '\n'])
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .collect::<Vec<_>>();
        self.post_json("dingtalk_robot", &url, json!({"msgtype":"text","text":{"content":message.body},"at":{"atMobiles":mobiles,"isAtAll":config.at_all}}), true).await
    }

    async fn send_dingtalk_app(
        &self,
        config: DingtalkAppConfig,
        message: &NotificationMessage,
    ) -> Result<DeliveryReceipt, NotifyError> {
        let base = if config.api_base_url.trim().is_empty() {
            "https://api.dingtalk.com"
        } else {
            config.api_base_url.trim().trim_end_matches('/')
        };
        let token_response = self
            .client
            .post(format!("{base}/v1.0/oauth2/accessToken"))
            .json(&json!({"appKey":config.app_key.trim(),"appSecret":config.app_secret.trim()}))
            .send()
            .await?;
        let token_body: Value = token_response.json().await?;
        let token = token_body
            .get("accessToken")
            .and_then(Value::as_str)
            .ok_or_else(|| NotifyError::Provider(compact(&token_body.to_string(), 240)))?;
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
        let response = self.client.post(format!("{base}/v1.0/robot/groupMessages/send"))
            .header("x-acs-dingtalk-access-token", token)
            .json(&json!({"robotCode":robot_code,"openConversationId":config.open_conversation_id.trim(),"msgKey":msg_key,"msgParam":json!({"content":message.body}).to_string()})).send().await?;
        self.finish("dingtalk_app", response, true).await
    }

    async fn send_feishu(
        &self,
        config: FeishuRobotConfig,
        message: &NotificationMessage,
    ) -> Result<DeliveryReceipt, NotifyError> {
        let url = robot_url(
            &config.webhook_url,
            &config.token,
            "https://open.feishu.cn/open-apis/bot/v2/hook/",
        )?;
        let mut payload = json!({"msg_type":"text","content":{"text":message.body}});
        if !config.secret.trim().is_empty() {
            let timestamp = unix_seconds().to_string();
            let key = format!("{timestamp}\n{}", config.secret.trim());
            payload["timestamp"] = json!(timestamp);
            payload["sign"] = json!(hmac_base64(&key, b"")?);
        }
        self.post_json("feishu_robot", &url, payload, true).await
    }

    async fn send_telegram(
        &self,
        config: TelegramConfig,
        message: &NotificationMessage,
    ) -> Result<DeliveryReceipt, NotifyError> {
        let url = format!(
            "{}/bot{}/sendMessage",
            config.api_base_url.trim().trim_end_matches('/'),
            config.bot_token.trim()
        );
        let mut payload = json!({"chat_id":config.chat_id.trim(),"text":message.body,"disable_web_page_preview":config.disable_web_page_preview});
        if !config.parse_mode.trim().is_empty() {
            payload["parse_mode"] = json!(config.parse_mode.trim());
        }
        self.post_json("telegram", &url, payload, true).await
    }

    async fn send_email(
        &self,
        config: EmailConfig,
        message: &NotificationMessage,
    ) -> Result<DeliveryReceipt, NotifyError> {
        let sender = mailbox(&config.sender_address, &config.sender_name, "sender")?;
        let receivers = config
            .receiver_addresses
            .split([',', ';', '\n', '\r'])
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|v| mailbox(v, "", "receiver"))
            .collect::<Result<Vec<_>, _>>()?;
        if receivers.is_empty() {
            return Err(invalid("at least one email receiver is required"));
        }
        let mut builder = Message::builder().from(sender).subject(&message.title);
        for receiver in receivers {
            builder = builder.to(receiver);
        }
        let part = match config.message_format.trim().to_ascii_lowercase().as_str() {
            "" | "plain" | "text" => SinglePart::plain(message.body.clone()),
            "html" => SinglePart::html(message.body.clone()),
            value => return Err(invalid(format!("unsupported email format: {value}"))),
        };
        let email = builder
            .singlepart(part)
            .map_err(|e| NotifyError::Email(e.to_string()))?;
        let tls = match config.smtp_security.trim().to_ascii_lowercase().as_str() {
            "" | "implicit_tls" | "tls" => Tls::Wrapper(tls_parameters(&config)?),
            "starttls" => Tls::Required(tls_parameters(&config)?),
            "none" => Tls::None,
            value => return Err(invalid(format!("unsupported SMTP security: {value}"))),
        };
        let mut transport =
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(config.smtp_host.trim())
                .port(config.smtp_port.max(1))
                .tls(tls);
        if !config.username.trim().is_empty() || !config.password.is_empty() {
            transport = transport.credentials(Credentials::new(
                config.username.trim().to_owned(),
                config.password,
            ));
        }
        transport
            .build()
            .send(email)
            .await
            .map_err(|e| NotifyError::Email(e.to_string()))?;
        Ok(DeliveryReceipt {
            provider: "email".into(),
            status_code: 250,
            response_summary: "SMTP accepted message".into(),
        })
    }

    async fn send_serverchan(
        &self,
        config: Serverchan3Config,
        message: &NotificationMessage,
    ) -> Result<DeliveryReceipt, NotifyError> {
        let uid = if !config.uid.trim().is_empty() {
            config.uid.trim().to_owned()
        } else {
            serverchan_uid(&config.send_key).ok_or_else(|| invalid("ServerChan UID is required"))?
        };
        let url = if config.api_url.trim().is_empty() {
            format!(
                "https://{uid}.push.ft07.com/send/{}.send",
                encode(&config.send_key)
            )
        } else {
            config.api_url.trim().to_owned()
        };
        let mut form = vec![
            ("title", message.title.as_str()),
            ("desp", message.body.as_str()),
        ];
        if !config.channel.trim().is_empty() {
            form.push(("channel", config.channel.trim()));
        }
        if !config.openid.trim().is_empty() {
            form.push(("group", config.openid.trim()));
        }
        self.finish(
            "serverchan3",
            self.client.post(url).form(&form).send().await?,
            true,
        )
        .await
    }

    async fn post_json(
        &self,
        provider: &str,
        url: &str,
        payload: Value,
        inspect_body: bool,
    ) -> Result<DeliveryReceipt, NotifyError> {
        self.finish(
            provider,
            self.client.post(url).json(&payload).send().await?,
            inspect_body,
        )
        .await
    }

    async fn finish(
        &self,
        provider: &str,
        response: Response,
        inspect_body: bool,
    ) -> Result<DeliveryReceipt, NotifyError> {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let summary = compact(&body, 240);
        if !status.is_success() {
            return Err(NotifyError::Http {
                status: status.as_u16(),
                body: summary,
            });
        }
        if inspect_body {
            validate_provider_response(provider, &body)?;
        }
        Ok(DeliveryReceipt {
            provider: provider.into(),
            status_code: status.as_u16(),
            response_summary: summary,
        })
    }
}

pub fn validate_config(channel_type: ChannelType, config: &Value) -> Result<(), NotifyError> {
    match channel_type {
        ChannelType::Webhook => {
            let c: WebhookConfig = parse(config)?;
            required(&c.url, "Webhook URL")?;
            let m = c.http_method.to_uppercase();
            if m != "GET" && m != "POST" {
                return Err(invalid("Webhook only supports GET and POST"));
            }
        }
        ChannelType::Bark => {
            let c: BarkConfig = parse(config)?;
            required(&c.server_url, "Bark server URL")?;
            required(&c.device_key, "Bark device key")?;
        }
        ChannelType::Pushplus => {
            required(&parse::<PushplusConfig>(config)?.token, "PushPlus token")?
        }
        ChannelType::WecomApp => {
            let c: WecomAppConfig = parse(config)?;
            required(&c.corp_id, "WeCom CorpID")?;
            required(&c.agent_id, "WeCom AgentID")?;
            required(&c.secret, "WeCom secret")?;
        }
        ChannelType::WecomRobot => {
            let c: WecomRobotConfig = parse(config)?;
            if c.webhook_url.trim().is_empty() {
                required(&c.key, "WeCom robot key")?;
            }
        }
        ChannelType::DingtalkRobot => {
            let c: DingtalkRobotConfig = parse(config)?;
            if c.webhook_url.trim().is_empty() {
                required(&c.access_token, "DingTalk access token")?;
            }
        }
        ChannelType::DingtalkApp => {
            let c: DingtalkAppConfig = parse(config)?;
            required(&c.app_key, "DingTalk AppKey")?;
            required(&c.app_secret, "DingTalk AppSecret")?;
            required(&c.open_conversation_id, "DingTalk OpenConversationId")?;
        }
        ChannelType::FeishuRobot => {
            let c: FeishuRobotConfig = parse(config)?;
            if c.webhook_url.trim().is_empty() {
                required(&c.token, "Feishu robot token")?;
            }
        }
        ChannelType::Telegram => {
            let c: TelegramConfig = parse(config)?;
            required(&c.bot_token, "Telegram bot token")?;
            required(&c.chat_id, "Telegram chat ID")?;
        }
        ChannelType::Email => {
            let c: EmailConfig = parse(config)?;
            required(&c.smtp_host, "SMTP host")?;
            required(&c.sender_address, "email sender")?;
            required(&c.receiver_addresses, "email receiver")?;
        }
        ChannelType::Serverchan3 => required(
            &parse::<Serverchan3Config>(config)?.send_key,
            "ServerChan SendKey",
        )?,
    }
    Ok(())
}

fn parse<T: DeserializeOwned>(value: &Value) -> Result<T, NotifyError> {
    serde_json::from_value(value.clone()).map_err(|e| invalid(e.to_string()))
}
fn invalid(value: impl Into<String>) -> NotifyError {
    NotifyError::InvalidConfig(value.into())
}
fn required(value: &str, label: &str) -> Result<(), NotifyError> {
    if value.trim().is_empty() {
        Err(invalid(format!("{label} is required")))
    } else {
        Ok(())
    }
}
fn post_method() -> String {
    "POST".into()
}
fn bark_server() -> String {
    "https://api.day.app".into()
}
fn wecom_api() -> String {
    "https://qyapi.weixin.qq.com".into()
}
fn telegram_api() -> String {
    "https://api.telegram.org".into()
}
fn at_all() -> String {
    "@all".into()
}
const fn default_true() -> bool {
    true
}
const fn smtp_port() -> u16 {
    465
}
fn smtp_security() -> String {
    "implicit_tls".into()
}
fn put(map: &mut Map<String, Value>, key: &str, value: &str) {
    if !value.trim().is_empty() {
        map.insert(key.into(), json!(value.trim()));
    }
}
fn encode(value: &str) -> String {
    utf8_percent_encode(value.trim(), URL_COMPONENT_ENCODE_SET).to_string()
}
fn robot_url(url: &str, token: &str, prefix: &str) -> Result<String, NotifyError> {
    if !url.trim().is_empty() {
        Ok(url.trim().into())
    } else if !token.trim().is_empty() {
        Ok(format!("{prefix}{}", encode(token)))
    } else {
        Err(invalid("robot webhook URL or token is required"))
    }
}
fn is_json(value: &str) -> bool {
    serde_json::from_str::<Value>(value).is_ok()
}
fn hmac_hex(secret: &str, body: &[u8]) -> Result<String, NotifyError> {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|e| invalid(e.to_string()))?;
    mac.update(body);
    Ok(hex::encode(mac.finalize().into_bytes()))
}
fn hmac_base64(secret: &str, body: &[u8]) -> Result<String, NotifyError> {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|e| invalid(e.to_string()))?;
    mac.update(body);
    Ok(BASE64.encode(mac.finalize().into_bytes()))
}
fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn unix_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
fn compact(value: &str, max: usize) -> String {
    let v = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if v.chars().count() <= max {
        v
    } else {
        format!("{}...", v.chars().take(max).collect::<String>())
    }
}
fn mailbox(address: &str, name: &str, label: &str) -> Result<Mailbox, NotifyError> {
    let address = address
        .trim()
        .parse::<Address>()
        .map_err(|e| invalid(format!("invalid {label} address: {e}")))?;
    Ok(Mailbox::new(
        (!name.trim().is_empty()).then(|| name.trim().to_owned()),
        address,
    ))
}
fn tls_parameters(config: &EmailConfig) -> Result<TlsParameters, NotifyError> {
    TlsParameters::builder(config.smtp_host.trim().to_owned())
        .dangerous_accept_invalid_certs(config.allow_insecure_tls)
        .dangerous_accept_invalid_hostnames(config.allow_insecure_tls)
        .build()
        .map_err(|e| invalid(e.to_string()))
}
fn serverchan_uid(key: &str) -> Option<String> {
    let rest = key
        .trim()
        .to_ascii_lowercase()
        .strip_prefix("sctp")?
        .to_owned();
    let digits = rest
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    (!digits.is_empty() && rest.get(digits.len()..=digits.len()) == Some("t")).then_some(digits)
}

fn validate_provider_response(provider: &str, body: &str) -> Result<(), NotifyError> {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return Ok(());
    };
    let accepted = match provider {
        "bark" => value
            .get("code")
            .and_then(Value::as_i64)
            .is_none_or(|v| v == 200),
        "pushplus" | "serverchan3" => value
            .get("code")
            .and_then(Value::as_i64)
            .is_none_or(|v| v == 200 || v == 0),
        "wecom_app" | "wecom_robot" | "dingtalk_robot" => value
            .get("errcode")
            .and_then(Value::as_i64)
            .is_none_or(|v| v == 0),
        "feishu_robot" => value
            .get("code")
            .or_else(|| value.get("StatusCode"))
            .and_then(Value::as_i64)
            .is_none_or(|v| v == 0),
        "telegram" => value.get("ok").and_then(Value::as_bool).unwrap_or(true),
        "dingtalk_app" => value
            .get("code")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty),
        _ => true,
    };
    if accepted {
        Ok(())
    } else {
        Err(NotifyError::Provider(compact(body, 240)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Bytes, extract::RawForm, http::HeaderMap, routing::post, Router};
    use std::sync::{Arc, Mutex};

    #[test]
    fn every_channel_is_supported_and_validated() {
        assert!(ChannelType::ALL.iter().all(|channel| channel.supported()));
        for channel in ChannelType::ALL {
            assert!(
                validate_config(channel, &json!({})).is_err(),
                "{} accepted empty config",
                channel.as_str()
            );
        }
    }

    #[tokio::test]
    async fn webhook_sends_body_headers_and_signature() {
        let captured = Arc::new(Mutex::new(None));
        let handler_capture = captured.clone();
        let app = Router::new().route(
            "/notify",
            post(move |headers: HeaderMap, body: Bytes| {
                let c = handler_capture.clone();
                async move {
                    *c.lock().unwrap() = Some((headers, body));
                    "accepted"
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let receipt = Sender::new().unwrap().send(ChannelType::Webhook, &json!({"url":format!("http://{address}/notify"),"headers":{"X-Test":"yes"},"secret":"secret"}), &NotificationMessage { title:"title".into(), body:"body".into(), custom_body:Some("{\"ok\":true}".into()) }).await.unwrap();
        assert_eq!(receipt.status_code, 200);
        let (headers, body) = captured.lock().unwrap().take().unwrap();
        assert_eq!(headers["x-test"], "yes");
        assert_eq!(body, "{\"ok\":true}");
        assert_eq!(
            headers["x-webhook-signature"],
            hmac_hex("secret", b"{\"ok\":true}").unwrap()
        );
    }

    #[tokio::test]
    async fn bark_uses_shared_sender_contract() {
        let captured = Arc::new(Mutex::new(None));
        let c = captured.clone();
        let app = Router::new().route(
            "/device-key",
            post(move |body: Bytes| {
                let c = c.clone();
                async move {
                    *c.lock().unwrap() = Some(body);
                    axum::Json(json!({"code":200}))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Sender::new()
            .unwrap()
            .send(
                ChannelType::Bark,
                &json!({"server_url":format!("http://{address}"),"device_key":"device-key"}),
                &NotificationMessage {
                    title: "title".into(),
                    body: "body".into(),
                    custom_body: None,
                },
            )
            .await
            .unwrap();
        let value: Value =
            serde_json::from_slice(&captured.lock().unwrap().take().unwrap()).unwrap();
        assert_eq!(value["body"], "body");
    }

    #[tokio::test]
    async fn robot_channel_forwards_custom_json_body() {
        let captured = Arc::new(Mutex::new(None));
        let handler_capture = captured.clone();
        let app = Router::new().route(
            "/robot",
            post(move |body: Bytes| {
                let capture = handler_capture.clone();
                async move {
                    *capture.lock().unwrap() = Some(body);
                    axum::Json(json!({"errcode": 0}))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        Sender::new()
            .unwrap()
            .send(
                ChannelType::WecomRobot,
                &json!({"webhook_url": format!("http://{address}/robot")}),
                &NotificationMessage {
                    title: "ignored title".into(),
                    body: "ignored body".into(),
                    custom_body: Some(r#"{"msgtype":"text","text":{"content":"custom"}}"#.into()),
                },
            )
            .await
            .unwrap();

        let value: Value =
            serde_json::from_slice(&captured.lock().unwrap().take().unwrap()).unwrap();
        assert_eq!(value["msgtype"], "text");
        assert_eq!(value["text"]["content"], "custom");
    }

    #[tokio::test]
    async fn serverchan3_maps_message_and_routing_fields_to_form() {
        let captured = Arc::new(Mutex::new(None));
        let handler_capture = captured.clone();
        let app = Router::new().route(
            "/send",
            post(move |RawForm(body): RawForm| {
                let capture = handler_capture.clone();
                async move {
                    *capture.lock().unwrap() = Some(body);
                    axum::Json(json!({"code": 0}))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        Sender::new()
            .unwrap()
            .send(
                ChannelType::Serverchan3,
                &json!({
                    "send_key": "sctp123t-test",
                    "channel": "9",
                    "openid": "group-a",
                    "api_url": format!("http://{address}/send")
                }),
                &NotificationMessage {
                    title: "设备告警".into(),
                    body: "信号过低".into(),
                    custom_body: None,
                },
            )
            .await
            .unwrap();

        let form = String::from_utf8(captured.lock().unwrap().take().unwrap().to_vec()).unwrap();
        assert!(form.contains("title=%E8%AE%BE%E5%A4%87%E5%91%8A%E8%AD%A6"));
        assert!(form.contains("desp=%E4%BF%A1%E5%8F%B7%E8%BF%87%E4%BD%8E"));
        assert!(form.contains("channel=9"));
        assert!(form.contains("group=group-a"));
    }

    #[test]
    fn email_config_supports_legacy_receiver_alias_and_secure_defaults() {
        let value = json!({
            "smtp_host": "smtp.example.com",
            "sender_address": "sender@example.com",
            "receiver_address": "receiver@example.com"
        });

        validate_config(ChannelType::Email, &value).unwrap();
        let config: EmailConfig = parse(&value).unwrap();
        assert_eq!(config.smtp_port, 465);
        assert_eq!(config.smtp_security, "implicit_tls");
        assert_eq!(config.receiver_addresses, "receiver@example.com");
        assert!(validate_config(
            ChannelType::Email,
            &json!({"smtp_host":"smtp.example.com","sender_address":"sender@example.com"})
        )
        .is_err());
    }

    #[test]
    fn serverchan3_extracts_uid_from_send_key() {
        assert_eq!(serverchan_uid("sctp12345t-token").as_deref(), Some("12345"));
        assert_eq!(serverchan_uid("invalid"), None);
    }
}
