import { FormEvent, useEffect, useMemo, useState } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import {
  Alert,
  Box,
  Button,
  Card,
  CircularProgress,
  IconButton,
  InputBase,
  Link,
  Stack,
  Tooltip,
  Typography,
  useTheme,
} from '@mui/material'
import {
  GitHub as GitHubIcon,
  KeyboardArrowRight as ArrowIcon,
  LockOutlined as LockIcon,
  Star as StarIcon,
} from '@mui/icons-material'
import { api } from '../api/current'
import type { SecurityConfig } from '../api/types'
import { PasswordStrengthHint } from '../components/PasswordStrengthHint'
import {
  DEFAULT_SECURITY_SETTINGS,
  PASSWORD_MAX_LENGTH,
  analyzePassword,
  enabledPasswordTypesText,
  normalizePasswordInput,
} from '../lib/passwordPolicy'
import qqQrUrl from '../../../static/Community/QQGroup_Light.png'
import tgQrUrl from '../../../static/Community/TG_Chat.png'

type AuthMode = 'login' | 'setup'

function getNextPath(next: string | null) {
  if (!next || !next.startsWith('/') || next.startsWith('/login')) return '/'
  return next
}

function LogoMark({ active }: { active: boolean }) {
  return (
    <Box
      sx={{
        width: 136,
        height: 136,
        borderRadius: '50%',
        display: 'grid',
        placeItems: 'center',
        bgcolor: (theme) => theme.palette.mode === 'light' ? 'rgba(255,255,255,0.44)' : 'rgba(255,255,255,0.06)',
        border: '1px solid',
        borderColor: (theme) => theme.palette.mode === 'light' ? 'rgba(255,255,255,0.74)' : 'rgba(148,163,184,0.18)',
        boxShadow: (theme) => theme.palette.mode === 'light'
          ? 'inset 0 1px 0 rgba(255,255,255,0.72), 0 16px 32px -24px rgba(15,23,42,0.42)'
          : 'inset 0 1px 0 rgba(255,255,255,0.08), 0 18px 34px -26px rgba(0,0,0,0.82)',
        transition: 'box-shadow 180ms ease, transform 180ms ease',
        transform: active ? 'translateY(-1px)' : 'none',
        overflow: 'hidden',
      }}
    >
      <Box
        component="img"
        src="/simadmin-logo.svg"
        alt="SimAdmin"
        sx={{
          width: 132,
          height: 132,
          display: 'block',
          filter: active
            ? 'drop-shadow(0 0 12px rgba(18,150,219,0.42))'
            : 'drop-shadow(0 14px 18px rgba(15,23,42,0.18))',
          transition: 'transform 180ms ease',
          '&:hover': { transform: 'scale(1.04)' },
        }}
      />
    </Box>
  )
}

function CommunityTooltip() {
  return (
    <Tooltip
      arrow
      placement="top"
      slotProps={{
        tooltip: {
          sx: {
            p: 0,
            maxWidth: 'none',
            bgcolor: 'rgba(255,255,255,0.94)',
            color: '#334155',
            border: '1px solid rgba(226,232,240,0.9)',
            boxShadow: '0 18px 48px -24px rgba(15,23,42,0.38)',
            backdropFilter: 'blur(18px)',
            WebkitBackdropFilter: 'blur(18px)',
          },
        },
        arrow: {
          sx: {
            color: 'rgba(255,255,255,0.94)',
            '&::before': {
              border: '1px solid rgba(226,232,240,0.9)',
              boxSizing: 'border-box',
            },
          },
        },
      }}
      title={(
        <Stack direction="row" spacing={2} alignItems="center" sx={{ p: 1.5 }}>
          <Stack spacing={0.75} alignItems="center">
            <Box
              component="img"
              src={qqQrUrl}
              alt="SimAdmin QQ 交流群"
              sx={{
                height: 132,
                width: 'auto',
                maxWidth: 240,
                objectFit: 'contain',
                borderRadius: 1,
                bgcolor: '#fff',
              }}
            />
            <Typography variant="caption" sx={{ color: 'text.secondary', whiteSpace: 'nowrap', fontWeight: 600 }}>
              QQ 交流群
            </Typography>
          </Stack>
          <Stack spacing={0.75} alignItems="center">
            <Box
              component="img"
              src={tgQrUrl}
              alt="SimAdmin TG 交流群"
              sx={{
                height: 132,
                width: 'auto',
                maxWidth: 240,
                objectFit: 'contain',
                borderRadius: 1,
                bgcolor: '#fff',
              }}
            />
            <Typography variant="caption" sx={{ color: 'text.secondary', whiteSpace: 'nowrap', fontWeight: 600 }}>
              Telegram 群
            </Typography>
          </Stack>
        </Stack>
      )}
    >
      <Link component="button" type="button" underline="none" color="inherit" sx={{ font: 'inherit' }}>
        社区
      </Link>
    </Tooltip>
  )
}

