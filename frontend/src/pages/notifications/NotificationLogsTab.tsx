import { useEffect, useState, type ChangeEvent, type KeyboardEvent, type ReactNode } from 'react'
import {
  Alert,
  Box,
  Button,
  Card,
  CardContent,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  IconButton,
  InputAdornment,
  MenuItem,
  Paper,
  Switch,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  TextField,
  Typography,
} from '@mui/material'
import {
  Close,
  Delete,
  DeleteSweep,
  FirstPage,
  SmartToy,
  KeyboardArrowLeft,
  KeyboardArrowRight,
  LastPage,
  Search,
} from '@mui/icons-material'
import type { NotificationLogCleanupConfig, NotificationLogEntry } from '../../api/current'
import { EVENT_TYPES, eventLabel, statusLabel } from './notificationModel'
import { notificationFilterTextFieldSx } from './notificationStyles'
import DateRangePicker from '../../components/DateRangePicker'

const LOG_STATUS_OPTIONS = [
  { value: 'success', label: '成功' },
  { value: 'failed', label: '失败' },
  { value: 'quiet_hours', label: '免打扰' },
  { value: 'unmatched', label: '未匹配规则' },
  { value: 'no_available_channel', label: '无可用通道' },
]

const EXPANDABLE_EVENT_TYPES = new Set(['system_event', 'device_status'])
function logSummaryText(log: NotificationLogEntry) {
  if (log.status === 'failed' && log.message) return `${log.summary}\n失败原因：${log.message}`
  if (log.status === 'quiet_hours' && log.message) return `${log.summary}；免打扰原因：${log.message}`
  if (log.status === 'unmatched' && log.message) return `${log.summary}\n未匹配规则原因：${log.message}`
  if (log.status === 'no_available_channel' && log.message) return `${log.summary}；无可用通道原因：${log.message}`
  return log.summary
}

function logSummaryLines(text: string) {
  return text.replace(/\r\n/g, '\n').split('\n')
}

function collapsedSummaryText(lines: string[]) {
  return `${lines.slice(0, 2).join('\n')}${lines.length > 1 ? '\n' : ''}${lines[2] ?? ''}`
}

type NotificationLogClearFilters = {
  type: string
  status: string
  start_date: string
  end_date: string
}

type NotificationLogsTabProps = {
  logs: NotificationLogEntry[]
  logTotal: number
  logsLoading: boolean
  logType: string
  logStatus: string
  logStartDate: string
  logEndDate: string
  logCleanup: NotificationLogCleanupConfig
  cleanupSaving: boolean
  logQuery: string
  logPage: number
  logPageSize: number
  onLogTypeChange: (value: string) => void
  onLogStatusChange: (value: string) => void
  onLogDateRangeChange: (startDate: string, endDate: string) => void
  onLogQueryChange: (value: string) => void
  onLogPageChange: (page: number) => void
  onClearLogs: (filters: NotificationLogClearFilters) => void
  onSaveLogCleanup: (logCleanup: NotificationLogCleanupConfig) => void
  filterExtension?: ReactNode
  renderSource?: (log: NotificationLogEntry) => ReactNode
}

