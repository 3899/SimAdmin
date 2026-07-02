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
  transport?: string
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
  bot_token: string
  chat_id: string
  parse_mode: string
  disable_web_page_preview: boolean
}

export interface ServerChan3Config extends MessageChannelConfig {
  uid: string
  send_key: string
}

export interface EmailConfig extends MessageChannelConfig {
  smtp_host: string
  smtp_port: number
  smtp_tls: boolean
  username: string
  password: string
  sender_address: string
  sender_name: string
  receiver_address: string
  message_format: string
  allow_insecure_connections: boolean
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
    "text": "🚀 SimAdmin 发现新版本\\n固件包: {{asset_name}}\\n版本号: {{version}}\\nCommit: {{commit}}\\n时间: {{time}}\\n来源: {{own_number}}\\n\\n请前往 OTA 更新页面的在线更新模块检查更新，可一键下载并升级。"
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
Commit: {{commit}}
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

export interface VowifiConfig {
  feature_enabled: boolean
  connection_enabled: boolean
  auto_restore_initial_delay_secs: number
  auto_restore_attempts: number
  auto_restore_retry_delay_secs: number
}

export interface VowifiCarrierProfile {
  profile_id: string
  mcc: string
  mnc: string
  mnc_len: number
  plmn: string
  country_iso2: string
  brand: string
  operator_legal_name: string
  aliases: string[]
  source_refs: string[]
  last_verified: string
  supported: boolean
  support_stage: string
}

export interface VowifiProfilesResponse {
  profiles: VowifiCarrierProfile[]
  count: number
}

export interface VowifiMaskedSimIdentity {
  present: boolean
  iccid: string
  imsi: string
  operator_id: string
}

export interface VowifiProfileMatchResponse {
  matched: boolean
  matched_prefix?: string
  profile?: VowifiCarrierProfile
  sim_auth?: VowifiAkaAdapterPlan | null
  epdg?: VowifiEpdgPlan | null
  ike?: VowifiIkePlan | null
  dataplane?: VowifiDataplanePlan | null
  ims?: VowifiImsPlan | null
  sim: VowifiMaskedSimIdentity
}

export interface VowifiEpdgPlan {
  host: string
  port: number
  ip_stack: string
  apn?: string | null
  dns_server?: string | null
  route_kind: string
  route_policy_id: string
  route_note: string
}

export interface VowifiIkeProposalPlan {
  proposal: string
  encryption: string
  integrity: string
  prf: string
  dh_group: string
}

export interface VowifiEspProposalPlan {
  proposal: string
  encryption: string
  integrity: string
  mode: string
}

export interface VowifiChildSaPlan {
  mode: string
  anti_replay_window: number
  mtu_strategy: string
  esp_proposals: VowifiEspProposalPlan[]
}

export interface VowifiIkePlan {
  exchange_phases: string[]
  aka_challenge_mode: string
  nat_keepalive_seconds: number
  dpd_interval_seconds: number
  reauth_interval_seconds?: number | null
  retransmit_policy: string
  mobike_policy: string
  ike_proposals: VowifiIkeProposalPlan[]
  child_sa: VowifiChildSaPlan
  sensitive_values_policy: string
}

export interface VowifiSecAgreeMechanismPlan {
  mechanism: string
  integrity: string
  encryption: string
  protocol: string
  mode: string
}

export interface VowifiImsPlan {
  domain: string
  realm: string
  registrar?: string | null
  pcscf?: string | null
  transport: string
  local_port: number
  user_agent_family: string
  identity_source: string
  supported_header: string
  include_pani_authenticated: boolean
  strict_security_server_offer: boolean
  enable_initial_reject_fallback: boolean
  security_client_mechanisms: VowifiSecAgreeMechanismPlan[]
  sms_receiver_transport: string
  sensitive_values_policy: string
}

export interface VowifiDataplaneEspProposalPlan {
  proposal: string
  encryption: string
  integrity: string
  encapsulation: string
}

export interface VowifiTrafficSelectorPlan {
  local_selector: string
  remote_selector: string
  address_assignment: string
}

export interface VowifiSmoltcpGatewayPlan {
  stack: string
  gateway_mode: string
  ip_stack: string
  tcp_enabled: boolean
  udp_enabled: boolean
  icmp_enabled: boolean
  socket_policy: string
}

export interface VowifiDataplanePlan {
  outer_encapsulation: string
  nat_t_port: number
  nat_keepalive_seconds: number
  anti_replay_window: number
  mtu_strategy: string
  mtu: VowifiMtuPlan
  traffic_selectors: VowifiTrafficSelectorPlan
  smoltcp: VowifiSmoltcpGatewayPlan
  esp_proposals: VowifiDataplaneEspProposalPlan[]
  plaintext_capture_policy: string
  sensitive_values_policy: string
}

export interface VowifiAntiReplayWindowSnapshot {
  window_size: number
  highest_sequence: number
  tracked_slots: number
  sensitive_values_policy: string
}

export interface VowifiMtuPlan {
  outer_mtu: number
  estimated_overhead_bytes: number
  inner_mtu: number
  ipv4_header_bytes: number
  udp_header_bytes: number
  esp_header_bytes: number
  iv_bytes: number
  trailer_budget_bytes: number
  integrity_check_bytes: number
  sensitive_values_policy: string
}

export interface VowifiEspPacketMetadata {
  sa_identifier_present: boolean
  sequence_number: number
  protected_bytes: number
  outer_frame_bytes: number
  header_bytes: number
  sensitive_values_policy: string
}

export interface VowifiNattPacketSummary {
  kind: string
  udp_port: number
  wire_bytes: number
  esp?: VowifiEspPacketMetadata | null
  sensitive_values_policy: string
}

export interface VowifiMtuDecision {
  decision: string
  accepted: boolean
  inner_packet_bytes: number
  estimated_outer_frame_bytes: number
  outer_mtu: number
  sensitive_values_policy: string
}

export interface VowifiInnerPacketMetadata {
  packet_id: number
  direction: string
  ip_version: string
  packet_bytes: number
  accepted: boolean
  drop_reason?: string | null
  sensitive_values_policy: string
}

export interface VowifiInnerGatewayPublicState {
  adapter: string
  ip_stack: string
  queue_capacity: number
  queued_packets: number
  packets_to_esp: number
  packets_from_esp: number
  last_packet?: VowifiInnerPacketMetadata | null
  sensitive_values_policy: string
}

export interface VowifiSaPairPublicState {
  inbound_sa_identifier_present: boolean
  outbound_sa_identifier_present: boolean
  outbound_sequence_allocated: number
  packets_in: number
  packets_out: number
  bytes_in: number
  bytes_out: number
  sensitive_values_policy: string
}

export interface VowifiEspFrameDecision {
  direction: string
  decision: string
  accepted: boolean
  sequence_number?: number | null
  outer_frame_bytes: number
  natt?: VowifiNattPacketSummary | null
  mtu?: VowifiMtuDecision | null
  sensitive_values_policy: string
}

export interface VowifiChildSaPublicState {
  profile_id: string
  plmn: string
  phase: string
  selected_esp_proposal?: string | null
  inbound_sa_identifier_present: boolean
  outbound_sa_identifier_present: boolean
  sa_pair: VowifiSaPairPublicState
  replay_window: VowifiAntiReplayWindowSnapshot
  mtu: VowifiMtuPlan
  mtu_drops: number
  smoltcp: VowifiSmoltcpGatewayPlan
  inner_gateway: VowifiInnerGatewayPublicState
  packets_in: number
  packets_out: number
  bytes_in: number
  bytes_out: number
  last_frame_decision?: VowifiEspFrameDecision | null
  last_error?: string | null
  sensitive_values_policy: string
}

export interface VowifiLogicalChannelPlan {
  application_priority: string[]
  channel_scope: string
  open_policy: string
  close_policy: string
  profile_switch_cleanup: string
}

export interface VowifiAkaChallengePlan {
  method: string
  challenge_source: string
  resync_supported: boolean
  failure_mapping: string
  secret_values_policy: string
}

export interface VowifiAkaAdapterPlan {
  identity_source: string
  sim_access: string
  qmi_proxy_policy: string
  logical_channel: VowifiLogicalChannelPlan
  challenge: VowifiAkaChallengePlan
  timeout_ms: number
}

export interface VowifiIkeKeySchedulePlan {
  prf: string
  encryption: string
  integrity: string
  prf_output_bytes: number
  encryption_key_bytes: number
  integrity_key_bytes: number
  total_secret_bytes: number
  exported_secret_values: boolean
  sensitive_values_policy: string
}

export interface VowifiIkeEncryptedPayloadPlan {
  mode: string
  outer_payload: string
  cipher: string
  integrity: string
  iv_bytes: number
  block_bytes: number
  icv_bytes: number
  encrypted_payload_bytes: number
  inner_payload_count: number
  exported_plaintext: boolean
  sensitive_values_policy: string
}

export interface VowifiIkeRetransmitState {
  message_id: number
  attempts: number
  elapsed_ms: number
  next_timeout_ms: number
  decision: string
}

export interface VowifiIkeControlEvent {
  kind: string
  message_id: number
  protocol_id: number
  spi_size: number
  spi_present: boolean
  notify_type?: number | null
  notify_name?: string | null
  delete_spi_count?: number | null
  action: string
  sensitive_values_policy: string
}

export interface VowifiEapAkaAttributeSummary {
  attribute_type: number
  role: string
  units: number
  value_bytes: number
  value_redacted: boolean
}

export interface VowifiEapAkaPacketSummary {
  code: string
  identifier: number
  method: string
  subtype: string
  attribute_count: number
  attributes: VowifiEapAkaAttributeSummary[]
  raw_len: number
  secret_values_policy: string
}

export interface VowifiIkeTranscriptEvent {
  message_id: number
  exchange: string
  direction: string
  payloads: string[]
  note: string
}

export interface VowifiIkePublicSnapshot {
  profile_id: string
  plmn: string
  phase: string
  initiator_spi_present: boolean
  responder_spi_present: boolean
  next_message_id: number
  selected_proposal?: string | null
  key_schedule?: VowifiIkeKeySchedulePlan | null
  encrypted_payload?: VowifiIkeEncryptedPayloadPlan | null
  retransmit: VowifiIkeRetransmitState
  last_control_event?: VowifiIkeControlEvent | null
  eap?: VowifiEapAkaPacketSummary | null
  transcript: VowifiIkeTranscriptEvent[]
  last_error?: string | null