function ForgotPasswordTooltip({ visible }: { visible: boolean }) {
  return (
    <Tooltip
      arrow
      placement="top-end"
      slotProps={{
        tooltip: {
          sx: {
            p: 0,
            maxWidth: 360,
            bgcolor: 'rgba(255,255,255,0.94)',
            color: '#334155',
            border: '1px solid rgba(226,232,240,0.9)',
            boxShadow: '0 18px 48px -24px rgba(15,23,42,0.38)',
            backdropFilter: 'blur(18px)',
            WebkitBackdropFilter: 'blur(18px)',
          },
        },
        arrow: {
          sx: {
            color: 'rgba(255,255,255,0.94)',
            '&::before': {
              border: '1px solid rgba(226,232,240,0.9)',
              boxSizing: 'border-box',
            },
          },
        },
      }}
      title={(
        <Stack spacing={1} sx={{ p: 1.5 }}>
          <Typography variant="body2">
            忘记密码可通过 ADB/SSH 登录设备执行命令操作
          </Typography>
          <Typography variant="caption" component="div" sx={{ color: 'text.secondary', lineHeight: 1.7 }}>
            重置密码：
            <Box component="code" sx={{ fontFamily: 'monospace', color: 'text.primary' }}>
              /opt/simadmin/simadmin auth reset-password
            </Box>
          </Typography>
          <Typography variant="caption" component="div" sx={{ color: 'text.secondary', lineHeight: 1.7 }}>
            清除密码：
            <Box component="code" sx={{ fontFamily: 'monospace', color: 'text.primary' }}>
              /opt/simadmin/simadmin auth clear
            </Box>
          </Typography>
        </Stack>
      )}
    >
      <Link
        component="button"
        type="button"
        tabIndex={visible ? 0 : -1}
        underline="none"
        color="text.secondary"
        sx={{
          display: 'inline-flex',
          alignItems: 'center',
          fontSize: 13,
          fontWeight: 400,
          '&:hover': { color: 'primary.main' },
        }}
      >
        忘记密码
      </Link>
    </Tooltip>
  )
}

