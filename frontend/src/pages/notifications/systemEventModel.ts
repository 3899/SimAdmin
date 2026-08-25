export type SystemEventDefinition = {
  code: string
  label: string
  defaultEnabled: boolean
}

export type SystemEventGroup = {
  key: string
  label: string
  events: SystemEventDefinition[]
}

export const SYSTEM_EVENT_GROUPS: SystemEventGroup[] = [
  {
    key: 'baseband',
    label: '基带',
    events: [
      { code: 'baseband.modem_missing_threshold', label: 'Modem 丢失达到阈值（连续 5 次）', defaultEnabled: true },
      { code: 'baseband.modemmanager_restarted', label: 'watchdog 重启 ModemManager', defaultEnabled: true },
      { code: 'baseband.modemmanager_restart_failed', label: 'ModemManager 重启失败', defaultEnabled: true },
      { code: 'baseband.modem_recovered', label: 'Modem 恢复成功', defaultEnabled: true },
      { code: 'baseband.scan_modems_triggered', label: '触发 mmcli 扫描 Modem（连续 3 次未找到 Modem）', defaultEnabled: false },
    ],
  },
  {
    key: 'device_network',
    label: '设备网络',
    events: [
      { code: 'device_network.wlan_connected', label: 'WLAN 连接', defaultEnabled: false },
      { code: 'device_network.wlan_disconnected', label: 'WLAN 断开', defaultEnabled: false },
      { code: 'device_network.wlan_ssid_changed', label: 'SSID 变化', defaultEnabled: false },
      { code: 'device_network.wlan_connect_failed', label: 'WLAN 连接失败', defaultEnabled: true },
    ],
  },
  {
    key: 'cellular',
    label: '蜂窝网络',
    events: [
      { code: 'cellular.searching_threshold', label: '长时间 searching（连续 4 次）', defaultEnabled: true },
      { code: 'cellular.auto_register_triggered', label: '自动驻网触发（searching 连续 4 次）', defaultEnabled: true },
      { code: 'cellular.radio_cycle_triggered', label: '射频循环触发（searching 8 次/状态卡住 6 次）', defaultEnabled: true },
      { code: 'cellular.activation_failed', label: '拨号/连接激活失败', defaultEnabled: true },
      { code: 'cellular.connection_recovered', label: '蜂窝连接恢复', defaultEnabled: true },
      { code: 'cellular.roaming_allowed_changed', label: '允许漫游开关变化', defaultEnabled: true },
      { code: 'cellular.airplane_mode_changed', label: '飞行模式开关变化', defaultEnabled: true },
      { code: 'cellular.data_enabled_changed', label: '数据开关变化', defaultEnabled: true },
    ],
  },
  {
    key: 'esim',
    label: 'SIM/eSIM',
    events: [
      { code: 'esim.work_mode_changed', label: '工作模式切换', defaultEnabled: true },
      { code: 'esim.lpac_repair_succeeded', label: 'lpac 修复成功', defaultEnabled: false },
      { code: 'esim.lpac_repair_failed', label: 'lpac 修复失败', defaultEnabled: true },
      { code: 'esim.profile_enable_succeeded', label: 'Profile 启用成功', defaultEnabled: false },
      { code: 'esim.profile_enable_failed', label: 'Profile 启用失败', defaultEnabled: true },
      { code: 'esim.profile_deleted', label: 'Profile 删除', defaultEnabled: true },
      { code: 'esim.profile_switch_baseband_recovery_failed', label: 'Profile 切换后基带恢复失败', defaultEnabled: true },
      { code: 'esim.rsp_notification_delivery_failed', label: '运营商 Profile 通知提交失败', defaultEnabled: true },
    ],
  },
  {
    key: 'system_service',
    label: '系统/服务',
    events: [
      { code: 'system_service.system_reboot_requested', label: '用户触发系统重启', defaultEnabled: true },
      { code: 'system_service.simadmin_restart_requested', label: 'SimAdmin 服务重启请求', defaultEnabled: true },
      { code: 'system_service.service_started', label: '服务启动完成', defaultEnabled: false },
      { code: 'system_service.reboot_prep_failed', label: '系统重启预处理失败', defaultEnabled: true },
    ],
  },
  {
    key: 'security',
    label: '安全审计',
    events: [
      { code: 'security.password_changed', label: '修改密码', defaultEnabled: true },
      { code: 'security.password_protection_disabled', label: '关闭密码保护', defaultEnabled: true },
      { code: 'security.policy_changed', label: '安全策略变更', defaultEnabled: true },
      { code: 'security.login_failed_threshold', label: '连续登录失败达到阈值（5 分钟内 5 次）', defaultEnabled: true },
    ],
  },
  {
    key: 'resource',
    label: '资源告警',
    events: [
      { code: 'resource.temperature_high', label: '高温（≥75°C）', defaultEnabled: true },
      { code: 'resource.temperature_recovered', label: '温度恢复（≤65°C 连续 2 次）', defaultEnabled: true },
      { code: 'resource.disk_low', label: '磁盘空间不足（≤10% 或 ≤500MB）', defaultEnabled: true },
      { code: 'resource.disk_recovered', label: '磁盘空间恢复（≥15% 且 ≥2GB）', defaultEnabled: true },
      { code: 'resource.memory_high', label: '内存持续高占用（≥90% 持续 5 分钟）', defaultEnabled: true },
      { code: 'resource.memory_recovered', label: '内存恢复（≤80% 持续 2 分钟）', defaultEnabled: true },
      { code: 'resource.cpu_high', label: 'CPU 持续高负载（≥90% 持续 5 分钟）', defaultEnabled: true },
      { code: 'resource.cpu_recovered', label: 'CPU 负载恢复（≤75% 持续 2 分钟）', defaultEnabled: true },
      { code: 'resource.interface_errors_increased', label: '网络接口错误包增长（连续 3 次）', defaultEnabled: false },
      { code: 'resource.interface_errors_recovered', label: '网络接口错误包恢复（连续 2 次）', defaultEnabled: false },
      { code: 'resource.ipv4_connectivity_failed', label: 'IPv4 连通性失败（连续 3 次）', defaultEnabled: true },
      { code: 'resource.ipv4_connectivity_recovered', label: 'IPv4 连通性恢复（连续 2 次）', defaultEnabled: true },
      { code: 'resource.ipv6_connectivity_failed', label: 'IPv6 连通性失败（连续 3 次）', defaultEnabled: false },
      { code: 'resource.ipv6_connectivity_recovered', label: 'IPv6 连通性恢复（连续 2 次）', defaultEnabled: false },
    ],
  },
]

export const SYSTEM_EVENT_TEMPLATE_VARIABLES = [
  { label: '分类', token: '{{分类}}' },
  { label: '事件', token: '{{事件}}' },
  { label: '等级', token: '{{等级}}' },
  { label: '状态', token: '{{状态}}' },
  { label: '对象', token: '{{对象}}' },
  { label: '消息', token: '{{消息}}' },
  { label: '时间', token: '{{时间}}' },
  { label: '本机号码', token: '{{本机号码}}' },
  { label: '运营商', token: '{{运营商}}' },
]

export const DEFAULT_SYSTEM_EVENT_TEMPLATE = '系统事件通知\n分类: {{分类}}\n事件: {{事件}}\n等级: {{等级}}\n状态: {{状态}}\n对象: {{对象}}\n消息: {{消息}}\n时间: {{时间}}'

export function defaultSystemEventCodes() {
  return SYSTEM_EVENT_GROUPS.flatMap((group) => group.events)
    .filter((event) => event.defaultEnabled)
    .map((event) => event.code)
}

export function systemEventLabel(code: string) {
  return SYSTEM_EVENT_GROUPS
    .flatMap((group) => group.events)
    .find((event) => event.code === code)?.label ?? code
}
