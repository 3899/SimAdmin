import { useEffect, useRef, useState } from 'react'
import {
  Alert,
  AppBar,
  Box,
  Button,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  Divider,
  IconButton,
  ListItemIcon,
  ListItemText,
  Menu,
  MenuItem,
  Snackbar,
  Stack,
  SvgIcon,
  Toolbar,
  Tooltip,
  Typography,
} from '@mui/material'
import {
  Brightness4 as DarkModeIcon,
  Brightness7 as LightModeIcon,
  Logout as LogoutIcon,
  Menu as MenuIcon,
  MoreVert as MoreVertIcon,
  Refresh as RefreshIcon,
  Router as RouterIcon,
  Speed as SpeedIcon,
} from '@mui/icons-material'
import { useTheme } from '../../contexts/ThemeContext'
import { useRefreshInterval } from '../../contexts/RefreshContext'
import { api } from '../../api/current'
import type { BasebandRestartResponse, BasebandRestartStep } from '../../api/types'

const TOPBAR_TRANSITION = '300ms cubic-bezier(0.4, 0, 0.2, 1)'

type RestartConfirmTarget = 'baseband' | 'service' | 'device'

interface TopBarProps {
  drawerWidth: number
  onMenuClick: () => void
  refreshInterval: number
  onRefreshIntervalChange: (interval: number) => void
  showLogout?: boolean
}

function ServiceRestartIcon() {
  return (
    <SvgIcon viewBox="0 0 1024 1024" sx={{ fontSize: 18 }}>
      <path d="M768.000512 495.104a128 128 0 1 0 128-128 128 128 0 0 0-128 128z m0 0" p-id="28061"></path><path d="M335.872512 128a96 96 0 1 0 96-96A96 96 0 0 0 335.872512 128z m0 0M0.000512 544.256a79.872 79.872 0 1 0 79.872-80.128A79.872 79.872 0 0 0 0.000512 544.256z m0 0" p-id="28062"></path><path d="M352.000512 864a64 64 0 1 0 64 63.744 64 64 0 0 0-64-63.744z m501.248 53.76a144.128 144.128 0 1 1 0-204.8 143.36 143.36 0 0 1 0 204.8z m-23.808-626.944a114.432 114.432 0 0 1-80.384 33.28 112.64 112.64 0 0 1-80.384-33.28 113.664 113.664 0 0 1 0-161.024 113.664 113.664 0 0 1 160.768 0 113.664 113.664 0 0 1 0 161.024zM246.528512 326.912a88.064 88.064 0 1 1 0-124.672 88.064 88.064 0 0 1 0 124.672zM218.880512 826.88a72.704 72.704 0 0 1-51.2 20.992 71.936 71.936 0 0 1 0-143.872 72.192 72.192 0 0 1 51.2 122.88z m0 0" fill="currentColor" />
    </SvgIcon>
  )
}

function DeviceRebootIcon() {
  return (
    <SvgIcon viewBox="0 0 1024 1024" sx={{ fontSize: 18 }}>
      <path d="M561.312102 68.191078l-98.624205 0 0 493.121024 98.624205 0L561.312102 68.191078zM799.735283 174.951591l-69.77618 69.77618c77.420277 63.36619 127.225613 159.27761 127.225613 267.271206 0 190.590779-154.592914 345.184717-345.184717 345.184717S166.815283 702.590779 166.815283 512c0-107.993596 49.805336-203.905016 127.225613-267.271206l-69.77618-69.77618C129.0911 256.316713 68.191078 376.884696 68.191078 512c0 245.080811 198.72811 443.808922 443.808922 443.808922s443.808922-198.72811 443.808922-443.808922C955.808922 376.884696 894.907876 256.316713 799.735283 174.951591z" fill="#d81e06" />
    </SvgIcon>
  )
}

