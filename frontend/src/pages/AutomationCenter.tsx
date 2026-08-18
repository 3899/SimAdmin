import { useState, useEffect, useCallback, useMemo, useRef } from 'react'
import {
  Box,
  Button,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  Grid,
  Paper,
  Tabs,
  Tab,
  Typography,
  Chip,
  Alert,
  Snackbar,
} from '@mui/material'
import {
  Add,
  AutoMode,
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

import AutomationTaskCard from './automation/AutomationTaskCard'
import AutomationTaskDialog from './automation/AutomationTaskDialog'
import AutomationLogsTab from './automation/AutomationLogsTab'
import AutoCleanDialog from './automation/AutoCleanDialog'
import AdvancedClearDialog from './automation/AdvancedClearDialog'

const LOG_PAGE_SIZE = 15

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
  const [filterType, setFilterType] = useState('')
  const [filterStatus, setFilterStatus] = useState('')
  const [logStartDate, setLogStartDate] = useState('')
  const [logEndDate, setLogEndDate] = useState('')
  const [searchQuery, setSearchQuery] = useState('')

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
        <AutomationLogsTab
          logs={logs}
          total={logTotal}
          loading={logsLoading}
          type={filterType}
          status={filterStatus}
          startDate={logStartDate}
          endDate={logEndDate}
          query={searchQuery}
          page={logPage}
          pageSize={LOG_PAGE_SIZE}
          height={cardHeight}
          onTypeChange={(value) => { setFilterType(value); setLogPage(0) }}
          onStatusChange={(value) => { setFilterStatus(value); setLogPage(0) }}
          onDateRangeChange={(start, end) => { setLogStartDate(start); setLogEndDate(end); setLogPage(0) }}
          onQueryChange={(value) => { setSearchQuery(value); setLogPage(0) }}
          onPageChange={setLogPage}
          footerActions={(
            <>
              <Box sx={{ width: '1px', height: 18, bgcolor: 'divider', flex: '0 0 1px' }} />
              <Button size="small" variant="text" startIcon={<SmartToy />} onClick={openAutoDialog} sx={{ flexShrink: 0, minWidth: 110, whiteSpace: 'nowrap' }}>
                {notificationConfig && (notificationConfig.log_cleanup.retention_days_enabled || notificationConfig.log_cleanup.max_entries_enabled)
                  ? '自动清理:开启'
                  : '自动清理:关闭'}
              </Button>
              <Button size="small" color="error" variant="text" startIcon={<DeleteSweep />} onClick={() => setAdvancedClearOpen(true)} sx={{ flexShrink: 0, minWidth: 88, whiteSpace: 'nowrap' }}>
                高级清理
              </Button>
            </>
          )}
        />
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
