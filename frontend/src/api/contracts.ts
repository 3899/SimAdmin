export interface ApiResponse<T> {
  status: string
  message: string
  data?: T
}

export interface AuthStatusResponse {
  configured: boolean
  authenticated: boolean
  settings?: SecurityConfig
}

export interface SecurityConfig {
  password_protection_enabled: boolean
  password_min_length: number
  password_require_letters: boolean
  password_require_digits: boolean
  password_require_symbols: boolean
  session_ttl_seconds: number
  idle_timeout_seconds: number
}

export interface AuthSettingsResponse {
  configured: boolean
  settings: SecurityConfig
}

export interface LoginRequest {
  password: string
}

export interface ChangePasswordRequest {
  new_password: string
}

export type WorkMode = 'sim' | 'esim'

export interface WorkModeResponse {
  mode: WorkMode
  worker_running: boolean
}

export interface WorkModeRequest {
  mode: WorkMode
  confirm: boolean
}

export interface EsimCommandResponse {
  code: number
  status: string
  action: string
  msg: string
  data?: unknown
}

export interface EsimEuiccInfo {
  eid: string
  status: string
  manufacturer: string
  memory_total_kb?: number
  memory_available_kb?: number
  memory_total_customizable?: boolean
  updated_at?: string
  raw: unknown
}

export interface EsimConfig {
  lpac_path: string
  custom_memory_total_kb?: number | null
}

export interface EsimProfile {
  iccid: string
  name: string
  provider: string
  state: string
  class: string
  imsi?: string
  msisdn?: string
  smsc?: string
  smdp?: string
  matching_id?: string
  isdp_aid?: string
  mcc?: string
  mnc?: string
  disable_allowed?: boolean
  delete_allowed?: boolean
  updated_at?: string
  raw: unknown
}

export interface EsimProfilesResponse {
  profiles: EsimProfile[]
}

export interface EsimLpacStatusResponse {
  installed: boolean
  usable: boolean
  path: string
  arch: string
  glibc_version: string
  asset_name: string
  message: string
  source?: string
}

export interface EsimLpacRepairRequest {
  proxy_prefix?: string
  asset_url?: string
}

export interface EsimDownloadRequest {
  smdp: string
  matching_id: string
  confirmation_code?: string
  imei?: string
}

export interface EsimLpacRepairResponse {
  installed: boolean
  path: string
  arch: string
  asset_name: string
  asset_url: string
  message: string
}

export interface DeviceInfo {
  imei: string
  manufacturer: string
  model: string
  revision?: string
  online: boolean
  powered: boolean
}

export interface SimInfo {
  present: boolean
  iccid: string
  imsi: string
  phone_numbers: string[]
  sms_center: string
  mcc: string
  mnc: string
  phone_number_is_manual?: boolean
  sms_center_is_manual?: boolean
  sim_path: string
  modem_path: string
  sim_type: string
  esim_status: string
  active: boolean
  operator_name: string
  registered_operator_name: string
  registered_operator_code: string
  lock_status: string
  pin1_retries?: number
  puk1_retries?: number
  pin2_retries?: number
  puk2_retries?: number
  carrier_config: string
  carrier_config_revision: string
  sms_used?: number
  sms_total?: number
}

export interface UpdateSimCacheRequest {
  phone_number?: string
  sms_center?: string
}

export interface NetworkInfo {
  operator_name: string
  registration_status: string
  technology_preference: string
  signal_strength: number
  mcc?: string
  mnc?: string
}

export interface ServingCell {
  tech: string
  cell_id: number
  tac: number
}

export interface CellInfo {
  is_serving: boolean
  tech: string
  cell_id?: number
  band: string
  arfcn: string
  pci: string
  rsrp: string
  rsrq: string
  sinr: string
  earfcn?: string
  nrarfcn?: string
  type?: string
  ssb_rsrp?: string
  ssb_rsrq?: string
  ssb_sinr?: string
}

export interface CellsResponse {
  serving_cell: ServingCell
  cells: CellInfo[]
}

export interface QosInfo {
  qci: number
  dl_speed: number
  ul_speed: number
  raw_response?: string
  /** When set, dl/ul are estimated from WWAN netdev byte counters (not 3GPP QCI/AMBR). */
  source?: 'interface'
}

export interface ThermalZone {
  zone: string
  type: string
  label?: string
  temperature: number
}

export interface DataConnectionStatus {
  active: boolean
}

