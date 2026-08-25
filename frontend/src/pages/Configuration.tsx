import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { useLocation } from 'react-router-dom'
import {
  Alert,
  Box,
  Button,
  ButtonBase,
  Card,
  CardContent,
  CardHeader,
  Chip,
  CircularProgress,
  Collapse,
  Divider,
  FormControlLabel,
  Snackbar,
  Stack,
  Switch,
  TextField,
  Tooltip,
  Typography,
} from '@mui/material'
import Grid from '@mui/material/Grid'
import {
  FlightTakeoff,
  Save,
  Wifi,
  Hub,
  ExpandMore,
  LinkOff,
  CheckCircle,
  Devices,
} from '@mui/icons-material'
import type { Theme } from '@mui/material/styles'
import { useSimAdminApi } from '../contexts/ApiContext'
import ErrorSnackbar from '../components/ErrorSnackbar'
import { LAYOUT_BOTTOM_ACTION_BAR_ID } from '../components/Layout/layoutConstants'
import {
  SingleAdminSecurityPanel,
  type SingleAdminSecurityClient,
} from '../components/SingleAdminSecurityPanel'
import type { AirplaneModeResponse, HubConfig, HubRuntimeStatus } from '../api/types'

interface HealthStatus {
  status: string
  timestamp?: string
}

const primaryStatusChipSx = (theme: Theme) => ({
  bgcolor: theme.palette.mode === 'light' ? 'rgba(25, 118, 210, 0.06)' : 'rgba(144, 202, 249, 0.14)',
  borderColor: theme.palette.primary.light,
  color: theme.palette.primary.main,
  fontWeight: 600,
})

const controlFollowupGap = 2
const DEFAULT_HUB_CONFIG: HubConfig = { enabled: false, url: '', local_fallback_timeout_seconds: 120, local_fallback_enabled: true }

const compactCardAlertSx = {
  alignItems: 'center',
  minHeight: 64,
  py: 0.75,
  '& .MuiAlert-icon': {
    alignItems: 'center',
    py: 0.25,
  },
  '& .MuiAlert-message': {
    lineHeight: 1.5,
    py: 0.25,
  },
}

function hubConnectionLabel(config: HubConfig, runtime: HubRuntimeStatus | null) {
  if (!config.enabled) return '本机管理'
  const labels: Record<string, string> = {
    waiting_for_hub: '等待 SimAdminHub 接入',
    registering: '正在注册',
    awaiting_approval: '等待 SimAdminHub 确认',
    connecting: '正在连接',
    connected: '已连接',
    offline: '连接中断',
  }
  return labels[runtime?.connection_state ?? ''] ?? '准备连接'
}

function fallbackStateLabel(state?: HubRuntimeStatus['local_fallback_state']) {
  return ({ inactive: '未启用', disabled: '已关闭', armed: '等待接管', active: '本地规则已接管', standby: 'Hub 在线' } as Record<string, string>)[state ?? ''] ?? '--'
}

function formatHubTime(value?: string | null) {
  if (!value) return '--'
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? '--' : date.toLocaleString('zh-CN')
}

