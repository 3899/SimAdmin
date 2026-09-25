import type { VolteControlResponse, VolteRuntimeStatus } from '@/api/types'

export type VolteDisplayState = 'off' | 'starting' | 'success' | 'failed'
export type VolteStatusColor = 'default' | 'primary' | 'success' | 'warning' | 'error'

export interface VolteDisplay {
  enabled: boolean
  state: VolteDisplayState
  label: string
  color: VolteStatusColor
  phaseLabel: string
  summary: string
}

export interface VolteStep {
  label: string
  status: 'done' | 'active' | 'error' | 'pending'
}

export function isVolteEnabled(control?: VolteControlResponse | null) {
  return Boolean(control?.config.feature_enabled && control?.config.connection_enabled)
}

export function phaseLabel(phase?: string | null) {
  const phases: Record<string, string> = {
    disabled: '未启动',
    waiting_for_network: '等待驻网',
    starting: '准备启动',
    registered: '已注册',
    degraded: '等待恢复',
    stopping: '正在停止',
    suspended: '已暂停',
    unsupported: '平台不支持',
  }
  return phases[phase ?? ''] ?? phase ?? '未知阶段'
}

export function volteStageLabel(stage?: string | null) {
  const stages: Record<string, string> = {
    disabled: '未启动',
    waiting_for_network: '等待驻网',
    starting: '准备启动',
    identity: '读取 USIM',
    identity_aka: '读取鉴权材料',
    radio: '等待 LTE',
    data_path: '分配数据承载',
    pcscf: '发现 P-CSCF',
    modem: '等待 ModemManager',
    bearer: '建立 IMS bearer',
    register_ipsec: 'IMS 注册（IPsec）',
    register_udp: 'IMS 注册（UDP）',
    registered: '双通道接收已就绪',
    stopping: '正在停止',
    suspended: '已暂停',
    unsupported: '平台不支持',
  }
  return stages[stage ?? ''] ?? stage ?? '未知节点'
}

export function volteModeLabel(mode?: string | null) {
  const modes: Record<string, string> = { ipsec: 'IPsec', udp: 'UDP' }
  return modes[(mode ?? '').toLowerCase()] ?? ''
}

function runtimeModeLabel(runtime?: VolteRuntimeStatus | null) {
  const direct = volteModeLabel(runtime?.registration_mode)
  if (direct) return direct
  if (runtime?.stage === 'register_ipsec') return 'IPsec'
  if (runtime?.stage === 'register_udp') return 'UDP'
  return ''
}

function withMode(label: string, mode: string) {
  return mode ? `${label} · ${mode}` : label
}