export interface DataConnectionRequest {
  active: boolean
}

export interface RoamingResponse {
  roaming_allowed: boolean
  is_roaming: boolean
}

export interface RoamingRequest {
  allowed: boolean
}

export interface AirplaneModeRequest {
  enabled: boolean
}

export interface AirplaneModeResponse {
  enabled: boolean
  powered: boolean
  online: boolean
}

export interface BasebandRestartStep {
  step: string
  status: string
  detail?: string
}

export interface BasebandRestartResponse {
  steps: BasebandRestartStep[]
  running: boolean
  current_registration?: string
}

export interface NetworkSpeed {
  interface: string
  rx_bytes_per_sec: number
  tx_bytes_per_sec: number
  total_rx_bytes: number
  total_tx_bytes: number
}

export interface NetworkSpeedResponse {
  interfaces: NetworkSpeed[]
  interval_seconds: number
}

export interface MemoryInfo {
  total_bytes: number
  available_bytes: number
  used_bytes: number
  used_percent: number
  cached_bytes: number
  buffers_bytes: number
}

export interface UptimeInfo {
  uptime_seconds: number
  idle_seconds: number
  uptime_formatted: string
}

export interface SystemInfo {
  sysname: string
  nodename: string
  release: string
  version: string
  machine: string
  domainname?: string
  full_info: string
}

export interface DiskInfo {
  mount_point: string
  fs_type: string
  total_bytes: number
  used_bytes: number
  available_bytes: number
  used_percent: number
}

export interface CpuLoadInfo {
  load_1min: number
  load_5min: number
  load_15min: number
  core_count: number
  load_percent: number
}

export interface SystemStatsResponse {
  network_speed: NetworkSpeedResponse
  memory: MemoryInfo
  disk: DiskInfo[]
  cpu_load: CpuLoadInfo
  uptime: UptimeInfo
  system_info: SystemInfo
  temperature: ThermalZone[]
}

export interface IpAddress {
  address: string
  prefix_len: number
  ip_type: string
  scope: string
}

export interface NetworkInterfaceInfo {
  name: string
  status: string
  is_wireless?: boolean
  is_cellular?: boolean
  is_default_ipv4?: boolean
  is_default_ipv6?: boolean
  mac_address?: string
  mtu: number
  ip_addresses: IpAddress[]
  rx_bytes: number
  tx_bytes: number
  rx_packets: number
  tx_packets: number
  rx_errors: number
  tx_errors: number
}

export interface NetworkInterfacesResponse {
  interfaces: NetworkInterfaceInfo[]
  total_count: number
}

export interface ConnectionAddressesResponse {
  ipv4: string[]
  ipv6: string[]
  ipv4_interface?: string
  ipv6_interface?: string
}

export type RadioMode = 'auto' | 'lte' | 'nr'

export interface RadioModeResponse {
  mode: string
  technology_preference: string
  supported_modes: string[]
}

export interface BandLockStatus {
  locked: boolean
  supported_lte_fdd_bands: number[]
  supported_lte_tdd_bands: number[]
  supported_nr_fdd_bands: number[]
  supported_nr_tdd_bands: number[]
  lte_fdd_bands: number[]
  lte_tdd_bands: number[]
  nr_fdd_bands: number[]
  nr_tdd_bands: number[]
}

export interface BandLockRequest {
  lte_fdd_bands: number[]
  lte_tdd_bands: number[]
  nr_fdd_bands: number[]
  nr_tdd_bands: number[]
}

export interface CellLockRatStatus {
  rat: number
  rat_name: string
  enabled: boolean
  lock_type: number
  pci: number | null
  arfcn: number | null
}

export interface CellLockStatusResponse {
  rat_status?: CellLockRatStatus[]
  any_locked: boolean
}

export interface CellLockRequest {
  rat: number
  enable: boolean
  lock_type?: number
  pci?: number
  arfcn?: number
}

export interface CellLockResult {
  locked?: boolean
  tech?: string
  arfcn?: number
  pci?: number
  success?: boolean
  steps?: string[]
  raw_response?: string
}

export interface SmsMessage {
  id: number
  direction: string
  phone_number: string
  content: string
  timestamp: string
  status: string
  pdu?: string
}

export interface SmsListRequest {
  limit?: number
  offset?: number
  direction?: 'incoming' | 'outgoing'
}

export interface SmsConversationRequest {
  phone_number: string
  limit?: number
}