function ManagementModeOption({
  selected,
  title,
  detail,
  icon,
  disabled,
  onClick,
}: {
  selected: boolean
  title: string
  detail: string
  icon: ReactNode
  disabled: boolean
  onClick: () => void
}) {
  return (
    <ButtonBase
      aria-label={title}
      aria-pressed={selected}
      disabled={disabled}
      onClick={onClick}
      sx={(theme) => ({
        alignItems: 'stretch',
        bgcolor: selected
          ? theme.palette.mode === 'light' ? 'rgba(18, 150, 219, 0.055)' : 'rgba(66, 165, 245, 0.12)'
          : 'transparent',
        border: '1px solid',
        borderColor: selected ? 'primary.main' : 'divider',
        borderRadius: 1.5,
        minHeight: 82,
        p: 1.5,
        textAlign: 'left',
        transition: 'background-color 150ms ease, border-color 150ms ease, box-shadow 150ms ease',
        width: '100%',
        '&:hover': {
          bgcolor: selected
            ? theme.palette.mode === 'light' ? 'rgba(18, 150, 219, 0.09)' : 'rgba(66, 165, 245, 0.17)'
            : 'action.hover',
          borderColor: selected ? 'primary.main' : 'text.disabled',
        },
        '&:focus-visible': {
          boxShadow: `0 0 0 3px ${theme.palette.mode === 'light' ? 'rgba(18, 150, 219, 0.2)' : 'rgba(66, 165, 245, 0.28)'}`,
        },
      })}
    >
      <Box display="flex" alignItems="center" gap={1.25} width="100%" minWidth={0}>
        <Box
          sx={(theme) => ({
            alignItems: 'center',
            bgcolor: selected
              ? theme.palette.mode === 'light' ? 'rgba(18, 150, 219, 0.1)' : 'rgba(66, 165, 245, 0.18)'
              : 'action.hover',
            borderRadius: 1,
            color: selected ? 'primary.main' : 'text.secondary',
            display: 'flex',
            flex: '0 0 auto',
            height: 38,
            justifyContent: 'center',
            width: 38,
          })}
        >
          {icon}
        </Box>
        <Box minWidth={0} flex={1}>
          <Typography variant="body2" fontWeight={700} color={selected ? 'primary.main' : 'text.primary'}>
            {title}
          </Typography>
          <Typography variant="caption" color="text.secondary" display="block" mt={0.25} lineHeight={1.45}>
            {detail}
          </Typography>
        </Box>
        {selected && <CheckCircle color="primary" fontSize="small" sx={{ flex: '0 0 auto' }} />}
      </Box>
    </ButtonBase>
  )
}





