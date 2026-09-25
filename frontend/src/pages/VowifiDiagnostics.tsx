import { useCallback, useEffect, useMemo, useState, useRef, type ReactNode } from 'react'
import { useNavigate } from 'react-router-dom'
import {
  Alert,
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
  // Table,
  // TableBody,
  // TableCell,
  // TableContainer,
  // TableHead,
  // TableRow,
  // TextField,
  Typography,
} from '@mui/material'
import {
  // Public,
  Refresh,
  // VpnKey,
} from '@mui/icons-material'
import { useSimAdminApi } from '../contexts/ApiContext'
import type {
  VowifiCarrierProfile,
  VowifiConfig,
  VowifiDiagnosticsResponse,
  VowifiEsimRestoreEntry,
  VowifiProfileMatchResponse,
  VowifiProfilesResponse,
  VowifiRuntimeEventsResponse,
  VowifiSmsDeliveriesResponse,
  VowifiSoakRunsResponse,
  VowifiReadiness,
  VowifiStatusResponse,
} from '../api/types'
import ErrorSnackbar from '../components/ErrorSnackbar'
import { useRefreshInterval } from '../contexts/RefreshContext'

type LoadState = {
  diagnostics: VowifiDiagnosticsResponse | null
  status: VowifiStatusResponse | null
  profile: VowifiProfileMatchResponse | null
  profiles: VowifiProfilesResponse | null
  events: VowifiRuntimeEventsResponse | null
  smsDeliveries: VowifiSmsDeliveriesResponse | null
  soakRuns: VowifiSoakRunsResponse | null
  restore: VowifiEsimRestoreEntry | null
}

type StepState = 'waiting' | 'active' | 'done' | 'failed' | 'skipped'

type FlowStep = {
  id: string
  title: string
  description: string
  state: StepState
  detail?: string | null
}

const EMPTY_STATE: LoadState = {
  diagnostics: null,
  status: null,
  profile: null,
  profiles: null,
  events: null,
  smsDeliveries: null,
  soakRuns: null,
  restore: null,
}



function currentProfile(state: LoadState): VowifiCarrierProfile | undefined {
  return state.profile?.profile ?? state.status?.profile.profile
}

function currentMatch(state: LoadState): VowifiProfileMatchResponse | null {
  return state.profile ?? state.status?.profile ?? null
}

function currentReadiness(state: LoadState): VowifiReadiness | undefined {
  return state.status?.readiness
}

function currentEpdg(state: LoadState) {
  return state.profile?.epdg ?? state.status?.profile.epdg ?? null
}

// function currentSimAuth(state: LoadState) {
//   return state.profile?.sim_auth ?? state.status?.profile.sim_auth ?? null
// }

// function currentIke(state: LoadState) {
//   return state.profile?.ike ?? state.status?.profile.ike ?? null
// }

// function currentDataplane(state: LoadState) {
//   return state.profile?.dataplane ?? state.status?.profile.dataplane ?? null
// }

function currentIms(state: LoadState) {
  return state.profile?.ims ?? state.status?.profile.ims ?? null
}

// function formatList(values?: { proposal: string }[]) {
//   if (!values || values.length === 0) return 'N/A'
//   return values.map((item) => item.proposal).join(', ')
// }

// function formatMechanisms(values?: { mechanism: string }[]) {
//   if (!values || values.length === 0) return 'N/A'
//   return values.map((item) => item.mechanism).join(', ')
// }

function profileEpdgDomain(state: LoadState, profile?: VowifiCarrierProfile) {
  const epdg = currentEpdg(state)
  if (epdg) return epdg.host
  if (!profile) return 'N/A'
  return `epdg.epc.mnc${profile.mnc.padStart(3, '0')}.mcc${profile.mcc}.pub.3gppnetwork.org`
}

function profileImsDomain(profile?: VowifiCarrierProfile) {
  if (!profile) return 'N/A'
  return `ims.mnc${profile.mnc.padStart(3, '0')}.mcc${profile.mcc}.3gppnetwork.org`
}

const SUB_STEP_TITLES: Record<string, string> = {
  usim_aka: 'USIM 鉴权',
  sim_auth: 'USIM 鉴权',
  epdg_transport: '连接 ePDG 网关',
  epdg: '连接 ePDG 网关',
  ikev2_eap_aka: 'IKEv2 安全协商',
  ike: 'IKEv2 安全协商',
  child_sa: 'IPsec 安全关联协商',
  userspace_esp: '建立 ESP 加密隧道',
  esp: '建立 ESP 加密隧道',
}

function deriveSteps(state: LoadState, connectionEnabled: boolean): FlowStep[] {
  const readiness = currentReadiness(state)
  const match = currentMatch(state)
  const profile = currentProfile(state)
  const phase = state.status?.phase ?? 'not_started'
  const runtimeActive = phase !== 'not_started' && phase !== 'scaffold_only'

  const backendSteps = state.status?.flow?.steps ?? []

  const steps: FlowStep[] = [
    {
      id: 'identity',
      title: '识别 SIM 卡',
      description: '读取 SIM 身份并确定运营商归属',
      state: 'waiting',
    },
    {
      id: 'profile',
      title: '匹配运营商',
      description: '按公开品牌、国家码和 MCC/MNC 匹配内置 Profile',
      state: 'waiting',
    },
    {
      id: 'secure_path',
      title: '连接网关',
      description: '建立安全通道并完成 USIM 鉴权',
      state: 'waiting',
    },
    {
      id: 'ims',
      title: '注册服务',
      description: '向 IMS 核心网发起注册请求',
      state: 'waiting',
    },
    {
      id: 'sms',
      title: '启用能力',
      description: '启用短信通道，接管短信收发能力',
      state: 'waiting',
    },
  ]

  const getBackendStep = (id: string) => backendSteps.find(s => s.id === id)

  // 1. SIM Identification
  const simStep = getBackendStep('identity')
  if (readiness?.identity_ready || (simStep && simStep.state === 'done')) {
    steps[0].state = 'done'
    steps[0].description = '已读取 SIM 状态、运营商归属和短信中心信息'
  } else if (simStep && simStep.state === 'failed') {
    steps[0].state = 'failed'
    steps[0].description = '识别失败：未检测到物理 SIM 卡或 eSIM 未激活。请确认卡片插入状态'
    steps[0].detail = simStep.blocking_reason || undefined
  } else if (simStep && (simStep.state === 'active' || simStep.state === 'ready')) {
    steps[0].state = 'active'
    steps[0].description = '正在读取 SIM 物理卡片信息...'
  } else {
    if (match?.sim.present) {
      steps[0].state = 'done'
      steps[0].description = '已读取 SIM 状态、运营商归属和短信中心信息'
    }
  }

  // 2. Profile Matching
  const profileStep = getBackendStep('profile')
  if (steps[0].state === 'done') {
    if (readiness?.profile_matched || (profileStep && profileStep.state === 'done')) {
      steps[1].state = 'done'
      steps[1].description = `已匹配运营商 ${profile?.brand || '未知'} (${profile ? `${profile.mcc}${profile.mnc}` : 'N/A'})`
    } else if (profileStep && profileStep.state === 'failed') {
      steps[1].state = 'failed'
      steps[1].description = '匹配失败：当前运营商暂不支持'
      steps[1].detail = profileStep.blocking_reason || undefined
    } else if (profileStep && (profileStep.state === 'active' || profileStep.state === 'ready')) {
      steps[1].state = 'active'
      steps[1].description = '正在检索内置运营商配置模板...'
    } else {
      if (match?.matched) {
        steps[1].state = 'done'
        steps[1].description = `已匹配运营商 ${profile?.brand || '未知'} (${profile ? `${profile.mcc}${profile.mnc}` : 'N/A'})`
      }
    }
  }

  // WiFi Calling 未连接或开关关闭时，后续步骤保持等待状态，避免残留
  if (!runtimeActive || !connectionEnabled) {
    return steps
  }

  // 3. Connect Gateway
  const tunnelStepIds = ['usim_aka', 'sim_auth', 'epdg_transport', 'epdg', 'ikev2_eap_aka', 'ike', 'child_sa', 'userspace_esp', 'esp']
  const tunnelSteps = backendSteps.filter(s => tunnelStepIds.includes(s.id))

  if (steps[1].state === 'done') {
    if (readiness?.esp_ready || (tunnelSteps.length > 0 && tunnelSteps.every(s => s.state === 'done' || (s.id === 'userspace_esp' && s.state === 'done')))) {
      steps[2].state = 'done'
      steps[2].description = '已连接运营商 WiFi Calling 网关'
    } else {
      const failedStep = tunnelSteps.find(s => s.state === 'failed')
      if (failedStep) {
        steps[2].state = 'failed'
        if (failedStep.id === 'usim_aka' || failedStep.id === 'sim_auth') {
          steps[2].description = 'SIM 卡鉴权失败。排查建议：请检查 SIM 卡卡槽、运营商套餐状态，或确认是否开启了连接授权'
        } else if (failedStep.id === 'epdg_transport' || failedStep.id === 'epdg') {
          steps[2].description = '网关域名解析或连接失败。排查建议：请检查当前网络是否可用，确认是否开启了对应区域的 VPN 代理，并确保 DNS 未被污染'
        } else if (failedStep.id === 'ikev2_eap_aka' || failedStep.id === 'ike' || failedStep.id === 'child_sa') {
          steps[2].description = '安全通道协商失败。排查建议：请检查上游网络连接，确认是否已开启对应区域的 VPN 代理，并确保 UDP 500/4500 端口未被防火墙或运营商阻断'
        } else {
          steps[2].description = '连接超时：网关握手无响应。排查建议：请检查上游网络与 VPN 代理连接，并确保 UDP 500/4500 端口未被限制'
        }
        steps[2].detail = failedStep.blocking_reason || undefined
      } else {
        const activeStep = tunnelSteps.find(s => s.state === 'active' || s.state === 'ready')
        if (activeStep || runtimeActive) {
          steps[2].state = 'active'
          const subTitle = activeStep ? (SUB_STEP_TITLES[activeStep.id] || activeStep.component.replaceAll('_', ' ').toUpperCase()) : '正在建立通道'
          steps[2].description = `正在建立安全通道 (${subTitle})...`
        }
      }
    }
  }

  // 4. IMS Registration
  const imsStep = getBackendStep('ims')
  if (steps[2].state === 'done') {
    if (readiness?.ims_registered || (imsStep && imsStep.state === 'done')) {
      steps[3].state = 'done'
      steps[3].description = 'WiFi Calling 服务注册成功'
    } else if (imsStep && imsStep.state === 'failed') {
      steps[3].state = 'failed'
      steps[3].description = '注册被拒绝：运营商 IMS 鉴权未通过。排查建议：请确认该卡是否已开通 VoWiFi 业务或套餐是否欠费'
      steps[3].detail = imsStep.blocking_reason || undefined
    } else if (imsStep && (imsStep.state === 'active' || imsStep.state === 'ready')) {
      steps[3].state = 'active'
      steps[3].description = '正在连接 IMS 核心网并注册会话...'
    }
  }

  // 5. SMS over IMS
  const smsStep = getBackendStep('sms')
  if (steps[3].state === 'done') {
    if (readiness?.sms_ready || (smsStep && smsStep.state === 'done')) {
      steps[4].state = 'done'
      steps[4].description = 'WiFi Calling 短信服务已启用，已接管蜂窝短信'
    } else if (smsStep && smsStep.state === 'failed') {
      steps[4].state = 'failed'
      steps[4].description = '激活失败：短信链路映射失败，请尝试点击重新连接'
      steps[4].detail = smsStep.blocking_reason || undefined
    } else if (smsStep && (smsStep.state === 'active' || smsStep.state === 'ready')) {
      steps[4].state = 'active'
      steps[4].description = '正在协商 SMS over IMS 传输接口...'
    }
  }

  // Handle skip logic if any previous step failed
  let hasFailed = false
  for (let i = 0; i < steps.length; i++) {
    if (hasFailed) {
      steps[i].state = 'skipped'
      steps[i].description = '前置步骤失败，已跳过'
    } else if (steps[i].state === 'failed') {
      hasFailed = true
    }
  }

  return steps
}





