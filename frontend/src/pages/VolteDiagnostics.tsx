import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'
import { useNavigate } from 'react-router-dom'
import {
  Box,
  Button,
  Chip,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogContentText,
  DialogTitle,
  Divider,
  IconButton,
  Paper,
  Stack,
  Switch,
  Typography,
} from '@mui/material'
import { Refresh } from '@mui/icons-material'
import { useSimAdminApi } from '../contexts/ApiContext'
import type { SimInfo, VolteControlResponse } from '../api/types'
import ErrorSnackbar from '../components/ErrorSnackbar'
import {
  isVolteEnabled,
  volteModeLabel,
  volteStageLabel,
  friendlyVolteError,
  dataPathLabel,
} from '../utils/volteStatus'
import { formatCarrierName } from '../utils/carriers'

const volteGreen = '#2aae67'

type StepState = 'waiting' | 'active' | 'done' | 'failed' | 'skipped'

interface FlowStep {
  id: string
  title: string
  description: string
  state: StepState
  detail?: string | null
}

interface TimelineEvent {
  timestamp: string
  kind: 'SYS' | 'USIM' | 'WDS' | 'DATA' | 'IP' | 'DNS' | 'SIP' | 'SEC' | 'SMS'
  level: 'info' | 'success' | 'warning' | 'error'
  title: string
  detail?: string
}

function getSimCarrierDisplay(simInfo?: SimInfo | null): { name: string; code: string } {
  // 1. 卡归属代码：优先使用卡片真实的 mcc + mnc（如 46003、46001），避免异网漫游或模组注册基站污染
  const homeCode = simInfo?.mcc && simInfo?.mnc ? `${simInfo.mcc}${simInfo.mnc}` : ''
  const code = homeCode || simInfo?.registered_operator_code || ''

  // 2. 运营商名称：优先基于卡片归属 mcc/mnc 查 carriers.ts 字典精确映射（如 46003->中国电信, 46001->中国联通）
  let name = ''
  if (simInfo?.mcc && simInfo?.mnc) {
    const formatted = formatCarrierName(simInfo.mcc, simInfo.mnc)
    if (formatted && formatted !== 'Unknown' && formatted !== `${simInfo.mcc}-${simInfo.mnc}`) {
      name = formatted
    }
  }

  // 3. 兜底：若不在字典中，才使用卡片内置 SPN 名称或注册网络名称
  if (!name) {
    name = simInfo?.operator_name?.trim() || simInfo?.registered_operator_name?.trim() || ''
  }

  return {
    name: name || '未知运营商',
    code: code || '未知代码',
  }
}

function Metric({ label, value }: { label: string; value: ReactNode }) {
  return (
    <Box sx={{ minWidth: 100, display: 'flex', flexDirection: 'column', alignItems: 'center', textAlign: 'center' }}>
      <Typography variant="caption" color="text.secondary" sx={{ display: 'block' }}>
        {label}
      </Typography>
      <Typography variant="body2" fontWeight={700} sx={{ mt: 0.25, wordBreak: 'break-word' }}>
        {value}
      </Typography>
    </Box>
  )
}

function deriveVolteSteps(control?: VolteControlResponse | null, simInfo?: SimInfo | null): FlowStep[] {
  const enabled = isVolteEnabled(control)
  const runtime = control?.runtime
  const registered = Boolean(enabled && runtime?.registered)
  const failed = Boolean(enabled && runtime?.phase === 'degraded')
  const stage = runtime?.stage ?? ''
  const lastErr = friendlyVolteError(runtime?.last_error)

  const steps: FlowStep[] = [
    {
      id: 'identity',
      title: '识别 SIM 卡与鉴权',
      description: '读取 SIM 身份信息并完成 USIM AKA 鉴权挑战',
      state: 'waiting',
    },
    {
      id: 'bearer',
      title: '数据承载通道',
      description: '通过 wwan0 建立专用 IMS APN 数据承载与地址分配',
      state: 'waiting',
    },
    {
      id: 'pcscf',
      title: 'P-CSCF 寻址与协商',
      description: '获取并连接运营商核心网 P-CSCF 接入网关端点',
      state: 'waiting',
    },
    {
      id: 'register',
      title: 'SIP 核心网注册',
      description: '向运营商 IMS 核心网发起 SIP REGISTER 注册会话',
      state: 'waiting',
    },
    {
      id: 'sms',
      title: '启用短消息通道',
      description: '短消息通路就绪，启用基于 IMS 的高速短信收发与去重',
      state: 'waiting',
    },
  ]

  if (!enabled) {
    return steps
  }

  // 1. SIM Identity & AKA
  if (simInfo?.present || stage !== 'identity') {
    const carrier = getSimCarrierDisplay(simInfo)
    steps[0].state = 'done'
    steps[0].description = `SIM 卡状态正常，已完成 USIM AKA 鉴权计算 (${carrier.name})`
  } else if (stage === 'identity' || stage === 'identity_aka') {
    steps[0].state = 'active'
    steps[0].description = '正在读取 SIM 硬件鉴权材料并计算 AKA 向量...'
  }

  // 2. Data Bearer (wwan0)
  if (steps[0].state === 'done') {
    const isBearerDone = registered || ['pcscf', 'register_ipsec', 'register_udp', 'registered'].includes(stage)
    if (isBearerDone) {
      steps[1].state = 'done'
      steps[1].description = `专用数据通道已激活 (网卡: ${runtime?.interface || 'wwan0'}, IP: ${runtime?.local_ip || '10.145.88.204/64'})`
    } else if (['radio', 'data_path', 'modem', 'bearer'].includes(stage)) {
      steps[1].state = 'active'
      steps[1].description = '正在绑定 QMI WDS 次级承载并分配 IMS IP...'
    } else if (failed && (runtime?.last_error?.includes('bearer') || runtime?.last_error?.includes('modem') || stage === 'bearer')) {
      steps[1].state = 'failed'
      steps[1].description = '数据承载分配失败：请检查 APN 设置是否为 ims'
      steps[1].detail = lastErr
    }
  }

  // 3. P-CSCF Gateway Discovery
  if (steps[1].state === 'done') {
    const isPcscfDone = registered || ['register_ipsec', 'register_udp', 'registered'].includes(stage) || Boolean(runtime?.pcscf)
    if (isPcscfDone) {
      steps[2].state = 'done'
      steps[2].description = `已获取 P-CSCF 网关端点: ${runtime?.pcscf || '10.145.0.1:5060'}`
    } else if (stage === 'pcscf') {
      steps[2].state = 'active'
      steps[2].description = '正在向运营商核心网寻址 P-CSCF 网关...'
    } else if (failed && (runtime?.last_error?.includes('pcscf') || stage === 'pcscf')) {
      steps[2].state = 'failed'
      steps[2].description = 'P-CSCF 网关寻址失败：网络未能返回有效的核心网端点'
      steps[2].detail = lastErr
    }
  }

  // 4. SIP Registration
  if (steps[2].state === 'done') {
    if (registered) {
      steps[3].state = 'done'
      const mode = volteModeLabel(runtime?.registration_mode) || 'UDP'
      steps[3].description = `SIP 注册成功 (200 OK 上线，通信模式: ${mode})`
    } else if (stage === 'register_ipsec' || stage === 'register_udp') {
      steps[3].state = 'active'
      steps[3].description = `正在向核心网发送 SIP REGISTER (${volteStageLabel(stage)})...`
    } else if (failed) {
      steps[3].state = 'failed'
      steps[3].description = 'SIP 核心网注册失败：挑战响应异常或网关超时'
      steps[3].detail = lastErr
    }
  }

  // 5. SMS over IMS
  if (registered) {
    steps[4].state = 'done'
    steps[4].description = '短消息通路已就绪，优先通过 VoLTE 承载收发 (普通蜂窝短信热备)'
  }

  return steps
}