export interface SmsStats {
  total: number
  incoming: number
  outgoing: number
  pushed?: number
  push_attempted?: number
}

export interface CallInfo {
  path: string
  phone_number: string
  state: string
  direction: string
  start_time?: string
}

export interface CallListResponse {
  calls: CallInfo[]
}

export interface CallRecord {
  id: number
  direction: string
  phone_number: string
  duration: number
  start_time: string
  end_time?: string
  answered: boolean
}

export interface CallStats {
  total: number
  incoming: number
  outgoing: number
  missed: number
  total_duration: number
}

export interface CallHistoryResponse {
  records: CallRecord[]
  stats: CallStats
}

export interface CallSettingsResponse {
  calling_line_presentation: string
  calling_name_presentation: string
  connected_line_presentation: string
  connected_line_restriction: string
  called_line_presentation: string
  calling_line_restriction: string
  hide_caller_id: string
  voice_call_waiting: string
}

export interface SignalStrengthResponse {
  strength: number
}

export interface CellLocationInfo {
  mcc: string
  mnc: string
  lac: number
  cid: number
  signal_strength: number
  radio_type: string
  arfcn?: number
  pci?: number
  rsrq?: number
  sinr?: number
}

export interface CellLocationResponse {
  available: boolean
  cell_info?: CellLocationInfo
  neighbor_cells: CellLocationInfo[]
  cells?: CellLocationInfo[]
}

export interface OperatorInfo {
  path: string
  name: string
  status: string
  mcc: string
  mnc: string
  technologies: string[]
}

export interface OperatorListResponse {
  operators: OperatorInfo[]
}

export interface ManualRegisterRequest {
  mccmnc: string
}

export interface ApnContext {
  path: string
  name: string
  active: boolean
  apn: string
  protocol: string
  username: string
  password: string
  auth_method: string
  context_type?: string
}

export interface ApnListResponse {
  contexts: ApnContext[]
}

export interface SetApnRequest {
  context_path: string
  apn?: string
  protocol?: string
  username?: string
  password?: string
  auth_method?: string
}

export interface PingResult {
  success: boolean
  latency_ms?: number
  target: string
  error?: string
}

export interface ConnectivityCheckResponse {
  ipv4: PingResult
  ipv6: PingResult
}

export interface WebhookConfig {
  enabled: boolean
  url: string
  forward_sms: boolean
  forward_calls: boolean
  forward_ddns: boolean
  forward_updates: boolean
  headers: Record<string, string>
  secret: string
  sms_template: string
  call_template: string
  ddns_template: string
  update_template: string
}

export type NotificationChannelKey =
  | 'webhook'
  | 'bark'
  | 'pushplus'
  | 'wecom_app'
  | 'wecom_robot'
  | 'dingtalk_robot'
  | 'dingtalk_app'
  | 'feishu_robot'
  | 'telegram'
  | 'email'
  | 'serverchan3'

export type NotificationEventType = 'sms' | 'ddns' | 'version_update' | 'system_event' | 'device_status' | 'automation'
export type NotificationLogStatus = 'success' | 'failed' | 'no_available_channel' | 'quiet_hours' | 'unmatched'
export type MatcherOperator = 'always' | 'contains' | 'not_contains' | 'equals' | 'regex'

export interface MessageChannelConfig {
  enabled: boolean
  forward_sms: boolean
  forward_calls: boolean
  forward_ddns: boolean
  forward_updates: boolean
  sms_template: string
  call_template: string
  ddns_template: string
  update_template: string
}

export interface BarkConfig extends MessageChannelConfig {
  server_url: string
  device_key: string
  title_template: string
  group: string
  sound: string
  level: string
  icon: string
  click_url: string
  copy: string
  auto_copy: boolean
  save_history: boolean
}

export interface PushPlusConfig extends MessageChannelConfig {
  token: string
  title_template: string
  topic: string
  template: string
  channel: string
  option: string
  callback_url: string
}

export interface WecomAppConfig extends MessageChannelConfig {
  api_base_url: string
  corp_id: string
  agent_id: string
  secret: string
  to_user: string
  to_party: string
  to_tag: string
  safe: boolean
}

export interface WecomRobotConfig extends MessageChannelConfig {
  webhook_url: string
  key: string
}

export interface DingtalkRobotConfig extends MessageChannelConfig {
  webhook_url: string
  access_token: string
  secret: string
  at_mobiles: string
  at_all: boolean
}