const vowifiGreen = '#2aae67'

// function ReadinessChip({ label, value }: { label: string; value: boolean }) {
//   return (
//     <Chip
//       label={label}
//       variant={value ? 'filled' : 'outlined'}
//       size="small"
//       sx={{
//         fontWeight: 700,
//         ...(value ? {
//           bgcolor: vowifiGreen,
//           color: '#fff',
//           borderColor: vowifiGreen,
//           '&:hover': {
//             bgcolor: '#2aae67',
//           }
//         } : {})
//       }}
//     />
//   )
// }

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

export default function VowifiDiagnosticsPage() {
  const api = useSimAdminApi()
  const { refreshInterval, refreshKey } = useRefreshInterval()
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [state, setState] = useState<LoadState>(EMPTY_STATE)
  const traceFilter = ''
  const [vowifiControl, setVowifiControl] = useState<VowifiConfig | null>(null)

  const containerRef = useRef<HTMLDivElement | null>(null)
  const [containerHeight, setContainerHeight] = useState<string | number>('calc(100vh - 220px)')

  const updateHeight = useCallback(() => {
    const el = containerRef.current
    if (el) {
      const rect = el.getBoundingClientRect()
      const availableHeight = window.innerHeight - rect.top - 24
      setContainerHeight(Math.max(520, availableHeight))
    }
  }, [])

  useEffect(() => {
    updateHeight()
    window.addEventListener('resize', updateHeight)
    return () => {
      window.removeEventListener('resize', updateHeight)
    }
  }, [updateHeight, loading])

  useEffect(() => {
    updateHeight()
  }, [state, vowifiControl, updateHeight])

  const loadData = useCallback(async (background = false) => {
    if (!background) setLoading(true)
    setError(null)

    try {
      const [diagnosticsRes, profilesRes, controlRes] = await Promise.all([
        api.getVowifiDiagnostics({ limit: 50, traceId: traceFilter }),
        api.getVowifiProfiles(),
        api.getVowifiControl(),
      ])
      const diagnostics = diagnosticsRes.data ?? null

      setState({
        diagnostics,
        status: diagnostics?.status ?? null,
        profile: diagnostics?.status.profile ?? null,
        profiles: profilesRes.data ?? null,
        events: diagnostics?.events ?? null,
        smsDeliveries: diagnostics?.sms_deliveries ?? null,
        soakRuns: diagnostics?.soak_runs ?? null,
        restore: diagnostics?.restore ?? null,
      })
      if (controlRes.data) setVowifiControl(controlRes.data)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [api, traceFilter])

  useEffect(() => {
    void loadData(false)
    if (refreshInterval <= 0) return undefined

    const timer = window.setInterval(() => void loadData(true), refreshInterval)
    return () => window.clearInterval(timer)
  }, [loadData, refreshInterval, refreshKey])

  const readiness = currentReadiness(state)
  const match = currentMatch(state)
  const profile = currentProfile(state)
  // const simAuth = currentSimAuth(state)
  const epdg = currentEpdg(state)
  // const ike = currentIke(state)
  // const dataplane = currentDataplane(state)
  const ims = currentIms(state)
  const steps = useMemo(() => deriveSteps(state, vowifiControl?.connection_enabled ?? false), [state, vowifiControl])
  // const registry = state.profiles?.profiles ?? []
  const diagnostics = state.diagnostics
  // const diagnosticsSummary = diagnostics?.summary
  // const diagnosticsPrivacy = diagnostics?.privacy
  // const m10Audit = diagnostics?.m10_audit
  const timeline = diagnostics?.timeline ?? []
  // const smsDeliveries = state.smsDeliveries?.deliveries ?? []
  // const soakRuns = state.soakRuns?.runs ?? []
  // const restore = state.restore
  // const executor = state.status?.executor
  // const dataplaneDryRun = executor?.dataplane_dry_run
  // const imsRegisterDryRun = executor?.ims_register_dry_run
  // const smsDryRun = executor?.sms_dry_run
  // const restoreDryRun = executor?.esim_restore_dry_run
  // const liveGate = executor?.live_gate
  const readyCount = steps.filter((step) => step.state === 'done').length
  const smsReady = Boolean(readiness?.sms_ready)
  // const switchPhase = restore?.switch_phase ?? state.status?.switch_phase
  // const switchRetry = restore?.retry_count ?? state.status?.switch_retry_count ?? 0

  const navigate = useNavigate()
  const [secondsElapsed, setSecondsElapsed] = useState<number>(0)
  const [confirmDialogOpen, setConfirmDialogOpen] = useState(false)
  const [actionLoading, setActionLoading] = useState(false)
  // const showDevTables = false

  const isConnected = state.status?.phase !== 'not_started' && state.status?.phase !== 'scaffold_only'

  // 计算和更新运行时长 —— 仅在 sms_ready（连接完全成功）时从最近一次连接的启动点开始计时
  useEffect(() => {
    if (!smsReady) {
      setSecondsElapsed(0)
      return undefined
    }

    // 寻找最近一次连接发起（connect_start）的事件作为计时基准
    const latestConnectEvent = state.diagnostics?.timeline?.find(
      (item) => item.phase === 'connect_start'
    )
    const timeline = state.diagnostics?.timeline
    const lastTimestamp = timeline && timeline.length > 0 ? timeline[timeline.length - 1].timestamp : null
    const startedAt = latestConnectEvent?.timestamp 
      ? Date.parse(latestConnectEvent.timestamp.replace(' ', 'T')) 
      : (lastTimestamp 
          ? Date.parse(lastTimestamp.replace(' ', 'T')) 
          : null)

    let initialSeconds = 0
    if (startedAt && !isNaN(startedAt)) {
      initialSeconds = Math.max(0, Math.floor((Date.now() - startedAt) / 1000))
    }
    setSecondsElapsed(initialSeconds)

    const timer = window.setInterval(() => {
      setSecondsElapsed(prev => prev + 1)
    }, 1000)

    return () => window.clearInterval(timer)
  }, [smsReady, state.diagnostics?.timeline])

  // 格式化时间为 hh:mm:ss
  const formattedRuntime = useMemo(() => {
    if (vowifiControl && !vowifiControl.connection_enabled) return '未启用'
    if (!isConnected) return '未启动'
    if (state.status?.phase === 'failed') return '连接失败'
    if (!smsReady) return '连接中...'
    const hrs = Math.floor(secondsElapsed / 3600).toString().padStart(2, '0')
    const mins = Math.floor((secondsElapsed % 3600) / 60).toString().padStart(2, '0')
    const secs = (secondsElapsed % 60).toString().padStart(2, '0')
    return `${hrs}:${mins}:${secs}`
  }, [secondsElapsed, isConnected, smsReady, state.status?.phase, vowifiControl])

  const getStatusIndicator = () => {
    if (!isConnected || (vowifiControl && !vowifiControl.connection_enabled)) {
      return {
        label: 'WiFi Calling：未启用',
        color: 'text.disabled',
        pulse: false,
      }
    }
    const phase = state.status?.phase
    if (phase === 'sms_ready') {
      return {
        label: 'WiFi Calling：已就绪',
        color: vowifiGreen,
        pulse: true,
      }
    }
    if (phase === 'failed') {
      return {
        label: 'WiFi Calling：连接失败',
        color: '#ef4444',
        pulse: false,
      }
    }
    return {
      label: `WiFi Calling：正在连接 (${readyCount}/5)`,
      color: '#ed6c02',
      pulse: true,
    }
  }

  const statusInfo = getStatusIndicator()



  // 今日接收和发送短信统计
  const smsStats = useMemo(() => {
    const deliveries = state.diagnostics?.sms_deliveries?.deliveries ?? []

    // 获取今日北京时间 YYYY-MM-DD
    const tzOffset = 8 * 60; // Beijing offset in minutes
    const beijingTime = new Date(new Date().getTime() + (new Date().getTimezoneOffset() + tzOffset) * 60000);
    const yyyy = beijingTime.getFullYear();
    const mm = String(beijingTime.getMonth() + 1).padStart(2, '0');
    const dd = String(beijingTime.getDate()).padStart(2, '0');
    const todayStr = `${yyyy}-${mm}-${dd}`;

    const todayDeliveries = deliveries.filter(d => d.created_at && d.created_at.startsWith(todayStr))
    const incoming = todayDeliveries.filter(d => {
      const dir = d.direction?.toLowerCase()
      return dir === 'incoming' || dir === 'mobile_terminated' || dir === 'mt'
    })
    const outgoing = todayDeliveries.filter(d => {
      const dir = d.direction?.toLowerCase()
      return dir === 'outgoing' || dir === 'mobile_originated' || dir === 'mo'
    })
    const successOutgoing = outgoing.filter(d => 
      d.state?.toLowerCase() === 'delivered' || 
      d.state?.toLowerCase() === 'success' || 
      d.state?.toLowerCase() === 'sent' ||
      d.state?.toLowerCase() === 'accepted'
    )

    return {
      receivedCount: incoming.length,
      sentRatio: outgoing.length > 0 ? `${successOutgoing.length} / ${outgoing.length}` : '0 / 0'
    }
  }, [state.diagnostics?.sms_deliveries?.deliveries])

  const epdgDomainText = useMemo(() => {
    return profileEpdgDomain(state, profile)
  }, [state, profile])

  const handleSimCardClick = () => {
    void navigate('/sim')
  }

  const handleSmsCardClick = () => {
    void navigate('/sms')
  }

  const handleSwitchChange = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const checked = event.target.checked
    if (checked) {
      setConfirmDialogOpen(true)
    } else {
      // 乐观更新：先切换 UI，再调用 API
      const snapshot = vowifiControl
      if (snapshot) setVowifiControl({ ...snapshot, connection_enabled: false })
      setActionLoading(true)
      try {
        await api.setVowifiConnection(false)
        const controlRes = await api.getVowifiControl()
        if (controlRes.data) setVowifiControl(controlRes.data)
        await loadData(true)
      } catch (err) {
        if (snapshot) setVowifiControl(snapshot)
        setError(err instanceof Error ? err.message : String(err))
      } finally {
        setActionLoading(false)
      }
    }
  }

  const handleConfirmEnable = async () => {
    setConfirmDialogOpen(false)
    // 乐观更新：先切换 UI
    const snapshot = vowifiControl
    if (snapshot) setVowifiControl({ ...snapshot, connection_enabled: true })
    setActionLoading(true)
    try {
      await api.setVowifiConnection(true)
      await api.connectVowifi()
      await loadData(false)
    } catch (err) {
      if (snapshot) setVowifiControl(snapshot)
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setActionLoading(false)
    }
  }

  const handleReconnect = async () => {
    setActionLoading(true)
    try {
      // 乐观更新：先切换 UI 为开启状态
      const snapshot = vowifiControl
      if (snapshot) setVowifiControl({ ...snapshot, connection_enabled: true })

      // 先强制断开连接，以释放底层资源和恢复蜂窝数据
      await api.setVowifiConnection(false)
      // 再重新启用连接并执行连接过程
      await api.setVowifiConnection(true)
      await api.connectVowifi()

      // 使用后台刷新避免页面闪屏
      await loadData(true)
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

      {/* Sleek Status Summary Card */}
      <Paper variant="outlined" sx={{ p: 2.5, borderRadius: 2 }}>
        <Box sx={{ display: 'flex', flexDirection: { xs: 'column', md: 'row' }, gap: 2.5, alignItems: 'center', justifyContent: 'space-between' }}>
          <Box sx={{ display: 'flex', flexDirection: { xs: 'column', md: 'row' }, alignItems: 'center', gap: 2.5, flex: 1, flexWrap: 'wrap' }}>
            <Box sx={{ display: 'flex', alignItems: 'center', gap: 2, flexShrink: 0 }}>
              {/* statusIndicatorDot with pulse animation */}
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

            <Stack direction={{ xs: 'column', sm: 'row' }} spacing={3} sx={{ px: { xs: 0, md: 1 } }}>
              <Metric
                label="当前 SIM"
                value={`${profile?.brand || '未知'} / ${profile ? `${profile.mcc}-${profile.mnc}` : match?.matched_prefix || 'N/A'}`}
              />
              <Metric
                label="短信路径"
                value={smsReady ? 'WiFi Calling 短信' : '蜂窝短信'}
              />
              <Metric
                label="运行监控"
                value={formattedRuntime}
              />
            </Stack>
          </Box>

          <Divider orientation="vertical" flexItem sx={{ display: { xs: 'none', md: 'block' } }} />

          <Stack direction="row" spacing={1.5} alignItems="center">
            {/* Reconnect Icon Button without border */}
            <IconButton
              onClick={() => void handleReconnect()}
              disabled={actionLoading}
              title="重新连接"
              sx={{
                border: 'none',
                color: 'text.secondary',
                '&:hover': { bgcolor: 'action.hover' }
              }}
            >
              <Refresh fontSize="small" />
            </IconButton>
            <Switch
              checked={vowifiControl?.connection_enabled ?? false}
              onChange={(e) => void handleSwitchChange(e)}
              disabled={actionLoading}
              color="primary"
            />
          </Stack>
        </Box>
      </Paper>

      {!match?.matched && match?.sim.present && (
        <Alert severity="warning" variant="outlined">
          当前 SIM 尚未匹配到内置 WiFi Calling profile，短信继续保持蜂窝路径。
        </Alert>
      )}

      {/* Two-Column Flex Layout (Align Heights) */}
      <Box
        ref={containerRef}
        sx={{
          display: 'flex',
          flexDirection: { xs: 'column', lg: 'row' },
          gap: 2.5,
          alignItems: 'stretch',
          width: '100%',
          height: { xs: 'auto', lg: containerHeight },
        }}
      >
        {/* Left Column - Connection Stages */}
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

            {/* Custom Vertical Stepper (overall row spacing 40px center-to-center when compact) */}
            <Box sx={{ display: 'flex', flexDirection: 'column', gap: 0, pl: 1 }}>
              {steps.map((step, idx) => {
                const isDone = step.state === 'done';
                const isActive = step.state === 'active';
                const isFailed = step.state === 'failed';
                const isSkipped = step.state === 'skipped';

                let statusColor = '#64748b';
                let bgColor: string | ((theme: { palette: { mode: string } }) => string) = (theme: { palette: { mode: string } }) => theme.palette.mode === 'dark' ? '#334155' : '#e2e8f0';
                let textColor = 'text.secondary';

                if (isDone) {
                  statusColor = vowifiGreen;
                  bgColor = vowifiGreen;
                  textColor = 'white';
                } else if (isActive) {
                  statusColor = '#1296db';
                  bgColor = '#1296db';
                  textColor = 'white';
                } else if (isFailed) {
                  statusColor = '#ef4444';
                  bgColor = '#ef4444';
                  textColor = 'white';
                }

                const prevStep = idx > 0 ? steps[idx - 1] : null;
                const isPrevDone = prevStep?.state === 'done';
                const isPrevActive = prevStep?.state === 'active';
                const prevColor = isPrevDone ? vowifiGreen : isPrevActive ? '#1296db' : 'divider';
                const currentColor = isDone ? vowifiGreen : isActive ? '#1296db' : 'divider';

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
                      // Top line segment
                      ...(idx > 0 ? {
                        '&::before': {
                          content: '""',
                          position: 'absolute',
                          left: '15px', // Center of the 32px circle
                          top: 0,
                          height: '50%',
                          width: '2px',
                          bgcolor: prevColor,
                          zIndex: 1,
                        }
                      } : {}),
                      // Bottom line segment
                      ...(idx < steps.length - 1 ? {
                        '&::after': {
                          content: '""',
                          position: 'absolute',
                          left: '15px',
                          top: '50%',
                          bottom: '-40px', // Extends through the 40px margin gap
                          width: '2px',
                          bgcolor: currentColor,
                          zIndex: 1,
                        }
                      } : {})
                    }}
                  >
                    {/* Step Icon (Circle) */}
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
                        flexShrink: 0
                      }}
                    >
                      {idx + 1}
                    </Box>

                    {/* Step Label Content */}
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
                            bgcolor:
                              isDone
                                ? 'rgba(42, 174, 103, 0.08)'
                                : isActive
                                  ? 'rgba(18, 150, 219, 0.08)'
                                  : isFailed
                                    ? 'rgba(239, 68, 68, 0.08)'
                                    : 'rgba(100, 116, 139, 0.08)',
                            color: statusColor,
                          }}
                        >
                          {isDone
                            ? '成功'
                            : isActive
                              ? '运行中'
                              : isFailed
                                ? '失败'
                                : isSkipped
                                  ? '已跳过'
                                  : '等待中'}
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
                          {step.detail.replaceAll('_', ' ')}
                        </Typography>
                      )}
                    </Box>
                  </Box>
                );
              })}
            </Box>

            {/* Stepper bottom 2x2 diagnostics info (Indented, not bolded, 12px, aligned like excel tab stop at 360px) */}
            <Box sx={{ mt: 4, pl: '32px', pr: 2, pt: 2.5, borderTop: '1px solid', borderColor: 'divider' }}>
              <Box sx={{ display: 'grid', gridTemplateColumns: '360px 1fr', rowGap: 2 }}>
                {/* Row 1 */}
                <Box>
                  <Typography variant="caption" color="text.secondary" display="block" sx={{ fontSize: '12px', mb: 0.5, fontWeight: 400 }}>
                    ePDG 域名
                  </Typography>
                  <Typography variant="body2" color="text.primary" sx={{ wordBreak: 'break-all', fontSize: '12px', fontWeight: 400 }}>
                    {epdgDomainText || '-'}
                  </Typography>
                </Box>
                <Box sx={{ pl: 2 }}>
                  <Typography variant="caption" color="text.secondary" display="block" sx={{ fontSize: '12px', mb: 0.5, fontWeight: 400 }}>
                    今日接收
                  </Typography>
                  <Typography variant="body2" color="text.primary" sx={{ fontSize: '12px', fontWeight: 400 }}>
                    {smsStats.receivedCount} 条
                  </Typography>
                </Box>

                {/* Row 2 */}
                <Box>
                  <Typography variant="caption" color="text.secondary" display="block" sx={{ fontSize: '12px', mb: 0.5, fontWeight: 400 }}>
                    IMS 域名
                  </Typography>
                  <Typography variant="body2" color="text.primary" sx={{ wordBreak: 'break-all', fontSize: '12px', fontWeight: 400 }}>
                    {ims?.domain ?? profileImsDomain(profile)}
                  </Typography>
                </Box>
                <Box sx={{ pl: 2 }}>
                  <Typography variant="caption" color="text.secondary" display="block" sx={{ fontSize: '12px', mb: 0.5, fontWeight: 400 }}>
                    今日发送
                  </Typography>
                  <Typography variant="body2" color="text.primary" sx={{ fontSize: '12px', fontWeight: 400 }}>
                    {smsStats.sentRatio}
                  </Typography>
                </Box>
              </Box>
            </Box>
          </Paper>
        </Box>

        {/* Right Column - Capabilities (compact) & Recent Events (stretches to fill height) */}
        <Box sx={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column', gap: 2.5 }}>
          {/* Capabilities Card */}
          <Paper variant="outlined" sx={{ p: 2.5, borderRadius: 2 }}>
            <Typography variant="subtitle1" fontWeight={800} sx={{ mb: 2 }}>
              能力状态
            </Typography>
            <Box sx={{ display: 'grid', gap: 2, width: '100%', gridTemplateColumns: { xs: '1fr', sm: 'repeat(2, minmax(0, 1fr))', xl: 'repeat(4, minmax(0, 1fr))' } }}>
              {/* SIM */}
              <Box sx={{ flex: 1, minWidth: 0 }}>
                <Box
                  onClick={handleSimCardClick}
                  sx={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 1.5,
                    p: 1.5,
                    border: '1px solid',
                    borderColor: 'divider',
                    borderRadius: 2,
                    bgcolor: (theme) => theme.palette.mode === 'dark' ? '#1e293b' : '#f8fafc',
                    transition: 'all 0.25s ease',
                    cursor: 'pointer',
                    '&:hover': {
                      transform: 'translateY(-2px)',
                      boxShadow: '0 4px 6px -1px rgba(0, 0, 0, 0.05)',
                      borderColor: 'primary.light'
                    }
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
                      bgcolor: match?.matched ? vowifiGreen : 'text.disabled',
                    }}
                  >
                    SIM
                  </Box>
                  <Typography variant="body2" fontWeight={700} color="text.primary" noWrap>
                    运营商：{match?.matched ? '已支持' : '不支持'}
                  </Typography>
                </Box>
              </Box>

              {/* NET */}
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
                    bgcolor: (theme) => theme.palette.mode === 'dark' ? '#1e293b' : '#f8fafc',
                    transition: 'all 0.25s ease',
                    '&:hover': {
                      transform: 'translateY(-2px)',
                      boxShadow: '0 4px 6px -1px rgba(0, 0, 0, 0.05)',
                      borderColor: 'primary.light'
                    }
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
                      bgcolor: isConnected ? vowifiGreen : 'text.disabled',
                    }}
                  >
                    NET
                  </Box>
                  <Typography variant="body2" fontWeight={700} color="text.primary" noWrap>
                    连接方式：{isConnected ? (epdg?.route_policy_id === 'direct' ? '直连' : '代理') : '未连接'}
                  </Typography>
                </Box>
              </Box>

              {/* SMS */}
              <Box sx={{ flex: 1, minWidth: 0 }}>
                <Box
                  onClick={handleSmsCardClick}
                  sx={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 1.5,
                    p: 1.5,
                    border: '1px solid',
                    borderColor: 'divider',
                    borderRadius: 2,
                    bgcolor: (theme) => theme.palette.mode === 'dark' ? '#1e293b' : '#f8fafc',
                    transition: 'all 0.25s ease',
                    cursor: 'pointer',
                    '&:hover': {
                      transform: 'translateY(-2px)',
                      boxShadow: '0 4px 6px -1px rgba(0, 0, 0, 0.05)',
                      borderColor: 'primary.light'
                    }
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
                      bgcolor: smsReady ? vowifiGreen : 'text.disabled',
                    }}
                  >
                    SMS
                  </Box>
                  <Typography variant="body2" fontWeight={700} color="text.primary" noWrap>
                    短信：{smsReady ? '可用' : '不可用'}
                  </Typography>
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
                    bgcolor: (theme) => theme.palette.mode === 'dark' ? '#1e293b' : '#f8fafc',
                    transition: 'all 0.25s ease',
                    '&:hover': {
                      transform: 'translateY(-2px)',
                      boxShadow: '0 4px 6px -1px rgba(0, 0, 0, 0.05)',
                      borderColor: 'primary.light'
                    }
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
                    }}
                  >
                    CALL
                  </Box>
                  <Typography variant="body2" fontWeight={700} color="text.primary" noWrap>
                    通话：暂不支持
                  </Typography>
                </Box>
              </Box>
            </Box>
          </Paper>

          {/* Recent Events Card (Takes remaining vertical space) */}
          <Paper variant="outlined" sx={{ p: 2.5, borderRadius: 2, flex: 1, display: 'flex', flexDirection: 'column', minHeight: 320 }}>
            <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, mb: 1.5 }}>
              <Typography variant="subtitle1" fontWeight={800}>
                最近事件
              </Typography>
              <Chip size="small" label={Math.min(timeline.length, 50)} />
            </Box>

            <Box
              sx={{
                flex: 1,
                overflowY: 'auto',
                p: 1.5,
                bgcolor: (theme) => theme.palette.mode === 'dark' ? '#0b1329' : '#f4f7fa',
                border: '1px solid',
                borderColor: (theme) => theme.palette.mode === 'dark' ? '#1e293b' : '#e2e8f0',
                borderRadius: 1.5,
                fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
                display: 'flex',
                flexDirection: 'column',
                gap: 1,
                '&::-webkit-scrollbar': {
                  width: '6px',
                },
                '&::-webkit-scrollbar-track': {
                  bgcolor: 'transparent',
                },
                '&::-webkit-scrollbar-thumb': {
                  bgcolor: 'action.selected',
                  borderRadius: '3px',
                },
                '&::-webkit-scrollbar-thumb:hover': {
                  bgcolor: 'action.focus',
                }
              }}
            >
              {timeline.length === 0 ? (
                <Typography variant="body2" color="text.secondary" sx={{ py: 3, textAlign: 'center' }}>
                  暂无最近事件记录。
                </Typography>
              ) : (
                timeline.slice(0, 50).map((item, index) => {
                  const getTagColors = (kind: string) => {
                    const k = kind.toUpperCase()
                    if (k === 'SYS') return { bgcolor: 'rgba(71, 85, 105, 0.1)', color: '#475569' }
                    if (k === 'SMS') return { bgcolor: 'rgba(190, 24, 93, 0.1)', color: '#be185d' }
                    if (k === 'IMS' || k === 'SIP') return { bgcolor: 'rgba(15, 118, 110, 0.1)', color: '#0f766e' }
                    if (k === 'IPSEC') return { bgcolor: 'rgba(29, 78, 216, 0.1)', color: '#1d4ed8' }
                    if (k === 'IMSI' || k === 'USIM') return { bgcolor: 'rgba(217, 119, 6, 0.1)', color: '#b45309' }
                    if (k === 'DNS') return { bgcolor: 'rgba(14, 116, 144, 0.1)', color: '#0e7490' }
                    if (k === 'PROFILE') return { bgcolor: 'rgba(109, 40, 217, 0.1)', color: '#6d28d9' }
                    return { bgcolor: 'action.hover', color: 'text.secondary' }
                  }

                  const tagColors = getTagColors(item.kind)

                  const formatTime = (ts?: string | null) => {
                    if (!ts) return ''
                    const parts = ts.split(/[ T]/)
                    if (parts.length > 1) {
                      return parts[1].substring(0, 8)
                    }
                    return ts
                  }

                  return (
                    <Box
                      key={index}
                      sx={{
                        display: 'flex',
                        alignItems: 'flex-start',
                        gap: 1.5,
                        fontSize: '0.75rem',
                        lineHeight: 1.5
                      }}
                    >
                      <Typography
                        variant="caption"
                        sx={{
                          fontFamily: 'inherit',
                          color: 'text.disabled',
                          fontWeight: 600,
                          flexShrink: 0,
                          userSelect: 'none'
                        }}
                      >
                        {formatTime(item.timestamp)}
                      </Typography>
                      <Box
                        sx={{
                          px: 0.5,
                          py: 0.15,
                          borderRadius: 0.5,
                          fontSize: '0.65rem',
                          fontWeight: 700,
                          textTransform: 'uppercase',
                          flexShrink: 0,
                          userSelect: 'none',
                          minWidth: 42,
                          textAlign: 'center',
                          ...tagColors
                        }}
                      >
                        {item.kind}
                      </Box>
                      <Typography
                        variant="caption"
                        sx={{
                          fontFamily: 'inherit',
                          color:
                            item.level === 'error'
                              ? 'error.main'
                              : item.level === 'warning'
                                ? 'warning.main'
                                : item.level === 'success'
                                  ? vowifiGreen
                                  : 'text.primary',
                          wordBreak: 'break-all',
                          flex: 1
                        }}
                      >
                        {item.title}
                        {item.detail && ` - ${item.detail}`}
                      </Typography>
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
        aria-labelledby="vowifi-confirm-dialog-title"
        aria-describedby="vowifi-confirm-dialog-description"
      >
        <DialogTitle id="vowifi-confirm-dialog-title" sx={{ fontWeight: 800 }}>
          温馨提示
        </DialogTitle>
        <DialogContent>
          <DialogContentText id="vowifi-confirm-dialog-description">
            启用 WiFi Calling 会切换短信路由路径，可能需要临时调整蜂窝数据以避免原生 IMS 冲突。是否确认启用？
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

      {/* 注释掉的高级诊断数据以备日后开发调试使用 */}
      {/* showDevTables && (
        <>
          <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
            <Box sx={{ display: 'flex', flexDirection: { xs: 'column', lg: 'row' }, gap: 2, alignItems: { xs: 'stretch', lg: 'center' }, mb: 2 }}>
              <Box sx={{ flex: 1 }}>
                <Typography variant="subtitle1" fontWeight={800}>
                  Diagnostics Summary
                </Typography>
                <Typography variant="body2" color="text.secondary">
                  Read-only aggregated runtime view
                </Typography>
              </Box>
              <TextField
                label="Trace ID filter"
                size="small"
                value={traceFilter}
                onChange={(event) => setTraceFilter(event.target.value)}
                placeholder="exact trace_id"
                sx={{ minWidth: { xs: '100%', sm: 280 } }}
              />
            </Box>

            <Box sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', md: 'repeat(2, 1fr)', xl: 'repeat(6, 1fr)' }, gap: 1.5 }}>
              <Metric label="Ready stages" value={diagnosticsSummary ? `${diagnosticsSummary.ready_stage_count}/${diagnosticsSummary.total_stage_count}` : 'N/A'} />
              <Metric label="Pending SMS" value={diagnosticsSummary?.pending_sms_deliveries ?? 0} />
              <Metric label="Failed SMS" value={diagnosticsSummary?.failed_sms_deliveries ?? 0} />
              <Metric label="Running soak" value={diagnosticsSummary?.running_soak_runs ?? 0} />
              <Metric label="Failed soak" value={diagnosticsSummary?.failed_soak_runs ?? 0} />
              <Metric label="Last event" value={diagnosticsSummary?.last_event_at ?? 'N/A'} />
              <Metric label="Active trace" value={diagnosticsSummary?.active_trace_id ?? diagnostics?.trace_filter ?? 'N/A'} />
              <Metric label="Read only" value={diagnosticsSummary?.read_only ? 'yes' : 'N/A'} />
              <Metric label="Persisted phase" value={diagnostics?.persisted_snapshot?.phase?.replaceAll('_', ' ') ?? 'N/A'} />
              <Metric label="Snapshot updated" value={diagnostics?.persisted_snapshot?.updated_at ?? 'N/A'} />
              <Metric label="Redaction" value={diagnosticsPrivacy?.redaction_policy ?? 'masked_identity_and_metadata_only'} />
              <Metric label="Sensitive fields" value={diagnosticsPrivacy?.sensitive_fields_returned ? 'present' : 'not returned'} />
              <Metric label="Actions" value={diagnosticsPrivacy?.action_interfaces_enabled ? 'enabled' : 'disabled'} />
              <Metric label="Trace policy" value={diagnosticsPrivacy?.trace_filter_policy ?? 'exact_trace_id_match_when_supplied'} />
            </Box>
          </Paper>

          <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
            <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, mb: 2 }}>
              <VpnKey color="primary" fontSize="small" />
              <Typography variant="subtitle1" fontWeight={800}>
                Readiness
              </Typography>
            </Box>

            <Stack direction="row" flexWrap="wrap" gap={1}>
              <ReadinessChip label="identity" value={Boolean(readiness?.identity_ready)} />
              <ReadinessChip label="profile" value={Boolean(readiness?.profile_matched)} />
              <ReadinessChip label="sim auth" value={Boolean(readiness?.sim_auth_ready)} />
              <ReadinessChip label="epdg" value={Boolean(readiness?.epdg_ready)} />
              <ReadinessChip label="ike" value={Boolean(readiness?.ike_ready)} />
              <ReadinessChip label="child sa" value={Boolean(readiness?.child_sa_ready)} />
              <ReadinessChip label="esp" value={Boolean(readiness?.esp_ready)} />
              <ReadinessChip label="ims" value={Boolean(readiness?.ims_registered)} />
              <ReadinessChip label="sms" value={Boolean(readiness?.sms_ready)} />
            </Stack>

            <Box sx={{ mt: 2, display: 'grid', gridTemplateColumns: { xs: '1fr', md: '1fr 1fr' }, gap: 2 }}>
              <Metric label="ePDG endpoint" value={profileEpdgDomain(state, profile)} />
              <Metric label="IMS domain" value={ims?.domain ?? profileImsDomain(profile)} />
              <Metric label="SIM access" value={simAuth?.sim_access ?? 'N/A'} />
              <Metric label="SIMAuth timeout" value={simAuth ? `${simAuth.timeout_ms}ms` : 'N/A'} />
              <Metric label="Logical channel" value={simAuth?.logical_channel.channel_scope ?? 'N/A'} />
              <Metric label="Profile switch cleanup" value={simAuth?.logical_channel.profile_switch_cleanup ?? 'N/A'} />
              <Metric label="AKA method" value={simAuth?.challenge.method ?? 'N/A'} />
              <Metric label="AKA resync" value={simAuth ? (simAuth.challenge.resync_supported ? 'enabled' : 'disabled') : 'N/A'} />
              <Metric label="Route policy" value={epdg?.route_policy_id ?? 'direct'} />
              <Metric label="IP stack" value={epdg?.ip_stack ?? (profile ? 'ipv4v6' : 'N/A')} />
              <Metric label="IKE proposals" value={formatList(ike?.ike_proposals)} />
              <Metric label="ESP proposals" value={formatList(ike?.child_sa.esp_proposals)} />
              <Metric label="Data plane" value={dataplane?.outer_encapsulation ?? 'N/A'} />
              <Metric label="NAT-T port" value={dataplane?.nat_t_port ?? 'N/A'} />
              <Metric label="Anti-replay" value={dataplane?.anti_replay_window ?? 'N/A'} />
              <Metric label="Inner stack" value={dataplane?.smoltcp.gateway_mode ?? 'N/A'} />
              <Metric label="Traffic selector" value={dataplane?.traffic_selectors.remote_selector ?? 'N/A'} />
              <Metric label="MTU strategy" value={dataplane?.mtu_strategy ?? 'N/A'} />
              <Metric label="NAT keepalive" value={ike ? `${ike.nat_keepalive_seconds}s` : 'N/A'} />
              <Metric label="DPD interval" value={ike ? `${ike.dpd_interval_seconds}s` : 'N/A'} />
              <Metric label="AKA mode" value={ike?.aka_challenge_mode ?? 'N/A'} />
              <Metric label="MOBIKE" value={ike?.mobike_policy ?? 'N/A'} />
              <Metric label="SIP transport" value={ims ? `${ims.transport}:${ims.local_port}` : 'N/A'} />
              <Metric label="sec-agree" value={formatMechanisms(ims?.security_client_mechanisms)} />
              <Metric label="IMS identity" value={ims?.identity_source ?? 'N/A'} />
              <Metric label="SMS transport" value={ims?.sms_receiver_transport ?? 'N/A'} />
              <Metric label="ICCID" value={match?.sim.iccid || 'N/A'} />
              <Metric label="IMSI" value={match?.sim.imsi || 'N/A'} />
              <Metric label="Switch phase" value={formatOptionalPhase(switchPhase)} />
              <Metric label="Switch retry" value={switchRetry} />
              <Metric label="Live network" value={executor?.live_network_allowed ? 'allowed' : 'disabled'} />
              <Metric label="Device changes" value={executor?.device_state_changes_allowed ? 'allowed' : 'disabled'} />
              <Metric label="Live Gate" value={liveGate?.implementation_ready ? 'implemented' : 'not implemented'} />
              <Metric label="Live auth" value={liveGate?.live_network_authorized ? 'authorized' : 'missing'} />
              <Metric label="Device auth" value={liveGate?.device_state_changes_authorized ? 'authorized' : 'missing'} />
              <Metric label="ADB path" value={liveGate?.adb_path_configured ? 'configured' : 'missing'} />
              <Metric label="Device panel" value={liveGate?.device_admin_url_configured ? 'configured' : 'missing'} />
              <Metric label="Live blockers" value={liveGate?.blockers.length ? liveGate.blockers.map((blocker) => blocker.replaceAll('_', ' ')).join(', ') : 'none'} />
            </Box>
          </Paper>

          {m10Audit && (
            <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
              <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, mb: 2 }}>
                <Typography variant="subtitle1" fontWeight={800}>
                  M10 Readiness Audit
                </Typography>
                <Chip size="small" label={m10Audit.stage.replaceAll('_', ' ')} />
              </Box>

              <Box sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', md: 'repeat(2, 1fr)', xl: 'repeat(6, 1fr)' }, gap: 1.5, mb: 2 }}>
                <Metric label="Profiles ready" value={`${m10Audit.profiles_ready}/${m10Audit.profile_count}`} />
                <Metric label="Dry-run ready" value={`${m10Audit.dry_run_profiles_ready}/${m10Audit.profile_count}`} />
                <Metric label="Live-test ready" value={`${m10Audit.live_profiles_ready}/${m10Audit.profile_count}`} />
                <Metric label="Long-run gates" value={`${m10Audit.long_run_ready_count}/${m10Audit.long_run_gate_count}`} />
                <Metric label="Live stages" value={`${m10Audit.live_stage_ready_count}/${m10Audit.live_stage_count}`} />
                <Metric label="Soak scenarios" value={`${m10Audit.soak_scenario_ready_count}/${m10Audit.soak_scenario_count}`} />
                <Metric label="Live network" value={m10Audit.live_network_allowed ? 'allowed' : 'disabled'} />
                <Metric label="Device changes" value={m10Audit.device_state_changes_allowed ? 'allowed' : 'disabled'} />
                <Metric label="Clean-room policy" value={m10Audit.clean_room_policy.replaceAll('_', ' ')} />
                <Metric label="Sensitive values" value={m10Audit.sensitive_values_policy.replaceAll('_', ' ')} />
                <Metric label="Blockers" value={m10Audit.blockers.length ? m10Audit.blockers.map((item) => item.replaceAll('_', ' ')).join(', ') : 'none'} />
              </Box>

              <Box sx={{ display: 'grid', gap: 2 }}>
                <TableContainer>
                  <Table size="small">
                    <TableHead>
                      <TableRow>
                        <TableCell>Live Stage</TableCell>
                        <TableCell>Component</TableCell>
                        <TableCell>Status</TableCell>
                        <TableCell>Plan</TableCell>
                        <TableCell>Dry Run</TableCell>
                        <TableCell>Needs</TableCell>
                        <TableCell>Evidence</TableCell>
                        <TableCell>Next Step</TableCell>
                        <TableCell>Blockers</TableCell>
                      </TableRow>
                    </TableHead>
                    <TableBody>
                      {m10Audit.live_stage_readiness.map((item) => (
                        <TableRow key={item.stage_id} hover>
                          <TableCell sx={{ fontFamily: 'monospace' }}>{item.stage_id}</TableCell>
                          <TableCell>{item.component.replaceAll('_', ' ')}</TableCell>
                          <TableCell>
                            <Chip
                              size="small"
                              label={item.status}
                              color={item.status === 'ready' ? 'success' : item.status === 'blocked' ? 'warning' : 'default'}
                              variant="outlined"
                            />
                          </TableCell>
                          <TableCell>{item.offline_ready ? 'ready' : 'missing'}</TableCell>
                          <TableCell>{item.dry_run_ready ? 'ready' : 'N/A'}</TableCell>
                          <TableCell>
                            {[
                              item.live_network_required ? 'network' : null,
                              item.device_state_change_required ? 'device' : null,
                            ].filter(Boolean).join(', ') || 'none'}
                          </TableCell>
                          <TableCell>{item.evidence.replaceAll('_', ' ')}</TableCell>
                          <TableCell>{item.next_step.replaceAll('_', ' ')}</TableCell>
                          <TableCell>{item.blockers.length ? item.blockers.map((blocker) => blocker.replaceAll('_', ' ')).join(', ') : 'none'}</TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                </TableContainer>

                <TableContainer>
                  <Table size="small">
                    <TableHead>
                      <TableRow>
                        <TableCell>Soak Scenario</TableCell>
                        <TableCell>Status</TableCell>
                        <TableCell>Window</TableCell>
                        <TableCell>Needs</TableCell>
                        <TableCell>Metrics</TableCell>
                        <TableCell>Pass Criteria</TableCell>
                        <TableCell>Evidence</TableCell>
                        <TableCell>Blockers</TableCell>
                      </TableRow>
                    </TableHead>
                    <TableBody>
                      {m10Audit.soak_scenarios.map((item) => (
                        <TableRow key={item.scenario_id} hover>
                          <TableCell sx={{ fontFamily: 'monospace' }}>{item.scenario_id}</TableCell>
                          <TableCell>
                            <Chip
                              size="small"
                              label={item.status}
                              color={item.status === 'ready' ? 'success' : 'default'}
                              variant="outlined"
                            />
                          </TableCell>
                          <TableCell>{`${item.duration_hours}h / ${item.sample_interval_seconds}s`}</TableCell>
                          <TableCell>
                            {[
                              item.live_network_required ? 'network' : null,
                              item.device_state_change_required ? 'device' : null,
                              item.sms_test_required ? 'sms' : null,
                            ].filter(Boolean).join(', ') || 'none'}
                          </TableCell>
                          <TableCell>{item.metrics.map((metric) => metric.replaceAll('_', ' ')).join(', ')}</TableCell>
                          <TableCell>{item.pass_criteria.map((criterion) => criterion.replaceAll('_', ' ')).join(', ')}</TableCell>
                          <TableCell>{item.evidence_source.replaceAll('_', ' ')}</TableCell>
                          <TableCell>{item.blockers.length ? item.blockers.map((blocker) => blocker.replaceAll('_', ' ')).join(', ') : 'none'}</TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                </TableContainer>

                <Box sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', xl: '1fr 1fr' }, gap: 2 }}>
                  <TableContainer>
                    <Table size="small">
                      <TableHead>
                        <TableRow>
                          <TableCell>Profile</TableCell>
                          <TableCell>PLMN</TableCell>
                          <TableCell>Plan</TableCell>
                          <TableCell>Dry Run</TableCell>
                          <TableCell>Live</TableCell>
                          <TableCell>Blockers</TableCell>
                        </TableRow>
                      </TableHead>
                      <TableBody>
                        {m10Audit.profile_audits.map((item) => (
                          <TableRow key={item.profile_id} hover>
                            <TableCell sx={{ fontFamily: 'monospace' }}>{item.profile_id}</TableCell>
                            <TableCell sx={{ fontFamily: 'monospace' }}>{item.plmn}</TableCell>
                            <TableCell>
                              <Chip size="small" label={item.offline_plan_ready ? 'ready' : 'fail'} color={item.offline_plan_ready ? 'success' : 'error'} variant="outlined" />
                            </TableCell>
                            <TableCell>
                              <Chip size="small" label={item.dry_run_ready ? 'ready' : 'fail'} color={item.dry_run_ready ? 'success' : 'error'} variant="outlined" />
                            </TableCell>
                            <TableCell>
                              <Chip size="small" label={item.live_test_ready ? 'ready' : 'blocked'} color={item.live_test_ready ? 'success' : 'default'} variant="outlined" />
                            </TableCell>
                            <TableCell>{item.blockers.length ? item.blockers.map((blocker) => blocker.replaceAll('_', ' ')).join(', ') : 'none'}</TableCell>
                          </TableRow>
                        ))}
                      </TableBody>
                    </Table>
                  </TableContainer>

                  <TableContainer>
                    <Table size="small">
                      <TableHead>
                        <TableRow>
                          <TableCell>Gate</TableCell>
                          <TableCell>Status</TableCell>
                          <TableCell>Target</TableCell>
                          <TableCell>Blocker</TableCell>
                        </TableRow>
                      </TableHead>
                      <TableBody>
                        {m10Audit.long_run_gates.map((gate) => (
                          <TableRow key={gate.gate_id} hover>
                            <TableCell sx={{ fontFamily: 'monospace' }}>{gate.gate_id}</TableCell>
                            <TableCell>
                              <Chip
                                size="small"
                                label={gate.status}
                                color={gate.status === 'pass' ? 'success' : gate.status === 'fail' ? 'error' : 'default'}
                                variant="outlined"
                              />
                            </TableCell>
                            <TableCell>{gate.target.replaceAll('_', ' ')}</TableCell>
                            <TableCell>{gate.blocker?.replaceAll('_', ' ') ?? 'none'}</TableCell>
                          </TableRow>
                        ))}
                      </TableBody>
                    </Table>
                  </TableContainer>
                </Box>
              </Box>
            </Paper>
          )}

          <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
            <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, mb: 2 }}>
              <Typography variant="subtitle1" fontWeight={800}>
                Soak Results
              </Typography>
              <Chip size="small" label={state.soakRuns?.total ?? 0} />
              <Chip size="small" variant="outlined" label={state.soakRuns?.read_only ? 'read only' : 'no actions'} />
            </Box>

            {soakRuns.length === 0 ? (
              <Typography variant="body2" color="text.secondary">
                No VoWiFi soak result records.
              </Typography>
            ) : (
              <TableContainer>
                <Table size="small">
                  <TableHead>
                    <TableRow>
                      <TableCell>Run</TableCell>
                      <TableCell>Scenario</TableCell>
                      <TableCell>Status</TableCell>
                      <TableCell>Profile</TableCell>
                      <TableCell>Duration</TableCell>
                      <TableCell>Samples</TableCell>
                      <TableCell>Failures</TableCell>
                      <TableCell>Last Sample</TableCell>
                      <TableCell>Last Error</TableCell>
                    </TableRow>
                  </TableHead>
                  <TableBody>
                    {soakRuns.map((run) => {
                      const lastSample = run.samples[0]
                      return (
                        <TableRow key={run.run_id} hover>
                          <TableCell sx={{ fontFamily: 'monospace' }}>{run.run_id}</TableCell>
                          <TableCell>{run.scenario_id.replaceAll('_', ' ')}</TableCell>
                          <TableCell>
                            <Chip
                              size="small"
                              label={run.status}
                              color={run.status === 'passed' ? 'success' : run.status === 'failed' ? 'error' : run.status === 'running' ? 'primary' : 'default'}
                              variant="outlined"
                            />
                          </TableCell>
                          <TableCell sx={{ fontFamily: 'monospace' }}>{run.profile_id ?? run.plmn ?? 'N/A'}</TableCell>
                          <TableCell>{`${run.duration_seconds}s`}</TableCell>
                          <TableCell>{run.sample_count}</TableCell>
                          <TableCell>{run.failure_count}</TableCell>
                          <TableCell>
                            {lastSample ? `${lastSample.metric_name.replaceAll('_', ' ')}=${lastSample.metric_value} (${lastSample.state})` : 'N/A'}
                          </TableCell>
                          <TableCell>{run.last_error?.replaceAll('_', ' ') ?? 'none'}</TableCell>
                        </TableRow>
                      )
                    })}
                  </TableBody>
                </Table>
              </TableContainer>
            )}
          </Paper>

          <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
            <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, mb: 2 }}>
              <Typography variant="subtitle1" fontWeight={800}>
                Runtime Executor
              </Typography>
              <Chip size="small" label={executor?.executor_id ?? 'noop_runtime_executor'} />
            </Box>

            <TableContainer>
              <Table size="small">
                <TableHead>
                  <TableRow>
                    <TableCell>Stage</TableCell>
                    <TableCell>Component</TableCell>
                    <TableCell>Mode</TableCell>
                    <TableCell>Enabled</TableCell>
                    <TableCell>Reason</TableCell>
                  </TableRow>
                </TableHead>
                <TableBody>
                  {(executor?.capabilities ?? []).map((capability) => (
                    <TableRow key={capability.stage} hover>
                      <TableCell sx={{ fontFamily: 'monospace' }}>{capability.stage}</TableCell>
                      <TableCell>{capability.component}</TableCell>
                      <TableCell>{capability.mode}</TableCell>
                      <TableCell>
                        <Chip
                          size="small"
                          label={capability.enabled ? 'enabled' : 'disabled'}
                          color={capability.enabled ? 'success' : 'default'}
                          variant="outlined"
                        />
                      </TableCell>
                      <TableCell>{capability.reason.replaceAll('_', ' ')}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </TableContainer>

            {dataplaneDryRun && (
              <Box sx={{ mt: 2, pt: 2, borderTop: '1px solid', borderColor: 'divider' }}>
                <Typography variant="subtitle2" fontWeight={800} sx={{ mb: 1.25 }}>
                  Dataplane Dry Run
                </Typography>
                <Box sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', md: 'repeat(3, 1fr)', xl: 'repeat(6, 1fr)' }, gap: 1.5 }}>
                  <Metric label="Phase" value={dataplaneDryRun.phase.replaceAll('_', ' ')} />
                  <Metric label="Proposal" value={dataplaneDryRun.selected_esp_proposal ?? 'N/A'} />
                  <Metric label="Seq out" value={dataplaneDryRun.sa_pair.outbound_sequence_allocated} />
                  <Metric label="Replay high" value={dataplaneDryRun.replay_window.highest_sequence} />
                  <Metric label="Inner MTU" value={dataplaneDryRun.mtu.inner_mtu} />
                  <Metric label="MTU drops" value={dataplaneDryRun.mtu_drops} />
                  <Metric label="Inner adapter" value={dataplaneDryRun.inner_gateway.adapter} />
                  <Metric label="Inner queue" value={`${dataplaneDryRun.inner_gateway.queued_packets}/${dataplaneDryRun.inner_gateway.queue_capacity}`} />
                  <Metric label="Packets in" value={dataplaneDryRun.sa_pair.packets_in} />
                  <Metric label="Packets out" value={dataplaneDryRun.sa_pair.packets_out} />
                  <Metric label="Last frame" value={dataplaneDryRun.last_frame_decision?.decision?.replaceAll('_', ' ') ?? 'N/A'} />
                  <Metric label="NAT-T" value={dataplaneDryRun.last_frame_decision?.natt?.kind?.replaceAll('_', ' ') ?? 'N/A'} />
                </Box>
              </Box>
            )}

            {imsRegisterDryRun && (
              <Box sx={{ mt: 2, pt: 2, borderTop: '1px solid', borderColor: 'divider' }}>
                <Typography variant="subtitle2" fontWeight={800} sx={{ mb: 1.25 }}>
                  IMS Register Dry Run
                </Typography>
                <Box sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', md: 'repeat(3, 1fr)', xl: 'repeat(6, 1fr)' }, gap: 1.5 }}>
                  <Metric label="Phase" value={imsRegisterDryRun.phase.replaceAll('_', ' ')} />
                  <Metric label="SIP status" value={imsRegisterDryRun.last_sip_status ?? 'N/A'} />
                  <Metric label="Security mode" value={imsRegisterDryRun.security_mode} />
                  <Metric label="Mechanism" value={imsRegisterDryRun.selected_security_mechanism ?? 'N/A'} />
                  <Metric label="Digest" value={imsRegisterDryRun.aka_digest?.algorithm ?? 'N/A'} />
                  <Metric label="Challenge" value={imsRegisterDryRun.challenge?.challenge_token_present ? 'present' : 'N/A'} />
                  <Metric label="Security verify" value={imsRegisterDryRun.sec_agree?.security_verify_ready ? 'ready' : 'N/A'} />
                  <Metric label="Protected transport" value={imsRegisterDryRun.sec_agree?.protected_transport_ready ? 'ready' : 'N/A'} />
                  <Metric label="Policy" value={imsRegisterDryRun.sec_agree?.policy_installed ? 'installed' : 'N/A'} />
                  <Metric label="Expires" value={imsRegisterDryRun.registered_expires_seconds ? `${imsRegisterDryRun.registered_expires_seconds}s` : 'N/A'} />
                  <Metric label="Service route" value={imsRegisterDryRun.service_route_present ? 'present' : 'N/A'} />
                  <Metric label="Transcript" value={imsRegisterDryRun.transcript.length} />
                </Box>
              </Box>
            )}

            {restoreDryRun && (
              <Box sx={{ mt: 2, pt: 2, borderTop: '1px solid', borderColor: 'divider' }}>
                <Typography variant="subtitle2" fontWeight={800} sx={{ mb: 1.25 }}>
                  eSIM Restore Dry Run
                </Typography>
                <Box sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', md: 'repeat(3, 1fr)', xl: 'repeat(6, 1fr)' }, gap: 1.5 }}>
                  <Metric label="Phase" value={restoreDryRun.switch_phase.replaceAll('_', ' ')} />
                  <Metric label="Retry" value={restoreDryRun.retry_count} />
                  <Metric label="Phase ms" value={restoreDryRun.phase_ms} />
                  <Metric label="Identity" value={restoreDryRun.identity_ready ? 'ready' : 'N/A'} />
                  <Metric label="SIMAuth" value={restoreDryRun.sim_auth_ready ? 'ready' : 'N/A'} />
                  <Metric label="First failure" value={restoreDryRun.runtime_restore.first_failure_reason?.replaceAll('_', ' ') ?? 'N/A'} />
                  <Metric label="Retryable" value={restoreDryRun.runtime_restore.first_failure_retryable ? 'yes' : 'N/A'} />
                  <Metric label="Attempts" value={restoreDryRun.runtime_restore.attempts} />
                  <Metric label="Teardown" value={restoreDryRun.cleanup.runtime_teardown_done ? 'done' : 'N/A'} />
                  <Metric label="SMS rollback" value={restoreDryRun.cleanup.qmi_sms_restored ? 'qmi/at' : 'N/A'} />
                  <Metric label="APDU cleanup" value={restoreDryRun.cleanup.apdu_sessions_cleared ? 'done' : 'N/A'} />
                  <Metric label="Card settle" value={`${restoreDryRun.gate.card_reset_settling_ms}ms`} />
                  <Metric label="PLMN source" value={restoreDryRun.gate.home_plmn_source.replaceAll('_', ' ')} />
                  <Metric label="REGISTER" value={restoreDryRun.runtime_restore.final_register_verified ? 'verified' : 'N/A'} />
                  <Metric label="SMS ready" value={restoreDryRun.runtime_restore.final_sms_ready ? 'ready' : 'N/A'} />
                  <Metric label="Events" value={restoreDryRun.events.length} />
                </Box>
              </Box>
            )}
            {smsDryRun && (
              <Box sx={{ mt: 2, pt: 2, borderTop: '1px solid', borderColor: 'divider' }}>
                <Typography variant="subtitle2" fontWeight={800} sx={{ mb: 1.25 }}>
                  SMS over IMS Dry Run
                </Typography>
                <Box sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', md: 'repeat(3, 1fr)', xl: 'repeat(6, 1fr)' }, gap: 1.5 }}>
                  <Metric label="SMS ready" value={smsDryRun.sms_ready ? 'ready' : 'N/A'} />
                  <Metric label="Receiver" value={smsDryRun.receiver_transport} />
                  <Metric label="SUBSCRIBE(reg)" value={smsDryRun.subscribe_reg_ready ? 'ready' : 'N/A'} />
                  <Metric label="Pending" value={smsDryRun.pending_delivery_count} />
                  <Metric label="MO state" value={`${smsDryRun.mo.state}/${smsDryRun.mo.api_status}`} />
                  <Metric label="MO RP ACK" value={smsDryRun.mo.rpdu_ack} />
                  <Metric label="MT state" value={`${smsDryRun.mt.state}/${smsDryRun.mt.api_status}`} />
                  <Metric label="MT parts" value={`${smsDryRun.mt.parts.filter((part) => part.received).length}/${smsDryRun.reassembly?.expected_parts ?? smsDryRun.mt.parts.length}`} />
                  <Metric label="Reassembly" value={smsDryRun.reassembly?.complete ? 'complete' : 'N/A'} />
                  <Metric label="Last MESSAGE" value={smsDryRun.last_sip_message?.sip_state ?? 'N/A'} />
                  <Metric label="Last ACK" value={smsDryRun.last_ack?.ack_kind?.replaceAll('_', ' ') ?? 'N/A'} />
                  <Metric label="Fact source" value={smsDryRun.mo.db_fact_source} />
                </Box>
              </Box>
            )}
          </Paper>

          <Box sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', xl: '1fr 1fr' }, gap: 2.5 }}>
            <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
              <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, mb: 2 }}>
                <Typography variant="subtitle1" fontWeight={800}>
                  SMS Delivery
                </Typography>
                <Chip size="small" label={state.smsDeliveries?.total ?? 0} />
              </Box>

              {smsDeliveries.length === 0 ? (
                <Typography variant="body2" color="text.secondary">
                  暂无 SMS over IMS delivery 记录。
                </Typography>
              ) : (
                <TableContainer>
                  <Table size="small">
                    <TableHead>
                      <TableRow>
                        <TableCell>Message</TableCell>
                        <TableCell>Direction</TableCell>
                        <TableCell>State</TableCell>
                        <TableCell>RPDU</TableCell>
                        <TableCell>Parts</TableCell>
                      </TableRow>
                    </TableHead>
                    <TableBody>
                      {smsDeliveries.map((item) => (
                        <TableRow key={item.message_id} hover>
                          <TableCell sx={{ fontFamily: 'monospace' }}>{item.message_id}</TableCell>
                          <TableCell>{item.direction}</TableCell>
                          <TableCell>{item.state}</TableCell>
                          <TableCell>{item.rpdu_ack}</TableCell>
                          <TableCell>{item.parts.filter((part) => part.received).length}/{item.parts.length}</TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                </TableContainer>
              )}
            </Paper>

            <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
              <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, mb: 2 }}>
                <Typography variant="subtitle1" fontWeight={800}>
                  Runtime Timeline
                </Typography>
                <Chip size="small" label={timeline.length} />
                {diagnostics?.trace_filter && <Chip size="small" variant="outlined" label={`trace ${diagnostics.trace_filter}`} />}
              </Box>

              {timeline.length === 0 ? (
                <Typography variant="body2" color="text.secondary">
                  暂无 VoWiFi runtime timeline 记录。
                </Typography>
              ) : (
                <TableContainer>
                  <Table size="small">
                    <TableHead>
                      <TableRow>
                        <TableCell>Time</TableCell>
                        <TableCell>Kind</TableCell>
                        <TableCell>Level</TableCell>
                        <TableCell>Phase</TableCell>
                        <TableCell>Event</TableCell>
                        <TableCell>Trace</TableCell>
                      </TableRow>
                    </TableHead>
                    <TableBody>
                      {timeline.map((item, index) => (
                        <TableRow key={`${item.kind}-${item.timestamp ?? index}-${item.title}`} hover>
                          <TableCell>{item.timestamp ?? 'N/A'}</TableCell>
                          <TableCell>{item.kind}</TableCell>
                          <TableCell>{item.level}</TableCell>
                          <TableCell>{item.phase.replaceAll('_', ' ')}</TableCell>
                          <TableCell>{item.title.replaceAll('_', ' ')}</TableCell>
                          <TableCell sx={{ fontFamily: 'monospace' }}>{item.trace_id ?? 'N/A'}</TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                </TableContainer>
              )}
            </Paper>
          </Box>

          <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
            <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, mb: 2 }}>
              <Public color="primary" fontSize="small" />
              <Typography variant="subtitle1" fontWeight={800}>
                Built-in Profile Registry
              </Typography>
              <Chip size="small" label={registry.length} />
            </Box>

            <TableContainer>
              <Table size="small">
                <TableHead>
                  <TableRow>
                    <TableCell>Profile ID</TableCell>
                    <TableCell>Brand</TableCell>
                    <TableCell>PLMN</TableCell>
                    <TableCell>Country</TableCell>
                    <TableCell>Legal name</TableCell>
                    <TableCell>Verified</TableCell>
                  </TableRow>
                </TableHead>
                <TableBody>
                  {registry.map((item) => (
                    <TableRow key={item.profile_id} hover selected={item.profile_id === profile?.profile_id}>
                      <TableCell sx={{ fontFamily: 'monospace' }}>{item.profile_id}</TableCell>
                      <TableCell>{item.brand}</TableCell>
                      <TableCell sx={{ fontFamily: 'monospace' }}>{item.plmn}</TableCell>
                      <TableCell>{item.country_iso2.toUpperCase()}</TableCell>
                      <TableCell>{item.operator_legal_name}</TableCell>
                      <TableCell>{item.last_verified}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </TableContainer>
          </Paper>
        </>
      ) */}


    </Box>
  )
}
