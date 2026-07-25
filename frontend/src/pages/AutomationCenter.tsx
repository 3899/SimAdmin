import { useState, useEffect, useCallback, useMemo, useRef } from 'react'
import {
  Box,
  Button,
  Card,
  CardContent,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  Grid,
  IconButton,
  InputAdornment,
  MenuItem,
  Paper,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  Tabs,
  Tab,
  TextField,
  Typography,
  Chip,
  Alert,
  Snackbar,
} from '@mui/material'
import {
  Add,
  AutoMode,
  Search,
  Clear,
  FirstPage,
  LastPage,
  KeyboardArrowLeft,
  KeyboardArrowRight,
  DeleteSweep,
  SmartToy,
} from '@mui/icons-material'
import { api } from '../api/current'
import type {
  AutomationConfig,
  AutomationTask,
  AutomationLogEntry,
  NotificationConfig,
} from '../api/contracts'
import ErrorSnackbar from '../components/ErrorSnackbar'

// 导入拆分后的子组件
import DateRangePicker from '../components/DateRangePicker'
import AutomationTaskCard from './automation/AutomationTaskCard'
import AutomationTaskDialog from './automation/AutomationTaskDialog'
import AutoCleanDialog from './automation/AutoCleanDialog'
import AdvancedClearDialog from './automation/AdvancedClearDialog'

const LOG_PAGE_SIZE = 15

const filterTextFieldSx = {
  '& .MuiInputBase-input': {
    fontSize: '14px',
  },
  '& .MuiInputBase-input::placeholder': {
    fontSize: '14px',
  },
  '& .MuiInputLabel-root': {
    fontSize: '14px',
  },
  '& .MuiSelect-select': {
    fontSize: '14px',
  },
  '& .MuiFormControlLabel-label': {
    fontSize: '14px',
  },
} as const

