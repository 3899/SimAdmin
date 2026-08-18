import { useEffect, useState, type ReactNode } from 'react'
import { useLocation } from 'react-router-dom'
import { createPortal } from 'react-dom'
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
  FormControl,
  FormControlLabel,
  IconButton,
  InputBase,
  InputLabel,
  MenuItem,
  Select,
  Snackbar,
  Stack,
  Switch,
  TextField,
  Tooltip,
  Typography,
} from '@mui/material'
import Grid from '@mui/material/Grid'
import {
  AdminPanelSettings,
  Add,
  FlightTakeoff,
  Key,
  Remove,
  Save,
  Shield,
  Timer,
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
import { PasswordStrengthHint } from '../components/PasswordStrengthHint'
import {
  DEFAULT_SECURITY_SETTINGS,
  PASSWORD_MAX_LENGTH,
  normalizePasswordInput,
  passwordPolicyHelperText,
  validatePasswordAgainstSecurity,
} from '../lib/passwordPolicy'
import type { AirplaneModeResponse, HubConfig, HubRuntimeStatus, SecurityConfig } from '../api/types'

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
const PASSWORD_MIN_LENGTH_MIN = 1
const SESSION_TTL_OPTIONS = [
  { value: 24 * 60 * 60, label: '1 天' },
  { value: 7 * 24 * 60 * 60, label: '7 天' },
  { value: 14 * 24 * 60 * 60, label: '14 天' },
  { value: 30 * 24 * 60 * 60, label: '30 天' },
  { value: -1, label: '永不过期' },
]
const IDLE_TIMEOUT_OPTIONS = [
  { value: 30 * 60, label: '30 分钟' },
  { value: 60 * 60, label: '1 小时' },
  { value: 2 * 60 * 60, label: '2 小时' },
  { value: 3 * 60 * 60, label: '3 小时' },
  { value: 6 * 60 * 60, label: '6 小时' },
  { value: 0, label: '关闭' },
]
const DEFAULT_SECURITY_CONFIG: SecurityConfig = DEFAULT_SECURITY_SETTINGS
const SECURITY_SETTINGS_UPDATED_EVENT = 'simadmin-security-settings-updated'
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
  return ({ inactive: '未启用', disabled: '已关闭', armed: '已就绪', standby: 'Hub 在线' } as Record<string, string>)[state ?? ''] ?? '--'
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





function mergeSecurityConfig(config?: Partial<SecurityConfig>): SecurityConfig {
  return {
    ...DEFAULT_SECURITY_CONFIG,
    ...config,
  }
}

function securityConfigEqual(a: SecurityConfig, b: SecurityConfig) {
  return JSON.stringify(a) === JSON.stringify(b)
}

function countSecurityConfigChanges(a: SecurityConfig, b: SecurityConfig) {
  const keys: Array<keyof SecurityConfig> = [
    'password_protection_enabled',
    'password_min_length',
    'password_require_letters',
    'password_require_digits',
    'password_require_symbols',
    'session_ttl_seconds',
    'idle_timeout_seconds',
  ]
  return keys.filter((key) => a[key] !== b[key]).length
}

function validateSecurityConfig(config: SecurityConfig) {
  if (!Number.isInteger(config.password_min_length)
    || config.password_min_length < PASSWORD_MIN_LENGTH_MIN
    || config.password_min_length > PASSWORD_MAX_LENGTH) {
    return `密码最小长度需为 ${PASSWORD_MIN_LENGTH_MIN}-${PASSWORD_MAX_LENGTH} 之间的整数`
  }
  if (!config.password_require_letters
    && !config.password_require_digits
    && !config.password_require_symbols) {
    return '字符类型要求至少需要选择一项'
  }
  return null
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
  const [authConfigured, setAuthConfigured] = useState(false)
  const [securityConfig, setSecurityConfig] = useState<SecurityConfig>(() => DEFAULT_SECURITY_CONFIG)
  const [savedSecurityConfig, setSavedSecurityConfig] = useState<SecurityConfig>(() => DEFAULT_SECURITY_CONFIG)
  const [passwordMinLengthInput, setPasswordMinLengthInput] = useState(String(DEFAULT_SECURITY_CONFIG.password_min_length))
  const [securitySaving, setSecuritySaving] = useState(false)
  const [passwordUpdating, setPasswordUpdating] = useState(false)
  const [newPassword, setNewPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [bottomActionBarHost, setBottomActionBarHost] = useState<HTMLElement | null>(null)
  const [hubConfig, setHubConfig] = useState<HubConfig>(DEFAULT_HUB_CONFIG)
  const [hubRuntime, setHubRuntime] = useState<HubRuntimeStatus | null>(null)
  const [hubUrlDraft, setHubUrlDraft] = useState('')
  const [hubSaving, setHubSaving] = useState(false)
  const [hubAdvancedOpen, setHubAdvancedOpen] = useState(false)

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
      const [dataRes, airplaneModeRes, authSettingsRes, hubRes] = await Promise.all([
        api.getDataStatus(),
        api.getAirplaneMode(),
        embedded ? Promise.resolve(null) : api.getAuthSettings(),
        embedded ? Promise.resolve(null) : api.getHubSettings(),
      ])

      if (dataRes.data) setDataStatus(dataRes.data.active)
      if (airplaneModeRes.data) setAirplaneMode(airplaneModeRes.data)
      if (authSettingsRes?.data) {
        const loadedSecurityConfig = mergeSecurityConfig(authSettingsRes.data.settings)
        setAuthConfigured(authSettingsRes.data.configured)
        setSecurityConfig(loadedSecurityConfig)
        setSavedSecurityConfig(loadedSecurityConfig)
        setPasswordMinLengthInput(String(loadedSecurityConfig.password_min_length))
      }
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

  useEffect(() => {
    setBottomActionBarHost(document.getElementById(LAYOUT_BOTTOM_ACTION_BAR_ID))
  }, [])



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



  const patchSecurityConfig = (patch: Partial<SecurityConfig>) => {
    setSecurityConfig((prev) => ({ ...prev, ...patch }))
  }

  const updatePasswordMinLength = (value: number) => {
    const clampedValue = Math.min(Math.max(value, PASSWORD_MIN_LENGTH_MIN), PASSWORD_MAX_LENGTH)
    setPasswordMinLengthInput(String(clampedValue))
    patchSecurityConfig({ password_min_length: clampedValue })
  }

  const handlePasswordMinLengthInputChange = (value: string) => {
    const digits = value.replace(/\D/g, '').slice(0, 2)
    if (!digits) {
      setPasswordMinLengthInput('')
      return
    }

    const numericValue = Number(digits)
    updatePasswordMinLength(numericValue)
  }

  const commitPasswordMinLengthInput = () => {
    if (!passwordMinLengthInput) {
      setPasswordMinLengthInput(String(securityConfig.password_min_length))
    }
  }

  const resetSecuritySettings = () => {
    setSecurityConfig(savedSecurityConfig)
    setPasswordMinLengthInput(String(savedSecurityConfig.password_min_length))
  }

  const saveSecuritySettings = async () => {
    const validationError = validateSecurityConfig(securityConfig)
    if (validationError) {
      setError(validationError)
      return
    }

    setSecuritySaving(true)
    setError(null)
    setSuccess(null)
    try {
      const response = await api.setAuthSettings(securityConfig)
      const nextSecurityConfig = mergeSecurityConfig(response.data)
      setSecurityConfig(nextSecurityConfig)
      setSavedSecurityConfig(nextSecurityConfig)
      setPasswordMinLengthInput(String(nextSecurityConfig.password_min_length))
      window.dispatchEvent(new CustomEvent(SECURITY_SETTINGS_UPDATED_EVENT, { detail: nextSecurityConfig }))
      setSuccess('安全设置已保存')
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setSecuritySaving(false)
    }
  }

  const updateAdminPassword = async () => {
    if (!newPassword) {
      setError('请输入新管理员密码')
      return
    }
    if (newPassword !== confirmPassword) {
      setError('两次输入的新密码不一致')
      return
    }
    const passwordError = validatePasswordAgainstSecurity(newPassword, savedSecurityConfig)
    if (passwordError) {
      setError(passwordError)
      return
    }

    setPasswordUpdating(true)
    setError(null)
    setSuccess(null)
    try {
      if (authConfigured) {
        await api.changeAdminPassword(newPassword)
      } else {
        await api.setupAdminPassword(newPassword)
      }
      setAuthConfigured(true)
      setNewPassword('')
      setConfirmPassword('')
      setSuccess('管理员密码已更新')
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setPasswordUpdating(false)
    }
  }

  const handleNewPasswordChange = (value: string) => {
    const normalized = normalizePasswordInput(value, savedSecurityConfig)
    setNewPassword(normalized)
    if (value !== normalized) {
      setError(`${passwordPolicyHelperText(savedSecurityConfig)}，不能包含空格、中文或未启用的字符类型`)
    } else if (error?.includes('不能包含空格、中文或未启用的字符类型')) {
      setError(null)
    }
  }

  const handleConfirmPasswordChange = (value: string) => {
    const normalized = normalizePasswordInput(value, savedSecurityConfig)
    setConfirmPassword(normalized)
    if (value !== normalized) {
      setError(`${passwordPolicyHelperText(savedSecurityConfig)}，不能包含空格、中文或未启用的字符类型`)
    } else if (error?.includes('不能包含空格、中文或未启用的字符类型')) {
      setError(null)
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



  const renderSecurityPanel = () => {
    const securityDirty = !securityConfigEqual(securityConfig, savedSecurityConfig)
    const dirtySettingCount = countSecurityConfigChanges(securityConfig, savedSecurityConfig)
    const typeRequirementValid = securityConfig.password_require_letters
      || securityConfig.password_require_digits
      || securityConfig.password_require_symbols

    return (
      <Box>
        <Stack spacing={3}>
          <Card>
            <CardHeader
              avatar={<AdminPanelSettings color="primary" />}
              title="账户安全"
              titleTypographyProps={{ variant: 'h6', fontWeight: 600 }}
              action={
                <Chip
                  label={securityConfig.password_protection_enabled ? '已启用' : '已关闭'}
                  color={securityConfig.password_protection_enabled ? 'success' : 'default'}
                  variant={securityConfig.password_protection_enabled ? 'outlined' : undefined}
                  size="small"
                />
              }
            />
            <CardContent>
              <Typography variant="body2" color="text.secondary">
                控制 Web 管理界面的访问权限，启用密码保护可防止未经授权的修改。
              </Typography>

              <Box
                sx={{
                  mt: 2.5,
                  p: 2,
                  border: '1px solid',
                  borderColor: 'divider',
                  borderRadius: 1.5,
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  gap: 2,
                }}
              >
                <Box minWidth={0}>
                  <Typography fontWeight={700}>启用密码保护</Typography>
                  <Typography variant="body2" color="text.secondary">
                    启用后，进入系统需验证管理员密码。
                  </Typography>
                </Box>
                <Switch
                  checked={securityConfig.password_protection_enabled}
                  onChange={(event) => patchSecurityConfig({ password_protection_enabled: event.target.checked })}
                />
              </Box>

              {!securityConfig.password_protection_enabled && (
                <Alert severity="warning" sx={{ mt: 2 }}>
                  关闭密码保护后，所有 Web 页面和业务 API 将跳过管理员密码校验。
                </Alert>
              )}

              <Divider sx={{ my: 3 }} />

              <Stack spacing={2}>
                <Box display="flex" alignItems="center" gap={1}>
                  <Key color="primary" fontSize="small" />
                  <Typography fontWeight={700}>修改管理员密码</Typography>
                </Box>
                <Grid container spacing={2}>
                  <Grid size={{ xs: 12, md: 6 }}>
                    <TextField
                      label="新密码"
                      type="password"
                      value={newPassword}
                      onChange={(event) => handleNewPasswordChange(event.target.value)}
                      disabled={passwordUpdating}
                      helperText={passwordPolicyHelperText(savedSecurityConfig)}
                      fullWidth
                    />
                    <Box mt={1}>
                      <PasswordStrengthHint password={newPassword} settings={savedSecurityConfig} />
                    </Box>
                  </Grid>
                  <Grid size={{ xs: 12, md: 6 }}>
                    <TextField
                      label="确认新密码"
                      type="password"
                      value={confirmPassword}
                      onChange={(event) => handleConfirmPasswordChange(event.target.value)}
                      disabled={passwordUpdating}
                      fullWidth
                    />
                  </Grid>
                </Grid>
                <Box>
                  <Button
                    variant="contained"
                    onClick={() => void updateAdminPassword()}
                    disabled={passwordUpdating || !newPassword || !confirmPassword}
                    startIcon={passwordUpdating ? <CircularProgress size={16} color="inherit" /> : <Key />}
                  >
                    更新密码
                  </Button>
                </Box>
              </Stack>
            </CardContent>
          </Card>

          <Grid container spacing={3} alignItems="stretch">
            <Grid size={{ xs: 12, md: 6 }} sx={{ display: 'flex' }}>
              <Card sx={{ width: 1, height: '100%', display: 'flex', flexDirection: 'column' }}>
                <CardHeader
                  avatar={<Shield color="primary" />}
                  title="密码策略"
                  titleTypographyProps={{ variant: 'h6', fontWeight: 600 }}
                />
                <CardContent sx={{ flexGrow: 1, display: 'flex', flexDirection: 'column' }}>
                  <Typography variant="body2" color="text.secondary">
                    设定系统接受的管理员密码强度要求，后续首次设置或修改密码时生效。
                  </Typography>

                  <Box display="flex" alignItems="center" justifyContent="space-between" gap={2} mt={3}>
                    <Box>
                      <Typography fontWeight={700}>最小长度</Typography>
                      <Typography variant="caption" color="text.secondary">
                        限制密码的最低字符数
                      </Typography>
                    </Box>
                    <Box
                      sx={(theme) => ({
                        alignItems: 'center',
                        bgcolor: theme.palette.background.paper,
                        border: '1px solid',
                        borderColor: 'divider',
                        borderRadius: 1,
                        display: 'inline-flex',
                        height: 40,
                        overflow: 'hidden',
                      })}
                    >
                      <IconButton
                        aria-label="减少密码最小长度"
                        disabled={securityConfig.password_min_length <= PASSWORD_MIN_LENGTH_MIN}
                        onClick={() => updatePasswordMinLength(securityConfig.password_min_length - 1)}
                        size="small"
                        sx={{ borderRadius: 0, height: 40, width: 40 }}
                      >
                        <Remove fontSize="small" />
                      </IconButton>
                      <InputBase
                        value={passwordMinLengthInput}
                        onBlur={commitPasswordMinLengthInput}
                        onChange={(event) => handlePasswordMinLengthInputChange(event.target.value)}
                        inputProps={{
                          'aria-label': '密码最小长度',
                          inputMode: 'numeric',
                          maxLength: 2,
                          pattern: '[0-9]*',
                        }}
                        aria-live="polite"
                        sx={(theme) => ({
                          alignSelf: 'stretch',
                          borderLeft: `1px solid ${theme.palette.divider}`,
                          borderRight: `1px solid ${theme.palette.divider}`,
                          minWidth: 44,
                          width: 44,
                          px: 1,
                          '& input': {
                            color: theme.palette.text.primary,
                            fontSize: theme.typography.body2.fontSize,
                            fontWeight: 400,
                            height: '100%',
                            p: 0,
                            textAlign: 'center',
                          },
                        })}
                      />
                      <IconButton
                        aria-label="增加密码最小长度"
                        disabled={securityConfig.password_min_length >= PASSWORD_MAX_LENGTH}
                        onClick={() => updatePasswordMinLength(securityConfig.password_min_length + 1)}
                        size="small"
                        sx={{ borderRadius: 0, height: 40, width: 40 }}
                      >
                        <Add fontSize="small" />
                      </IconButton>
                    </Box>
                  </Box>

                  <Divider sx={{ my: 2 }} />

                  <Stack spacing={1.5}>
                    <Box display="flex" alignItems="center" justifyContent="space-between" gap={2}>
                      <Box>
                        <Typography component="div" fontWeight={600}>
                          包含英文字母
                          <Typography component="span" variant="caption" color="text.secondary">
                            （a-z、A-Z）
                          </Typography>
                        </Typography>
                      </Box>
                      <Switch
                        checked={securityConfig.password_require_letters}
                        onChange={(event) => patchSecurityConfig({ password_require_letters: event.target.checked })}
                      />
                    </Box>
                    <Box display="flex" alignItems="center" justifyContent="space-between" gap={2}>
                      <Box>
                        <Typography component="div" fontWeight={600}>
                          包含阿拉伯数字
                          <Typography component="span" variant="caption" color="text.secondary">
                            （0-9）
                          </Typography>
                        </Typography>
                      </Box>
                      <Switch
                        checked={securityConfig.password_require_digits}
                        onChange={(event) => patchSecurityConfig({ password_require_digits: event.target.checked })}
                      />
                    </Box>
                    <Box display="flex" alignItems="center" justifyContent="space-between" gap={2}>
                      <Box>
                        <Typography component="div" fontWeight={600}>
                          包含特殊符号
                          <Typography component="span" variant="caption" color="text.secondary">
                            （! @ # $ 等可见符号）
                          </Typography>
                        </Typography>
                      </Box>
                      <Switch
                        checked={securityConfig.password_require_symbols}
                        onChange={(event) => patchSecurityConfig({ password_require_symbols: event.target.checked })}
                      />
                    </Box>
                  </Stack>

                  {!typeRequirementValid && (
                    <Alert severity="error" sx={{ mt: 2 }}>
                      字符类型要求至少需要选择一项。
                    </Alert>
                  )}
                </CardContent>
              </Card>
            </Grid>

            <Grid size={{ xs: 12, md: 6 }} sx={{ display: 'flex' }}>
              <Card sx={{ width: 1, height: '100%', display: 'flex', flexDirection: 'column' }}>
                <CardHeader
                  avatar={<Timer color="primary" />}
                  title="会话控制"
                  titleTypographyProps={{ variant: 'h6', fontWeight: 600 }}
                />
                <CardContent sx={{ flexGrow: 1, display: 'flex', flexDirection: 'column' }}>
                  <Typography variant="body2" color="text.secondary">
                    管理用户登录状态的有效期以及浏览器空闲自动退出行为。
                  </Typography>

                  <Stack spacing={2.5} mt={3} sx={{ flexGrow: 1 }}>
                    <FormControl fullWidth>
                      <InputLabel>会话有效期</InputLabel>
                      <Select
                        value={securityConfig.session_ttl_seconds}
                        label="会话有效期"
                        onChange={(event) => patchSecurityConfig({ session_ttl_seconds: Number(event.target.value) })}
                      >
                        {SESSION_TTL_OPTIONS.map((option) => (
                          <MenuItem key={option.value} value={option.value}>{option.label}</MenuItem>
                        ))}
                      </Select>
                    </FormControl>

                    <FormControl fullWidth>
                      <InputLabel>空闲超时</InputLabel>
                      <Select
                        value={securityConfig.idle_timeout_seconds}
                        label="空闲超时"
                        onChange={(event) => patchSecurityConfig({ idle_timeout_seconds: Number(event.target.value) })}
                      >
                        {IDLE_TIMEOUT_OPTIONS.map((option) => (
                          <MenuItem key={option.value} value={option.value}>{option.label}</MenuItem>
                        ))}
                      </Select>
                    </FormControl>

                    <Alert severity="warning" sx={{ ...compactCardAlertSx, mt: 'auto' }}>
                      公共网络环境建议设置较短的空闲超时，避免设备被未授权人员操作。
                    </Alert>
                  </Stack>
                </CardContent>
              </Card>
            </Grid>
          </Grid>
        </Stack>

        {isSecurity && bottomActionBarHost && securityDirty && createPortal(
          <Box
            sx={{
              '@keyframes securityActionBarIn': {
                from: {
                  opacity: 0,
                  transform: 'translateY(8px)',
                },
                to: {
                  opacity: 1,
                  transform: 'translateY(0)',
                },
              },
              alignItems: 'center',
              animation: 'securityActionBarIn 180ms ease',
              display: 'flex',
              gap: 1.5,
              justifyContent: 'space-between',
              minWidth: 0,
              width: 1,
            }}
          >
            <Typography
              variant="body2"
              color="warning.main"
              sx={{
                fontWeight: 500,
                minWidth: 0,
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                whiteSpace: 'nowrap',
              }}
            >
              有未保存的设置项：{dirtySettingCount}
            </Typography>
            <Box display="flex" justifyContent="flex-end" gap={1.5} flexShrink={0}>
              <Button
                variant="outlined"
                disabled={securitySaving}
                onClick={resetSecuritySettings}
              >
                还原
              </Button>
              <Button
                variant="contained"
                startIcon={securitySaving ? <CircularProgress size={16} color="inherit" /> : <Save />}
                disabled={securitySaving || !typeRequirementValid}
                onClick={() => void saveSecuritySettings()}
              >
                保存安全设置
              </Button>
            </Box>
          </Box>,
          bottomActionBarHost,
        )}
      </Box>
    )
  }

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