export default function ConfigurationPage({ embedded = false }: { embedded?: boolean }) {
  const api = useSimAdminApi()
  const location = useLocation()
  const isSecurity = location.pathname === '/config/security'
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState<string | null>(null)
  const [dataStatus, setDataStatus] = useState(false)
  const [airplaneMode, setAirplaneMode] = useState<AirplaneModeResponse | null>(null)
  const [airplaneSwitching, setAirplaneSwitching] = useState(false)
  const [healthStatus, setHealthStatus] = useState<HealthStatus | null>(null)
  const [healthLoading, setHealthLoading] = useState(false)
  const [hubConfig, setHubConfig] = useState<HubConfig>(DEFAULT_HUB_CONFIG)
  const [hubRuntime, setHubRuntime] = useState<HubRuntimeStatus | null>(null)
  const [hubUrlDraft, setHubUrlDraft] = useState('')
  const [hubSaving, setHubSaving] = useState(false)
  const [hubAdvancedOpen, setHubAdvancedOpen] = useState(false)
  const securityClient = useMemo<SingleAdminSecurityClient>(() => ({
    async getSettings() {
      const response = await api.getAuthSettings()
      if (!response.data) throw new Error('读取安全设置失败')
      return response.data
    },
    async saveSettings(settings) {
      const response = await api.setAuthSettings(settings)
      if (!response.data) throw new Error('保存安全设置失败')
      return response.data
    },
    async setupPassword(password) {
      await api.setupAdminPassword(password)
    },
    async changePassword(password) {
      await api.changeAdminPassword(password)
    },
  }), [api])

  const checkHealth = async () => {
    setHealthLoading(true)
    try {
      const response = await api.health()
      setHealthStatus({
        status: response.status,
        timestamp: new Date().toISOString(),
      })
    } catch {
      setHealthStatus({
        status: 'error',
        timestamp: new Date().toISOString(),
      })
    } finally {
      setHealthLoading(false)
    }
  }

  const loadData = async () => {
    setLoading(true)
    setError(null)

    try {
      const [dataRes, airplaneModeRes, hubRes] = await Promise.all([
        api.getDataStatus(),
        api.getAirplaneMode(),
        embedded ? Promise.resolve(null) : api.getHubSettings(),
      ])

      if (dataRes.data) setDataStatus(dataRes.data.active)
      if (airplaneModeRes.data) setAirplaneMode(airplaneModeRes.data)
      if (hubRes?.data) {
        setHubConfig(hubRes.data.config)
        setHubRuntime(hubRes.data.runtime)
        setHubUrlDraft(hubRes.data.config.url)
      }
      if (!embedded) await checkHealth()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    void loadData()
    const interval = window.setInterval(() => {
      if (!embedded) {
        void checkHealth()
        void api.getHubSettings().then((response) => {
          if (response.data) setHubRuntime(response.data.runtime)
        }).catch(() => undefined)
      }
    }, 30000)
    return () => window.clearInterval(interval)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [embedded])

  const toggleDataConnection = async () => {
    try {
      setError(null)
      setSuccess(null)
      const newStatus = !dataStatus
      await api.setDataStatus(newStatus)
      setDataStatus(newStatus)
      setSuccess(`数据连接已${newStatus ? '启用' : '禁用'}`)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const toggleAirplaneMode = async () => {
    const snapshot = airplaneMode
    const newEnabled = !snapshot?.enabled
    if (snapshot) {
      setAirplaneMode({ ...snapshot, enabled: newEnabled })
    }
    try {
      setError(null)
      setSuccess(null)
      setAirplaneSwitching(true)
      const response = await api.setAirplaneMode(newEnabled)
      if (response.data) {
        setAirplaneMode(response.data)
        setSuccess(`飞行模式已${response.data.enabled ? '开启' : '关闭'}`)
      }
    } catch (err) {
      if (snapshot) setAirplaneMode(snapshot)
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setAirplaneSwitching(false)
    }
  }

  const persistHub = async (next: HubConfig, successMessage: string) => {
    try {
      setHubSaving(true)
      setError(null)
      const response = await api.setHubSettings(next)
      if (response.data) {
        setHubConfig(response.data.config)
        setHubRuntime(response.data.runtime)
        setHubUrlDraft(response.data.config.url)
      }
      setSuccess(successMessage)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setHubSaving(false)
    }
  }

  const changeHubMode = async (enabled: boolean) => {
    const next = { ...hubConfig, enabled }
    setHubConfig(next)
    await persistHub(next, enabled ? '已切换为 SimAdminHub 管理' : '已切换为本机管理')
  }

  const unbindHub = async () => {
    try {
      setHubSaving(true)
      const response = await api.unbindHub()
      if (response.data) {
        setHubConfig(response.data.config)
        setHubRuntime(response.data.runtime)
        setHubUrlDraft(response.data.config.url)
      }
      setSuccess('已解除 Hub 绑定')
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setHubSaving(false)
    }
  }



  const renderHealthBadge = () => {
    const healthOk = healthStatus?.status === 'ok'
    const healthKnown = Boolean(healthStatus)
    const statusLabel = healthKnown ? (healthOk ? '正常' : '异常') : '检查中'
    const lastChecked = healthStatus?.timestamp
      ? new Date(healthStatus.timestamp).toLocaleTimeString()
      : '未检查'

    return (
      <Tooltip title={healthLoading ? '正在刷新后端存活状态' : '点击刷新后端存活状态'}>
        <Box component="span" sx={{ display: 'inline-flex' }}>
          <ButtonBase
            aria-label="刷新后端服务健康状态"
            disabled={healthLoading}
            onClick={() => void checkHealth()}
            sx={(theme) => {
              const mainColor = healthOk
                ? theme.palette.success.main
                : healthKnown
                  ? theme.palette.error.main
                  : theme.palette.warning.main
              const bgColor = healthOk
                ? theme.palette.mode === 'light' ? 'rgba(42, 174, 103, 0.08)' : 'rgba(102, 187, 106, 0.16)'
                : healthKnown
                  ? theme.palette.mode === 'light' ? 'rgba(211, 47, 47, 0.08)' : 'rgba(244, 67, 54, 0.16)'
                  : theme.palette.mode === 'light' ? 'rgba(237, 108, 2, 0.08)' : 'rgba(255, 167, 38, 0.16)'
              const hoverBgColor = healthOk
                ? theme.palette.mode === 'light' ? 'rgba(42, 174, 103, 0.12)' : 'rgba(102, 187, 106, 0.22)'
                : healthKnown
                  ? theme.palette.mode === 'light' ? 'rgba(211, 47, 47, 0.12)' : 'rgba(244, 67, 54, 0.22)'
                  : theme.palette.mode === 'light' ? 'rgba(237, 108, 2, 0.12)' : 'rgba(255, 167, 38, 0.22)'

              return {
                alignItems: 'center',
                bgcolor: bgColor,
                border: '1px solid',
                borderColor: mainColor,
                borderRadius: 1,
                gap: 1,
                justifyContent: 'flex-start',
                minHeight: 48,
                minWidth: 146,
                px: 1.5,
                py: 0.75,
                textAlign: 'left',
                transition: 'background-color 150ms ease, border-color 150ms ease, box-shadow 150ms ease',
                '&:hover': {
                  bgcolor: hoverBgColor,
                  boxShadow: `0 0 0 1px ${mainColor}`,
                },
                '&.Mui-disabled': {
                  opacity: 0.82,
                },
              }
            }}
          >
            {healthLoading ? (
              <CircularProgress
                size={14}
                sx={{
                  color: healthOk ? 'success.main' : healthKnown ? 'error.main' : 'warning.main',
                  flex: '0 0 auto',
                }}
              />
            ) : (
              <Box
                sx={{
                  bgcolor: healthOk ? 'success.main' : healthKnown ? 'error.main' : 'warning.main',
                  borderRadius: '50%',
                  boxShadow: (theme) => `0 0 0 5px ${
                    healthOk
                      ? theme.palette.mode === 'light' ? 'rgba(42, 174, 103, 0.12)' : 'rgba(102, 187, 106, 0.18)'
                      : healthKnown
                        ? theme.palette.mode === 'light' ? 'rgba(211, 47, 47, 0.12)' : 'rgba(244, 67, 54, 0.18)'
                        : theme.palette.mode === 'light' ? 'rgba(237, 108, 2, 0.12)' : 'rgba(255, 167, 38, 0.18)'
                  }`,
                  flex: '0 0 auto',
                  height: 10,
                  width: 10,
                }}
              />
            )}
            <Box minWidth={0}>
              <Typography variant="caption" color="text.primary" fontWeight={700} lineHeight={1.35} display="block">
                后端服务: {statusLabel}
              </Typography>
              <Typography variant="caption" color="text.secondary" lineHeight={1.35} display="block">
                上次检查: {lastChecked}
              </Typography>
            </Box>
          </ButtonBase>
        </Box>
      </Tooltip>
    )
  }



  const renderSecurityPanel = () => (
    <SingleAdminSecurityPanel
      client={securityClient}
      actionBarHostId={LAYOUT_BOTTOM_ACTION_BAR_ID}
    />
  )
  if (loading) {
    return (
      <Box display="flex" justifyContent="center" alignItems="center" minHeight="60vh">
        <CircularProgress />
      </Box>
    )
  }

  const hubConnectionState = hubRuntime?.connection_state ?? 'waiting_for_hub'
  const hubConnectionBusy = hubConnectionState === 'registering' || hubConnectionState === 'connecting'
  const hubAddress = hubRuntime?.hub_url || hubConfig.url
  const hubStatusColor = hubConnectionState === 'connected'
    ? 'success.main'
    : hubConnectionState === 'offline'
      ? 'error.main'
      : hubConnectionState === 'waiting_for_hub' || hubConnectionState === 'awaiting_approval'
        ? 'warning.main'
        : 'primary.main'

  return (
    <Box>
      <Box
        mb={2}
        display="flex"
        alignItems={{ xs: 'flex-start', sm: 'center' }}
        justifyContent="space-between"
        gap={2}
        flexWrap="wrap"
      >
        <Box minWidth={0}>
          <Typography variant="h5" gutterBottom fontWeight={700}>
            {isSecurity ? '安全性设置' : '基本配置'}
          </Typography>
          <Typography variant="body2" color="text.secondary">
            {isSecurity ? '管理账户安全及密码强度策略' : '管理设备连接和其他系统参数'}
          </Typography>
        </Box>
        {!embedded && renderHealthBadge()}
      </Box>

      <ErrorSnackbar error={error} onClose={() => setError(null)} />
      {success && (
        <Snackbar
          open
          autoHideDuration={3000}
          resumeHideDuration={3000}
          onClose={() => setSuccess(null)}
          anchorOrigin={{ vertical: 'top', horizontal: 'center' }}
        >
          <Alert severity="success" variant="filled" onClose={() => setSuccess(null)}>
            {success}
          </Alert>
        </Snackbar>
      )}

      {isSecurity ? (
        <Box sx={{ pt: 2 }}>
          {renderSecurityPanel()}
        </Box>
      ) : (
        <Box display="flex" flexDirection="column" gap={3} sx={{ pt: 2 }}>

          {!embedded && <Card>
            <CardHeader
              avatar={<Devices color="primary" />}
              title="设备管理方式"
              titleTypographyProps={{ variant: 'h6', fontWeight: 600 }}
            />
            <CardContent>
              <Grid container spacing={1.5}>
                <Grid size={{ xs: 12, sm: 6 }}>
                  <ManagementModeOption
                    selected={!hubConfig.enabled}
                    title="本机管理"
                    detail="仅通过当前 SimAdmin 管理设备"
                    icon={<Devices fontSize="small" />}
                    disabled={hubSaving}
                    onClick={() => {
                      if (hubConfig.enabled) void changeHubMode(false)
                    }}
                  />
                </Grid>
                <Grid size={{ xs: 12, sm: 6 }}>
                  <ManagementModeOption
                    selected={hubConfig.enabled}
                    title="SimAdminHub 管理"
                    detail="接入 Hub 进行集中管理和调度"
                    icon={<Hub fontSize="small" />}
                    disabled={hubSaving}
                    onClick={() => {
                      if (!hubConfig.enabled) void changeHubMode(true)
                    }}
                  />
                </Grid>
              </Grid>

              <Collapse in={hubConfig.enabled} timeout={180} unmountOnExit>
                <Box>
                  <Divider sx={{ my: 2.25 }} />

                  <Box
                    sx={{
                      bgcolor: 'action.hover',
                      borderRadius: 1.5,
                      display: 'flex',
                      gap: 1.25,
                      p: 1.5,
                    }}
                  >
                    {hubConnectionBusy ? (
                      <CircularProgress size={16} sx={{ color: hubStatusColor, flex: '0 0 auto', mt: 0.25 }} />
                    ) : (
                      <Box
                        sx={{
                          bgcolor: hubStatusColor,
                          borderRadius: '50%',
                          flex: '0 0 auto',
                          height: 10,
                          mt: 0.65,
                          width: 10,
                        }}
                      />
                    )}
                    <Box minWidth={0} flex={1}>
                      <Typography variant="body2" fontWeight={700}>
                        {hubConnectionLabel(hubConfig, hubRuntime)}
                      </Typography>

                      {hubConnectionState === 'waiting_for_hub' && !hubAddress && (
                        <Typography variant="body2" color="text.secondary" mt={0.35}>
                          设备发现已开启，可在 SimAdminHub 中添加当前设备。
                        </Typography>
                      )}

                      {hubAddress && (
                        <Typography variant="body2" color="text.secondary" mt={0.35} sx={{ wordBreak: 'break-all' }}>
                          {hubAddress}
                        </Typography>
                      )}

                      {hubConnectionState === 'registering' && (
                        <Typography variant="caption" color="text.secondary" display="block" mt={0.35}>
                          正在向 SimAdminHub 注册当前设备
                        </Typography>
                      )}
                      {hubConnectionState === 'awaiting_approval' && (
                        <Typography variant="caption" color="text.secondary" display="block" mt={0.35}>
                          已提交接入请求，请在 SimAdminHub 中确认
                        </Typography>
                      )}
                      {hubConnectionState === 'connecting' && (
                        <Typography variant="caption" color="text.secondary" display="block" mt={0.35}>
                          正在建立安全连接
                        </Typography>
                      )}
                      {(hubConnectionState === 'connected' || hubConnectionState === 'offline') && (
                        <Stack direction="row" spacing={1.25} useFlexGap flexWrap="wrap" mt={0.45}>
                          {hubRuntime?.hub_version && (
                            <Typography variant="caption" color="text.secondary">
                              版本 {hubRuntime.hub_version}
                            </Typography>
                          )}
                          {hubRuntime?.last_connected_at && (
                            <Typography variant="caption" color="text.secondary">
                              最后连接 {formatHubTime(hubRuntime.last_connected_at)}
                            </Typography>
                          )}
                        </Stack>
                      )}
                      {hubRuntime?.last_error && (
                        <Typography variant="caption" color="warning.main" display="block" mt={0.6}>
                          {hubRuntime.last_error}
                        </Typography>
                      )}
                    </Box>
                  </Box>

                  <Divider sx={{ my: 2 }} />
                  <Box display="flex" alignItems="center" justifyContent="space-between" gap={2}>
                    <Box minWidth={0}>
                      <Typography variant="body2" fontWeight={700}>Hub 离线时使用设备本地规则</Typography>
                      <Typography variant="caption" color="text.secondary">
                        当前状态：{fallbackStateLabel(hubRuntime?.local_fallback_state)}
                      </Typography>
                    </Box>
                    <Switch
                      checked={hubConfig.local_fallback_enabled}
                      inputProps={{ 'aria-label': 'Hub 离线时使用设备本地规则' }}
                      onChange={(event) => {
                        const next = { ...hubConfig, local_fallback_enabled: event.target.checked }
                        setHubConfig(next)
                        void persistHub(next, '离线兜底设置已更新')
                      }}
                    />
                  </Box>

                  <Collapse in={hubConfig.local_fallback_enabled} timeout={150} unmountOnExit>
                    <Stack
                      direction={{ xs: 'column', sm: 'row' }}
                      alignItems={{ sm: 'center' }}
                      justifyContent="space-between"
                      spacing={1}
                      mt={1.25}
                    >
                      <Typography variant="body2" color="text.secondary">
                        启用本地规则前等待
                      </Typography>
                      <TextField
                        size="small"
                        type="number"
                        label="等待时间（秒）"
                        value={hubConfig.local_fallback_timeout_seconds}
                        inputProps={{ min: 30, max: 3600 }}
                        sx={{ width: { xs: '100%', sm: 164 } }}
                        onChange={(event) => setHubConfig((current) => ({
                          ...current,
                          local_fallback_timeout_seconds: Math.min(3600, Math.max(30, Number(event.target.value) || 120)),
                        }))}
                        onBlur={(event) => {
                          const timeout = Math.min(3600, Math.max(30, Number(event.target.value) || 120))
                          const next = { ...hubConfig, local_fallback_timeout_seconds: timeout }
                          setHubConfig(next)
                          void persistHub(next, '离线等待时间已更新')
                        }}
                      />
                    </Stack>
                  </Collapse>

                  <Box mt={1.5}>
                    <Button
                      size="small"
                      sx={{ px: 0.5 }}
                      endIcon={<ExpandMore sx={{ transform: hubAdvancedOpen ? 'rotate(180deg)' : 'none', transition: 'transform 150ms' }} />}
                      onClick={() => {
                        if (!hubAdvancedOpen) setHubUrlDraft(hubConfig.url)
                        setHubAdvancedOpen((open) => !open)
                      }}
                    >
                      {hubConfig.url ? '连接设置' : '手动指定 Hub'}
                    </Button>
                  </Box>
                  <Collapse in={hubAdvancedOpen} unmountOnExit>
                    <Stack spacing={1.5} pt={1}>
                      <TextField
                        fullWidth
                        size="small"
                        label="Hub 地址"
                        placeholder="https://hub.example.com"
                        value={hubUrlDraft}
                        onChange={(event) => setHubUrlDraft(event.target.value)}
                      />
                      <Stack direction={{ xs: 'column', sm: 'row' }} spacing={1}>
                        <Button
                          variant="outlined"
                          startIcon={hubSaving ? <CircularProgress size={16} /> : <Save />}
                          disabled={hubSaving || !hubUrlDraft.trim()}
                          onClick={() => void persistHub({ ...hubConfig, url: hubUrlDraft.trim() }, 'Hub 地址已保存')}
                        >
                          保存并连接
                        </Button>
                        {(hubConfig.url || hubRuntime?.hub_url) && (
                          <Button color="error" startIcon={<LinkOff />} disabled={hubSaving} onClick={() => void unbindHub()}>
                            解除绑定
                          </Button>
                        )}
                      </Stack>
                      {hubRuntime?.agent_id && (
                        <Typography variant="caption" color="text.secondary" sx={{ wordBreak: 'break-all' }}>
                          Agent ID：{hubRuntime.agent_id}
                        </Typography>
                      )}
                    </Stack>
                  </Collapse>
                </Box>
              </Collapse>
            </CardContent>
          </Card>}



          <Grid container spacing={3} alignItems="stretch">
            <Grid size={{ xs: 12, md: 6 }} sx={{ display: 'flex' }}>
              <Card sx={{ width: 1, height: 1, display: 'flex', flexDirection: 'column' }}>
                <CardHeader
                  avatar={<Wifi color="primary" />}
                  title="数据连接配置"
                  titleTypographyProps={{ variant: 'h6', fontWeight: 600 }}
                  action={
                    <Chip
                      label={dataStatus ? '已启用' : '已禁用'}
                      color={dataStatus ? 'primary' : 'default'}
                      variant={dataStatus ? 'outlined' : undefined}
                      size="small"
                      sx={dataStatus ? primaryStatusChipSx : undefined}
                    />
                  }
                />
                <CardContent sx={{ flexGrow: 1, display: 'flex', flexDirection: 'column' }}>
                  <Typography variant="body2" color="text.secondary">
                    控制设备的数据连接状态。禁用后设备将断开移动网络连接。
                  </Typography>
                  <Divider sx={{ my: 2 }} />
                  <FormControlLabel
                    control={
                      <Switch
                        checked={dataStatus}
                        onChange={() => void toggleDataConnection()}
                        color="primary"
                      />
                    }
                    label={
                      <Box>
                        <Typography variant="body1" fontWeight={600}>
                          {dataStatus ? '数据连接已启用' : '数据连接已禁用'}
                        </Typography>
                        <Typography variant="caption" color="text.secondary">
                          立即{dataStatus ? '断开' : '启用'}移动数据连接
                        </Typography>
                      </Box>
                    }
                  />
                  <Alert
                    severity="info"
                    sx={{
                      ...compactCardAlertSx,
                      mt: controlFollowupGap,
                    }}
                  >
                    禁用数据连接将中断所有使用移动网络的应用和服务
                  </Alert>
                </CardContent>
              </Card>
            </Grid>

            <Grid size={{ xs: 12, md: 6 }} sx={{ display: 'flex' }}>
              <Card sx={{ width: 1, height: 1, display: 'flex', flexDirection: 'column' }}>
                <CardHeader
                  avatar={<FlightTakeoff color={airplaneMode?.enabled ? 'warning' : 'primary'} />}
                  title="飞行模式"
                  titleTypographyProps={{ variant: 'h6', fontWeight: 600 }}
                  action={
                    <Chip
                      label={airplaneMode?.enabled ? '已开启' : '已关闭'}
                      color={airplaneMode?.enabled ? 'primary' : 'default'}
                      variant={airplaneMode?.enabled ? 'outlined' : undefined}
                      size="small"
                      sx={airplaneMode?.enabled ? primaryStatusChipSx : undefined}
                    />
                  }
                />
                <CardContent sx={{ flexGrow: 1, display: 'flex', flexDirection: 'column' }}>
                  <Typography variant="body2" color="text.secondary">
                    开启飞行模式将关闭射频，设备将无法连接移动网络。这不会影响本机 Web 管理访问。
                  </Typography>
                  <Divider sx={{ my: 2 }} />
                  <FormControlLabel
                    control={
                      <Switch
                        checked={airplaneMode?.enabled || false}
                        onChange={() => void toggleAirplaneMode()}
                        disabled={airplaneSwitching}
                        color="warning"
                      />
                    }
                    label={
                      <Box display="flex" alignItems="center" gap={1}>
                        {airplaneSwitching && <CircularProgress size={16} />}
                        <Box>
                          <Typography variant="body1" fontWeight={600}>
                            {airplaneMode?.enabled ? '飞行模式已开启' : '飞行模式已关闭'}
                          </Typography>
                          <Typography variant="caption" color="text.secondary">
                            {airplaneMode?.enabled ? '射频已关闭，无法连接网络' : '射频正常工作'}
                          </Typography>
                        </Box>
                      </Box>
                    }
                  />
                  <Box mt={controlFollowupGap} mb={controlFollowupGap} p={2} sx={{ bgcolor: 'action.hover', borderRadius: 1 }}>
                    <Typography variant="body2" color="text.secondary" gutterBottom>
                      <strong>当前状态详情</strong>
                    </Typography>
                    <Box display="flex" gap={2} flexWrap="wrap">
                      <Chip
                        label={`Modem 电源: ${airplaneMode?.powered ? '开启' : '关闭'}`}
                        size="small"
                        color={airplaneMode?.powered ? 'success' : 'default'}
                        variant="outlined"
                      />
                      <Chip
                        label={`射频: ${airplaneMode?.online ? '在线' : '离线'}`}
                        size="small"
                        color={airplaneMode?.online ? 'success' : 'error'}
                        variant="outlined"
                      />
                    </Box>
                  </Box>
                  <Alert severity="warning" sx={compactCardAlertSx}>
                    飞行模式通过设置 Modem 的 Online 属性来控制射频。
                  </Alert>
                </CardContent>
              </Card>
            </Grid>
          </Grid>
        </Box>
      )}


    </Box>
  )
}