export default function AutomationCenter() {
  const [tab, setTab] = useState(0)
  const [loading, setLoading] = useState(true)
  const [testingTaskId, setTestingTaskId] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState<string | null>(null)
  const [backupLocalDir, setBackupLocalDir] = useState('/opt/simadmin/backups')

  // DOM references and logic for dynamic height calculation
  const tabsRef = useRef<HTMLDivElement | null>(null)
  const [cardHeight, setCardHeight] = useState<string | number>('calc(100vh - 220px)')

  const updateHeight = useCallback(() => {
    const tabsEl = tabsRef.current
    if (tabsEl) {
      const rect = tabsEl.getBoundingClientRect()
      // tabsEl 的底部距离窗口顶部的像素高度即 rect.bottom
      // 扣除 Tabs 底部外边距 16px，以及底部预留 24px 间距，保证绝对不会溢出主窗口产生外层滚动条
      const availableHeight = window.innerHeight - rect.bottom - 16 - 24
      setCardHeight(Math.max(500, availableHeight))
    }
  }, [])

  useEffect(() => {
    updateHeight()
    window.addEventListener('resize', updateHeight)
    return () => {
      window.removeEventListener('resize', updateHeight)
    }
  }, [updateHeight, tab])

  // Config State
  const [config, setConfig] = useState<AutomationConfig>({
    enabled: true,
    tasks: [],
  })

  // Logs State
  const [logs, setLogs] = useState<AutomationLogEntry[]>([])
  const [logTotal, setLogTotal] = useState(0)
  const [logsLoading, setLogsLoading] = useState(false)
  const [logPage, setLogPage] = useState(0)
  const [pageInput, setPageInput] = useState(() => String(logPage + 1))
  const [filterType, setFilterType] = useState('')
  const [filterStatus, setFilterStatus] = useState('')
  const [logStartDate, setLogStartDate] = useState('')
  const [logEndDate, setLogEndDate] = useState('')
  const [searchQuery, setSearchQuery] = useState('')

  useEffect(() => {
    setPageInput(String(logPage + 1))
  }, [logPage])

  // Latest logs cache to display task status on cards
  const [latestLogs, setLatestLogs] = useState<Record<string, AutomationLogEntry>>({})

  // Dialog States
  const [taskDialogOpen, setTaskDialogOpen] = useState(false)
  const [editingTask, setEditingTask] = useState<AutomationTask | null>(null)
  const [dndAutoCleanOpen, setDndAutoCleanOpen] = useState(false)
  const [advancedClearOpen, setAdvancedClearOpen] = useState(false)
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false)
  const [taskToDelete, setTaskToDelete] = useState<AutomationTask | null>(null)

  // Log cleanup settings from notification center
  const [notificationConfig, setNotificationConfig] = useState<NotificationConfig | null>(null)

  // Load configuration and latest logs
  const loadData = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const configRes = await api.getAutomationConfig()
      if (configRes.data) {
        setConfig(configRes.data)
      }

      const backupRes = await api.getBackupConfig()
      setBackupLocalDir(backupRes.data?.storage?.local_dir || '/opt/simadmin/backups')

      // Load latest logs to map status
      const logsRes = await api.getAutomationLogs({ limit: 100 })
      if (logsRes.data?.logs) {
        const cache: Record<string, AutomationLogEntry> = {}
        // Since logs are returned in descending order, we iterate backwards to keep the latest one
        const reversed = [...logsRes.data.logs].reverse()
        reversed.forEach((log) => {
          cache[log.task_id] = log
        })
        setLatestLogs(cache)
      }

      // Load notification cleanup config
      const notifRes = await api.getNotificationConfig()
      if (notifRes.data) {
        setNotificationConfig(notifRes.data)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void loadData()
  }, [loadData])

  // Load logs for the logs tab
  const loadLogs = useCallback(async () => {
    setLogsLoading(true)
    try {
      const res = await api.getAutomationLogs({
        type: filterType,
        status: filterStatus,
        start_date: logStartDate,
        end_date: logEndDate,
        q: searchQuery,
        limit: LOG_PAGE_SIZE,
        offset: logPage * LOG_PAGE_SIZE,
      })
      setLogs(res.data?.logs ?? [])
      setLogTotal(res.data?.total ?? 0)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLogsLoading(false)
    }
  }, [filterType, filterStatus, logStartDate, logEndDate, searchQuery, logPage])

  const pageCount = Math.max(1, Math.ceil(logTotal / LOG_PAGE_SIZE))
  const startRecord = logTotal === 0 ? 0 : logPage * LOG_PAGE_SIZE + 1
  const endRecord = Math.min(logTotal, (logPage + 1) * LOG_PAGE_SIZE)
  const canGoPrev = logPage > 0
  const canGoNext = logPage < pageCount - 1

  const commitPageInput = () => {
    const parsed = Number(pageInput)
    if (!Number.isFinite(parsed) || parsed < 1) {
      setPageInput(String(logPage + 1))
      return
    }
    const nextPage = Math.min(pageCount, Math.max(1, Math.trunc(parsed))) - 1
    setPageInput(String(nextPage + 1))
    if (nextPage !== logPage) setLogPage(nextPage)
  }

  const handlePageInputKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.currentTarget.blur()
      commitPageInput()
    }
  }

  useEffect(() => {
    if (tab === 1) {
      void loadLogs()
    }
  }, [loadLogs, tab])

  // Statistics calculation
  const stats = useMemo(() => {
    const total = config.tasks.length
    const enabled = config.tasks.filter((t) => t.enabled).length
    let successCount = 0
    let failedCount = 0
    Object.values(latestLogs).forEach((log) => {
      if (log.status === 'success') successCount++
      else if (log.status === 'failed') failedCount++
    })
    return { total, enabled, success: successCount, failed: failedCount }
  }, [config.tasks, latestLogs])

  // Save config immediately to backend
  const updateConfig = async (newConfig: AutomationConfig) => {
    try {
      const configToSave = { ...newConfig, enabled: true }
      const res = await api.setAutomationConfig(configToSave)
      if (res.status === 'ok') {
        setConfig(configToSave)
        void loadData()
      } else {
        setError(res.message)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  // Toggle single task enabled
  const handleToggleTask = async (taskId: string, checked: boolean) => {
    const nextTasks = config.tasks.map((t) => (t.id === taskId ? { ...t, enabled: checked } : t))
    await updateConfig({ ...config, tasks: nextTasks })
  }

  // Delete task click
  const handleDeleteClick = (task: AutomationTask) => {
    setTaskToDelete(task)
    setDeleteConfirmOpen(true)
  }

  // Confirm delete task
  const handleConfirmDelete = async () => {
    if (!taskToDelete) return
    const nextTasks = config.tasks.filter((t) => t.id !== taskToDelete.id)
    setDeleteConfirmOpen(false)
    await updateConfig({ ...config, tasks: nextTasks })
    setSuccess('任务删除成功')
    setTaskToDelete(null)
  }

  // Manual Trigger Run
  const handleTestTask = async (taskId: string) => {
    setTestingTaskId(taskId)
    setError(null)
    try {
      const res = await api.testAutomationTask(taskId)
      if (res.status === 'ok') {
        setSuccess('任务测试执行指令已下发，请在日志中查看结果')
        // Refresh logs after a small delay
        setTimeout(() => {
          void loadData()
          if (tab === 1) void loadLogs()
        }, 1500)
      } else {
        setError(res.message)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setTestingTaskId(null)
    }
  }

  // Open Dialog for Add/Edit
  const handleOpenTaskDialog = (task: AutomationTask | null = null) => {
    setEditingTask(task)
    setTaskDialogOpen(true)
  }

  const handleSaveTask = async (task: AutomationTask) => {
    const exists = config.tasks.some((t) => t.id === task.id)
    const nextTasks = exists
      ? config.tasks.map((t) => (t.id === task.id ? task : t))
      : [...config.tasks, task]

    await updateConfig({ ...config, tasks: nextTasks })
    setSuccess(editingTask ? '编辑任务成功' : '添加任务成功')
  }

  // Open Auto Clean Dialog
  const openAutoDialog = () => {
    setDndAutoCleanOpen(true)
  }

  // Auto clean log settings save
  const handleSaveAutoClean = async (cleanup: {
    retention_days_enabled: boolean
    retention_days: number
    max_entries_enabled: boolean
    max_entries: number
  }) => {
    if (!notificationConfig) return
    const nextConfig = { ...notificationConfig, log_cleanup: cleanup }
    try {
      const res = await api.setNotificationConfig(nextConfig)
      if (res.status === 'ok') {
        setNotificationConfig(nextConfig)
        setSuccess('自动清理设置已保存')
      } else {
        setError(res.message)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  // Advanced Clear Logs execute
  const handleAdvancedClear = async (filters: {
    type: string
    status: string
    start_date: string
    end_date: string
  }) => {
    try {
      const res = await api.clearAutomationLogs(filters)
      if (res.status === 'ok') {
        setSuccess(`已清理 ${res.data?.deleted ?? 0} 条日志`)
        setLogPage(0)
        void loadLogs()
      } else {
        setError(res.message)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
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
      {/* 头部区域 */}
      <Box display="flex" alignItems="center" justifyContent="space-between" mb={2} flexWrap="wrap" gap={2}>
        <Box display="flex" alignItems="center" gap={1.5}>
          <Typography variant="h5" fontWeight={700}>
            自动化中心
          </Typography>
          {/* 内联统计指标 */}
          <Box display={{ xs: 'none', md: 'flex' }} gap={1} ml={2}>
            <Chip
              variant="outlined"
              size="small"
              label={`任务数 ${stats.total}`}
              sx={{ bgcolor: 'rgba(148, 163, 184, 0.06)' }}
            />
            <Chip
              variant="outlined"
              size="small"
              label={`已启用 ${stats.enabled}`}
              color={stats.enabled > 0 ? 'primary' : 'default'}
            />
            <Chip
              variant="outlined"
              size="small"
              label={`成功 ${stats.success}`}
              sx={{ color: 'success.main', borderColor: 'success.main', bgcolor: 'rgba(42, 174, 103, 0.04)' }}
            />
            <Chip
              variant="outlined"
              size="small"
              label={`失败 ${stats.failed}`}
              sx={{ color: 'error.main', borderColor: 'error.main', bgcolor: 'rgba(211, 47, 47, 0.04)' }}
            />
          </Box>
        </Box>

        <Box display="flex" gap={1}>
          <Button
            variant="contained"
            startIcon={<Add />}
            onClick={() => handleOpenTaskDialog(null)}
          >
            新建任务
          </Button>
        </Box>
      </Box>

      {/* 错误与成功消息提示 */}
      <ErrorSnackbar error={error} onClose={() => setError(null)} />
      <Snackbar
        open={!!success}
        autoHideDuration={3000}
        onClose={() => setSuccess(null)}
        anchorOrigin={{ vertical: 'top', horizontal: 'center' }}
      >
        <Alert severity="info" variant="filled" onClose={() => setSuccess(null)}>
          {success}
        </Alert>
      </Snackbar>

      {/* Tabs */}
      <Box sx={{ borderBottom: 1, borderColor: 'divider', mb: 2 }} ref={tabsRef}>
        <Tabs value={tab} onChange={(_, v) => setTab(v)}>
          <Tab label="自动化控制台" />
          <Tab label="运行日志" />
        </Tabs>
      </Box>

      {/* 面板 1：自动化控制台 */}
      {tab === 0 && (
        <Box>
          <Grid container spacing={2.5}>
            {config.tasks.map((task) => (
              <Grid size={{ xs: 12, md: 6, lg: 4 }} key={task.id}>
                <AutomationTaskCard
                  task={task}
                  latestLog={latestLogs[task.id]}
                  testingTaskId={testingTaskId}
                  onTest={(id) => void handleTestTask(id)}
                  onEdit={handleOpenTaskDialog}
                  onDelete={handleDeleteClick}
                  onToggle={(id, val) => void handleToggleTask(id, val)}
                />
              </Grid>
            ))}

            {config.tasks.length === 0 && (
              <Grid size={12}>
                <Paper variant="outlined" sx={{ p: 5, textAlign: 'center', color: 'text.secondary' }}>
                  <AutoMode sx={{ fontSize: 48, mb: 1, opacity: 0.3 }} />
                  <Typography>暂无自动化任务，点击上方“新建任务”开始添加</Typography>
                </Paper>
              </Grid>
            )}
          </Grid>
        </Box>
      )}

      {/* 面板 2：运行日志 */}
      {tab === 1 && (
        <Card sx={{ height: cardHeight, minHeight: 520, borderRadius: 1.5 }}>
          <CardContent sx={{ height: '100%', display: 'flex', flexDirection: 'column', p: 2, pb: 0, '&:last-child': { pb: 0 } }}>
            {/* 日志筛选与搜索工具栏 */}
            <Box display="flex" gap={1.5} flexWrap="wrap" mb={2}>
              <TextField
                select
                size="small"
                label="任务类型"
                value={filterType}
                onChange={(e) => {
                  setFilterType(e.target.value)
                  setLogPage(0)
                }}
                sx={[{ minWidth: 160 }, filterTextFieldSx]}
              >
                <MenuItem value="">所有任务类型</MenuItem>
                <MenuItem value="restart_baseband">基带维护</MenuItem>
                <MenuItem value="reboot_device">系统操作</MenuItem>
                <MenuItem value="backup_data">备份数据</MenuItem>
                <MenuItem value="send_sms">短信发送</MenuItem>
              </TextField>

              <TextField
                select
                size="small"
                label="执行状态"
                value={filterStatus}
                onChange={(e) => {
                  setFilterStatus(e.target.value)
                  setLogPage(0)
                }}
                sx={[{ minWidth: 140 }, filterTextFieldSx]}
              >
                <MenuItem value="">所有状态</MenuItem>
                <MenuItem value="success">成功</MenuItem>
                <MenuItem value="failed">失败</MenuItem>
              </TextField>

              <DateRangePicker
                startDate={logStartDate}
                endDate={logEndDate}
                onChange={(start, end) => {
                  setLogStartDate(start)
                  setLogEndDate(end)
                  setLogPage(0)
                }}
                minWidth={280}
              />

              <TextField
                size="small"
                placeholder="搜索关键字..."
                value={searchQuery}
                onChange={(e) => {
                  setSearchQuery(e.target.value)
                  setLogPage(0)
                }}
                sx={[{ flexGrow: 1, minWidth: { xs: '100%', sm: 260 } }, filterTextFieldSx]}
                slotProps={{
                  input: {
                    startAdornment: (
                      <InputAdornment position="start">
                        <Search fontSize="small" />
                      </InputAdornment>
                    ),
                    endAdornment: searchQuery && (
                      <InputAdornment position="end">
                        <IconButton size="small" onClick={() => { setSearchQuery(''); setLogPage(0); }}>
                          <Clear fontSize="small" />
                        </IconButton>
                      </InputAdornment>
                    ),
                  },
                }}
              />
            </Box>

            {/* 日志表格 */}
            <TableContainer component={Paper} variant="outlined" sx={{ flex: 1, minHeight: 0 }}>
              <Table size="small" stickyHeader>
                <TableHead>
                  <TableRow>
                    <TableCell sx={{ width: 150, fontWeight: 400 }}>时间</TableCell>
                    <TableCell sx={{ width: 150, fontWeight: 400 }}>任务名称</TableCell>
                    <TableCell sx={{ width: 120, fontWeight: 400 }}>任务类型</TableCell>
                    <TableCell sx={{ width: 100, fontWeight: 400 }}>执行结果</TableCell>
                    <TableCell sx={{ fontWeight: 400 }}>执行详情</TableCell>
                  </TableRow>
                </TableHead>
                <TableBody>
                  {logsLoading ? (
                    <TableRow>
                      <TableCell colSpan={5} align="center" sx={{ py: 5 }}>
                        <CircularProgress size={24} />
                      </TableCell>
                    </TableRow>
                  ) : logs.length === 0 ? (
                    <TableRow>
                      <TableCell colSpan={5} align="center" sx={{ py: 5, color: 'text.secondary' }}>
                        暂无运行日志记录
                      </TableCell>
                    </TableRow>
                  ) : (
                    logs.map((log) => (
                      <TableRow key={log.id} sx={{ height: 40, '& .MuiTableCell-root': { py: 0.5 } }}>
                        <TableCell sx={{ width: 150, whiteSpace: 'nowrap', fontWeight: 400 }}>{log.created_at}</TableCell>
                        <TableCell sx={{ width: 150, fontWeight: 400 }}>{log.task_name}</TableCell>
                        <TableCell sx={{ width: 120, fontWeight: 400 }}>
                          {log.task_type === 'restart_baseband' && '基带维护'}
                          {log.task_type === 'reboot_device' && '系统操作'}
                          {log.task_type === 'backup_data' && '备份数据'}
                          {log.task_type === 'send_sms' && '短信发送'}
                        </TableCell>
                        <TableCell
                          sx={{
                            width: 100,
                            fontWeight: 400,
                            color: log.status === 'success' ? 'primary.main' : 'error.main',
                          }}
                        >
                          {log.status === 'success' ? '成功' : '失败'}
                        </TableCell>
                        <TableCell sx={{ fontWeight: 400, wordBreak: 'break-all' }} title={log.detail}>
                          {log.detail}
                        </TableCell>
                      </TableRow>
                    ))
                  )}
                </TableBody>
              </Table>
            </TableContainer>

            {/* 日志底部统计与操作栏 */}
            <Box sx={{ height: 56, minHeight: 56, display: 'flex', justifyContent: 'space-between', alignItems: 'center', mt: 0, gap: 1.5, overflow: 'hidden' }}>
              <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, minWidth: 0, flex: '1 1 auto', overflow: 'hidden' }}>
                <Typography variant="body2" color="text.secondary" noWrap sx={{ flexShrink: 0 }}>
                  {logTotal === 0 ? '共 0 条记录' : `${startRecord}-${endRecord} / 共 ${logTotal} 条`}
                </Typography>
                <Box sx={{ width: '1px', height: 18, bgcolor: 'divider', flex: '0 0 1px' }} />

                <Button
                  size="small"
                  variant="text"
                  startIcon={<SmartToy />}
                  onClick={openAutoDialog}
                  sx={{ flexShrink: 0, minWidth: 110, whiteSpace: 'nowrap' }}
                >
                  {notificationConfig && (notificationConfig.log_cleanup.retention_days_enabled || notificationConfig.log_cleanup.max_entries_enabled)
                    ? '自动清理:开启'
                    : '自动清理:关闭'}
                </Button>

                <Button
                  size="small"
                  color="error"
                  variant="text"
                  startIcon={<DeleteSweep />}
                  onClick={() => setAdvancedClearOpen(true)}
                  sx={{ flexShrink: 0, minWidth: 88, whiteSpace: 'nowrap' }}
                >
                  高级清理
                </Button>
                {logsLoading && <CircularProgress size={16} sx={{ flexShrink: 0 }} />}
              </Box>

              <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5, flexShrink: 0 }}>
                <IconButton size="small" disabled={!canGoPrev} onClick={() => setLogPage(0)} aria-label="第一页">
                  <FirstPage fontSize="small" />
                </IconButton>
                <IconButton size="small" disabled={!canGoPrev} onClick={() => setLogPage(logPage - 1)} aria-label="上一页">
                  <KeyboardArrowLeft fontSize="small" />
                </IconButton>
                <TextField
                  size="small"
                  value={pageInput}
                  onChange={(event: React.ChangeEvent<HTMLInputElement>) => {
                    const next = event.target.value
                    if (/^\d{0,4}$/.test(next)) setPageInput(next)
                  }}
                  onBlur={commitPageInput}
                  onKeyDown={handlePageInputKeyDown}
                  slotProps={{
                    htmlInput: {
                      inputMode: 'numeric',
                      'aria-label': '页码',
                    },
                  }}
                  sx={{
                    width: 48,
                    '& .MuiInputBase-input': {
                      py: 0.5,
                      px: 0.75,
                      textAlign: 'center',
                      fontSize: '0.875rem',
                    },
                  }}
                />
                <Typography variant="body2" color="text.secondary">/ {pageCount}</Typography>
                <IconButton size="small" disabled={!canGoNext} onClick={() => setLogPage(logPage + 1)} aria-label="下一页">
                  <KeyboardArrowRight fontSize="small" />
                </IconButton>
                <IconButton size="small" disabled={!canGoNext} onClick={() => setLogPage(pageCount - 1)} aria-label="最后一页">
                  <LastPage fontSize="small" />
                </IconButton>
              </Box>
            </Box>
          </CardContent>
        </Card>
      )}

      {/* 弹窗 1：添加/修改自动化任务 */}
      <AutomationTaskDialog
        open={taskDialogOpen}
        onClose={() => setTaskDialogOpen(false)}
        editingTask={editingTask}
        onSave={handleSaveTask}
        defaultBackupLocalDir={backupLocalDir}
      />

      {/* 弹窗 2：自动清理配置 */}
      <AutoCleanDialog
        open={dndAutoCleanOpen}
        onClose={() => setDndAutoCleanOpen(false)}
        notificationConfig={notificationConfig}
        onSave={handleSaveAutoClean}
      />

      {/* 弹窗 3：高级清理 */}
      <AdvancedClearDialog
        open={advancedClearOpen}
        onClose={() => setAdvancedClearOpen(false)}
        defaultType={filterType}
        defaultStatus={filterStatus}
        defaultStartDate={logStartDate}
        defaultEndDate={logEndDate}
        onConfirm={handleAdvancedClear}
      />

      {/* 二次确认删除 Dialog */}
      <Dialog
        open={deleteConfirmOpen}
        onClose={() => setDeleteConfirmOpen(false)}
        slotProps={{
          paper: { sx: { borderRadius: 2.5 } },
        }}
      >
        <DialogTitle sx={{ fontWeight: 700 }}>确认删除任务</DialogTitle>
        <DialogContent>
          <Typography variant="body2">
            你确定要删除自动化任务“{taskToDelete?.name}”吗？此操作无法撤销。
          </Typography>
        </DialogContent>
        <DialogActions sx={{ px: 3, py: 2 }}>
          <Button variant="outlined" onClick={() => setDeleteConfirmOpen(false)}>
            取消
          </Button>
          <Button variant="contained" color="error" onClick={() => void handleConfirmDelete()}>
            确认删除
          </Button>
        </DialogActions>
      </Dialog>

    </Box>
  )
}