export function friendlyVolteError(reason?: string | null) {
  if (!reason) return ''
  if (reason.includes('volte_imsi_missing')) return '未读取到 IMSI'
  if (reason.includes('volte_at_')) return 'AT 通道未就绪'
  if (
    reason.includes('volte_runtime_mm_bearer_roaming_forbidden') ||
    (reason.includes('Registered in roaming network') && reason.includes('roaming not allowed'))
  ) {
    return 'IMS bearer 禁止漫游，请开启允许漫游后重试'
  }
  if (reason.includes('PdpAuthFailure') || reason.includes('PDP authentication failure')) {
    return 'IMS PDP 鉴权失败，当前漫游网络可能不支持 VoLTE/IMS'
  }
  if (
    reason.includes('volte_modemmanager_debug_required') ||
    reason.toLowerCase().includes('operation only allowed in debug mode') ||
    reason.includes('Error.Core.Unauthorized')
  ) {
    return 'ModemManager 未启用 debug 模式，请重启 ModemManager 或设备'
  }
  if (reason.includes('volte_modemmanager_service_start_failed')) {
    return 'ModemManager 服务启动失败'
  }
  if (reason.includes('volte_runtime_mm_modem_present_timeout') || reason.includes("couldn't find modem")) {
    return '未检测到可用的 ModemManager modem'
  }
  if (reason.includes('volte_ipsec_xfrm_unsupported')) {
    return '系统缺少 IPsec XFRM 支持（缺少 iproute2 中的 ip xfrm），中国移动等运营商强制要求 IPsec 模式，请安装 iproute2'
  }
  if (
    reason.includes('volte_dependency_missing:ip') ||
    reason.includes('volte_command_spawn_failed:ip:No such file or directory')
  ) {
    return '系统缺少网络配置工具（ip/busybox/ifconfig），请安装 iproute2 或 busybox 后重试'
  }
  if (
    reason.includes('volte_runtime_mm_modem_wait_timeout') ||
    reason.includes('volte_runtime_mm_modem_not_ready') ||
    reason.includes('volte_runtime_mm_modem_present') ||
    reason.includes('volte_command_failed:mmcli') ||
    reason.includes('ModemManager process') ||
    reason.includes("couldn't find modem")
  ) {
    return 'ModemManager 尚未就绪'
  }
  if (reason.includes('volte_runtime_mm_bearer') || reason.includes('volte_runtime_health_bearer')) {
    return 'IMS bearer 未就绪'
  }
  if (reason.includes('volte_aka_res_empty')) return 'USIM AKA 鉴权暂时无响应'
  if (reason.includes('volte_usim_aka_failed') || reason.includes('volte_aka_material_invalid')) {
    return 'USIM AKA 鉴权失败'
  }
  if (reason.includes(':420') || reason.includes('status:420')) {
    return 'IMS 核心网拒绝非加密请求(SIP 420)，该运营商强制要求 IPsec 模式，请确保系统已安装 iproute2 并启用 IPsec'
  }
  if (reason.includes('volte_register')) return 'IMS 注册失败'
  if (reason.includes('volte_ipsec')) return 'IPsec 注册失败'
  if (reason.includes('volte_runtime_udp') || reason.includes('volte_udp')) return 'UDP SIP 通道异常'
  if (reason.includes('volte_pcscf')) return 'P-CSCF 不可用'
  if (reason.includes('volte_command_timeout')) return '系统命令超时'
  if (reason.includes('suspend_timeout')) return 'VoLTE runtime 未能及时释放设备资源'
  return reason
}

export function volteErrorLabel(reason?: string | null) {
  return friendlyVolteError(reason) || null
}

export function volteRuntimePresentation(runtime: VolteRuntimeStatus) {
  if (runtime.registered) return { label: 'IMS 已注册', color: 'success' as VolteStatusColor }
  const states: Record<string, { label: string; color: VolteStatusColor }> = {
    disabled: { label: '未启用', color: 'default' },
    waiting_for_network: { label: '等待驻网', color: 'warning' },
    starting: { label: '注册中', color: 'primary' },
    degraded: { label: '注册异常', color: 'warning' },
    stopping: { label: '正在停止', color: 'default' },
    suspended: { label: '已暂停', color: 'warning' },
    unsupported: { label: '平台不支持', color: 'default' },
  }
  return states[runtime.phase] ?? { label: '未注册', color: 'default' as VolteStatusColor }
}

export function volteDisplay(control?: VolteControlResponse | null): VolteDisplay {
  const enabled = isVolteEnabled(control)
  const runtime = control?.runtime
  const displayPhase = phaseLabel(runtime?.phase)
  const displayStage = volteStageLabel(runtime?.stage)
  const mode = runtimeModeLabel(runtime)
  if (!enabled) {
    return { enabled, state: 'off', label: '未开启', color: 'default', phaseLabel: displayPhase, summary: 'IMS runtime 未启动' }
  }
  if (runtime?.phase === 'waiting_for_network') {
    return {
      enabled,
      state: 'starting',
      label: withMode('等待驻网', mode),
      color: 'warning',
      phaseLabel: '等待驻网',
      summary: '基带搜网中，驻网完成后将自动连接 VoLTE',
    }
  }
  if (runtime?.registered) {
    const sendRoute = control?.config.sms_enabled ? '发送使用 IMS' : '发送使用 ModemManager'
    return {
      enabled,
      state: 'success',
      label: withMode('成功', mode),
      color: 'success',
      phaseLabel: displayPhase,
      summary: `双通道接收已就绪 · ${sendRoute}${runtime.registered_at ? ` · ${runtime.registered_at}` : ''}`,
    }
  }
  if (runtime?.phase === 'degraded') {
    const error = friendlyVolteError(runtime.last_error)
    return {
      enabled,
      state: 'failed',
      label: withMode('失败', mode),
      color: 'error',
      phaseLabel: displayPhase,
      summary: `${error || '启动失败'}${mode ? ` · ${mode}` : ''}${runtime.next_retry_at ? ` · 下次重试 ${runtime.next_retry_at}` : ''}`,
    }
  }
  return {
    enabled,
    state: 'starting',
    label: withMode('启动中', mode),
    color: 'warning',
    phaseLabel: displayPhase,
    summary: `当前节点：${displayStage}${mode ? ` · ${mode}` : ''}`,
  }
}

