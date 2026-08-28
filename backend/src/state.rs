//! 应用状态模块
//! 统一管理应用的共享状态

use axum::extract::FromRef;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use zbus::Connection;

use crate::cell_lock_store::CellLockStore;
use crate::config::ConfigManager;
use crate::db::Database;
use crate::device_network::DdnsManager;
use crate::esim::EsimSupervisor;
use crate::hub_agent::HubAgentManager;
use crate::notification::NotificationSender;
use crate::sms_listener::SmsResyncHandle;
use crate::system_event::SystemEventEmitter;

#[derive(Clone)]
pub struct ActiveCallRecord {
    pub id: i64,
    pub answered_at: Option<Instant>,
    pub answered: bool,
}

/// 应用全局状态
///
/// 统一管理所有共享资源，避免在路由中多次调用 `.with_state()`
#[derive(Clone)]
pub struct AppState {
    /// D-Bus 连接（用于与 ofono 通信）
    pub dbus_conn: Arc<Connection>,
    /// 数据库连接（用于存储 SMS 和通话记录）
    pub database: Arc<Database>,
    /// 配置管理器（用于管理通知等配置）
    pub config_manager: Arc<ConfigManager>,
    /// 通知发送器（用于转发 SMS、通话和 DDNS 通知）
    pub notification_sender: Arc<NotificationSender>,
    pub system_event_emitter: Arc<SystemEventEmitter>,
    pub ddns_manager: Arc<DdnsManager>,
    pub esim_supervisor: Arc<EsimSupervisor>,
    pub sms_resync: SmsResyncHandle,
    pub sms_db_maintenance_pending: Arc<AtomicBool>,
    pub active_calls: Arc<Mutex<HashMap<String, ActiveCallRecord>>>,
    /// 小区锁定 UI 状态（底层无锁网时仅内存态）
    pub cell_lock: Arc<Mutex<CellLockStore>>,
    /// 用户在界面关闭蜂窝数据后，禁止 init/watchdog 自动再次 Connect。
    pub data_user_disabled: Arc<AtomicBool>,
    pub airplane_mode_requested: Arc<AtomicBool>,
    /// 小区/信号轮询是否已按需唤醒。
    pub cell_monitoring_active: Arc<AtomicBool>,
    pub hub_agent_manager: Arc<HubAgentManager>,
}

pub struct AppStateDependencies {
    pub dbus_conn: Arc<Connection>,
    pub database: Arc<Database>,
    pub config_manager: Arc<ConfigManager>,
    pub notification_sender: Arc<NotificationSender>,
    pub system_event_emitter: Arc<SystemEventEmitter>,
    pub ddns_manager: Arc<DdnsManager>,
    pub esim_supervisor: Arc<EsimSupervisor>,
    pub sms_resync: SmsResyncHandle,
    pub data_user_disabled: Arc<AtomicBool>,
    pub airplane_mode_requested: Arc<AtomicBool>,
    pub cell_monitoring_active: Arc<AtomicBool>,
}

impl AppState {
    /// 创建新的应用状态
    pub fn new(dependencies: AppStateDependencies) -> Self {
        let AppStateDependencies {
            dbus_conn,
            database,
            config_manager,
            notification_sender,
            system_event_emitter,
            ddns_manager,
            esim_supervisor,
            sms_resync,
            data_user_disabled,
            airplane_mode_requested,
            cell_monitoring_active,
        } = dependencies;
        Self {
            dbus_conn,
            database,
            config_manager,
            notification_sender,
            system_event_emitter,
            ddns_manager,
            esim_supervisor,
            sms_resync,
            sms_db_maintenance_pending: Arc::new(AtomicBool::new(false)),
            active_calls: Arc::new(Mutex::new(HashMap::new())),
            cell_lock: Arc::new(Mutex::new(CellLockStore::default())),
            data_user_disabled,
            airplane_mode_requested,
            cell_monitoring_active,
            hub_agent_manager: Arc::new(HubAgentManager::new()),
        }
    }
}

// 实现 FromRef trait，允许从 AppState 中提取子状态
// 这样现有的 handler 可以继续使用 State<Arc<Connection>> 等类型

impl FromRef<AppState> for Arc<Connection> {
    fn from_ref(state: &AppState) -> Self {
        state.dbus_conn.clone()
    }
}

impl FromRef<AppState> for Arc<Database> {
    fn from_ref(state: &AppState) -> Self {
        state.database.clone()
    }
}

impl FromRef<AppState> for Arc<ConfigManager> {
    fn from_ref(state: &AppState) -> Self {
        state.config_manager.clone()
    }
}

impl FromRef<AppState> for Arc<NotificationSender> {
    fn from_ref(state: &AppState) -> Self {
        state.notification_sender.clone()
    }
}

impl FromRef<AppState> for Arc<SystemEventEmitter> {
    fn from_ref(state: &AppState) -> Self {
        state.system_event_emitter.clone()
    }
}

impl FromRef<AppState> for Arc<DdnsManager> {
    fn from_ref(state: &AppState) -> Self {
        state.ddns_manager.clone()
    }
}

impl FromRef<AppState> for Arc<EsimSupervisor> {
    fn from_ref(state: &AppState) -> Self {
        state.esim_supervisor.clone()
    }
}

impl FromRef<AppState> for Arc<Mutex<CellLockStore>> {
    fn from_ref(state: &AppState) -> Self {
        state.cell_lock.clone()
    }
}

// 支持 (Arc<Connection>, Arc<Database>) 元组类型
impl FromRef<AppState> for (Arc<Connection>, Arc<Database>) {
    fn from_ref(state: &AppState) -> Self {
        (state.dbus_conn.clone(), state.database.clone())
    }
}