export default function Login() {
  const theme = useTheme()
  const navigate = useNavigate()
  const [searchParams] = useSearchParams()
  const nextPath = useMemo(() => getNextPath(searchParams.get('next')), [searchParams])
  const [mode, setMode] = useState<AuthMode>('login')
  const [password, setPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [focused, setFocused] = useState(false)
  const [loading, setLoading] = useState(false)
  const [checking, setChecking] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [authSettings, setAuthSettings] = useState<SecurityConfig>(DEFAULT_SECURITY_SETTINGS)
  const [showForgotPassword, setShowForgotPassword] = useState(false)
  const passwordAnalysis = useMemo(() => analyzePassword(password, authSettings), [authSettings, password])

  const handlePasswordChange = (value: string) => {
    const normalized = normalizePasswordInput(value, mode === 'setup' ? authSettings : undefined)
    setPassword(normalized)
    setShowForgotPassword(false)
    if (value !== normalized) {
      const allowedText = mode === 'setup' ? enabledPasswordTypesText(authSettings) : '英文字母、数字和符号'
      setError(`密码只能包含${allowedText}，不能包含空格或中文。`)
    } else if (error?.includes('不能包含空格或中文')) {
      setError(null)
    }
  }

  const handleConfirmPasswordChange = (value: string) => {
    const normalized = normalizePasswordInput(value, authSettings)
    setConfirmPassword(normalized)
    if (value !== normalized) {
      setError(`密码只能包含${enabledPasswordTypesText(authSettings)}，不能包含空格或中文。`)
    } else if (error?.includes('不能包含空格或中文')) {
      setError(null)
    }
  }

  useEffect(() => {
    let cancelled = false
    api.getAuthStatus()
      .then((response) => {
        if (cancelled) return
        const status = response.data
        if (status?.settings) setAuthSettings(status.settings)
        if (status?.authenticated) {
          void navigate(nextPath, { replace: true })
          return
        }
        setMode(status?.configured === false ? 'setup' : 'login')
      })
      .catch(() => {
        if (!cancelled) setMode('login')
      })
      .finally(() => {
        if (!cancelled) setChecking(false)
      })
    return () => { cancelled = true }
  }, [navigate, nextPath])

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    setError(null)
    setShowForgotPassword(false)

    if (!password) {
      setError(mode === 'setup' ? '请设置管理员密码。' : '请输入管理员密码。')
      return
    }

    if (mode === 'setup') {
      if (!passwordAnalysis.charsOk) {
        setError(`密码只能包含${enabledPasswordTypesText(authSettings)}，不能包含空格或中文。`)
        return
      }
      if (!passwordAnalysis.lengthOk) {
        setError(`密码长度需为 ${authSettings.password_min_length}-${PASSWORD_MAX_LENGTH} 个字符。`)
        return
      }
      if (!passwordAnalysis.valid) {
        setError('密码不符合安全要求，请根据上方规则调整。')
        return
      }
    }
    if (mode === 'setup' && password !== confirmPassword) {
      setError('两次输入的密码不一致')
      return
    }

    setLoading(true)
    try {
      if (mode === 'setup') {
        await api.setupAdminPassword(password)
      } else {
        await api.login(password)
      }
      void navigate(nextPath, { replace: true })
    } catch (err) {
      const message = err instanceof Error ? err.message : '登录失败'
      setError(message)
      if (mode === 'login' && message.includes('密码不正确')) {
        setShowForgotPassword(true)
      }
    } finally {
      setLoading(false)
    }
  }

  const title = mode === 'setup' ? '设置管理员密码' : 'SimAdmin'
  const subtitle = mode === 'setup' ? '此密码用于保护本设备的管理后台' : '开源 SIM/eSIM 设备管理后台'

  return (
    <Box
      sx={{
        minHeight: '100vh',
        minWidth: 320,
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        px: 2,
        py: 4,
        position: 'relative',
        overflow: 'hidden',
        bgcolor: 'background.default',
        color: 'text.primary',
        '&::before': {
          content: '""',
          position: 'fixed',
          inset: 0,
          pointerEvents: 'none',
          background: theme.palette.mode === 'light'
            ? 'radial-gradient(circle at 6% 2%, rgba(147,197,253,0.52), transparent 32%), radial-gradient(circle at 96% 22%, rgba(196,181,253,0.38), transparent 34%), radial-gradient(circle at 42% 108%, rgba(110,231,183,0.28), transparent 34%)'
            : 'radial-gradient(circle at 8% 0%, rgba(18,150,219,0.28), transparent 34%), radial-gradient(circle at 98% 24%, rgba(124,58,237,0.24), transparent 34%), radial-gradient(circle at 42% 110%, rgba(16,185,129,0.16), transparent 36%)',
        },
      }}
    >
      <Stack spacing={3} alignItems="center" sx={{ width: '100%', maxWidth: 430, position: 'relative', zIndex: 1 }}>
        <Stack spacing={0.8} alignItems="center" textAlign="center">
          <Typography variant="h4" sx={{ fontWeight: 800, letterSpacing: 0 }}>
            {title}
          </Typography>
          <Typography variant="body1" color="text.secondary">
            {subtitle}
          </Typography>
        </Stack>

        <Card
          sx={{
            width: '100%',
            p: { xs: 3, sm: 4 },
            borderRadius: 3,
            position: 'relative',
          }}
        >
          {checking ? (
            <Box sx={{ minHeight: 318, display: 'grid', placeItems: 'center' }}>
              <CircularProgress size={30} />
            </Box>
          ) : (
            <Stack component="form" spacing={2.5} alignItems="center" noValidate onSubmit={(event) => { void handleSubmit(event) }}>
              <LogoMark active={focused || loading} />

              <Stack spacing={1.5} sx={{ width: '100%' }}>
                <Box
                  sx={{
                    display: 'flex',
                    alignItems: 'center',
                    minHeight: 52,
                    overflow: 'hidden',
                    borderRadius: 1.5,
                    border: '1px solid',
                    borderColor: focused ? 'primary.main' : 'divider',
                    bgcolor: theme.palette.mode === 'light' ? 'rgba(255,255,255,0.62)' : 'rgba(2,6,23,0.34)',
                    boxShadow: focused ? `0 0 0 3px ${theme.palette.primary.main}22` : 'none',
                    transition: 'border-color 160ms ease, box-shadow 160ms ease, background-color 160ms ease',
                  }}
                >
                  <LockIcon sx={{ ml: 1.7, mr: 0.5, color: 'text.secondary', fontSize: 20 }} />
                  <InputBase
                    type="password"
                    value={password}
                    onChange={(event) => handlePasswordChange(event.target.value)}
                    onFocus={() => setFocused(true)}
                    onBlur={() => setFocused(false)}
                    placeholder={mode === 'setup' ? '设置管理员密码' : '管理员密码'}
                    autoFocus
                    required
                    inputProps={{ maxLength: PASSWORD_MAX_LENGTH }}
                    sx={{ flex: 1, px: 1.2, py: 1.1, fontSize: 16 }}
                  />
                  {mode === 'login' && (
                    <IconButton
                      type="submit"
                      aria-label="登录"
                      disabled={loading}
                      sx={{
                        alignSelf: 'stretch',
                        px: 2,
                        borderRadius: 0,
                        borderLeft: '1px solid',
                        borderColor: 'divider',
                      }}
                    >
                      {loading ? <CircularProgress size={20} color="inherit" /> : <ArrowIcon />}
                    </IconButton>
                  )}
                </Box>

                {mode === 'setup' && <PasswordStrengthHint password={password} settings={authSettings} />}

                {mode === 'setup' && (
                  <>
                    <Box
                      sx={{
                        display: 'flex',
                        alignItems: 'center',
                        minHeight: 52,
                        overflow: 'hidden',
                        borderRadius: 1.5,
                        border: '1px solid',
                        borderColor: 'divider',
                        bgcolor: theme.palette.mode === 'light' ? 'rgba(255,255,255,0.62)' : 'rgba(2,6,23,0.34)',
                      }}
                    >
                      <LockIcon sx={{ ml: 1.7, mr: 0.5, color: 'text.secondary', fontSize: 20 }} />
                      <InputBase
                        type="password"
                        value={confirmPassword}
                        onChange={(event) => handleConfirmPasswordChange(event.target.value)}
                        placeholder="确认管理员密码"
                        required
                        inputProps={{ maxLength: PASSWORD_MAX_LENGTH }}
                        sx={{ flex: 1, px: 1.2, py: 1.1, fontSize: 16 }}
                      />
                    </Box>
                    <Button
                      type="submit"
                      variant="contained"
                      size="large"
                      fullWidth
                      disabled={loading}
                      sx={{ minHeight: 46 }}
                    >
                      {loading ? <CircularProgress size={20} color="inherit" /> : '保存密码'}
                    </Button>
                  </>
                )}
              </Stack>

              {error && <Alert severity="error" sx={{ width: '100%' }}>{error}</Alert>}

              <Box sx={{ width: '100%', minHeight: 24, position: 'relative', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                <Link
                  href="https://github.com/3899/SimAdmin"
                  target="_blank"
                  rel="noopener noreferrer"
                  underline="none"
                  color="text.secondary"
                  sx={{
                    display: 'inline-flex',
                    alignItems: 'center',
                    gap: 0.75,
                    fontSize: 14,
                    fontWeight: 600,
                    '&:hover': { color: 'primary.main' },
                  }}
                >
                  <GitHubIcon sx={{ fontSize: 18 }} />
                  点亮 Star
                  <StarIcon sx={{ fontSize: 18, color: '#facc15' }} />
                </Link>
                {mode === 'login' && (
                  <Box
                    sx={{
                      position: 'absolute',
                      right: 0,
                      top: '50%',
                      opacity: showForgotPassword ? 1 : 0,
                      pointerEvents: showForgotPassword ? 'auto' : 'none',
                      transform: showForgotPassword ? 'translateY(-50%)' : 'translate(8px, -50%)',
                      transition: 'opacity 180ms ease, transform 180ms ease',
                    }}
                    aria-hidden={!showForgotPassword}
                  >
                    <ForgotPasswordTooltip visible={showForgotPassword} />
                  </Box>
                )}
              </Box>
            </Stack>
          )}
        </Card>

        <Stack
          direction="row"
          spacing={1.5}
          alignItems="center"
          justifyContent="center"
          flexWrap="wrap"
          color="text.secondary"
          sx={{ fontSize: 13 }}
        >
          <Link
            href="https://github.com/3899/SimAdmin"
            target="_blank"
            rel="noopener noreferrer"
            underline="none"
            color="inherit"
            sx={{ '&:hover': { color: 'primary.main' } }}
          >
            Copyright © 2026 GitHub 3899
          </Link>
          <Typography component="span" color="text.disabled">|</Typography>
          <CommunityTooltip />
          <Typography component="span" color="text.disabled">|</Typography>
          <Typography component="span" sx={{ font: 'inherit' }}>v{__APP_VERSION__}</Typography>
        </Stack>
      </Stack>
    </Box>
  )
}