function errorMatches(runtime: VolteRuntimeStatus | undefined, patterns: string[]) {
  const reason = runtime?.last_error || ''
  return patterns.some((pattern) => reason.includes(pattern))
}

type VolteStepGroup = 'switch' | 'usim' | 'bearer' | 'register' | 'sms'
const stepOrder: Record<VolteStepGroup, number> = { switch: 0, usim: 1, bearer: 2, register: 3, sms: 4 }

function stageGroup(stage?: string | null): VolteStepGroup {
  switch (stage) {
    case 'identity':
    case 'identity_aka':
      return 'usim'
    case 'radio':
    case 'data_path':
    case 'pcscf':
    case 'modem':
    case 'bearer':
      return 'bearer'
    case 'register_ipsec':
    case 'register_udp':
      return 'register'
    case 'registered':
      return 'sms'
    default:
      return 'switch'
  }
}

function statusForStep(
  group: VolteStepGroup,
  enabled: boolean,
  registered: boolean,
  failedGroup: VolteStepGroup | null,
  activeGroup: VolteStepGroup,
): VolteStep['status'] {
  if (!enabled) return 'pending'
  if (registered) return 'done'
  if (failedGroup) {
    if (group === failedGroup) return 'error'
    return stepOrder[group] < stepOrder[failedGroup] ? 'done' : 'pending'
  }
  if (stepOrder[group] < stepOrder[activeGroup]) return 'done'
  if (group === activeGroup) return 'active'
  return 'pending'
}

export function volteSteps(control?: VolteControlResponse | null): VolteStep[] {
  const enabled = isVolteEnabled(control)
  const runtime = control?.runtime
  const failed = enabled && runtime?.phase === 'degraded'
  const registered = enabled && Boolean(runtime?.registered)
  const currentGroup = stageGroup(runtime?.stage)
  const usimError = failed && errorMatches(runtime, ['volte_imsi_missing', 'volte_at_', 'volte_usim_aka', 'volte_aka_'])
  const bearerError = failed && (
    errorMatches(runtime, [
      'volte_runtime_mm_',
      'volte_runtime_health_bearer',
      'volte_pcscf',
      'volte_ims_settings',
      'volte_command_failed:mmcli',
      'ModemManager process',
      "couldn't find modem",
    ]) || currentGroup === 'bearer'
  )
  const failedGroup: VolteStepGroup | null = !failed ? null : usimError ? 'usim' : bearerError ? 'bearer' : 'register'
  const activeGroup: VolteStepGroup = currentGroup === 'switch' ? 'usim' : currentGroup
  return [
    { label: '开关', status: statusForStep('switch', enabled, registered, failedGroup, activeGroup) },
    { label: '读取 USIM', status: statusForStep('usim', enabled, registered, failedGroup, activeGroup) },
    { label: 'IMS bearer', status: statusForStep('bearer', enabled, registered, failedGroup, activeGroup) },
    { label: 'IMS 注册', status: statusForStep('register', enabled, registered, failedGroup, activeGroup) },
    { label: '双通道接收', status: statusForStep('sms', enabled, registered, failedGroup, activeGroup) },
  ]
}

export function dataPathLabel(mode?: string | null) {
  const labels: Record<string, string> = {
    managed_modemmanager: 'ModemManager 独立承载',
    independent_wwan1: '独立 WWAN 承载',
    shared_wwan0: '共享 WWAN0 承载',
    secondary_qmi_data: 'Secondary QMI 承载',
    networkmanager: 'NetworkManager 承载',
  }
  return labels[mode ?? ''] ?? mode ?? '--'
}