export interface DingtalkAppConfig extends MessageChannelConfig {
  app_key: string
  app_secret: string
  robot_code: string
  open_conversation_id: string
  msg_key: string
}

export interface FeishuRobotConfig extends MessageChannelConfig {
  webhook_url: string
  token: string
  secret: string
}

export interface TelegramConfig extends MessageChannelConfig {
  api_base_url: string
  bot_token: string
  chat_id: string
  parse_mode: string
  disable_web_page_preview: boolean
}

export interface EmailConfig extends MessageChannelConfig {
  smtp_host: string
  smtp_port: number
  smtp_security: 'implicit_tls' | 'starttls' | 'none'
  allow_insecure_tls: boolean
  username: string
  password: string
  sender_address: string
  sender_name: string
  receiver_addresses: string
  message_format: 'plain' | 'html'
}

export interface ServerChan3Config extends MessageChannelConfig {
  send_key: string
  uid: string
  channel: string
  openid: string
}

export interface NotificationConfig {
  version: number
  channels: NotificationChannelInstance[]
  rules: NotificationRule[]
  log_cleanup: NotificationLogCleanupConfig
}

export interface NotificationRateLimitConfig {
  enabled: boolean
  max_messages: number
  window_seconds: number
}

export interface NotificationLogCleanupConfig {
  retention_days_enabled: boolean
  retention_days: number
  max_entries_enabled: boolean
  max_entries: number
}

export interface NotificationChannelInstance {
  id: string
  type: NotificationChannelKey
  name: string
  enabled: boolean
  rate_limit: NotificationRateLimitConfig
  config: Record<string, unknown>
}

export interface RuleMatcher {
  field: string
  operator: MatcherOperator
  value: string
}

export interface QuietHoursSchedule {
  enabled: boolean
  weekdays: number[]
  start: string
  end: string
}

export interface DeviceStatusSchedule {
  mode: 'fixed' | 'interval'
  interval_minutes: number
  weekdays: number[]
  times: string[]
}

export interface NotificationRule {
  id: string
  type: NotificationEventType
  name: string
  enabled: boolean
  matcher: RuleMatcher
  channel_ids: string[]
  event_codes: string[]
  title_template: string
  template: string
  quiet_hours: QuietHoursSchedule[]
  ddns_failure_threshold: number
  device_status_items: string[]
  device_status_schedule: DeviceStatusSchedule
  device_status_sms_period: 'today' | 'last_24h' | 'last_7d' | 'all'
}

export interface NotificationLogEntry {
  id: number
  event_type: NotificationEventType
  status: NotificationLogStatus
  summary: string
  rule_id: string
  rule_name: string
  channel_id: string
  channel_name: string
  message: string
  created_at: string
}

export interface NotificationLogsResponse {
  logs: NotificationLogEntry[]
  total: number
}

export type NotificationQueueItemStatus = 'pending' | 'scheduled' | 'retrying' | 'sending' | 'failed'

export interface NotificationQueueEntry {
  id: number
  status: NotificationQueueItemStatus
  event_type: NotificationEventType
  event_label: string
  summary: string
  reason: string
  channel_id: string
  channel_name: string
  channel_type: NotificationChannelKey
  rule_id: string
  rule_name: string
  title: string
  body: string
  next_attempt_at: string
  attempt_count: number
  max_attempts: number
  created_at: string
  updated_at: string
}

export interface NotificationQueueResponse {
  items: NotificationQueueEntry[]
  total: number
}

export const DEFAULT_SMS_TEMPLATE = `{
  "msg_type": "text",
  "content": {
    "text": "📱 短信通知\\n号码: {{phone_number}}\\n内容: {{content}}\\n时间: {{timestamp}}\\n来源: {{own_number}}"
  }
}`

export const DEFAULT_CALL_TEMPLATE = `{
  "msg_type": "text",
  "content": {
    "text": "📞 来电通知\\n号码: {{phone_number}}\\n类型: {{direction}}\\n时间: {{start_time}}\\n时长: {{duration}}秒\\n已接听: {{answered}}"
  }
}`

export const DEFAULT_DDNS_TEMPLATE = `{
  "msg_type": "text",
  "content": {
    "text": "SimAdmin DDNS 通知\\n域名: {{domains}}\\nIP类型: {{ip_type}}\\n新IP: {{new_ip}}\\n旧IP: {{old_ip}}\\n服务商: {{provider}}\\n记录类型: {{record_type}}\\n状态: {{status}}\\n消息: {{message}}\\n更新时间: {{timestamp}}"
  }
}`

