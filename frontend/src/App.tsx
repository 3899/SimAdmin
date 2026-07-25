import { lazy, Suspense, useEffect, useState } from 'react'
import { BrowserRouter, Routes, Route, Navigate, useLocation } from 'react-router-dom'
import { QueryClientProvider } from '@tanstack/react-query'
import { Box, CircularProgress } from '@mui/material'
import { ThemeProvider } from './contexts/ThemeContext'
import { WorkModeProvider } from './contexts/WorkModeContext'
import { queryClient } from './lib/queryClient'
import MainLayout from './components/Layout/MainLayout'
import { api, type AuthStatusResponse } from './api/current'

const SECURITY_SETTINGS_UPDATED_EVENT = 'simadmin-security-settings-updated'

// 路由级别代码分割 - 按需加载页面组件
const Dashboard = lazy(() => import('./pages/Dashboard'))
const SimCard = lazy(() => import('./pages/SimCard'))
const Network = lazy(() => import('./pages/Network'))
const DeviceNetwork = lazy(() => import('./pages/DeviceNetwork'))
const SMS = lazy(() => import('./pages/SMS'))
const NotificationCenter = lazy(() => import('./pages/NotificationCenter'))
const Phone = lazy(() => import('./pages/Phone'))
const Configuration = lazy(() => import('./pages/Configuration'))
const BackupRestore = lazy(() => import('./pages/BackupRestore'))
const OtaUpdate = lazy(() => import('./pages/OtaUpdate'))
const Login = lazy(() => import('./pages/Login'))
const AutomationCenter = lazy(() => import('./pages/AutomationCenter'))

// 页面加载中的 fallback
function PageLoading() {
  return (
    <Box display="flex" justifyContent="center" alignItems="center" minHeight="50vh">
      <CircularProgress size={32} />
    </Box>
  )
}

function ProtectedShell() {
  const location = useLocation()
  const [checking, setChecking] = useState(true)
  const [allowed, setAllowed] = useState(false)
  const [authStatus, setAuthStatus] = useState<AuthStatusResponse | null>(null)

  useEffect(() => {
    let cancelled = false
    api.getAuthStatus()
      .then((response) => {
        if (cancelled) return
        setAuthStatus(response.data ?? null)
        setAllowed(response.data?.authenticated === true)
      })
      .catch(() => {
        if (!cancelled) {
          setAuthStatus(null)
          setAllowed(false)
        }
      })
      .finally(() => {
        if (!cancelled) setChecking(false)
      })
    return () => { cancelled = true }
  }, [])

  useEffect(() => {
    const handleSecuritySettingsUpdated = (event: Event) => {
      const settings = (event as CustomEvent<AuthStatusResponse['settings']>).detail
      if (!settings) return
      setAuthStatus((prev) => (prev ? { ...prev, settings } : prev))
    }

    window.addEventListener(SECURITY_SETTINGS_UPDATED_EVENT, handleSecuritySettingsUpdated)
    return () => window.removeEventListener(SECURITY_SETTINGS_UPDATED_EVENT, handleSecuritySettingsUpdated)
  }, [])

  useEffect(() => {
    const idleTimeoutSeconds = authStatus?.settings?.idle_timeout_seconds ?? 0
    const passwordProtectionEnabled = authStatus?.settings?.password_protection_enabled ?? false
    if (!allowed || !passwordProtectionEnabled || idleTimeoutSeconds <= 0) return undefined

    let timer: number | undefined
    const redirectToLogin = () => {
      const next = `${window.location.pathname}${window.location.search}`
      window.location.assign(next === '/' ? '/login' : `/login?next=${encodeURIComponent(next)}`)
    }
    const logoutForIdle = () => {
      void api.logout().finally(redirectToLogin)
    }
    const resetTimer = () => {
      if (timer !== undefined) window.clearTimeout(timer)
      timer = window.setTimeout(logoutForIdle, idleTimeoutSeconds * 1000)
    }
    const events = ['mousemove', 'mousedown', 'keydown', 'scroll', 'touchstart']

    events.forEach((eventName) => window.addEventListener(eventName, resetTimer))
    resetTimer()

    return () => {
      if (timer !== undefined) window.clearTimeout(timer)
      events.forEach((eventName) => window.removeEventListener(eventName, resetTimer))
    }
  }, [
    allowed,
    authStatus?.settings?.idle_timeout_seconds,
    authStatus?.settings?.password_protection_enabled,
  ])

  if (checking) return <PageLoading />
  if (!allowed) {
    const next = `${location.pathname}${location.search}`
    return <Navigate to={next === '/' ? '/login' : `/login?next=${encodeURIComponent(next)}`} replace />
  }

  return (
    <WorkModeProvider>
      <MainLayout />
    </WorkModeProvider>
  )
}

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <ThemeProvider>
        <BrowserRouter>
          <Routes>
            <Route path="/login" element={<Suspense fallback={<PageLoading />}><Login /></Suspense>} />
            <Route path="/" element={<ProtectedShell />}>
              <Route index element={<Suspense fallback={<PageLoading />}><Dashboard /></Suspense>} />
              <Route path="sim" element={<Suspense fallback={<PageLoading />}><SimCard /></Suspense>} />
              <Route path="esim" element={<Navigate to="/sim?tab=esim" replace />} />
              <Route path="network" element={<Suspense fallback={<PageLoading />}><Network /></Suspense>} />
              <Route path="device-network" element={<Suspense fallback={<PageLoading />}><DeviceNetwork /></Suspense>} />
              {/* 旧路由重定向到网络状态页面 */}
              <Route path="network-interfaces" element={<Navigate to="/network" replace />} />
              <Route path="band-lock" element={<Navigate to="/network" replace />} />
              <Route path="sms" element={<Suspense fallback={<PageLoading />}><SMS /></Suspense>} />
              <Route path="notifications" element={<Suspense fallback={<PageLoading />}><NotificationCenter /></Suspense>} />
              <Route path="automation" element={<Suspense fallback={<PageLoading />}><AutomationCenter /></Suspense>} />
              <Route path="phone" element={<Suspense fallback={<PageLoading />}><Phone /></Suspense>} />
              <Route path="config" element={<Suspense fallback={<PageLoading />}><Configuration /></Suspense>} />
              <Route path="config/security" element={<Suspense fallback={<PageLoading />}><Configuration /></Suspense>} />
              <Route path="config/backup" element={<Suspense fallback={<PageLoading />}><BackupRestore /></Suspense>} />
              <Route path="ota" element={<Suspense fallback={<PageLoading />}><OtaUpdate /></Suspense>} />
            </Route>
          </Routes>
        </BrowserRouter>
      </ThemeProvider>
    </QueryClientProvider>
  )
}

export default App