export default function TopBar({
  drawerWidth,
  onMenuClick,
  refreshInterval,
  onRefreshIntervalChange,
  showLogout = false,
}: TopBarProps) {
  const { mode, toggleTheme } = useTheme()
  const { triggerRefresh } = useRefreshInterval()
  const [anchorEl, setAnchorEl] = useState<null | HTMLElement>(null)
  const [refreshMenuAnchor, setRefreshMenuAnchor] = useState<null | HTMLElement>(null)
  const [basebandRestarting, setBasebandRestarting] = useState(false)
  const [basebandProgressOpen, setBasebandProgressOpen] = useState(false)
  const [basebandSteps, setBasebandSteps] = useState<BasebandRestartStep[]>([])
  const [basebandCurrentRegistration, setBasebandCurrentRegistration] = useState<string | null>(null)
  const [systemActionLoading, setSystemActionLoading] = useState<'service' | 'device' | null>(null)
  const [systemActionMessage, setSystemActionMessage] = useState<string | null>(null)
  const [systemActionSeverity, setSystemActionSeverity] = useState<'info' | 'success' | 'error'>('info')
  const [deviceRebootProgressOpen, setDeviceRebootProgressOpen] = useState(false)
  const [deviceRebootSteps, setDeviceRebootSteps] = useState<BasebandRestartStep[]>([])
  const [restartConfirmTarget, setRestartConfirmTarget] = useState<RestartConfirmTarget | null>(null)
  const [logoutLoading, setLogoutLoading] = useState(false)
  const deviceRebootTimersRef = useRef<number[]>([])
  const title = drawerWidth <= 80 ? 'SimAdmin - SIM/eSIM 中枢' : 'SIM/eSIM 中枢'

  useEffect(() => {
    return () => {
      deviceRebootTimersRef.current.forEach((timer) => window.clearTimeout(timer))
      deviceRebootTimersRef.current = []
    }
  }, [])

  const applyBasebandProgress = (data?: BasebandRestartResponse) => {
    if (!data) return
    setBasebandSteps(data.steps ?? [])
    setBasebandCurrentRegistration(data.current_registration ?? null)
  }

  const getBasebandErrorStep = () => basebandSteps.find(s => s.status === 'error')

  const getCurrentBasebandMessage = () => {
    const errorStep = getBasebandErrorStep()
    if (errorStep) return errorStep.detail || `${errorStep.step} 失败`
    if (!basebandRestarting && basebandSteps.length > 0) return '基带重启和网络恢复成功！'
    if (basebandSteps.length === 0) return '正在启动基带重启程序...'
    const lastStep = basebandSteps[basebandSteps.length - 1]
    return lastStep.status === 'running' ? `正在进行：${lastStep.step}` : `已完成：${lastStep.step}`
  }

  const getDeviceRebootErrorStep = () => deviceRebootSteps.find(s => s.status === 'error')

  const getCurrentDeviceRebootMessage = () => {
    const errorStep = getDeviceRebootErrorStep()
    if (errorStep) return errorStep.detail || `${errorStep.step} 失败`
    if (systemActionLoading !== 'device' && deviceRebootSteps.length > 0) return '设备已请求重启，即将离线。'
    const activeStep = deviceRebootSteps.find(s => s.status === 'running')
    if (activeStep) return `正在进行：${activeStep.step} (${activeStep.detail})`
    return '正在执行系统重启...'
  }

  const loadBasebandProgress = async () => {
    const response = await api.getBasebandRestartStatus()
    applyBasebandProgress(response.data)
  }



  const handleRestartBaseband = async () => {
    if (basebandRestarting) return
    setBasebandRestarting(true)
    setBasebandProgressOpen(true)
    setBasebandSteps([])
    setBasebandCurrentRegistration(null)
    let progressTimer: number | undefined
    try {
      progressTimer = window.setInterval(() => void loadBasebandProgress(), 1000)
      const response = await api.restartBaseband()
      applyBasebandProgress(response.data)
      triggerRefresh()
    } catch (err) {
      await loadBasebandProgress().catch(() => undefined)
      setBasebandSteps((steps) => {
        if (steps.some((step) => step.status === 'error')) return steps
        return [...steps, { step: '重启基带失败', status: 'error', detail: err instanceof Error ? err.message : '未知错误' }]
      })
    } finally {
      if (progressTimer) window.clearInterval(progressTimer)
      await loadBasebandProgress().catch(() => undefined)
      setBasebandRestarting(false)
    }
  }

  const handleRestartService = async () => {
    if (systemActionLoading) return
    setSystemActionLoading('service')
    setSystemActionSeverity('info')
    setSystemActionMessage('正在重启 SimAdmin 服务')
    try {
      await api.restartService()
      setSystemActionSeverity('success')
      setSystemActionMessage('SimAdmin 服务正在重启')
    } catch (err) {
      setSystemActionSeverity('error')
      setSystemActionMessage(err instanceof Error ? err.message : '重启服务失败')
    } finally {
      setSystemActionLoading(null)
    }
  }

  const handleRebootDevice = async () => {
    if (systemActionLoading) return
    setSystemActionLoading('device')
    setDeviceRebootProgressOpen(true)
    setDeviceRebootSteps([
      { step: '提交安全重启请求', status: 'running', detail: '等待后端接管重启序列' },
      { step: '关闭射频模块', status: 'running', detail: 'mmcli -m 0 -d' },
      { step: '停止 ModemManager', status: 'running', detail: '切断 D-Bus / QMI 通信链路' },
      { step: '停止 qmi-proxy', status: 'running', detail: '释放底层 QMI 代理进程' },
      { step: '清理 ModemManager 缓存', status: 'running', detail: '删除 /var/lib/ModemManager 残留状态' },
      { step: '同步文件系统缓存', status: 'running', detail: 'sync 后等待硬件稳定' },
      { step: '执行系统重启', status: 'running', detail: '设备即将离线' },
    ])
    deviceRebootTimersRef.current.forEach((timer) => window.clearTimeout(timer))
    deviceRebootTimersRef.current = []
    try {
      await api.rebootSystem(1)
      const scheduleStep = (index: number, status: BasebandRestartStep['status'], detail: string, delay = 0) => {
        const timer = window.setTimeout(() => {
          setDeviceRebootSteps((steps) =>
            steps.map((step, stepIndex) => stepIndex === index ? { ...step, status, detail } : step),
          )
        }, delay)
        deviceRebootTimersRef.current.push(timer)
      }
      scheduleStep(0, 'ok', '后端已开始 Safe OS Reboot', 0)
      scheduleStep(1, 'ok', '射频已请求进入低功耗休眠状态', 1000)
      scheduleStep(2, 'ok', 'ModemManager 停止命令已下发', 1600)
      scheduleStep(3, 'ok', 'qmi-proxy 清理命令已下发', 2200)
      scheduleStep(4, 'ok', '运行状态缓存清理命令已执行', 2800)
      scheduleStep(5, 'ok', '缓存同步并等待 2 秒', 3400)
      scheduleStep(6, 'ok', 'reboot 命令已下发，页面连接将中断，请等待设备重启。', 4800)
      const doneTimer = window.setTimeout(() => setSystemActionLoading(null), 5600)
      deviceRebootTimersRef.current.push(doneTimer)
    } catch (err) {
      setDeviceRebootSteps((steps) =>
        steps.map((step, index) => index === 0 ? { ...step, status: 'error', detail: err instanceof Error ? err.message : '重启设备失败' } : step),
      )
      setSystemActionLoading(null)
    }
  }

  const restartConfirmTitle = restartConfirmTarget === 'baseband'
    ? '确认重启基带'
    : restartConfirmTarget === 'service'
      ? '确认重启服务'
      : '确认重启设备'

  const restartConfirmContent = restartConfirmTarget === 'baseband'
    ? '确定要重启基带吗？重启期间网络注册和数据连接可能会短暂中断。'
    : restartConfirmTarget === 'service'
      ? '确定要重启 SimAdmin 服务吗？重启期间页面可能会短暂不可用。'
      : '确定要重启设备吗？设备会离线并中断当前连接，请确认后再继续。'

  const handleConfirmRestart = () => {
    const target = restartConfirmTarget
    if (!target) return

    setRestartConfirmTarget(null)
    if (target === 'baseband') {
      void handleRestartBaseband()
    } else if (target === 'service') {
      void handleRestartService()
    } else {
      void handleRebootDevice()
    }
  }

  const handleRefreshIntervalChange = (interval: number) => {
    onRefreshIntervalChange(interval)
    setRefreshMenuAnchor(null)
  }

  const handleLogout = async () => {
    if (logoutLoading) return
    setAnchorEl(null)
    setLogoutLoading(true)
    try {
      await api.logout()
      window.location.replace('/login')
    } catch (err) {
      setSystemActionSeverity('error')
      setSystemActionMessage(err instanceof Error ? err.message : '退出登录失败')
      setLogoutLoading(false)
    }
  }

  const getRefreshLabel = () => {
    if (refreshInterval === 0) return '手动'
    return `${refreshInterval / 1000}秒`
  }

  return (
    <AppBar
      position="static"
      sx={{
        color: 'text.primary',
        bgcolor: 'transparent',
        borderBottom: 0,
        backdropFilter: 'none',
        WebkitBackdropFilter: 'none',
        flexShrink: 0,
        transition: `width ${TOPBAR_TRANSITION}`,
        willChange: 'width',
      }}
    >
      <Toolbar sx={{ minHeight: { xs: 56, sm: 56 }, px: { xs: 1.5, sm: 2 } }}>
        <IconButton
          color="default"
          aria-label="切换侧边栏"
          edge="start"
          onClick={onMenuClick}
          sx={{
            mr: 1.5,
            color: 'text.primary',
            border: '1px solid transparent',
            bgcolor: 'transparent',
            '&:hover': {
              borderColor: 'divider',
              bgcolor: (theme) => theme.palette.mode === 'light' ? 'rgba(255,255,255,0.62)' : 'rgba(30,41,59,0.82)',
            },
          }}
        >
          <MenuIcon />
        </IconButton>

        <Typography
          variant="h6"
          noWrap
          component="div"
          sx={{ flexGrow: 1, fontSize: { xs: '1rem', sm: '1.05rem' }, fontWeight: 700, letterSpacing: 0 }}
        >
          {title}
        </Typography>

        <Box sx={{ display: 'flex', alignItems: 'center', gap: { xs: 0.5, sm: 1 } }}>
          <Tooltip title="刷新页面">
            <IconButton color="default" onClick={triggerRefresh}>
              <RefreshIcon sx={{ fontSize: 22 }} />
            </IconButton>
          </Tooltip>
          <Tooltip title="重启基带">
            <span>
              <IconButton size="small" color="default" onClick={() => setRestartConfirmTarget('baseband')} disabled={basebandRestarting || systemActionLoading !== null} sx={{ p: 0.75 }}>
                {basebandRestarting ? <CircularProgress size={18} color="inherit" /> : <RouterIcon sx={{ fontSize: 22 }} />}
              </IconButton>
            </span>
          </Tooltip>
          <Tooltip title="重启服务">
            <span>
              <IconButton size="small" color="default" onClick={() => setRestartConfirmTarget('service')} disabled={basebandRestarting || systemActionLoading !== null} sx={{ p: 0.75 }}>
                {systemActionLoading === 'service' ? <CircularProgress size={18} color="inherit" /> : <ServiceRestartIcon />}
              </IconButton>
            </span>
          </Tooltip>
          <Tooltip title="重启设备">
            <span>
              <IconButton size="small" color="default" onClick={() => setRestartConfirmTarget('device')} disabled={basebandRestarting || systemActionLoading !== null} sx={{ p: 0.75 }}>
                {systemActionLoading === 'device' ? <CircularProgress size={18} color="inherit" /> : <DeviceRebootIcon />}
              </IconButton>
            </span>
          </Tooltip>
          <IconButton color="default" onClick={(event) => setAnchorEl(event.currentTarget)} title="更多选项">
            <MoreVertIcon />
          </IconButton>
          {showLogout && (
            <Box sx={{ display: { xs: 'none', sm: 'inline-flex' } }}>
              <Tooltip title="退出登录">
                <IconButton
                  color="default"
                  aria-label="退出登录"
                  disabled={logoutLoading}
                  onClick={() => void handleLogout()}
                >
                  {logoutLoading
                    ? <CircularProgress size={19} color="inherit" />
                    : <LogoutIcon sx={{ fontSize: 22 }} />}
                </IconButton>
              </Tooltip>
            </Box>
          )}
        </Box>

        <Menu anchorEl={anchorEl} open={Boolean(anchorEl)} onClose={() => setAnchorEl(null)} PaperProps={{ sx: { minWidth: 200, mt: 1 } }}>
          <MenuItem
            onClick={() => {
              toggleTheme()
              setAnchorEl(null)
            }}
          >
            <ListItemIcon>
              {mode === 'dark' ? <LightModeIcon fontSize="small" /> : <DarkModeIcon fontSize="small" />}
            </ListItemIcon>
            <ListItemText>{mode === 'dark' ? '浅色模式' : '深色模式'}</ListItemText>
          </MenuItem>
          <Divider />
          <MenuItem onClick={(event) => setRefreshMenuAnchor(event.currentTarget)}>
            <ListItemIcon><SpeedIcon fontSize="small" /></ListItemIcon>
            <ListItemText primary="刷新频率" secondary={getRefreshLabel()} secondaryTypographyProps={{ variant: 'caption' }} />
          </MenuItem>
          {showLogout && (
            <MenuItem
              disabled={logoutLoading}
              onClick={() => void handleLogout()}
              sx={{ display: { xs: 'flex', sm: 'none' } }}
            >
              <ListItemIcon>
                {logoutLoading
                  ? <CircularProgress size={18} color="inherit" />
                  : <LogoutIcon fontSize="small" />}
              </ListItemIcon>
              <ListItemText>退出登录</ListItemText>
            </MenuItem>
          )}
        </Menu>

        <Menu anchorEl={refreshMenuAnchor} open={Boolean(refreshMenuAnchor)} onClose={() => setRefreshMenuAnchor(null)}>
          {[1000, 3000, 5000, 10000].map((interval) => (
            <MenuItem key={interval} selected={refreshInterval === interval} onClick={() => handleRefreshIntervalChange(interval)}>
              {interval / 1000}秒/次
            </MenuItem>
          ))}
          <Divider />
          <MenuItem selected={refreshInterval === 0} onClick={() => handleRefreshIntervalChange(0)}>
            手动刷新
          </MenuItem>
        </Menu>

        <Dialog open={basebandProgressOpen} onClose={() => { if (!basebandRestarting) setBasebandProgressOpen(false) }} maxWidth="xs" fullWidth>
          <DialogTitle>重启基带</DialogTitle>
          <DialogContent>
            <Stack spacing={2} alignItems="center" sx={{ py: 2 }}>
              {basebandRestarting && !getBasebandErrorStep() && (
                <CircularProgress size={48} />
              )}
              {getBasebandErrorStep() ? (
                <Alert severity="error" sx={{ width: '100%' }}>{getCurrentBasebandMessage()}</Alert>
              ) : !basebandRestarting && basebandSteps.length > 0 ? (
                <Alert severity="success" sx={{ width: '100%' }}>{getCurrentBasebandMessage()}</Alert>
              ) : (
                <Typography variant="body1" color="text.secondary" textAlign="center">
                  {getCurrentBasebandMessage()}
                </Typography>
              )}
              {basebandCurrentRegistration && basebandRestarting && (
                <Typography variant="caption" color="text.secondary" textAlign="center">
                  当前注册状态：{basebandCurrentRegistration}
                </Typography>
              )}
            </Stack>
          </DialogContent>
          <DialogActions>
            <Button disabled={basebandRestarting} onClick={() => setBasebandProgressOpen(false)}>关闭</Button>
          </DialogActions>
        </Dialog>

        <Dialog open={deviceRebootProgressOpen} onClose={() => { if (systemActionLoading !== 'device') setDeviceRebootProgressOpen(false) }} maxWidth="xs" fullWidth>
          <DialogTitle>重启设备</DialogTitle>
          <DialogContent>
            <Stack spacing={2} alignItems="center" sx={{ py: 2 }}>
              {systemActionLoading === 'device' && !getDeviceRebootErrorStep() && (
                <CircularProgress size={48} />
              )}
              {getDeviceRebootErrorStep() ? (
                <Alert severity="error" sx={{ width: '100%' }}>{getCurrentDeviceRebootMessage()}</Alert>
              ) : systemActionLoading !== 'device' && deviceRebootSteps.length > 0 ? (
                <Alert severity="success" sx={{ width: '100%' }}>{getCurrentDeviceRebootMessage()}</Alert>
              ) : (
                <Typography variant="body1" color="text.secondary" textAlign="center">
                  {getCurrentDeviceRebootMessage()}
                </Typography>
              )}
            </Stack>
          </DialogContent>
          <DialogActions>
            <Button disabled={systemActionLoading === 'device'} onClick={() => setDeviceRebootProgressOpen(false)}>关闭</Button>
          </DialogActions>
        </Dialog>

        <Dialog open={!!restartConfirmTarget} onClose={() => setRestartConfirmTarget(null)}>
          <DialogTitle>{restartConfirmTitle}</DialogTitle>
          <DialogContent>
            <Typography>{restartConfirmContent}</Typography>
          </DialogContent>
          <DialogActions>
            <Button onClick={() => setRestartConfirmTarget(null)}>取消</Button>
            <Button
              onClick={handleConfirmRestart}
              color="error"
              variant="contained"
            >
              确认重启
            </Button>
          </DialogActions>
        </Dialog>

        <Snackbar
          open={!!systemActionMessage}
          autoHideDuration={systemActionLoading ? null : 3000}
          resumeHideDuration={3000}
          onClose={() => { if (!systemActionLoading) setSystemActionMessage(null) }}
          anchorOrigin={{ vertical: 'top', horizontal: 'center' }}
          sx={{ top: { xs: 72, sm: 80 } }}
        >
          <Alert
            severity={systemActionSeverity}
            variant="filled"
            icon={systemActionLoading ? <CircularProgress size={18} color="inherit" /> : undefined}
            onClose={systemActionLoading ? undefined : () => setSystemActionMessage(null)}
          >
            {systemActionMessage}
          </Alert>
        </Snackbar>
      </Toolbar>
    </AppBar>
  )
}
