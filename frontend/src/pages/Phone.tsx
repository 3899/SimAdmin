import { useCallback, useEffect, useState } from 'react'
import {
  Alert,
  Avatar,
  Box,
  Button,
  Card,
  CardContent,
  Chip,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  Fade,
  IconButton,
  List,
  ListItem,
  ListItemAvatar,
  ListItemSecondaryAction,
  ListItemText,
  Paper,
  Snackbar,
  Tab,
  Tabs,
  TextField,
  Tooltip,
  Typography,
} from '@mui/material'
import {
  Backspace,
  AddCircleOutline,
  Call,
  CallEnd,
  CallMade,
  CallReceived,
  Delete,
  DeleteSweep,
  Dialpad,
  History,
  Phone as PhoneIcon,
  PhoneCallback,
  PhoneMissed,
  Refresh,
} from '@mui/icons-material'
import { type CallInfo, type CallRecord, type CallStats } from '../api/current'
import { useSimAdminApi } from '../contexts/ApiContext'

const dialpadButtons = [
  ['1', '2', '3'],
  ['4', '5', '6'],
  ['7', '8', '9'],
  ['*', '0', '#'],
]

const emptyStats: CallStats = {
  total: 0,
  incoming: 0,
  outgoing: 0,
  missed: 0,
  total_duration: 0,
}

function getStateLabel(state: string): string {
  const labels: Record<string, string> = {
    active: '通话中',
    dialing: '拨号中',
    alerting: '响铃中',
    incoming: '来电',
    waiting: '等待接听',
    held: '保持',
    terminated: '已结束',
  }
  return labels[state] || state
}

function directionLabel(direction: string): string {
  if (direction === 'incoming') return '来电'
  if (direction === 'outgoing') return '拨出'
  if (direction === 'missed') return '未接'
  return '未知'
}

function formatDuration(seconds: number): string {
  const mins = Math.floor(seconds / 60)
  const secs = seconds % 60
  return `${mins}:${secs.toString().padStart(2, '0')}`
}

function formatTime(timestamp: string): string {
  const date = new Date(timestamp)
  if (Number.isNaN(date.getTime())) return timestamp
  const now = new Date()
  if (date.toDateString() === now.toDateString()) {
    return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })
  }
  return date.toLocaleDateString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  })
}

function getCallIcon(direction: string, answered: boolean) {
  if (direction === 'missed') return <PhoneMissed color="error" />
  if (direction === 'incoming') return answered ? <CallReceived color="success" /> : <PhoneMissed color="error" />
  return <CallMade color="primary" />
}

