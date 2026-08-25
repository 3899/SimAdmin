import { useEffect, useState } from 'react'
import { createPortal } from 'react-dom'
import {
  Alert,
  Box,
  Button,
  Card,
  CardContent,
  CardHeader,
  Chip,
  CircularProgress,
  Divider,
  FormControl,
  IconButton,
  InputBase,
  InputLabel,
  MenuItem,
  Select,
  Snackbar,
  Stack,
  Switch,
  TextField,
  Typography,
} from '@mui/material'
import Grid from '@mui/material/Grid'
import {
  Add,
  AdminPanelSettings,
  Key,
  Remove,
  Save,
  Shield,
  Timer,
} from '@mui/icons-material'
import type { SecurityConfig } from '../api/types'
import { PasswordStrengthHint } from './PasswordStrengthHint'
import {
  DEFAULT_SECURITY_SETTINGS,
  PASSWORD_MAX_LENGTH,
  normalizePasswordInput,
  passwordPolicyHelperText,
  validatePasswordAgainstSecurity,
} from '../lib/passwordPolicy'

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

export const SECURITY_SETTINGS_UPDATED_EVENT = 'simadmin-security-settings-updated'

export interface SingleAdminSecuritySettings {
  configured: boolean
  settings: SecurityConfig
}

export interface SingleAdminSecurityClient {
  getSettings(): Promise<SingleAdminSecuritySettings>
  saveSettings(settings: SecurityConfig): Promise<SecurityConfig>
  setupPassword(password: string): Promise<void>
  changePassword(password: string): Promise<void>
}

export interface SingleAdminSecurityPanelProps {
  client: SingleAdminSecurityClient
  actionBarHostId?: string
  cardTitleFontSize?: number | string
  onSettingsSaved?: (settings: SecurityConfig) => void
}