export const DEFAULT_UPDATE_TEMPLATE = `{
  "msg_type": "text",
  "content": {
    "text": "🚀 SimAdmin 发现新版本\\n固件包: {{asset_name}}\\n版本号: {{version}}\\n时间: {{time}}\\n来源: {{own_number}}\\n\\n请前往 OTA 更新页面的在线更新模块检查更新，可一键下载并升级。"
  }
}`

export const DEFAULT_PLAIN_SMS_TEMPLATE = `📱 短信通知
号码: {{phone_number}}
内容: {{content}}
时间: {{timestamp}}
来源: {{own_number}}`

export const DEFAULT_PLAIN_CALL_TEMPLATE = `📞 来电通知
号码: {{phone_number}}
类型: {{direction}}
时间: {{start_time}}
时长: {{duration}}秒
已接听: {{answered}}`

export const DEFAULT_PLAIN_DDNS_TEMPLATE = `SimAdmin DDNS 通知
域名: {{domains}}
IP类型: {{ip_type}}
新IP: {{new_ip}}
旧IP: {{old_ip}}
服务商: {{provider}}
记录类型: {{record_type}}
状态: {{status}}
消息: {{message}}
更新时间: {{timestamp}}`

export const DEFAULT_PLAIN_UPDATE_TEMPLATE = `🚀 SimAdmin 发现新版本
固件包: {{asset_name}}
版本号: {{version}}
时间: {{time}}
来源: {{own_number}}

请前往 OTA 更新页面的在线更新模块检查更新，可一键下载并升级。`

export interface WebhookTestResponse {
  success: boolean
  message: string
}

export interface OtaMeta {
  version: string
  commit: string
  build_time: string
  binary_md5: string
  frontend_md5: string
  arch: string
  min_version?: string
}

export interface OtaValidation {
  valid: boolean
  is_newer: boolean
  binary_md5_match: boolean
  frontend_md5_match: boolean
  arch_match: boolean
  error?: string
}

export interface OtaStatusResponse {
  current_version: string
  current_commit: string
  pending_update: boolean
  pending_meta?: OtaMeta
}

export interface OtaUploadResponse {
  meta: OtaMeta
  validation: OtaValidation
}

export interface OtaOnlinePrepareRequest {
  proxy_prefix?: string
}

export interface OtaReleaseAsset {
  name: string
  size: number
  browser_download_url: string
}

export interface OtaLatestReleaseResponse {
  tag_name: string
  name?: string
  published_at: string
  target_commitish?: string
  body?: string
  html_url?: string
  assets?: OtaReleaseAsset[]
}

export type DdnsProvider = 'cloudflare' | 'alidns' | 'tencentcloud'
export type DdnsIpGetType = 'api' | 'interface'

export interface DdnsIpConfig {
  enabled: boolean
  get_type: DdnsIpGetType
  interface_name: string
  urls: string[]
  domains: string[]
}

export interface DdnsConfig {
  enabled: boolean
  provider: DdnsProvider
  access_id: string
  access_secret: string
  access_secret_set?: boolean
  interval_seconds: number
  ttl: number
  ipv4: DdnsIpConfig
  ipv6: DdnsIpConfig
}

export interface DdnsStatusResponse {
  enabled: boolean
  running: boolean
  provider: string
  last_sync_at?: string
  last_ipv4?: string
  last_ipv6?: string
  last_message?: string
}

export interface DdnsRecordSyncResult {
  record_type: string
  domains: string[]
  old_ip?: string
  new_ip?: string
  status: string
  message: string
}

export interface DdnsSyncResponse {
  started_at: string
  finished_at: string
  records: DdnsRecordSyncResult[]
}

export interface DdnsLogEntry {
  timestamp: string
  level: string
  record_type: string
  domains: string[]
  message: string
}

export interface DdnsLogsResponse {
  entries: DdnsLogEntry[]
}

export interface WlanStatusResponse {
  available: boolean
  enabled: boolean
  hardware_enabled: boolean
  interface_name?: string
  connected: boolean
  ssid?: string
  connection_id?: string
  ipv4_addresses: string[]
  ipv4_gateway?: string
  ipv6_addresses: string[]
}

export interface WlanNetwork {
  ssid: string
  bssid: string
  signal: number
  security: string
  secure: boolean
  connected: boolean
}

export interface WlanScanResponse {
  networks: WlanNetwork[]
}