export default function PhonePage() {
  const api = useSimAdminApi()
  const [tabValue, setTabValue] = useState(0)
  const [calls, setCalls] = useState<CallInfo[]>([])
  const [, setCallsLoading] = useState(false)
  const [dialNumber, setDialNumber] = useState('')
  const [dialLoading, setDialLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState<string | null>(null)
  const [callHistory, setCallHistory] = useState<CallRecord[]>([])
  const [callStats, setCallStats] = useState<CallStats>(emptyStats)
  const [historyLoading, setHistoryLoading] = useState(false)
  const [clearDialogOpen, setClearDialogOpen] = useState(false)

  const fetchCalls = useCallback(async () => {
    setCallsLoading(true)
    try {
      const response = await api.getCalls()
      setCalls(response.data?.calls ?? [])
    } catch (err) {
      console.warn('获取通话列表失败:', err)
    } finally {
      setCallsLoading(false)
    }
  }, [api])

  const fetchCallHistory = useCallback(async () => {
    setHistoryLoading(true)
    try {
      const response = await api.getCallHistory({ limit: 100, offset: 0 })
      setCallHistory(response.data?.records ?? [])
      setCallStats(response.data?.stats ?? emptyStats)
    } catch (err) {
      console.warn('获取通话记录失败:', err)
    } finally {
      setHistoryLoading(false)
    }
  }, [api])

  useEffect(() => {
    void fetchCalls()
    void fetchCallHistory()
    const timer = window.setInterval(() => {
      void fetchCalls()
    }, 3000)
    return () => window.clearInterval(timer)
  }, [fetchCalls, fetchCallHistory])

  const handleDialpadPress = (digit: string) => {
    setDialNumber((prev) => prev + digit)
  }

  const handleDial = async (number = dialNumber) => {
    const target = number.trim()
    if (!target) {
      setError('请输入电话号码')
      return
    }

    setDialLoading(true)
    setError(null)
    setSuccess(null)
    try {
      const response = await api.dialCall(target)
      const path = response.data?.path
      if (path) {
        setCalls((prev) => (
          prev.some((call) => call.path === path)
            ? prev
            : [{ path, phone_number: target, state: 'dialing', direction: 'outgoing' }, ...prev]
        ))
      }
      setSuccess(`正在拨打 ${target}`)
      setDialNumber('')
      void fetchCallHistory()
    } catch (err) {
      setError(err instanceof Error ? err.message : '拨号失败')
    } finally {
      setDialLoading(false)
    }
  }

  const handleHangupAll = async () => {
    setError(null)
    setSuccess(null)
    const currentCalls = calls
    try {
      if (currentCalls.length === 1) {
        await api.hangupCall(currentCalls[0].path)
      } else {
        await api.hangupAllCalls()
      }
      setCalls([])
      setSuccess('已挂断所有通话')
      void fetchCalls()
      void fetchCallHistory()
    } catch (err) {
      setError(err instanceof Error ? err.message : '挂断失败')
    }
  }

  const handleAnswer = async (call: CallInfo) => {
    setError(null)
    setSuccess(null)
    try {
      await api.answerCall(call.path)
      setSuccess(`已接听 ${call.phone_number || '来电'}`)
      void fetchCalls()
    } catch (err) {
      setError(err instanceof Error ? err.message : '接听失败')
    }
  }

  const handleDeleteRecord = async (id: number) => {
    setError(null)
    try {
      await api.deleteCallRecord(id)
      setSuccess('记录已删除')
      void fetchCallHistory()
    } catch (err) {
      setError(err instanceof Error ? err.message : '删除记录失败')
    }
  }

  const handleClearHistory = async () => {
    setClearDialogOpen(false)
    setError(null)
    try {
      await api.clearCallHistory()
      setSuccess('所有通话记录已清空')
      void fetchCallHistory()
    } catch (err) {
      setError(err instanceof Error ? err.message : '清空记录失败')
    }
  }

  const handleFillFromHistory = (phoneNumber: string) => {
    setDialNumber(phoneNumber)
    setTabValue(0)
  }

  const singleCall = calls.length === 1 ? calls[0] : null

  return (
    <Box>
      <Box display="flex" alignItems="center" gap={1} mb={2}>
        <Typography variant="h5" fontWeight={700}>电话管理</Typography>
      </Box>

      <Snackbar open={!!error} autoHideDuration={4000} resumeHideDuration={3000} onClose={() => setError(null)} anchorOrigin={{ vertical: 'top', horizontal: 'center' }}>
        <Alert severity="error" onClose={() => setError(null)} variant="filled">{error}</Alert>
      </Snackbar>
      <Snackbar open={!!success} autoHideDuration={3000} resumeHideDuration={3000} onClose={() => setSuccess(null)} anchorOrigin={{ vertical: 'top', horizontal: 'center' }}>
        <Alert severity="success" onClose={() => setSuccess(null)} variant="filled">{success}</Alert>
      </Snackbar>

      <Fade in={calls.length > 0}>
        <Paper
          elevation={6}
          sx={{
            mb: 2,
            p: 2,
            bgcolor: 'success.main',
            color: 'white',
            display: calls.length > 0 ? 'block' : 'none',
          }}
        >
          <Box display="flex" justifyContent="space-between" alignItems="center" gap={2}>
            <Box display="flex" alignItems="center" gap={2}>
              <PhoneCallback />
              <Box>
                <Typography variant="h6" fontWeight={600}>
                  {singleCall ? (singleCall.phone_number || '未知号码') : `${calls.length} 个通话中`}
                </Typography>
                {singleCall && (
                  <Typography variant="body2" sx={{ opacity: 0.9 }}>
                    {getStateLabel(singleCall.state)} - {directionLabel(singleCall.direction)}
                  </Typography>
                )}
              </Box>
            </Box>
            <Box display="flex" gap={1}>
              {singleCall && (singleCall.state === 'incoming' || singleCall.state === 'waiting') && (
                <Button
                  variant="contained"
                  color="inherit"
                  sx={{ color: 'success.main', bgcolor: 'white' }}
                  startIcon={<Call />}
                  onClick={() => void handleAnswer(singleCall)}
                >
                  接听
                </Button>
              )}
              <Button variant="contained" color="error" startIcon={<CallEnd />} onClick={() => void handleHangupAll()}>
                挂断{calls.length > 1 ? '全部' : ''}
              </Button>
            </Box>
          </Box>
        </Paper>
      </Fade>

      <Tabs value={tabValue} onChange={(_, value: number) => setTabValue(value)} sx={{ mb: 2 }}>
        <Tab icon={<Dialpad />} label="拨号" iconPosition="start" />
        <Tab icon={<History />} label="通话记录" iconPosition="start" />
      </Tabs>

      {tabValue === 0 && (
        <Card>
          <CardContent>
            <Box display="flex" flexDirection="column" alignItems="center" maxWidth={320} mx="auto">
              <TextField
                fullWidth
                variant="standard"
                value={dialNumber}
                onChange={(event) => setDialNumber(event.target.value)}
                placeholder="输入电话号码"
                inputProps={{ inputMode: 'tel', style: { textAlign: 'center', fontSize: '1.5rem' } }}
                InputProps={{
                  endAdornment: dialNumber ? (
                    <IconButton size="small" onClick={() => setDialNumber((prev) => prev.slice(0, -1))}>
                      <Backspace />
                    </IconButton>
                  ) : null,
                }}
                sx={{ mb: 3 }}
              />

              <Box sx={{ width: '100%' }}>
                {dialpadButtons.map((row) => (
                  <Box key={row.join('')} display="flex" justifyContent="center" gap={2} mb={1.5}>
                    {row.map((digit) => (
                      <Button
                        key={digit}
                        variant="outlined"
                        onClick={() => handleDialpadPress(digit)}
                        sx={{ width: 72, height: 72, borderRadius: '50%', fontSize: '1.5rem', fontWeight: 500 }}
                      >
                        {digit}
                      </Button>
                    ))}
                  </Box>
                ))}
              </Box>

              <Button
                variant="contained"
                color="success"
                size="large"
                startIcon={dialLoading ? <CircularProgress size={20} color="inherit" /> : <PhoneIcon />}
                onClick={() => void handleDial()}
                disabled={dialLoading || !dialNumber.trim()}
                sx={{ mt: 2, width: 160, height: 56, borderRadius: 28 }}
              >
                {dialLoading ? '拨号中' : '拨打'}
              </Button>
            </Box>
          </CardContent>
        </Card>
      )}

      {tabValue === 1 && (
        <Card>
          <CardContent>
            <Box display="flex" gap={2} mb={2} flexWrap="wrap">
              <Paper sx={{ p: 1.5, flex: 1, minWidth: 80 }}>
                <Typography variant="h6" color="primary" fontWeight={600}>{callStats.total}</Typography>
                <Typography variant="caption" color="text.secondary">总计</Typography>
              </Paper>
              <Paper sx={{ p: 1.5, flex: 1, minWidth: 80 }}>
                <Typography variant="h6" color="success.main" fontWeight={600}>{callStats.incoming}</Typography>
                <Typography variant="caption" color="text.secondary">来电</Typography>
              </Paper>
              <Paper sx={{ p: 1.5, flex: 1, minWidth: 80 }}>
                <Typography variant="h6" color="info.main" fontWeight={600}>{callStats.outgoing}</Typography>
                <Typography variant="caption" color="text.secondary">拨出</Typography>
              </Paper>
              <Paper sx={{ p: 1.5, flex: 1, minWidth: 80 }}>
                <Typography variant="h6" color="error.main" fontWeight={600}>{callStats.missed}</Typography>
                <Typography variant="caption" color="text.secondary">未接</Typography>
              </Paper>
            </Box>

            <Box display="flex" justifyContent="space-between" alignItems="center" mb={2}>
              <Typography variant="subtitle1" fontWeight={600}>通话记录 ({callHistory.length})</Typography>
              <Box display="flex" gap={1}>
                <IconButton color="primary" onClick={() => void fetchCallHistory()} disabled={historyLoading}>
                  {historyLoading ? <CircularProgress size={20} /> : <Refresh />}
                </IconButton>
                {callHistory.length > 0 && (
                  <Button variant="outlined" color="error" size="small" startIcon={<DeleteSweep />} onClick={() => setClearDialogOpen(true)}>
                    清空
                  </Button>
                )}
              </Box>
            </Box>

            {historyLoading && callHistory.length === 0 ? (
              <Box display="flex" justifyContent="center" py={4}>
                <CircularProgress />
              </Box>
            ) : callHistory.length === 0 ? (
              <Alert severity="info">暂无通话记录</Alert>
            ) : (
              <List sx={{ maxHeight: 460, overflow: 'auto' }}>
                {callHistory.map((record) => (
                  <ListItem key={record.id} divider>
                    <ListItemAvatar>
                      <Avatar sx={{ bgcolor: record.direction === 'missed' ? 'error.light' : record.direction === 'incoming' ? 'success.light' : 'primary.light' }}>
                        {getCallIcon(record.direction, record.answered)}
                      </Avatar>
                    </ListItemAvatar>
                    <ListItemText
                      primary={
                        <Box display="flex" alignItems="center" gap={1} flexWrap="wrap">
                          <Typography variant="body1" fontWeight={600}>{record.phone_number || '未知号码'}</Typography>
                          <Chip label={directionLabel(record.direction)} size="small" variant="outlined" />
                          {record.duration > 0 && <Chip label={formatDuration(record.duration)} size="small" variant="outlined" />}
                        </Box>
                      }
                      secondary={formatTime(record.start_time)}
                    />
                    <ListItemSecondaryAction>
                      <Box display="flex" gap={0.5}>
                        <Tooltip title="重拨">
                          <IconButton size="small" color="primary" onClick={() => void handleDial(record.phone_number)} disabled={dialLoading || !record.phone_number}>
                            <Call fontSize="small" />
                          </IconButton>
                        </Tooltip>
                        <Tooltip title="填入拨号盘">
                          <IconButton size="small" color="info" onClick={() => handleFillFromHistory(record.phone_number)} disabled={!record.phone_number}>
                            <AddCircleOutline fontSize="small" />
                          </IconButton>
                        </Tooltip>
                        <Tooltip title="删除">
                          <IconButton size="small" color="error" onClick={() => void handleDeleteRecord(record.id)}>
                            <Delete fontSize="small" />
                          </IconButton>
                        </Tooltip>
                      </Box>
                    </ListItemSecondaryAction>
                  </ListItem>
                ))}
              </List>
            )}
          </CardContent>
        </Card>
      )}

      <Dialog open={clearDialogOpen} onClose={() => setClearDialogOpen(false)}>
        <DialogTitle>确认清空</DialogTitle>
        <DialogContent>
          <Typography>确定要清空所有通话记录吗？此操作不可撤销。</Typography>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setClearDialogOpen(false)}>取消</Button>
          <Button onClick={() => void handleClearHistory()} color="error" variant="contained">确认清空</Button>
        </DialogActions>
      </Dialog>
    </Box>
  )
}