function mergeSecurityConfig(config?: Partial<SecurityConfig>): SecurityConfig {
  return { ...DEFAULT_SECURITY_SETTINGS, ...config }
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

export function SingleAdminSecurityPanel({
  client,
  actionBarHostId,
  cardTitleFontSize,
  onSettingsSaved,
}: SingleAdminSecurityPanelProps) {
  const [loading, setLoading] = useState(true)
  const [configured, setConfigured] = useState(false)
  const [settings, setSettings] = useState<SecurityConfig>(DEFAULT_SECURITY_SETTINGS)
  const [savedSettings, setSavedSettings] = useState<SecurityConfig>(DEFAULT_SECURITY_SETTINGS)
  const [passwordMinLengthInput, setPasswordMinLengthInput] = useState(String(DEFAULT_SECURITY_SETTINGS.password_min_length))
  const [settingsSaving, setSettingsSaving] = useState(false)
  const [passwordUpdating, setPasswordUpdating] = useState(false)
  const [newPassword, setNewPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [message, setMessage] = useState<{ text: string; error?: boolean } | null>(null)
  const [actionBarHost, setActionBarHost] = useState<HTMLElement | null>(null)

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    client.getSettings()
      .then((response) => {
        if (cancelled) return
        const next = mergeSecurityConfig(response.settings)
        setConfigured(response.configured)
        setSettings(next)
        setSavedSettings(next)
        setPasswordMinLengthInput(String(next.password_min_length))
      })
      .catch((error) => {
        if (!cancelled) setMessage({ text: error instanceof Error ? error.message : String(error), error: true })
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => { cancelled = true }
  }, [client])

  useEffect(() => {
    setActionBarHost(actionBarHostId ? document.getElementById(actionBarHostId) : null)
  }, [actionBarHostId])

  const patchSettings = (patch: Partial<SecurityConfig>) => {
    setSettings((current) => ({ ...current, ...patch }))
  }

  const updatePasswordMinLength = (value: number) => {
    const next = Math.min(Math.max(value, PASSWORD_MIN_LENGTH_MIN), PASSWORD_MAX_LENGTH)
    setPasswordMinLengthInput(String(next))
    patchSettings({ password_min_length: next })
  }

  const saveSettings = async () => {
    const validationError = validateSecurityConfig(settings)
    if (validationError) {
      setMessage({ text: validationError, error: true })
      return
    }
    if (settings.password_protection_enabled && !configured) {
      setMessage({ text: '请先设置管理员密码，再启用密码保护', error: true })
      return
    }
    setSettingsSaving(true)
    setMessage(null)
    try {
      const next = mergeSecurityConfig(await client.saveSettings(settings))
      setSettings(next)
      setSavedSettings(next)
      setPasswordMinLengthInput(String(next.password_min_length))
      window.dispatchEvent(new CustomEvent(SECURITY_SETTINGS_UPDATED_EVENT, { detail: next }))
      onSettingsSaved?.(next)
      setMessage({ text: '安全设置已保存' })
    } catch (error) {
      setMessage({ text: error instanceof Error ? error.message : String(error), error: true })
    } finally {
      setSettingsSaving(false)
    }
  }

  const updatePassword = async () => {
    if (!newPassword) {
      setMessage({ text: '请输入新管理员密码', error: true })
      return
    }
    if (newPassword !== confirmPassword) {
      setMessage({ text: '两次输入的新密码不一致', error: true })
      return
    }
    const passwordError = validatePasswordAgainstSecurity(newPassword, savedSettings)
    if (passwordError) {
      setMessage({ text: passwordError, error: true })
      return
    }
    setPasswordUpdating(true)
    setMessage(null)
    try {
      if (configured) await client.changePassword(newPassword)
      else await client.setupPassword(newPassword)
      setConfigured(true)
      setNewPassword('')
      setConfirmPassword('')
      setMessage({ text: configured ? '管理员密码已更新' : '管理员密码已设置' })
    } catch (error) {
      setMessage({ text: error instanceof Error ? error.message : String(error), error: true })
    } finally {
      setPasswordUpdating(false)
    }
  }

  const normalizePassword = (value: string, setter: (next: string) => void) => {
    const normalized = normalizePasswordInput(value, savedSettings)
    setter(normalized)
    if (value !== normalized) {
      setMessage({
        text: `${passwordPolicyHelperText(savedSettings)}，不能包含空格、中文或未启用的字符类型`,
        error: true,
      })
    }
  }

  if (loading) {
    return <Box display="flex" alignItems="center" justifyContent="center" minHeight={280}><CircularProgress /></Box>
  }

  const dirtySettingCount = countSecurityConfigChanges(settings, savedSettings)
  const settingsDirty = dirtySettingCount > 0
  const typeRequirementValid = settings.password_require_letters
    || settings.password_require_digits
    || settings.password_require_symbols
  const titleTypographyProps = cardTitleFontSize
    ? { fontSize: cardTitleFontSize, fontWeight: 600 }
    : { variant: 'h6' as const, fontWeight: 600 }

  const actionButtons = (
    <Box
      sx={{
        alignItems: 'center',
        display: 'flex',
        gap: 1.5,
        justifyContent: 'space-between',
        minWidth: 0,
        width: 1,
      }}
    >
      <Typography variant="body2" color="warning.main" noWrap>
        有未保存的设置项：{dirtySettingCount}
      </Typography>
      <Box display="flex" justifyContent="flex-end" gap={1.5} flexShrink={0}>
        <Button
          variant="outlined"
          disabled={settingsSaving}
          onClick={() => {
            setSettings(savedSettings)
            setPasswordMinLengthInput(String(savedSettings.password_min_length))
          }}
        >
          还原
        </Button>
        <Button
          variant="contained"
          startIcon={settingsSaving ? <CircularProgress size={16} color="inherit" /> : <Save />}
          disabled={settingsSaving || !typeRequirementValid || (settings.password_protection_enabled && !configured)}
          onClick={() => void saveSettings()}
        >
          保存安全设置
        </Button>
      </Box>
    </Box>
  )

  return (
    <Box>
      <Stack spacing={3}>
        <Card>
          <CardHeader
            avatar={<AdminPanelSettings color="primary" />}
            title="账户安全"
            titleTypographyProps={titleTypographyProps}
            action={
              <Chip
                label={settings.password_protection_enabled ? '已启用' : '已关闭'}
                color={settings.password_protection_enabled ? 'success' : 'default'}
                variant={settings.password_protection_enabled ? 'outlined' : undefined}
                size="small"
              />
            }
          />
          <CardContent>
            <Typography variant="body2" color="text.secondary">
              控制 Web 管理界面的访问权限，启用密码保护可防止未经授权的修改。
            </Typography>

            <Box sx={{ mt: 2.5, p: 2, border: '1px solid', borderColor: 'divider', borderRadius: 1.5, display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 2 }}>
              <Box minWidth={0}>
                <Typography fontWeight={700}>启用密码保护</Typography>
                <Typography variant="body2" color="text.secondary">
                  {configured ? '启用后，进入系统需验证管理员密码。' : '请先在下方设置管理员密码。'}
                </Typography>
              </Box>
              <Switch
                checked={settings.password_protection_enabled}
                disabled={!configured}
                onChange={(event) => patchSettings({ password_protection_enabled: event.target.checked })}
              />
            </Box>

            {!settings.password_protection_enabled && (
              <Alert severity="warning" sx={{ mt: 2 }}>
                关闭密码保护后，所有 Web 页面和业务 API 将跳过管理员密码校验。
              </Alert>
            )}

            <Divider sx={{ my: 3 }} />

            <Stack spacing={2}>
              <Box display="flex" alignItems="center" gap={1}>
                <Key color="primary" fontSize="small" />
                <Typography fontWeight={700}>{configured ? '修改管理员密码' : '设置管理员密码'}</Typography>
              </Box>
              <Grid container spacing={2}>
                <Grid size={{ xs: 12, md: 6 }}>
                  <TextField
                    label="新密码"
                    type="password"
                    value={newPassword}
                    onChange={(event) => normalizePassword(event.target.value, setNewPassword)}
                    disabled={passwordUpdating}
                    helperText={passwordPolicyHelperText(savedSettings)}
                    fullWidth
                  />
                  <Box mt={1}><PasswordStrengthHint password={newPassword} settings={savedSettings} /></Box>
                </Grid>
                <Grid size={{ xs: 12, md: 6 }}>
                  <TextField
                    label="确认新密码"
                    type="password"
                    value={confirmPassword}
                    onChange={(event) => normalizePassword(event.target.value, setConfirmPassword)}
                    disabled={passwordUpdating}
                    fullWidth
                  />
                </Grid>
              </Grid>
              <Box>
                <Button
                  variant="contained"
                  onClick={() => void updatePassword()}
                  disabled={passwordUpdating || !newPassword || !confirmPassword}
                  startIcon={passwordUpdating ? <CircularProgress size={16} color="inherit" /> : <Key />}
                >
                  {configured ? '更新密码' : '设置密码'}
                </Button>
              </Box>
            </Stack>
          </CardContent>
        </Card>

        <Grid container spacing={3} alignItems="stretch">
          <Grid size={{ xs: 12, md: 6 }} sx={{ display: 'flex' }}>
            <Card sx={{ width: 1, height: '100%', display: 'flex', flexDirection: 'column' }}>
              <CardHeader avatar={<Shield color="primary" />} title="密码策略" titleTypographyProps={titleTypographyProps} />
              <CardContent sx={{ flexGrow: 1, display: 'flex', flexDirection: 'column' }}>
                <Typography variant="body2" color="text.secondary">
                  设定系统接受的管理员密码强度要求，后续首次设置或修改密码时生效。
                </Typography>

                <Box display="flex" alignItems="center" justifyContent="space-between" gap={2} mt={3}>
                  <Box>
                    <Typography fontWeight={700}>最小长度</Typography>
                    <Typography variant="caption" color="text.secondary">限制密码的最低字符数</Typography>
                  </Box>
                  <Box sx={{ alignItems: 'center', bgcolor: 'background.paper', border: '1px solid', borderColor: 'divider', borderRadius: 1, display: 'inline-flex', height: 40, overflow: 'hidden' }}>
                    <IconButton
                      aria-label="减少密码最小长度"
                      disabled={settings.password_min_length <= PASSWORD_MIN_LENGTH_MIN}
                      onClick={() => updatePasswordMinLength(settings.password_min_length - 1)}
                      size="small"
                      sx={{ borderRadius: 0, height: 40, width: 40 }}
                    >
                      <Remove fontSize="small" />
                    </IconButton>
                    <InputBase
                      value={passwordMinLengthInput}
                      onBlur={() => {
                        if (!passwordMinLengthInput) setPasswordMinLengthInput(String(settings.password_min_length))
                      }}
                      onChange={(event) => {
                        const digits = event.target.value.replace(/\D/g, '').slice(0, 2)
                        if (!digits) setPasswordMinLengthInput('')
                        else updatePasswordMinLength(Number(digits))
                      }}
                      inputProps={{ 'aria-label': '密码最小长度', inputMode: 'numeric', maxLength: 2, pattern: '[0-9]*' }}
                      sx={{ alignSelf: 'stretch', borderLeft: '1px solid', borderRight: '1px solid', borderColor: 'divider', minWidth: 44, width: 44, px: 1, '& input': { fontSize: '0.875rem', height: '100%', p: 0, textAlign: 'center' } }}
                    />
                    <IconButton
                      aria-label="增加密码最小长度"
                      disabled={settings.password_min_length >= PASSWORD_MAX_LENGTH}
                      onClick={() => updatePasswordMinLength(settings.password_min_length + 1)}
                      size="small"
                      sx={{ borderRadius: 0, height: 40, width: 40 }}
                    >
                      <Add fontSize="small" />
                    </IconButton>
                  </Box>
                </Box>

                <Divider sx={{ my: 2 }} />
                <Stack spacing={1.5}>
                  {([
                    ['password_require_letters', '包含英文字母', '（a-z、A-Z）'],
                    ['password_require_digits', '包含阿拉伯数字', '（0-9）'],
                    ['password_require_symbols', '包含特殊符号', '（! @ # $ 等可见符号）'],
                  ] as const).map(([key, label, detail]) => (
                    <Box key={key} display="flex" alignItems="center" justifyContent="space-between" gap={2}>
                      <Typography component="div" fontWeight={600}>
                        {label}<Typography component="span" variant="caption" color="text.secondary">{detail}</Typography>
                      </Typography>
                      <Switch checked={settings[key]} onChange={(event) => patchSettings({ [key]: event.target.checked })} />
                    </Box>
                  ))}
                </Stack>
                {!typeRequirementValid && <Alert severity="error" sx={{ mt: 2 }}>字符类型要求至少需要选择一项。</Alert>}
              </CardContent>
            </Card>
          </Grid>

          <Grid size={{ xs: 12, md: 6 }} sx={{ display: 'flex' }}>
            <Card sx={{ width: 1, height: '100%', display: 'flex', flexDirection: 'column' }}>
              <CardHeader avatar={<Timer color="primary" />} title="会话控制" titleTypographyProps={titleTypographyProps} />
              <CardContent sx={{ flexGrow: 1, display: 'flex', flexDirection: 'column' }}>
                <Typography variant="body2" color="text.secondary">
                  管理用户登录状态的有效期以及浏览器空闲自动退出行为。
                </Typography>
                <Stack spacing={2.5} mt={3} sx={{ flexGrow: 1 }}>
                  <FormControl fullWidth>
                    <InputLabel>会话有效期</InputLabel>
                    <Select
                      value={settings.session_ttl_seconds}
                      label="会话有效期"
                      onChange={(event) => patchSettings({ session_ttl_seconds: Number(event.target.value) })}
                    >
                      {SESSION_TTL_OPTIONS.map((option) => <MenuItem key={option.value} value={option.value}>{option.label}</MenuItem>)}
                    </Select>
                  </FormControl>
                  <FormControl fullWidth>
                    <InputLabel>空闲超时</InputLabel>
                    <Select
                      value={settings.idle_timeout_seconds}
                      label="空闲超时"
                      onChange={(event) => patchSettings({ idle_timeout_seconds: Number(event.target.value) })}
                    >
                      {IDLE_TIMEOUT_OPTIONS.map((option) => <MenuItem key={option.value} value={option.value}>{option.label}</MenuItem>)}
                    </Select>
                  </FormControl>
                  <Alert severity="warning" sx={{ mt: 'auto' }}>
                    公共网络环境建议设置较短的空闲超时，避免设备被未授权人员操作。
                  </Alert>
                </Stack>
              </CardContent>
            </Card>
          </Grid>
        </Grid>

        {settingsDirty && !actionBarHost && actionButtons}
      </Stack>

      {settingsDirty && actionBarHost && createPortal(actionButtons, actionBarHost)}
      <Snackbar
        open={Boolean(message)}
        autoHideDuration={message?.error ? 5000 : 3000}
        onClose={() => setMessage(null)}
        anchorOrigin={{ vertical: 'top', horizontal: 'center' }}
      >
        <Alert severity={message?.error ? 'error' : 'success'} variant="filled" onClose={() => setMessage(null)}>
          {message?.text}
        </Alert>
      </Snackbar>
    </Box>
  )
}