export interface WlanSavedNetwork {
  id: string
  uuid: string
  ssid: string
  interface_name?: string
  active: boolean
  auto_join: boolean
}

export interface WlanProfilesResponse {
  profiles: WlanSavedNetwork[]
}

export interface WlanConnectRequest {
  ssid: string
  password?: string
  auto_join?: boolean
}

export interface WlanProfileRequest {
  connection_id: string
  auto_join?: boolean
  ipv4_mode?: 'dhcp' | 'auto' | 'manual'
  ipv4_address?: string
  ipv4_prefix?: number
  ipv4_gateway?: string
}

export interface WlanForgetRequest {
  uuid?: string
  connection_id?: string
}

export interface AutomationConfig {
  enabled: boolean
  tasks: AutomationTask[]
}

export type AutomationTrigger =
  | { type: 'fixed'; config: { weekdays: number[]; times: string[] } }
  | { type: 'interval'; config: { interval_value: number; interval_unit: string } }

export type AutomationAction =
  | { type: 'restart_baseband'; config: null | Record<string, never> }
  | { type: 'reboot_device'; config: { delay_seconds: number } }
  | {
      type: 'backup_data'
      config: {
        components: BackupComponentKey[]
        storage: BackupStorageConfig
      }
    }
  | {
      type: 'send_sms'
      config: {
        phone_number: string
        content: string
        random_delay_seconds?: number
        retry_limit?: number
      }
    }

export interface AutomationTask {
  id: string
  name: string
  enabled: boolean
  trigger: AutomationTrigger
  action: AutomationAction
}

export interface AutomationLogEntry {
  id: number
  task_id: string
  task_name: string
  task_type: string
  status: string
  detail: string
  created_at: string
}

export interface AutomationLogsResponse {
  logs: AutomationLogEntry[]
  total: number
}

export type BackupComponentKey =
  | 'config'
  | 'sms'
  | 'notification_config'
  | 'notification_logs'
  | 'notification_queue'
  | 'automation_config'
  | 'automation_logs'
  | 'sim_cache'
  | 'esim_cache'
  | 'auth'

export type BackupKind = 'full' | 'slim'
export type BackupImportMode = 'merge' | 'replace'
export type BackupScheduleMode = 'manual' | 'fixed' | 'interval'
export type BackupIntervalUnit = 'mins' | 'hours' | 'days'

export interface BackupComponentOption {
  key: BackupComponentKey
  label: string
  description: string
  default_selected: boolean
  sensitive: boolean
  records?: number | null
}

export interface BackupOptionsResponse {
  format_version: number
  default_components: BackupComponentKey[]
  components: BackupComponentOption[]
  local_dir: string
  pre_restore_dir: string
}

export interface BackupScheduleConfig {
  mode: BackupScheduleMode
  weekdays: number[]
  times: string[]
  interval_value: number
  interval_unit: BackupIntervalUnit
}

export interface BackupCleanupConfig {
  retention_days_enabled: boolean
  retention_days: number
  max_files_enabled: boolean
  max_files: number
}

export interface BackupStorageConfig {
  local_dir: string
}

export interface BackupConfig {
  enabled: boolean
  components: BackupComponentKey[]
  schedule: BackupScheduleConfig
  cleanup: BackupCleanupConfig
  storage: BackupStorageConfig
  last_run_at: string
  last_run_key: string
}

export interface BackupLocalFile {
  name: string
  size: number
  modified_at: string
  backup_kind?: BackupKind | null
  components: BackupComponentKey[]
  counts: Record<string, number>
  pre_restore: boolean
  valid: boolean
  error: string
}

export interface BackupExportLocalResponse {
  file: BackupLocalFile
}

export interface BackupImportComponentPreview {
  key: BackupComponentKey
  label: string
  records: number
  sensitive: boolean
}

export interface BackupImportPreview {
  filename?: string | null
  backup_kind: BackupKind
  format_version: number
  simadmin_version: string
  created_at: string
  contains_sensitive_data: boolean
  components: BackupImportComponentPreview[]
  warnings: string[]
}

export interface BackupImportApplyResponse {
  imported_components: BackupComponentKey[]
  backup_kind: BackupKind
  mode: BackupImportMode
  pre_restore_file?: BackupLocalFile | null
}

export interface BackupLocalFilesResponse {
  backups: BackupLocalFile[]
  pre_restore: BackupLocalFile[]
}

export interface BackupBlobResponse {
  blob: Blob
  filename: string
}