  sensitive_values_policy: string
}

export interface VowifiSipMessageSummary {
  direction: string
  message_kind: string
  method?: string | null
  status_code?: number | null
  transport: string
  target_domain: string
  authorization_present: boolean
  security_client_present: boolean
  security_server_present: boolean
  security_verify_present: boolean
  digest_challenge_present: boolean
  body_bytes: number
  sensitive_values_policy: string
}

export interface VowifiDigestChallengeSummary {
  header_kind: string
  algorithm: string
  realm_matches_profile: boolean
  challenge_token_present: boolean
  qop_present: boolean
  opaque_present: boolean
  stale: boolean
  values_redacted: boolean
  sensitive_values_policy: string
}

export interface VowifiAkaDigestPublicState {
  algorithm: string
  provider: string
  challenge_accepted: boolean
  auth_proof_present: boolean
  auth_proof_bytes: number
  sec_agree_key_source_ready: boolean
  exported_secret_values: boolean
  sensitive_values_policy: string
}

export interface VowifiSecAgreePublicState {
  security_mode: string
  client_offer_count: number
  server_offer_count: number
  selected_security_mechanism?: string | null
  server_offer_selected: boolean
  security_verify_ready: boolean
  protected_transport_ready: boolean
  local_sa_identifier_present: boolean
  remote_sa_identifier_present: boolean
  policy_installed: boolean
  exported_secret_values: boolean
  sensitive_values_policy: string
}

export interface VowifiImsRegisterPublicState {
  profile_id: string
  plmn: string
  phase: string
  transport: string
  target_domain: string
  last_sip_status?: number | null
  register_200_received: boolean
  registered_expires_seconds?: number | null
  security_mode: string
  selected_security_mechanism?: string | null
  challenge?: VowifiDigestChallengeSummary | null
  aka_digest?: VowifiAkaDigestPublicState | null
  sec_agree?: VowifiSecAgreePublicState | null
  service_route_present: boolean
  associated_uri_count: number
  sms_ready: boolean
  transcript: VowifiSipMessageSummary[]
  last_error?: string | null
  sensitive_values_policy: string
}

export interface VowifiSmsPartState {
  reference: number
  sequence: number
  total: number
  received: boolean
}

export interface VowifiSmsRpduSummary {
  direction: string
  rpdu_kind: string
  user_data_bytes: number
  segment_reference_present: boolean
  segment_reference?: number | null
  segment_sequence?: number | null
  segment_total?: number | null
  values_redacted: boolean
  sensitive_values_policy: string
}

export interface VowifiSmsSipMessageSummary {
  direction: string
  method: string
  transport: string
  sip_state: string
  sip_status?: number | null
  content_type: string
  body_bytes: number
  sensitive_values_policy: string
}

export interface VowifiSmsAckPlan {
  ack_kind: string
  transport: string
  sip_response_code: number
  rp_ack_present: boolean
  failure_cause?: string | null
  sensitive_values_policy: string
}

export interface VowifiSmsReassemblyState {
  key_scope: string
  reference: number
  expected_parts: number
  received_parts: number
  complete: boolean
  duplicate_parts: number
  last_sequence?: number | null
  sensitive_values_policy: string
}

export interface VowifiSmsDeliveryPublicRecord {
  trace_id: string
  message_id: string
  direction: string
  state: string
  api_status: string
  sip_state: string
  rpdu_ack: string
  delivery_reported: boolean
  failure_cause?: string | null
  retry_count: number
  parts: VowifiSmsPartState[]
  parts_complete: boolean
  db_fact_source: string
  sensitive_values_policy: string
}

export interface VowifiSmsRuntimePublicState {
  profile_id: string
  plmn: string
  sms_ready: boolean
  receiver_transport: string
  subscribe_reg_ready: boolean
  pending_delivery_count: number
  mo: VowifiSmsDeliveryPublicRecord
  mt: VowifiSmsDeliveryPublicRecord
  last_sip_message?: VowifiSmsSipMessageSummary | null
  last_rpdu?: VowifiSmsRpduSummary | null
  last_ack?: VowifiSmsAckPlan | null
  reassembly?: VowifiSmsReassemblyState | null
  state_consistency_policy: string
  sensitive_values_policy: string
}

export interface VowifiRestoreRuntimeSnapshotSummary {
  previous_runtime_active: boolean
  previous_tunnel_present: boolean
  previous_ims_registered: boolean
  previous_sms_ready: boolean
  previous_sms_mode: string
  profile_generation_captured: boolean
  sensitive_values_policy: string
}

export interface VowifiRestoreCleanupSummary {
  runtime_teardown_done: boolean
  qmi_sms_restored: boolean
  apdu_sessions_cleared: boolean
  stale_runtime_reuse_allowed: boolean
  sensitive_values_policy: string
}

export interface VowifiRestoreGateSummary {
  identity_ready: boolean
  identity_changed: boolean
  sim_auth_ready: boolean
  home_plmn_source: string
  card_reset_settling_ms: number
  sensitive_values_policy: string
}

export interface VowifiRestoreRuntimeAttemptSummary {
  attempts: number
  first_failure_retryable: boolean
  first_failure_reason?: string | null
  final_register_verified: boolean
  final_sms_ready: boolean
  rebuild_strategy: string
  sensitive_values_policy: string
}

export interface VowifiRestoreWorkflowEvent {
  phase: string
  phase_ms: number
  identity_ready: boolean
  sim_auth_ready: boolean
  retry_count: number
  sms_mode: string
  cleanup_done: boolean
  register_verified: boolean
  sms_ready: boolean
  degraded_reason?: string | null
  sensitive_values_policy: string
}

export interface VowifiEsimRestorePublicState {
  switch_token: string
  switch_phase: string
  phase_ms: number
  identity_ready: boolean
  sim_auth_ready: boolean
  degraded_reason?: string | null
  retry_count: number
  snapshot: VowifiRestoreRuntimeSnapshotSummary
  cleanup: VowifiRestoreCleanupSummary
  gate: VowifiRestoreGateSummary
  runtime_restore: VowifiRestoreRuntimeAttemptSummary
  events: VowifiRestoreWorkflowEvent[]
  sensitive_values_policy: string
}
export interface VowifiReadiness {
  identity_ready: boolean
  sim_auth_ready: boolean
  profile_matched: boolean
  epdg_ready: boolean
  ike_ready: boolean
  child_sa_ready: boolean
  esp_ready: boolean
  ims_registered: boolean
  sms_ready: boolean
}

export interface VowifiRuntimeFlowStep {
  id: string
  component: string
  stage: string
  state: string
  readiness_key: string
  blocking_reason?: string | null
}

export interface VowifiRuntimeFlowStatus {
  stage: string
  controlplane_mode: string
  dataplane_mode: string
  steps: VowifiRuntimeFlowStep[]
}

export interface VowifiExecutorCapability {
  stage: string
  component: string
  enabled: boolean
  mode: string
  reason: string
}

export interface VowifiLiveExecutorGateReport {
  live_network_authorized: boolean
  device_state_changes_authorized: boolean
  adb_path_configured: boolean
  device_admin_url_configured: boolean
  implementation_ready: boolean
  effective_live_network_allowed: boolean
  effective_device_state_changes_allowed: boolean
  blockers: string[]
  sensitive_values_policy: string
}

export interface VowifiRuntimeExecutorReport {
  executor_id: string
  mode: string
  live_network_allowed: boolean
  dataplane_dry_run?: VowifiChildSaPublicState | null
  ike_dry_run?: VowifiIkePublicSnapshot | null
  ims_register_dry_run?: VowifiImsRegisterPublicState | null
  sms_dry_run?: VowifiSmsRuntimePublicState | null
  esim_restore_dry_run?: VowifiEsimRestorePublicState | null
  device_state_changes_allowed: boolean
  live_gate: VowifiLiveExecutorGateReport
  capabilities: VowifiExecutorCapability[]
}

export interface VowifiStatusResponse {
  phase: string
  dataplane_mode: string
  controlplane_mode: string
  readiness: VowifiReadiness
  flow: VowifiRuntimeFlowStatus
  executor: VowifiRuntimeExecutorReport
  profile: VowifiProfileMatchResponse
  degraded_reason?: string | null
  switch_phase?: string | null
  switch_token?: string | null
  phase_ms?: number | null
  switch_identity_ready: boolean
  switch_sim_auth_ready: boolean
  switch_retry_count: number
}
export interface VowifiRuntimeSnapshotEntry {
  phase: string
  profile_id?: string | null
  plmn?: string | null
  identity_ready: boolean
  sim_auth_ready: boolean
  profile_matched: boolean
  epdg_ready: boolean
  ike_ready: boolean
  child_sa_ready: boolean
  esp_ready: boolean
  ims_registered: boolean
  sms_ready: boolean
  degraded_reason?: string | null
  updated_at: string
}

export interface VowifiDiagnosticsSummary {
  runtime_phase: string
  profile_id?: string | null
  plmn?: string | null
  ready_stage_count: number
  total_stage_count: number
  pending_sms_deliveries: number
  failed_sms_deliveries: number
  running_soak_runs: number
  failed_soak_runs: number
  last_event_at?: string | null
  active_trace_id?: string | null
  degraded: boolean
  read_only: boolean
}

export interface VowifiDiagnosticsPrivacy {
  redaction_policy: string
  sensitive_fields_returned: boolean
  event_detail_policy: string
  trace_filter_policy: string
  action_interfaces_enabled: boolean
}

export interface VowifiDiagnosticsTimelineEntry {
  kind: string
  timestamp?: string | null
  trace_id?: string | null
  level: string
  phase: string
  title: string
  detail: string
  state: string
}

export interface VowifiAuditCheck {
  check_id: string
  status: string
  detail: string
}

export interface VowifiProfileAuditEntry {
  profile_id: string
  plmn: string
  country_iso2: string
  brand: string
  offline_plan_ready: boolean
  dry_run_ready: boolean
  live_test_ready: boolean
  blockers: string[]
  checks: VowifiAuditCheck[]
}

export interface VowifiLongRunGate {
  gate_id: string
  status: string
  target: string
  evidence: string
  blocker?: string | null
}

export interface VowifiLiveStageReadiness {
  stage_id: string
  component: string
  status: string
  offline_ready: boolean
  dry_run_ready: boolean
  live_network_required: boolean
  device_state_change_required: boolean
  live_network_authorized: boolean
  device_state_changes_authorized: boolean
  implementation_ready: boolean
  evidence: string
  next_step: string
  blockers: string[]
  sensitive_values_policy: string
}

export interface VowifiSoakScenarioPlan {
  scenario_id: string
  status: string
  duration_hours: number
  sample_interval_seconds: number
  live_network_required: boolean
  device_state_change_required: boolean
  sms_test_required: boolean
  metrics: string[]
  pass_criteria: string[]
  evidence_source: string
  blockers: string[]
  sensitive_values_policy: string
}

export interface VowifiReadinessAuditReport {
  stage: string
  clean_room_policy: string
  live_network_allowed: boolean
  device_state_changes_allowed: boolean
  profile_count: number
  profiles_ready: number
  dry_run_profiles_ready: number
  live_profiles_ready: number
  long_run_gate_count: number
  long_run_ready_count: number
  live_stage_count: number
  live_stage_ready_count: number
  soak_scenario_count: number
  soak_scenario_ready_count: number
  profile_audits: VowifiProfileAuditEntry[]
  long_run_gates: VowifiLongRunGate[]
  live_stage_readiness: VowifiLiveStageReadiness[]
  soak_scenarios: VowifiSoakScenarioPlan[]
  blockers: string[]
  sensitive_values_policy: string
}
export interface VowifiDiagnosticsResponse {
  status: VowifiStatusResponse
  persisted_snapshot?: VowifiRuntimeSnapshotEntry | null
  events: VowifiRuntimeEventsResponse
  sms_deliveries: VowifiSmsDeliveriesResponse
  soak_runs: VowifiSoakRunsResponse
  restore?: VowifiEsimRestoreEntry | null
  summary: VowifiDiagnosticsSummary
  timeline: VowifiDiagnosticsTimelineEntry[]
  trace_filter?: string | null
  privacy: VowifiDiagnosticsPrivacy
  m10_audit: VowifiReadinessAuditReport
}

export interface VowifiRuntimeEventEntry {
  id: number
  trace_id?: string | null
  level: string
  phase: string
  profile_id?: string | null
  event_type: string
  detail_json: string
  created_at: string
}

export interface VowifiRuntimeEventsResponse {
  events: VowifiRuntimeEventEntry[]
  total: number
}

export interface VowifiSmsPartEntry {
  message_id: string
  reference: number
  sequence: number
  total: number
  received: boolean
  updated_at: string
}

export interface VowifiSmsDeliveryEntry {
  message_id: string
  trace_id: string
  direction: string
  state: string
  sip_state: string
  rpdu_ack: string
  delivery_reported: boolean
  failure_cause?: string | null
  retry_count: number
  api_sms_id?: number | null
  parts: VowifiSmsPartEntry[]
  created_at: string
  updated_at: string
}

export interface VowifiSmsDeliveriesResponse {
  deliveries: VowifiSmsDeliveryEntry[]
  total: number
}

export interface VowifiSoakSampleEntry {
  id: number
  run_id: string
  sample_kind: string
  metric_name: string
  metric_value: number
  state: string
  created_at: string
}

export interface VowifiSoakRunEntry {
  run_id: string
  scenario_id: string
  profile_id?: string | null
  plmn?: string | null
  status: string
  started_at: string
  finished_at?: string | null
  duration_seconds: number
  sample_count: number
  failure_count: number
  last_error?: string | null
  sensitive_values_policy: string
  samples: VowifiSoakSampleEntry[]
}

export interface VowifiSoakRunsResponse {
  runs: VowifiSoakRunEntry[]
  total: number
  read_only: boolean
}

export interface VowifiEsimRestoreEntry {
  switch_token?: string | null
  switch_phase?: string | null
  phase_ms?: number | null
  identity_ready: boolean
  sim_auth_ready: boolean
  degraded_reason?: string | null
  retry_count: number
  updated_at: string
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
