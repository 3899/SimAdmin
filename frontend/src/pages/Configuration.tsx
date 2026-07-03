import { useEffect, useState } from 'react'
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
  Dialog,
  DialogActions,
  DialogContent,
  DialogContentText,
  DialogTitle,
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
  CheckCircle,
  FlightTakeoff,
  Key,
  Memory,
  Remove,
  Save,
  Shield,
  SimCard,
  Timer,
  Wifi,
} from '@mui/icons-material'
import type { Theme } from '@mui/material/styles'
import { api } from '../api/current'
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
import { useWorkMode } from '../contexts/WorkModeContext'
import type { AirplaneModeResponse, SecurityConfig, VowifiConfig, WorkMode } from '../api/types'

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



function modeLabel(mode: WorkMode) {
  return mode === 'esim' ? 'eSIM 卡' : '普通 SIM 卡'
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



export default function ConfigurationPage() {
  const { mode, refreshWorkMode } = useWorkMode()
  const location = useLocation()
  const isSecurity = location.pathname === '/config/security'
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState<string | null>(null)
  const [dataStatus, setDataStatus] = useState(false)
  const [airplaneMode, setAirplaneMode] = useState<AirplaneModeResponse | null>(null)
  const [vowifiConfig, setVowifiConfig] = useState<VowifiConfig | null>(null)
  const [vowifiFeatureSwitching, setVowifiFeatureSwitching] = useState(false)
  const [airplaneSwitching, setAirplaneSwitching] = useState(false)
  const [healthStatus, setHealthStatus] = useState<HealthStatus | null>(null)
  const [healthLoading, setHealthLoading] = useState(false)
  const [pendingMode, setPendingMode] = useState<WorkMode | null>(null)
  const [modeSwitching, setModeSwitching] = useState(false)
  const [authConfigured, setAuthConfigured] = useState(false)
  const [securityConfig, setSecurityConfig] = useState<SecurityConfig>(() => DEFAULT_SECURITY_CONFIG)
  const [savedSecurityConfig, setSavedSecurityConfig] = useState<SecurityConfig>(() => DEFAULT_SECURITY_CONFIG)
  const [passwordMinLengthInput, setPasswordMinLengthInput] = useState(String(DEFAULT_SECURITY_CONFIG.password_min_length))
  const [securitySaving, setSecuritySaving] = useState(false)
  const [passwordUpdating, setPasswordUpdating] = useState(false)
  const [newPassword, setNewPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [bottomActionBarHost, setBottomActionBarHost] = useState<HTMLElement | null>(null)

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
      const [dataRes, airplaneModeRes, authSettingsRes, vowifiControlRes] = await Promise.all([
        api.getDataStatus(),
        api.getAirplaneMode(),
        api.getAuthSettings(),
        api.getVowifiControl(),
      ])

      if (dataRes.data) setDataStatus(dataRes.data.active)
      if (airplaneModeRes.data) setAirplaneMode(airplaneModeRes.data)
      if (vowifiControlRes.data) setVowifiConfig(vowifiControlRes.data)
      if (authSettingsRes.data) {
        const loadedSecurityConfig = mergeSecurityConfig(authSettingsRes.data.settings)
        setAuthConfigured(authSettingsRes.data.configured)
        setSecurityConfig(loadedSecurityConfig)
        setSavedSecurityConfig(loadedSecurityConfig)
        setPasswordMinLengthInput(String(loadedSecurityConfig.password_min_length))
      }
      await checkHealth()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    void loadData()
    const interval = window.setInterval(() => {
      void checkHealth()
    }, 30000)
    return () => window.clearInterval(interval)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

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

  const toggleVowifiFeature = async () => {
    const snapshot = vowifiConfig
    const nextEnabled = !snapshot?.feature_enabled
    if (snapshot) {
      setVowifiConfig({
        ...snapshot,
        feature_enabled: nextEnabled,
        connection_enabled: nextEnabled ? snapshot.connection_enabled : false,
      })
    }
    setVowifiFeatureSwitching(true)
    try {
      const response = await api.setVowifiFeature(nextEnabled)
      if (response.data) setVowifiConfig(response.data)
      setSuccess(`WiFi Calling 功能模块已${nextEnabled ? '开启' : '关闭'}`)
    } catch (err) {
      if (snapshot) setVowifiConfig(snapshot)
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setVowifiFeatureSwitching(false)
    }
  }

  const confirmModeSwitch = async () => {
    if (!pendingMode) return
    setModeSwitching(true)
    setError(null)
    setSuccess(null)
    try {
      await api.setWorkMode(pendingMode)
      await refreshWorkMode()
      setSuccess(`工作模式已切换为${modeLabel(pendingMode)}`)
      setPendingMode(null)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setModeSwitching(false)
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
                  boxShadow: (theme) => `0 0 0 5px ${healthOk
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

  const renderModeOption = (targetMode: WorkMode) => {
    const selected = mode === targetMode
    const Icon = targetMode === 'esim' ? Memory : SimCard
    return (
      <Box
        role="button"
        tabIndex={0}
        onClick={() => {
          if (!modeSwitching && !selected) setPendingMode(targetMode)
        }}
        onKeyDown={(event) => {
          if ((event.key === 'Enter' || event.key === ' ') && !modeSwitching && !selected) {
            setPendingMode(targetMode)
          }
        }}
        sx={{
          position: 'relative',
          cursor: selected || modeSwitching ? 'default' : 'pointer',
          height: '100%',
          minHeight: 92,
          p: 1.5,
          borderRadius: 1,
          border: '1px solid',
          borderColor: selected ? 'primary.main' : 'divider',
          bgcolor: selected ? 'rgba(25, 118, 210, 0.06)' : 'background.paper',
          boxShadow: selected ? '0 0 0 1px rgba(25, 118, 210, 0.28) inset' : 'none',
          color: 'text.primary',
          transition: 'border-color 150ms ease, background-color 150ms ease, box-shadow 150ms ease, transform 150ms ease',
          '&:hover': selected || modeSwitching ? {} : {
            borderColor: 'primary.light',
            bgcolor: 'action.hover',
            transform: 'translateY(-1px)',
          },
        }}
      >
        <Box display="flex" alignItems="center" gap={1.25} mb={1}>
          <Icon color="primary" fontSize="small" />
          <Typography fontWeight={700}>{modeLabel(targetMode)}</Typography>
          <Box flexGrow={1} />
          {selected && <CheckCircle color="primary" fontSize="small" />}
        </Box>
        <Typography variant="body2" color="text.secondary">
          {targetMode === 'esim'
            ? '开放 eUICC Profile 管理能力，用于管理插入设备的实体 eSIM 卡。'
            : '隐藏 eSIM 管理模块，并阻止 eSIM Profile 管理接口。'}
        </Typography>
      </Box>
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
        {renderHealthBadge()}
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
          <Card>
            <CardHeader
              avatar={<SimCard color="primary" />}
              title="工作模式"
              titleTypographyProps={{ variant: 'h6', fontWeight: 600 }}
              action={
                <Chip
                  label={mode === 'esim' ? 'eSIM 已启用' : '普通 SIM 已启用'}
                  color="primary"
                  variant="outlined"
                  size="small"
                  sx={primaryStatusChipSx}
                />
              }
            />
            <CardContent>
              <Typography variant="body2" color="text.secondary" paragraph>
                工作模式只控制 eSIM 管理功能是否开放，不切换设备硬件。普通 SIM 模式下不会加载 eSIM 管理页面，也不会调用 lpac。
              </Typography>
              <Grid container spacing={2}>
                <Grid size={{ xs: 12, md: 6 }}>
                  {renderModeOption('sim')}
                </Grid>
                <Grid size={{ xs: 12, md: 6 }}>
                  {renderModeOption('esim')}
                </Grid>
              </Grid>
            </CardContent>
          </Card>

          <Card>
            <CardHeader
              avatar={<Wifi color={vowifiConfig?.feature_enabled ? 'primary' : 'disabled'} />}
              title="WiFi Calling 功能模块"
              titleTypographyProps={{ variant: 'h6', fontWeight: 600 }}
              action={
                <Chip
                  label={vowifiConfig?.feature_enabled ? '已开启' : '已关闭'}
                  color={vowifiConfig?.feature_enabled ? 'primary' : 'default'}
                  variant={vowifiConfig?.feature_enabled ? 'outlined' : undefined}
                  size="small"
                  sx={vowifiConfig?.feature_enabled ? primaryStatusChipSx : undefined}
                />
              }
            />
            <CardContent>
              <Typography variant="body2" color="text.secondary" paragraph>
                本开关仅控制 WiFi Calling 模块的显示状态与相关能力开放，不会自动连接 WiFi 通话。
              </Typography>
              <Divider sx={{ my: 2 }} />
              <FormControlLabel
                control={
                  <Switch
                    checked={vowifiConfig?.feature_enabled || false}
                    onChange={() => void toggleVowifiFeature()}
                    disabled={vowifiFeatureSwitching}
                    color="primary"
                  />
                }
                label={
                  <Box display="flex" alignItems="center" gap={1}>
                    {vowifiFeatureSwitching && <CircularProgress size={16} />}
                    <Box>
                      <Typography variant="body1" fontWeight={600}>
                        {vowifiConfig?.feature_enabled ? 'WiFi Calling 功能模块已开启' : 'WiFi Calling 功能模块已关闭'}
                      </Typography>
                      <Typography variant="caption" color="text.secondary">
                        {vowifiConfig?.feature_enabled ? '您可在「仪表盘 - 快捷控制」卡片，或「SIM 卡 - WiFi Calling」页签中，手动开启 WiFi Calling 连接。' : '系统将完全隐藏所有 WiFi Calling 相关入口，不加载任何对应进程，避免额外占用系统资源。'}
                      </Typography>
                    </Box>
                  </Box>
                }
              />
            </CardContent>
          </Card>

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

      <Dialog open={!!pendingMode} onClose={() => !modeSwitching && setPendingMode(null)} maxWidth="sm" fullWidth>
        <DialogTitle>确认切换工作模式</DialogTitle>
        <DialogContent>
          <DialogContentText>
            确定要切换为{pendingMode ? modeLabel(pendingMode) : ''}吗？
          </DialogContentText>
          {pendingMode === 'sim' && (
            <Alert severity="info" sx={{ mt: 2 }}>
              切换后将隐藏 eSIM 管理模块，并阻止 eSIM Profile 管理接口。
            </Alert>
          )}
          {pendingMode === 'esim' && (
            <Alert severity="info" sx={{ mt: 2 }}>
              切换后将显示 eSIM 管理模块，打开页面或执行操作时才会按需调用 lpac。
            </Alert>
          )}
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setPendingMode(null)} disabled={modeSwitching}>取消</Button>
          <Button
            variant="contained"
            color="primary"
            onClick={() => void confirmModeSwitch()}
            disabled={modeSwitching}
            startIcon={modeSwitching ? <CircularProgress size={16} /> : undefined}
          >
            确认切换
          </Button>
        </DialogActions>
      </Dialog>
    </Box>
  )
}