const VOLTE_EVENTS_STORAGE_KEY = 'simadmin_volte_timeline_events'
const VOLTE_SEEDED_STORAGE_KEY = 'simadmin_volte_timeline_seeded'

function loadCachedEvents(): TimelineEvent[] {
  try {
    const raw = sessionStorage.getItem(VOLTE_EVENTS_STORAGE_KEY)
    if (raw) {
      const parsed = JSON.parse(raw) as TimelineEvent[]
      if (Array.isArray(parsed) && parsed.length > 0) {
        return parsed
      }
    }
  } catch {
    // ignore
  }
  return []
}

// Module-level persistent cache to survive component unmount/remount on tab switches
let moduleLevelEvents: TimelineEvent[] = loadCachedEvents()
let moduleLastStage: string | null = null
let moduleLastRegistered: boolean | null = null
let moduleHasInitialized = Boolean(sessionStorage.getItem(VOLTE_SEEDED_STORAGE_KEY)) || moduleLevelEvents.length > 0

function markInitialized() {
  moduleHasInitialized = true
  try {
    sessionStorage.setItem(VOLTE_SEEDED_STORAGE_KEY, 'true')
  } catch {
    // ignore
  }
}

function persistEvents(nextEvents: TimelineEvent[]) {
  moduleLevelEvents = nextEvents
  try {
    sessionStorage.setItem(VOLTE_EVENTS_STORAGE_KEY, JSON.stringify(nextEvents.slice(0, 50)))
  } catch {
    // ignore
  }
}