export default function NotificationLogsTab({
  logs,
  logTotal,
  logsLoading,
  logType,
  logStatus,
  logStartDate,
  logEndDate,
  logCleanup,
  cleanupSaving,
  logQuery,
  logPage,
  logPageSize,
  onLogTypeChange,
  onLogStatusChange,
  onLogDateRangeChange,
  onLogQueryChange,
  onLogPageChange,
  onClearLogs,
  onSaveLogCleanup,
  filterExtension,
  renderSource,
}: NotificationLogsTabProps) {
  const pageCount = Math.max(1, Math.ceil(logTotal / logPageSize))
  const startRecord = logTotal === 0 ? 0 : logPage * logPageSize + 1
  const endRecord = Math.min(logTotal, (logPage + 1) * logPageSize)
  const canGoPrev = logPage > 0
  const canGoNext = logPage < pageCount - 1
  const [pageInput, setPageInput] = useState(() => String(logPage + 1))
  const [clearDialogOpen, setClearDialogOpen] = useState(false)
  const [clearType, setClearType] = useState(logType)
  const [clearStatus, setClearStatus] = useState(logStatus)
  const [clearStartDate, setClearStartDate] = useState(logStartDate)
  const [clearEndDate, setClearEndDate] = useState(logEndDate)
  const [autoDialogOpen, setAutoDialogOpen] = useState(false)
  const [autoRetentionEnabled, setAutoRetentionEnabled] = useState(logCleanup.retention_days_enabled)
  const [autoRetentionDays, setAutoRetentionDays] = useState(String(logCleanup.retention_days))
  const [autoMaxEntriesEnabled, setAutoMaxEntriesEnabled] = useState(logCleanup.max_entries_enabled)
  const [autoMaxEntries, setAutoMaxEntries] = useState(String(logCleanup.max_entries))
  const [expandedLogIds, setExpandedLogIds] = useState<Set<number>>(() => new Set())

  useEffect(() => {
    setPageInput(String(logPage + 1))
  }, [logPage])

  const commitPageInput = () => {
    const parsed = Number(pageInput)
    if (!Number.isFinite(parsed) || parsed < 1) {
      setPageInput(String(logPage + 1))
      return
    }
    const nextPage = Math.min(pageCount, Math.max(1, Math.trunc(parsed))) - 1
    setPageInput(String(nextPage + 1))
    if (nextPage !== logPage) onLogPageChange(nextPage)
  }

  const handlePageInputKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.currentTarget.blur()
      commitPageInput()
    }
  }

  const toggleExpandedLog = (logId: number) => {
    setExpandedLogIds((current) => {
      const next = new Set(current)
      if (next.has(logId)) {
        next.delete(logId)
      } else {
        next.add(logId)
      }
      return next
    })
  }

  const openClearDialog = () => {
    setClearType(logType)
    setClearStatus(logStatus)
    setClearStartDate(logStartDate)
    setClearEndDate(logEndDate)
    setClearDialogOpen(true)
  }

  const openAutoDialog = () => {
    setAutoRetentionEnabled(logCleanup.retention_days_enabled)
    setAutoRetentionDays(String(logCleanup.retention_days))
    setAutoMaxEntriesEnabled(logCleanup.max_entries_enabled)
    setAutoMaxEntries(String(logCleanup.max_entries))
    setAutoDialogOpen(true)
  }

  const positiveInt = (value: string, fallback: number) => {
    const parsed = Number(value)
    if (!Number.isFinite(parsed) || parsed < 1) return fallback
    return Math.trunc(parsed)
  }

  const confirmAutoCleanup = () => {
    onSaveLogCleanup({
      retention_days_enabled: autoRetentionEnabled,
      retention_days: positiveInt(autoRetentionDays, logCleanup.retention_days || 90),
      max_entries_enabled: autoMaxEntriesEnabled,
      max_entries: positiveInt(autoMaxEntries, logCleanup.max_entries || 10000),
    })
    setAutoDialogOpen(false)
  }

  const confirmClear = () => {
    onClearLogs({
      type: clearType,
      status: clearStatus,
      start_date: clearStartDate,
      end_date: clearEndDate,
    })
    setClearDialogOpen(false)
  }

  return (
    <Card sx={{ height: 'calc(100vh - 220px)', minHeight: 520 }}>
      <CardContent sx={{ height: '100%', display: 'flex', flexDirection: 'column', p: 2, pb: 0, '&:last-child': { pb: 0 } }}>
        <Box display="flex" gap={1.5} flexWrap="wrap" mb={2}>
          {filterExtension}
          <TextField
            select
            size="small"
            label="消息类型"
            value={logType}
            onChange={(event: ChangeEvent<HTMLInputElement>) => onLogTypeChange(event.target.value)}
            sx={[{ minWidth: 150 }, notificationFilterTextFieldSx]}
            SelectProps={{ MenuProps: { PaperProps: { sx: { '& .MuiMenuItem-root': { fontSize: '14px' } } } } }}
          >
            <MenuItem value="" sx={{ fontSize: '14px' }}>所有消息类型</MenuItem>
            {EVENT_TYPES.map((type) => <MenuItem key={type.key} value={type.key} sx={{ fontSize: '14px' }}>{type.label}</MenuItem>)}
          </TextField>
          <TextField
            select
            size="small"
            label="状态"
            value={logStatus}
            onChange={(event: ChangeEvent<HTMLInputElement>) => onLogStatusChange(event.target.value)}
            sx={[{ minWidth: 140 }, notificationFilterTextFieldSx]}
            SelectProps={{ MenuProps: { PaperProps: { sx: { '& .MuiMenuItem-root': { fontSize: '14px' } } } } }}
          >
            <MenuItem value="" sx={{ fontSize: '14px' }}>所有状态</MenuItem>
            {LOG_STATUS_OPTIONS.map((status) => <MenuItem key={status.value} value={status.value} sx={{ fontSize: '14px' }}>{status.label}</MenuItem>)}
          </TextField>
          <DateRangePicker startDate={logStartDate} endDate={logEndDate} onChange={onLogDateRangeChange} minWidth={280} />
          <TextField
            size="small"
            value={logQuery}
            onChange={(event: ChangeEvent<HTMLInputElement>) => onLogQueryChange(event.target.value)}
            placeholder="搜索关键字..."
            sx={[{ minWidth: { xs: '100%', sm: 260 } }, notificationFilterTextFieldSx]}
            slotProps={{
              input: {
                startAdornment: (
                  <InputAdornment position="start">
                    <Search fontSize="small" />
                  </InputAdornment>
                ),
              },
            }}
          />
        </Box>

        <TableContainer component={Paper} variant="outlined" sx={{ flex: 1, minHeight: 0 }}>
          <Table size="small" stickyHeader>
            <TableHead>
              <TableRow>
                <TableCell sx={{ width: 150, fontWeight: 400 }}>时间</TableCell>
                <TableCell sx={{ width: 96, fontWeight: 400 }}>类型</TableCell>
                <TableCell sx={{ width: 88, fontWeight: 400 }}>状态</TableCell>
                {renderSource && <TableCell sx={{ width: 150, fontWeight: 400 }}>来源设备</TableCell>}
                <TableCell sx={{ width: '42%', minWidth: 360, fontWeight: 400 }}>内容摘要</TableCell>
                <TableCell sx={{ width: 160, fontWeight: 400 }}>转发规则</TableCell>
                <TableCell sx={{ width: 160, fontWeight: 400 }}>通知通道</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {logs.map((log) => {
                const summaryText = logSummaryText(log)
                const summaryLines = logSummaryLines(summaryText)
                const canExpandSummary = EXPANDABLE_EVENT_TYPES.has(log.event_type) && summaryLines.length > 3
                const expanded = expandedLogIds.has(log.id)
                const visibleSummary = canExpandSummary && !expanded
                  ? collapsedSummaryText(summaryLines)
                  : summaryText
                return (
                  <TableRow key={log.id} sx={{ height: 40, '& .MuiTableCell-root': { py: 0.5 } }}>
                  <TableCell sx={{ width: 150, whiteSpace: 'nowrap', fontWeight: 400 }}>{log.created_at}</TableCell>
                  <TableCell sx={{ width: 96, fontWeight: 400 }}>{eventLabel(log.event_type)}</TableCell>
                  <TableCell
                    sx={{
                      width: 88,
                      fontWeight: 400,
                      color: log.status === 'success'
                        ? 'primary.main'
                        : log.status === 'failed'
                          ? 'error.main'
                          : log.status === 'quiet_hours'
                            ? 'warning.main'
                            : 'text.secondary',
                    }}
                  >
                    {statusLabel(log.status)}
                  </TableCell>
                  {renderSource && <TableCell sx={{ width: 150, fontWeight: 400 }}>{renderSource(log)}</TableCell>}
                  <TableCell sx={{ fontWeight: 400, whiteSpace: 'pre-line' }} title={summaryText}>
                    {visibleSummary}
                    {canExpandSummary && (
                      <Button
                        size="small"
                        variant="text"
                        onClick={() => toggleExpandedLog(log.id)}
                        sx={{
                          display: expanded ? 'block' : 'inline',
                          minWidth: 0,
                          mt: expanded ? 0.25 : 0,
                          p: 0,
                          fontWeight: 400,
                          verticalAlign: 'baseline',
                        }}
                      >
                        {expanded ? '收起' : ' ...查看更多'}
                      </Button>
                    )}
                  </TableCell>
                  <TableCell sx={{ fontWeight: 400 }}>{log.rule_name || '-'}</TableCell>
                  <TableCell sx={{ fontWeight: 400 }}>{log.channel_name || '-'}</TableCell>
                  </TableRow>
                )
              })}
              {logs.length === 0 && (
                <TableRow>
                  <TableCell colSpan={renderSource ? 7 : 6} align="center" sx={{ py: 4, color: 'text.secondary' }}>暂无转发日志</TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </TableContainer>
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
              {logCleanup.retention_days_enabled || logCleanup.max_entries_enabled
                ? '自动清理:开启'
                : '自动清理:关闭'}
            </Button>
            <Button size="small" color="error" variant="text" startIcon={<DeleteSweep />} onClick={openClearDialog} sx={{ flexShrink: 0, minWidth: 88, whiteSpace: 'nowrap' }}>
              高级清理
            </Button>
            {logsLoading && <CircularProgress size={16} sx={{ flexShrink: 0 }} />}
          </Box>
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5, flexShrink: 0 }}>
            <IconButton size="small" disabled={!canGoPrev} onClick={() => onLogPageChange(0)} aria-label="第一页">
              <FirstPage fontSize="small" />
            </IconButton>
            <IconButton size="small" disabled={!canGoPrev} onClick={() => onLogPageChange(logPage - 1)} aria-label="上一页">
              <KeyboardArrowLeft fontSize="small" />
            </IconButton>
            <TextField
              size="small"
              value={pageInput}
              onChange={(event: ChangeEvent<HTMLInputElement>) => {
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
            <IconButton size="small" disabled={!canGoNext} onClick={() => onLogPageChange(logPage + 1)} aria-label="下一页">
              <KeyboardArrowRight fontSize="small" />
            </IconButton>
            <IconButton size="small" disabled={!canGoNext} onClick={() => onLogPageChange(pageCount - 1)} aria-label="最后一页">
              <LastPage fontSize="small" />
            </IconButton>
          </Box>
        </Box>
      </CardContent>
      <Dialog open={clearDialogOpen} onClose={() => setClearDialogOpen(false)} fullWidth maxWidth="xs">
        <DialogTitle sx={{ display: 'flex', alignItems: 'center', gap: 1, pr: 1 }}>
          <DeleteSweep color="primary" fontSize="small" />
          <Typography component="span" variant="subtitle1" fontWeight={700}>高级清理日志</Typography>
          <Box flexGrow={1} />
          <IconButton size="small" onClick={() => setClearDialogOpen(false)} aria-label="关闭">
            <Close fontSize="small" />
          </IconButton>
        </DialogTitle>
        <DialogContent dividers sx={{ pt: 3 }}>
          <Box display="flex" flexDirection="column" gap={2}>            
            <Alert severity="warning">
              清理操作不可逆，请谨慎选择过滤条件。默认条件为当前表格的筛选状态。
            </Alert>
            <TextField
              select
              size="small"
              label="消息类型"
              value={clearType}
              onChange={(event: ChangeEvent<HTMLInputElement>) => setClearType(event.target.value)}
              fullWidth
              SelectProps={{ MenuProps: { PaperProps: { sx: { '& .MuiMenuItem-root': { fontSize: '14px' } } } } }}
            >
              <MenuItem value="" sx={{ fontSize: '14px' }}>所有类型 (不限)</MenuItem>
              {EVENT_TYPES.map((type) => <MenuItem key={type.key} value={type.key} sx={{ fontSize: '14px' }}>{type.label}</MenuItem>)}
            </TextField>
            <TextField
              select
              size="small"
              label="转发状态"
              value={clearStatus}
              onChange={(event: ChangeEvent<HTMLInputElement>) => setClearStatus(event.target.value)}
              fullWidth
              sx={notificationFilterTextFieldSx}
              SelectProps={{ MenuProps: { PaperProps: { sx: { '& .MuiMenuItem-root': { fontSize: '14px' } } } } }}
            >
              <MenuItem value="" sx={{ fontSize: '14px' }}>所有状态 (不限)</MenuItem>
              {LOG_STATUS_OPTIONS.map((status) => <MenuItem key={status.value} value={status.value} sx={{ fontSize: '14px' }}>{status.label}</MenuItem>)}
            </TextField>
            <Box>
              <Typography variant="body2" color="text.secondary" mb={1}>时间范围 (按日计算)</Typography>
              <DateRangePicker
                startDate={clearStartDate}
                endDate={clearEndDate}
                onChange={(startDate, endDate) => {
                  setClearStartDate(startDate)
                  setClearEndDate(endDate)
                }}
                fullWidth
              />
              <Typography variant="caption" color="text.secondary">留空表示不限制开始或结束时间</Typography>
            </Box>
          </Box>
        </DialogContent>
        <DialogActions sx={{ px: 3, py: 2 }}>
          <Button variant="outlined" onClick={() => setClearDialogOpen(false)}>取消</Button>
          <Button color="error" variant="contained" startIcon={<Delete />} onClick={confirmClear}>
            确认清理
          </Button>
        </DialogActions>
      </Dialog>
      <Dialog open={autoDialogOpen} onClose={() => setAutoDialogOpen(false)} fullWidth maxWidth="xs">
        <DialogTitle sx={{ display: 'flex', alignItems: 'center', gap: 1, pr: 1 }}>
          <SmartToy color="primary" fontSize="small" />
          <Typography component="span" variant="subtitle1" fontWeight={700}>自动清理设置</Typography>
          <Box flexGrow={1} />
          <IconButton size="small" onClick={() => setAutoDialogOpen(false)} aria-label="关闭">
            <Close fontSize="small" />
          </IconButton>
        </DialogTitle>
        <DialogContent dividers sx={{ pt: 3 }}>
          <Box display="flex" flexDirection="column" gap={3}>
            <Box>
              <Box display="flex" alignItems="center" justifyContent="space-between" mb={0.5}>
                <Typography variant="subtitle2">按保留时长清理</Typography>
                <Switch
                  checked={autoRetentionEnabled}
                  onChange={(event: ChangeEvent<HTMLInputElement>) => setAutoRetentionEnabled(event.target.checked)}
                />
              </Box>
              <Typography variant="caption" color="text.secondary">超过设定天数的记录将被永久删除</Typography>
              <TextField
                size="small"
                type="number"
                value={autoRetentionDays}
                onChange={(event: ChangeEvent<HTMLInputElement>) => {
                  const next = event.target.value
                  if (/^\d{0,5}$/.test(next)) setAutoRetentionDays(next)
                }}
                fullWidth
                disabled={!autoRetentionEnabled}
                sx={{ mt: 1, ...notificationFilterTextFieldSx }}
                slotProps={{
                  input: { endAdornment: <InputAdornment position="end">天</InputAdornment> },
                  htmlInput: { min: 1 },
                }}
              />
            </Box>
            <Box sx={{ borderTop: 1, borderColor: 'divider' }} />
            <Box>
              <Box display="flex" alignItems="center" justifyContent="space-between" mb={0.5}>
                <Typography variant="subtitle2">按最大条数清理</Typography>
                <Switch
                  checked={autoMaxEntriesEnabled}
                  onChange={(event: ChangeEvent<HTMLInputElement>) => setAutoMaxEntriesEnabled(event.target.checked)}
                />
              </Box>
              <Typography variant="caption" color="text.secondary">总数超过此阈值时，自动删除最旧记录</Typography>
              <TextField
                size="small"
                type="number"
                value={autoMaxEntries}
                onChange={(event: ChangeEvent<HTMLInputElement>) => {
                  const next = event.target.value
                  if (/^\d{0,8}$/.test(next)) setAutoMaxEntries(next)
                }}
                fullWidth
                disabled={!autoMaxEntriesEnabled}
                sx={{ mt: 1, ...notificationFilterTextFieldSx }}
                slotProps={{
                  input: { endAdornment: <InputAdornment position="end">条</InputAdornment> },
                  htmlInput: { min: 1 },
                }}
              />
            </Box>
          </Box>
        </DialogContent>
        <DialogActions sx={{ px: 3, py: 2 }}>
          <Button variant="outlined" onClick={() => setAutoDialogOpen(false)}>取消</Button>
          <Button variant="contained" disabled={cleanupSaving} onClick={confirmAutoCleanup}>
            保存设置
          </Button>
        </DialogActions>
      </Dialog>
    </Card>
  )
}