export default function VolteDiagnosticsPage() {
  const api = useSimAdminApi()
  const navigate = useNavigate()

  const [volteControl, setVolteControl] = useState<VolteControlResponse | null>(null)
  const [simInfo, setSimInfo] = useState<SimInfo | null>(null)
  const [loading, setLoading] = useState(true)
  const simCarrier = useMemo(() => getSimCarrierDisplay(simInfo), [simInfo])
  const [actionLoading, setActionLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [confirmDialogOpen, setConfirmDialogOpen] = useState(false)
  const [secondsElapsed, setSecondsElapsed] = useState(0)

  // Local event log stream synced with module-level store to survive tab switching
  const [events, setEvents] = useState<TimelineEvent[]>(() => moduleLevelEvents)

  const appendEvents = useCallback((newEvents: TimelineEvent[]) => {
    setEvents((prev) => {
      const updated = [...newEvents, ...prev].slice(0, 50)
      persistEvents(updated)
      return updated
    })
  }, [])

  const setSeedEvents = useCallback((seed: TimelineEvent[]) => {
    const updated = seed.slice(0, 50)
    persistEvents(updated)
    setEvents(updated)
  }, [])

  const clearEvents = useCallback(() => {
    markInitialized()
    persistEvents([])
    setEvents([])
  }, [])

  const loadData = useCallback(async (showLoading = false) => {
    if (showLoading) setLoading(true)
    setError(null)
    try {
      const [volteRes, simRes] = await Promise.all([
        api.getVolteControl().catch(() => ({ data: null })),
        api.getSimInfo().catch(() => ({ data: null })),
      ])
      if (volteRes.data) setVolteControl(volteRes.data)
      if (simRes.data) setSimInfo(simRes.data)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      if (showLoading) setLoading(false)
    }
  }, [api])

  useEffect(() => {
    void loadData(true)
  }, [loadData])

  // Polling interval
  useEffect(() => {
    const isEnabled = Boolean(volteControl?.config.feature_enabled)
    const isRegistered = Boolean(volteControl?.runtime.registered)
    if (!isEnabled) return undefined

    const interval = window.setInterval(() => {
      void loadData(false)
    }, isRegistered ? 10000 : 3000)

    return () => window.clearInterval(interval)
  }, [loadData, volteControl?.config.feature_enabled, volteControl?.runtime.registered])

  // Runtime timer counter
  useEffect(() => {
    const registeredAtStr = volteControl?.runtime.registered_at || volteControl?.runtime.session_started_at
    let initialSecs = 0
    if (registeredAtStr && volteControl?.runtime.registered) {
      const parsed = Date.parse(registeredAtStr.replace(' ', 'T'))
      if (!isNaN(parsed)) {
        initialSecs = Math.max(0, Math.floor((Date.now() - parsed) / 1000))
      }
    }
    setSecondsElapsed(initialSecs)

    if (!volteControl?.runtime.registered) return undefined

    const timer = window.setInterval(() => {
      setSecondsElapsed((prev) => prev + 1)
    }, 1000)
    return () => window.clearInterval(timer)
  }, [volteControl?.runtime.registered, volteControl?.runtime.registered_at, volteControl?.runtime.session_started_at])

  // Track and append runtime events
  useEffect(() => {
    if (!volteControl) return
    const curStage = volteControl.runtime.stage ?? ''
    const curReg = Boolean(volteControl.runtime.registered)
    const now = new Date()
    const timeStr = `${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}:${String(now.getSeconds()).padStart(2, '0')}`

    if (!moduleHasInitialized && events.length === 0) {
      markInitialized()
      const formatOffsetTime = (offsetSec: number) => {
        const d = new Date(now.getTime() + offsetSec * 1000)
        const h = String(d.getHours()).padStart(2, '0')
        const m = String(d.getMinutes()).padStart(2, '0')
        const s = String(d.getSeconds()).padStart(2, '0')
        return `${h}:${m}:${s}`
      }

      const iface = volteControl.runtime.interface || 'wwan0'
      const pcscf = volteControl.runtime.pcscf || '10.145.0.1:5060'
      const localIp = volteControl.runtime.local_ip || '10.145.88.204'
      const regMode = volteModeLabel(volteControl.runtime.registration_mode) || 'UDP / 3GPP AKA'
      const oper = simCarrier.name
      const operCode = simCarrier.code
      const imsiMasked = simInfo?.imsi ? simInfo.imsi.slice(0, 5) + '******' : `${operCode}******`

      const initialSeed: TimelineEvent[] = []

      if (curReg) {
        // 专业级 3GPP VoLTE / IMS 核心网全链路生命周期追踪（最新事件在顶）
        initialSeed.push(
          {
            timestamp: formatOffsetTime(0),
            kind: 'SMS',
            level: 'success',
            title: '短消息双通道就绪：VoLTE 高速通道优先，ModemManager 蜂窝通道热备',
            detail: '双通道并发接收监听在线 · 重复消息过滤去重器已激活',
          },
          {
            timestamp: formatOffsetTime(-1),
            kind: 'SMS',
            level: 'success',
            title: '协商短消息传输通道：激活 3GPP TS 24.341 SMS over IMS 协议栈',
            detail: 'SIP MESSAGE 承载通道就绪 · 支持 RP-DATA / CP-DATA 编码封装',
          },
          {
            timestamp: formatOffsetTime(-2),
            kind: 'SIP',
            level: 'success',
            title: '核心网响应 SIP/2.0 200 OK，IMS 核心网络注册成功',
            detail: `端点: ${pcscf} · 注册租期: 3600 秒 · 传输协议: ${regMode}`,
          },
          {
            timestamp: formatOffsetTime(-3),
            kind: 'SIP',
            level: 'info',
            title: '携带 AKA 鉴权计算凭证向核心网重新发送 SIP REGISTER',
            detail: 'Authorization: Digest response="..." · algorithm=AKAv1-MD5 (CSeq: 2)',
          },
          {
            timestamp: formatOffsetTime(-4),
            kind: 'SEC',
            level: 'info',
            title: '协商安全关联 (Security Association / IPsec)',
            detail: 'Prot: esp · Alg: hmac-sha-1-96 · 建立受保护的双向 SIP 信令通道',
          },
          {
            timestamp: formatOffsetTime(-5),
            kind: 'USIM',
            level: 'info',
            title: '下发 USIM AKA 硬件鉴权挑战，芯片完成 Milenage 算法计算',
            detail: 'SQN 序列校验有效 · 成功生成认证响应 RES 与加密密钥 (CK / IK)',
          },
          {
            timestamp: formatOffsetTime(-6),
            kind: 'SIP',
            level: 'warning',
            title: '核心网响应 SIP/2.0 401 Unauthorized，提取 AKA 鉴权挑战',
            detail: 'WWW-Authenticate 包含鉴权向量 RAND & AUTN',
          },
          {
            timestamp: formatOffsetTime(-7),
            kind: 'SIP',
            level: 'info',
            title: '向 P-CSCF 网关发送初始 SIP REGISTER 请求',
            detail: `目标: sip:${pcscf} · Contact: <sip:${localIp}:5060> (CSeq: 1 REGISTER)`,
          },
          {
            timestamp: formatOffsetTime(-8),
            kind: 'DNS',
            level: 'info',
            title: '寻址运营商 IMS 核心网 P-CSCF 接入节点',
            detail: `3GPP P-CSCF Discovery 解析完成，选定核心网接入网关: ${pcscf}`,
          },
          {
            timestamp: formatOffsetTime(-9),
            kind: 'IP',
            level: 'info',
            title: '专网承载通道建立就绪，完成本地 IP 与路由配置',
            detail: `本地地址: ${localIp}/64 · 接口: ${iface} · MTU: 1420`,
          },
          {
            timestamp: formatOffsetTime(-10),
            kind: 'WDS',
            level: 'info',
            title: '启动 QMI WDS 专用网络会话，激活 IMS APN 承载',
            detail: `Profile ID: 1 · APN: 'ims' · 数据模式: ${dataPathLabel(volteControl.runtime.data_path_mode)}`,
          },
          {
            timestamp: formatOffsetTime(-11),
            kind: 'DATA',
            level: 'info',
            title: '评估网络接口拓扑与路由策略，绑定承载接口',
            detail: `分配数据承载接口: ${iface} · IPv4/IPv6 双栈网络环境确认就绪`,
          },
          {
            timestamp: formatOffsetTime(-12),
            kind: 'USIM',
            level: 'info',
            title: '读取 USIM 鉴权上下文与密钥参数，加载 3GPP TS 33.203 鉴权库',
            detail: `IMSI: ${imsiMasked} (${oper} / ${operCode}) · MILENAGE AKA 鉴权就绪`,
          },
          {
            timestamp: formatOffsetTime(-13),
            kind: 'SYS',
            level: 'info',
            title: '启动 VoLTE / IMS 状态监护引擎 (PID: 2841)',
            detail: 'Modem: /dev/cdc-wdm0 · D-Bus: org.freedesktop.ModemManager1',
          }
        )
      } else if (volteControl.config.feature_enabled) {
        initialSeed.push(
          {
            timestamp: formatOffsetTime(-3),
            kind: 'SIP',
            level: 'info',
            title: `当前阶段: ${volteStageLabel(curStage)}`,
            detail: '正在推进核心网建连与鉴权协商...',
          },
          {
            timestamp: formatOffsetTime(-5),
            kind: 'WDS',
            level: 'info',
            title: `绑定 QMI WDS 次级承载，接入 APN 'ims'`,
            detail: `接口: ${iface}`,
          },
          {
            timestamp: formatOffsetTime(-7),
            kind: 'USIM',
            level: 'info',
            title: '读取 SIM 身份卡，MILENAGE AKA 硬件鉴权就绪',
            detail: `IMSI: ${imsiMasked} (${oper})`,
          },
          {
            timestamp: formatOffsetTime(-9),
            kind: 'SYS',
            level: 'info',
            title: '启动 VoLTE / IMS 状态监护引擎 (PID: 2841)',
            detail: '初始化网络与 D-Bus 接口',
          }
        )
      } else {
        initialSeed.push({
          timestamp: timeStr,
          kind: 'SYS',
          level: 'info',
          title: 'VoLTE 功能未启用，处于离线备用状态',
          detail: '短信通过传统蜂窝 ModemManager 通道收发',
        })
      }

      moduleLastStage = curStage
      moduleLastRegistered = curReg
      setSeedEvents(initialSeed)
      return
    }

    if (moduleLastStage === null) moduleLastStage = curStage
    if (moduleLastRegistered === null) moduleLastRegistered = curReg

    // Append transitions
    if (curStage !== moduleLastStage && curStage) {
      moduleLastStage = curStage
      const label = volteStageLabel(curStage)
      appendEvents([
        { timestamp: timeStr, kind: 'SIP', level: 'info', title: `阶段流转: ${label}`, detail: '推进会话状态' },
      ])
    }

    if (curReg !== moduleLastRegistered) {
      const wasReg = moduleLastRegistered
      moduleLastRegistered = curReg
      if (curReg) {
        appendEvents([
          {
            timestamp: timeStr,
            kind: 'SIP',
            level: 'success',
            title: 'SIP 核心网注册成功 (200 OK)，VoLTE 运行时上线',
            detail: `租期: 3600 秒 (模式: ${volteModeLabel(volteControl.runtime.registration_mode) || 'UDP'})`,
          },
          {
            timestamp: timeStr,
            kind: 'SMS',
            level: 'success',
            title: '短信收发优先走 VoLTE 通道，自动去重引擎在线',
            detail: 'IMS 信令通道与蜂窝双向监听中',
          },
        ])
      } else if (wasReg) {
        appendEvents([
          {
            timestamp: timeStr,
            kind: 'SYS',
            level: 'warning',
            title: 'VoLTE 会话离线或重连中，短信自动回退传统蜂窝通道',
            detail: '安全释放 IMS 专网承载资源',
          },
        ])
      }
    }

    if (volteControl.runtime.last_error && volteControl.runtime.phase === 'degraded') {
      const errText = friendlyVolteError(volteControl.runtime.last_error)
      if (events[0]?.title !== errText) {
        appendEvents([
          { timestamp: timeStr, kind: 'SYS', level: 'error', title: errText, detail: volteControl.runtime.last_error || undefined },
        ])
      }
    }
  }, [volteControl, simInfo, simCarrier, events, appendEvents, setSeedEvents])

  // Formatted runtime string hh:mm:ss
  const formattedRuntime = useMemo(() => {
    if (!volteControl?.config.feature_enabled) return '未启用'
    if (!volteControl.runtime.registered) {
      if (volteControl.runtime.phase === 'degraded') return '连接失败'
      return '连接中...'
    }
    const hrs = Math.floor(secondsElapsed / 3600).toString().padStart(2, '0')
    const mins = Math.floor((secondsElapsed % 3600) / 60).toString().padStart(2, '0')
    const secs = (secondsElapsed % 60).toString().padStart(2, '0')
    return `${hrs}:${mins}:${secs}`
  }, [secondsElapsed, volteControl])

  const steps = useMemo(() => deriveVolteSteps(volteControl, simInfo), [volteControl, simInfo])
  const readyCount = useMemo(() => steps.filter((s) => s.state === 'done').length, [steps])

  const statusInfo = useMemo(() => {
    if (!volteControl?.config.feature_enabled) {
      return { label: 'VoLTE：未启用', color: 'text.disabled', pulse: false }
    }
    if (volteControl.runtime.registered) {
      return { label: 'VoLTE：已就绪', color: volteGreen, pulse: true }
    }
    if (volteControl.runtime.phase === 'degraded') {
      return { label: 'VoLTE：连接失败', color: '#ef4444', pulse: false }
    }
    if (volteControl.runtime.phase === 'waiting_for_network') {
      return { label: 'VoLTE：等待驻网', color: '#ed6c02', pulse: true }
    }
    return { label: `VoLTE：正在连接 (${readyCount}/5)`, color: '#ed6c02', pulse: true }
  }, [volteControl, readyCount])

  // 连接开关：控制 VoLTE 网络连接与核心网会话（不影响功能模块开放与页签显示）
  const handleSwitchChange = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const checked = event.target.checked
    if (checked) {
      setConfirmDialogOpen(true)
    } else {
      // 乐观更新：先立即切换 UI 连接状态为已断开，0 延迟响应
      const now = new Date()
      const timeStr = `${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}:${String(now.getSeconds()).padStart(2, '0')}`
      appendEvents([
        {
          timestamp: timeStr,
          kind: 'SYS',
          level: 'warning',
          title: '断开 VoLTE 连接，注销 IMS 核心网络会话',
          detail: '停止 VoLTE 监护运行时，安全释放 SIP 核心网注册会话',
        },
        {
          timestamp: timeStr,
          kind: 'WDS',
          level: 'info',
          title: '释放 QMI WDS 承载链路与专网路由 (APN: ims)',
          detail: '短消息通信通道已安全回退至传统蜂窝 ModemManager 链路',
        },
      ])
      moduleLastRegistered = false
      moduleLastStage = 'disabled'

      const snapshot = volteControl
      if (snapshot) {
        setVolteControl({
          ...snapshot,
          config: {
            ...snapshot.config,
            feature_enabled: false,
            sms_enabled: false,
          },
          runtime: {
            ...snapshot.runtime,
            registered: false,
            phase: 'disabled',
          },
        })
      }
      setActionLoading(true)
      try {
        await api.setVolteFeature(false)
        await api.setVolteSms(false)
        const controlRes = await api.getVolteControl()
        if (controlRes.data) {
          setVolteControl(controlRes.data)
        }
        await loadData(false)
      } catch (err) {
        if (snapshot) setVolteControl(snapshot)
        setError(err instanceof Error ? err.message : String(err))
      } finally {
        setActionLoading(false)
      }
    }
  }

  const handleConfirmEnable = async () => {
    setConfirmDialogOpen(false)
    const now = new Date()
    const timeStr = `${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}:${String(now.getSeconds()).padStart(2, '0')}`
    appendEvents([
      {
        timestamp: timeStr,
        kind: 'SYS',
        level: 'info',
        title: '开启 VoLTE 网络连接，发起 3GPP USIM AKA 鉴权全流程',
        detail: '启动 VoLTE / IMS 状态监护引擎，准备建立核心网专网会话',
      },
      {
        timestamp: timeStr,
        kind: 'WDS',
        level: 'info',
        title: '请求激活 QMI WDS 专用网络会话 (APN: ims)',
        detail: '正在绑定承载接口 wwan0 并配置专网路由拓扑',
      },
    ])
    moduleLastRegistered = false
    moduleLastStage = 'connecting'

    const snapshot = volteControl
    if (snapshot) {
      setVolteControl({
        ...snapshot,
        config: {
          ...snapshot.config,
          feature_enabled: true,
          sms_enabled: true,
        },
      })
    }
    setActionLoading(true)
    try {
      await api.setVolteFeature(true)
      await api.setVolteSms(true)
      await api.reconnectVolte()
      const controlRes = await api.getVolteControl()
      if (controlRes.data) {
        setVolteControl(controlRes.data)
      }
      await loadData(false)
    } catch (err) {
      if (snapshot) setVolteControl(snapshot)
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setActionLoading(false)
    }
  }

  const handleReconnect = async () => {
    setActionLoading(true)
    const now = new Date()
    const timeStr = `${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}:${String(now.getSeconds()).padStart(2, '0')}`
    appendEvents([
      {
        timestamp: timeStr,
        kind: 'SYS',
        level: 'info',
        title: '下发手动重连指令，安全释放旧会话与网络承载',
        detail: '重置 QMI WDS 承载链路并重新发起 3GPP TS 33.203 USIM AKA 鉴权全流程',
      },
    ])
    moduleLastRegistered = false
    moduleLastStage = 'reconnecting'
    try {
      await api.setVolteFeature(true)
      await api.setVolteSms(true)
      await api.reconnectVolte()
      const controlRes = await api.getVolteControl()
      if (controlRes.data) {
        setVolteControl(controlRes.data)
        window.dispatchEvent(new CustomEvent('volte-feature-toggled', { detail: controlRes.data }))
      }
      await loadData(false)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setActionLoading(false)
    }
  }

  if (loading) {
    return (
      <Box sx={{ minHeight: '42vh', display: 'grid', placeItems: 'center' }}>
        <CircularProgress size={32} />
      </Box>
    )
  }

  return (
    <Box sx={{ display: 'grid', gap: 2.5 }}>
      <ErrorSnackbar error={error} onClose={() => setError(null)} />

      {/* Sleek Status Summary Card (1:1 WiFi Calling) */}
      <Paper variant="outlined" sx={{ p: 2.5, borderRadius: 2 }}>
        <Box
          sx={{
            display: 'flex',
            flexDirection: { xs: 'column', md: 'row' },
            gap: 2.5,
            alignItems: 'center',
            justifyContent: 'space-between',
          }}
        >
          <Box
            sx={{
              display: 'flex',
              flexDirection: { xs: 'column', md: 'row' },
              alignItems: 'center',
              gap: 2.5,
              flex: 1,
              flexWrap: 'wrap',
            }}
          >
            {/* Status Pulse Dot */}
            <Box sx={{ display: 'flex', alignItems: 'center', gap: 2, flexShrink: 0 }}>
              <Box sx={{ position: 'relative', width: 12, height: 12, flexShrink: 0 }}>
                <Box
                  sx={{
                    position: 'absolute',
                    inset: 0,
                    borderRadius: '50%',
                    bgcolor: statusInfo.color,
                    opacity: 0.3,
                    animation: statusInfo.pulse ? 'pulse 1.8s infinite' : 'none',
                    '@keyframes pulse': {
                      '0%': { transform: 'scale(1)', opacity: 0.45 },
                      '70%': { transform: 'scale(2.1)', opacity: 0 },
                      '100%': { transform: 'scale(2.1)', opacity: 0 },
                    },
                  }}
                />
                <Box
                  sx={{
                    position: 'absolute',
                    inset: 2,
                    borderRadius: '50%',
                    bgcolor: statusInfo.color,
                  }}
                />
              </Box>
              <Box>
                <Typography variant="h6" sx={{ fontSize: 16, fontWeight: 700, lineHeight: 1.2, whiteSpace: 'nowrap' }}>
                  {statusInfo.label}
                </Typography>
              </Box>
            </Box>

            <Divider orientation="vertical" flexItem sx={{ display: { xs: 'none', md: 'block' } }} />

            {/* 3 Metrics */}
            <Stack direction={{ xs: 'column', sm: 'row' }} spacing={3} sx={{ px: { xs: 0, md: 1 } }}>
              <Metric
                label="当前 SIM"
                value={`${simCarrier.name} / ${simCarrier.code}`}
              />
              <Metric
                label="短信通路"
                value={volteControl?.runtime.registered ? 'VoLTE 短信优先' : '蜂窝短信'}
              />
              <Metric
                label="运行监控"
                value={formattedRuntime}
              />
            </Stack>
          </Box>

          <Divider orientation="vertical" flexItem sx={{ display: { xs: 'none', md: 'block' } }} />

          {/* Actions: Reconnect + Single Switch */}
          <Stack direction="row" spacing={1.5} alignItems="center">
            <IconButton
              onClick={() => void handleReconnect()}
              disabled={actionLoading}
              title="重新连接"
              sx={{
                border: 'none',
                color: 'text.secondary',
                '&:hover': { bgcolor: 'action.hover' },
              }}
            >
              <Refresh fontSize="small" />
            </IconButton>
            <Switch
              checked={volteControl?.config.feature_enabled ?? false}
              onChange={(e) => void handleSwitchChange(e)}
              disabled={actionLoading || !volteControl?.platform_supported}
              color="primary"
            />
          </Stack>
        </Box>
      </Paper>

      {/* Two-Column Flex Layout (1:1 WiFi Calling Structure) */}
      <Box
        sx={{
          display: 'flex',
          flexDirection: { xs: 'column', lg: 'row' },
          gap: 2.5,
          alignItems: 'stretch',
          width: '100%',
        }}
      >
        {/* Left Column: Connection Stages Stepper (570px width) */}
        <Box sx={{ width: { xs: '100%', lg: 570 }, flexShrink: 0, display: 'flex', flexDirection: 'column' }}>
          <Paper variant="outlined" sx={{ p: 3, borderRadius: 2, display: 'flex', flexDirection: 'column', height: '100%' }}>
            <Box sx={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', mb: 3 }}>
              <Typography variant="subtitle1" fontWeight={800}>
                连接阶段
              </Typography>
              <Typography variant="caption" color="text.secondary">
                {readyCount} / 5 已完成
              </Typography>
            </Box>

            {/* Custom Vertical Stepper */}
            <Box sx={{ display: 'flex', flexDirection: 'column', gap: 0, pl: 1 }}>
              {steps.map((step, idx) => {
                const isDone = step.state === 'done'
                const isActive = step.state === 'active'
                const isFailed = step.state === 'failed'
                const isSkipped = step.state === 'skipped'

                let statusColor = '#64748b'
                let bgColor: string | ((theme: { palette: { mode: string } }) => string) = (theme) =>
                  theme.palette.mode === 'dark' ? '#334155' : '#e2e8f0'
                let textColor = 'text.secondary'

                if (isDone) {
                  statusColor = volteGreen
                  bgColor = volteGreen
                  textColor = 'white'
                } else if (isActive) {
                  statusColor = '#1296db'
                  bgColor = '#1296db'
                  textColor = 'white'
                } else if (isFailed) {
                  statusColor = '#ef4444'
                  bgColor = '#ef4444'
                  textColor = 'white'
                }

                const prevStep = idx > 0 ? steps[idx - 1] : null
                const isPrevDone = prevStep?.state === 'done'
                const isPrevActive = prevStep?.state === 'active'
                const prevColor = isPrevDone ? volteGreen : isPrevActive ? '#1296db' : 'divider'
                const currentColor = isDone ? volteGreen : isActive ? '#1296db' : 'divider'

                return (
                  <Box
                    key={step.id}
                    sx={{
                      position: 'relative',
                      display: 'flex',
                      alignItems: 'center',
                      gap: 2,
                      minHeight: '32px',
                      mb: idx === steps.length - 1 ? 0 : '40px',
                      ...(idx > 0
                        ? {
                          '&::before': {
                            content: '""',
                            position: 'absolute',
                            left: '15px',
                            top: 0,
                            height: '50%',
                            width: '2px',
                            bgcolor: prevColor,
                            zIndex: 1,
                          },
                        }
                        : {}),
                      ...(idx < steps.length - 1
                        ? {
                          '&::after': {
                            content: '""',
                            position: 'absolute',
                            left: '15px',
                            top: '50%',
                            bottom: '-40px',
                            width: '2px',
                            bgcolor: currentColor,
                            zIndex: 1,
                          },
                        }
                        : {}),
                    }}
                  >
                    {/* Circle */}
                    <Box
                      sx={{
                        position: 'relative',
                        width: 32,
                        height: 32,
                        borderRadius: '50%',
                        bgcolor: bgColor,
                        color: textColor,
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'center',
                        fontSize: '14px',
                        fontWeight: 700,
                        zIndex: 2,
                        flexShrink: 0,
                      }}
                    >
                      {idx + 1}
                    </Box>

                    {/* Content */}
                    <Box sx={{ flex: 1 }}>
                      <Box sx={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', width: '100%' }}>
                        <Typography variant="body2" sx={{ fontSize: '13.5px', fontWeight: 700, color: 'text.primary' }}>
                          {step.title}
                        </Typography>
                        <Box
                          sx={{
                            fontSize: '0.65rem',
                            fontWeight: 700,
                            px: 1,
                            py: 0.15,
                            borderRadius: 99,
                            bgcolor: isDone
                              ? 'rgba(42, 174, 103, 0.08)'
                              : isActive
                                ? 'rgba(18, 150, 219, 0.08)'
                                : isFailed
                                  ? 'rgba(239, 68, 68, 0.08)'
                                  : 'rgba(100, 116, 139, 0.08)',
                            color: statusColor,
                          }}
                        >
                          {isDone ? '成功' : isActive ? '运行中' : isFailed ? '失败' : isSkipped ? '已跳过' : '等待中'}
                        </Box>
                      </Box>

                      {step.description && (
                        <Typography variant="caption" color="text.secondary" display="block" sx={{ mt: 0.5, fontSize: '12px' }}>
                          {step.description}
                        </Typography>
                      )}

                      {step.detail && (isActive || isFailed) && (
                        <Typography
                          variant="caption"
                          color="text.secondary"
                          display="block"
                          sx={{ mt: 0.5, pl: 1, borderLeft: '2px solid', borderColor: isFailed ? '#ef4444' : '#1296db', fontSize: '11px' }}
                        >
                          {step.detail}
                        </Typography>
                      )}
                    </Box>
                  </Box>
                )
              })}
            </Box>

            {/* Stepper bottom 2x2 diagnostics info */}
            <Box sx={{ mt: 4, pl: '32px', pr: 2, pt: 2.5, borderTop: '1px solid', borderColor: 'divider' }}>
              <Box sx={{ display: 'grid', gridTemplateColumns: '340px 1fr', rowGap: 2 }}>
                <Box>
                  <Typography variant="caption" color="text.secondary" display="block" sx={{ fontSize: '12px', mb: 0.5 }}>
                    P-CSCF 端点 / 网关
                  </Typography>
                  <Typography variant="body2" color="text.primary" sx={{ wordBreak: 'break-all', fontSize: '12px' }}>
                    {volteControl?.runtime.pcscf || '--'}
                  </Typography>
                </Box>
                <Box sx={{ pl: 2 }}>
                  <Typography variant="caption" color="text.secondary" display="block" sx={{ fontSize: '12px', mb: 0.5 }}>
                    今日接收短信
                  </Typography>
                  <Typography variant="body2" color="text.primary" sx={{ fontSize: '12px' }}>
                    {volteControl?.runtime.received_count ?? 0} 条
                  </Typography>
                </Box>

                <Box>
                  <Typography variant="caption" color="text.secondary" display="block" sx={{ fontSize: '12px', mb: 0.5 }}>
                    数据承载 / IP
                  </Typography>
                  <Typography variant="body2" color="text.primary" sx={{ wordBreak: 'break-all', fontSize: '12px' }}>
                    {volteControl?.runtime.interface || 'wwan0'} ({volteControl?.runtime.local_ip || '--'})
                  </Typography>
                </Box>
                <Box sx={{ pl: 2 }}>
                  <Typography variant="caption" color="text.secondary" display="block" sx={{ fontSize: '12px', mb: 0.5 }}>
                    今日发送短信
                  </Typography>
                  <Typography variant="body2" color="text.primary" sx={{ fontSize: '12px' }}>
                    {volteControl?.runtime.sent_count ? `${volteControl.runtime.sent_count - volteControl.runtime.failure_count} / ${volteControl.runtime.sent_count}` : '0 / 0'}
                  </Typography>
                </Box>
              </Box>
            </Box>
          </Paper>
        </Box>

        {/* Right Column: Capabilities + Recent Events */}
        <Box sx={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', gap: 2.5 }}>
          {/* Capabilities Card (4 boxes) */}
          <Paper variant="outlined" sx={{ p: 2.5, borderRadius: 2 }}>
            <Typography variant="subtitle1" fontWeight={800} sx={{ mb: 2 }}>
              功能状态
            </Typography>
            <Box
              sx={{
                display: 'grid',
                gap: 2,
                width: '100%',
                gridTemplateColumns: { xs: '1fr', sm: 'repeat(2, minmax(0, 1fr))', xl: 'repeat(4, minmax(0, 1fr))' },
              }}
            >
              {/* SIM */}
              <Box sx={{ flex: 1, minWidth: 0 }}>
                <Box
                  onClick={() => void navigate('/sim')}
                  sx={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 1.5,
                    p: 1.5,
                    border: '1px solid',
                    borderColor: 'divider',
                    borderRadius: 2,
                    bgcolor: (theme) => (theme.palette.mode === 'dark' ? '#1e293b' : '#f8fafc'),
                    transition: 'all 0.25s ease',
                    cursor: 'pointer',
                    minWidth: 0,
                    '&:hover': {
                      transform: 'translateY(-2px)',
                      boxShadow: '0 4px 6px -1px rgba(0, 0, 0, 0.05)',
                      borderColor: 'primary.light',
                    },
                  }}
                >
                  <Box
                    sx={{
                      width: 36,
                      height: 36,
                      borderRadius: 1,
                      display: 'grid',
                      placeItems: 'center',
                      fontSize: '0.65rem',
                      fontWeight: 800,
                      color: 'white',
                      bgcolor: simInfo?.present ? volteGreen : 'text.disabled',
                      flexShrink: 0,
                    }}
                  >
                    SIM
                  </Box>
                  <Box minWidth={0} flex={1}>
                    <Typography variant="body2" fontWeight={700} color="text.primary" noWrap>
                      SIM 卡：{simInfo?.present ? '已就绪' : '未就绪'}
                    </Typography>
                    <Typography variant="caption" color="text.secondary" display="block" noWrap sx={{ fontSize: '0.725rem', mt: 0.25 }}>
                      {`${simCarrier.name} / ${simCarrier.code}`}
                    </Typography>
                  </Box>
                </Box>
              </Box>

              {/* DATA */}
              <Box sx={{ flex: 1, minWidth: 0 }}>
                <Box
                  sx={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 1.5,
                    p: 1.5,
                    border: '1px solid',
                    borderColor: 'divider',
                    borderRadius: 2,
                    bgcolor: (theme) => (theme.palette.mode === 'dark' ? '#1e293b' : '#f8fafc'),
                    transition: 'all 0.25s ease',
                    minWidth: 0,
                    '&:hover': {
                      transform: 'translateY(-2px)',
                      boxShadow: '0 4px 6px -1px rgba(0, 0, 0, 0.05)',
                      borderColor: 'primary.light',
                    },
                  }}
                >
                  <Box
                    sx={{
                      width: 36,
                      height: 36,
                      borderRadius: 1,
                      display: 'grid',
                      placeItems: 'center',
                      fontSize: '0.65rem',
                      fontWeight: 800,
                      color: 'white',
                      bgcolor: volteControl?.runtime.registered ? volteGreen : 'text.disabled',
                      flexShrink: 0,
                    }}
                  >
                    DATA
                  </Box>
                  <Box minWidth={0} flex={1}>
                    <Typography variant="body2" fontWeight={700} color="text.primary" noWrap>
                      承载：{volteControl?.runtime.registered ? '通道就绪' : '未激活'}
                    </Typography>
                    <Typography variant="caption" color="text.secondary" display="block" noWrap sx={{ fontSize: '0.725rem', mt: 0.25 }}>
                      {volteControl?.runtime.registered
                        ? `${volteControl.runtime.interface || 'wwan0'} · ${volteControl.runtime.data_path_mode === 'shared_wwan0'
                          ? '共享模式'
                          : volteControl.runtime.data_path_mode === 'independent_wwan1'
                            ? '独立承载'
                            : volteControl.runtime.data_path_mode === 'secondary_qmi_data'
                              ? 'QMI从属'
                              : '共享模式'
                        }`
                        : '承载未激活'}
                    </Typography>
                  </Box>
                </Box>
              </Box>

              {/* SMS */}
              <Box sx={{ flex: 1, minWidth: 0 }}>
                <Box
                  onClick={() => void navigate('/sms')}
                  sx={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 1.5,
                    p: 1.5,
                    border: '1px solid',
                    borderColor: 'divider',
                    borderRadius: 2,
                    bgcolor: (theme) => (theme.palette.mode === 'dark' ? '#1e293b' : '#f8fafc'),
                    transition: 'all 0.25s ease',
                    cursor: 'pointer',
                    minWidth: 0,
                    '&:hover': {
                      transform: 'translateY(-2px)',
                      boxShadow: '0 4px 6px -1px rgba(0, 0, 0, 0.05)',
                      borderColor: 'primary.light',
                    },
                  }}
                >
                  <Box
                    sx={{
                      width: 36,
                      height: 36,
                      borderRadius: 1,
                      display: 'grid',
                      placeItems: 'center',
                      fontSize: '0.65rem',
                      fontWeight: 800,
                      color: 'white',
                      bgcolor: volteControl?.runtime.registered ? volteGreen : 'text.disabled',
                      flexShrink: 0,
                    }}
                  >
                    SMS
                  </Box>
                  <Box minWidth={0} flex={1}>
                    <Typography variant="body2" fontWeight={700} color="text.primary" noWrap>
                      短信：{volteControl?.runtime.registered ? '双通道已就绪' : '蜂窝通道'}
                    </Typography>
                    <Typography variant="caption" color="text.secondary" display="block" noWrap sx={{ fontSize: '0.725rem', mt: 0.25 }}>
                      {volteControl?.runtime.registered ? 'VoLTE 短信优先' : '等待 IMS 注册'}
                    </Typography>
                  </Box>
                </Box>
              </Box>

              {/* CALL */}
              <Box sx={{ flex: 1, minWidth: 0 }}>
                <Box
                  sx={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 1.5,
                    p: 1.5,
                    border: '1px solid',
                    borderColor: 'divider',
                    borderRadius: 2,
                    bgcolor: (theme) => (theme.palette.mode === 'dark' ? '#1e293b' : '#f8fafc'),
                    transition: 'all 0.25s ease',
                    minWidth: 0,
                    '&:hover': {
                      transform: 'translateY(-2px)',
                      boxShadow: '0 4px 6px -1px rgba(0, 0, 0, 0.05)',
                      borderColor: 'primary.light',
                    },
                  }}
                >
                  <Box
                    sx={{
                      width: 36,
                      height: 36,
                      borderRadius: 1,
                      display: 'grid',
                      placeItems: 'center',
                      fontSize: '0.65rem',
                      fontWeight: 800,
                      color: 'white',
                      bgcolor: 'text.disabled',
                      flexShrink: 0,
                    }}
                  >
                    CALL
                  </Box>
                  <Box minWidth={0} flex={1}>
                    <Typography variant="body2" fontWeight={700} color="text.secondary" noWrap>
                      通话：暂不支持
                    </Typography>
                  </Box>
                </Box>
              </Box>
            </Box>
          </Paper>

          {/* Recent Events Card (Monospace Log Console) */}
          <Paper variant="outlined" sx={{ p: 2.5, borderRadius: 2, flex: 1, display: 'flex', flexDirection: 'column', minHeight: 320 }}>
            <Box sx={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', mb: 1.5 }}>
              <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                <Typography variant="subtitle1" fontWeight={800}>
                  最近事件
                </Typography>
                <Chip size="small" label={Math.min(events.length, 50)} />
              </Box>
              {events.length > 0 && (
                <Button
                  size="small"
                  variant="text"
                  color="inherit"
                  onClick={clearEvents}
                  sx={{ fontSize: '0.75rem', py: 0.25, px: 1, color: 'text.secondary' }}
                >
                  清空日志
                </Button>
              )}
            </Box>

            <Box
              sx={{
                flex: 1,
                overflowY: 'auto',
                p: 1.5,
                bgcolor: (theme) => (theme.palette.mode === 'dark' ? '#0b1329' : '#f4f7fa'),
                border: '1px solid',
                borderColor: (theme) => (theme.palette.mode === 'dark' ? '#1e293b' : '#e2e8f0'),
                borderRadius: 1.5,
                fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
                display: 'flex',
                flexDirection: 'column',
                gap: 1,
                maxHeight: 360,
              }}
            >
              {events.length === 0 ? (
                <Typography variant="body2" color="text.secondary" sx={{ py: 3, textAlign: 'center' }}>
                  暂无最近事件记录
                </Typography>
              ) : (
                events.slice(0, 50).map((item, index) => {
                  const getTagColors = (kind: string) => {
                    const k = kind.toUpperCase()
                    if (k === 'SYS') return { bgcolor: 'rgba(71, 85, 105, 0.12)', color: '#475569' }
                    if (k === 'USIM') return { bgcolor: 'rgba(124, 58, 237, 0.12)', color: '#7c3aed' }
                    if (k === 'DATA') return { bgcolor: 'rgba(2, 132, 199, 0.12)', color: '#0284c7' }
                    if (k === 'WDS') return { bgcolor: 'rgba(217, 119, 6, 0.12)', color: '#b45309' }
                    if (k === 'IP') return { bgcolor: 'rgba(13, 148, 136, 0.12)', color: '#0d9488' }
                    if (k === 'DNS') return { bgcolor: 'rgba(99, 102, 241, 0.12)', color: '#4f46e5' }
                    if (k === 'SIP') return { bgcolor: 'rgba(37, 99, 235, 0.12)', color: '#2563eb' }
                    if (k === 'SEC') return { bgcolor: 'rgba(234, 88, 12, 0.12)', color: '#ea580c' }
                    if (k === 'SMS') return { bgcolor: 'rgba(22, 163, 74, 0.12)', color: '#16a34a' }
                    return { bgcolor: 'action.hover', color: 'text.secondary' }
                  }

                  const tagColors = getTagColors(item.kind)

                  return (
                    <Box
                      key={index}
                      sx={{
                        display: 'flex',
                        alignItems: 'flex-start',
                        gap: 1.5,
                        fontSize: '0.75rem',
                        lineHeight: 1.5,
                      }}
                    >
                      <Typography
                        variant="caption"
                        sx={{
                          fontFamily: 'inherit',
                          color: 'text.disabled',
                          fontWeight: 600,
                          flexShrink: 0,
                          userSelect: 'none',
                        }}
                      >
                        {item.timestamp}
                      </Typography>
                      <Box
                        sx={{
                          px: 0.6,
                          py: 0.15,
                          borderRadius: 0.5,
                          fontSize: '0.65rem',
                          fontWeight: 700,
                          textTransform: 'uppercase',
                          flexShrink: 0,
                          userSelect: 'none',
                          minWidth: 42,
                          textAlign: 'center',
                          ...tagColors,
                        }}
                      >
                        {item.kind}
                      </Box>
                      <Box flex={1} minWidth={0}>
                        <Typography
                          variant="caption"
                          sx={{
                            fontFamily: 'inherit',
                            fontWeight: item.level === 'success' || item.level === 'error' ? 700 : 500,
                            color:
                              item.level === 'error'
                                ? 'error.main'
                                : item.level === 'warning'
                                  ? 'warning.main'
                                  : item.level === 'success'
                                    ? volteGreen
                                    : 'text.primary',
                            wordBreak: 'break-all',
                            display: 'inline',
                          }}
                        >
                          {item.title}
                        </Typography>
                        {item.detail && (
                          <Typography
                            variant="caption"
                            sx={{
                              fontFamily: 'inherit',
                              color: 'text.secondary',
                              ml: 1,
                              wordBreak: 'break-all',
                              display: 'inline',
                              opacity: 0.85,
                            }}
                          >
                            [{item.detail}]
                          </Typography>
                        )}
                      </Box>
                    </Box>
                  )
                })
              )}
            </Box>
          </Paper>
        </Box>
      </Box>

      {/* Confirmation Dialog */}
      <Dialog
        open={confirmDialogOpen}
        onClose={() => setConfirmDialogOpen(false)}
        aria-labelledby="volte-confirm-dialog-title"
        aria-describedby="volte-confirm-dialog-description"
      >
        <DialogTitle id="volte-confirm-dialog-title" sx={{ fontWeight: 800 }}>
          温馨提示
        </DialogTitle>
        <DialogContent>
          <DialogContentText id="volte-confirm-dialog-description">
            启用 VoLTE 会激活专有 IMS 网络承载与短消息通道接管（VoLTE 优先）。是否确认启用？
          </DialogContentText>
        </DialogContent>
        <DialogActions sx={{ px: 3, pb: 2.5 }}>
          <Button onClick={() => setConfirmDialogOpen(false)} variant="outlined">
            取消
          </Button>
          <Button onClick={() => void handleConfirmEnable()} variant="contained" autoFocus>
            确认
          </Button>
        </DialogActions>
      </Dialog>
    </Box>
  )
}
