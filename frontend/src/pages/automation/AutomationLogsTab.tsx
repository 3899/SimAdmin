import { useState, type ChangeEvent, type KeyboardEvent, type ReactNode } from 'react'
import {
  Box,
  Card,
  CardContent,
  CircularProgress,
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
  TextField,
  Typography,
} from '@mui/material'
import {
  Clear,
  FirstPage,
  KeyboardArrowLeft,
  KeyboardArrowRight,
  LastPage,
  Search,
} from '@mui/icons-material'
import DateRangePicker from '../../components/DateRangePicker'

export interface AutomationLogItem {
  id: number | string
  task_id: string
  task_name: string
  task_type: string
  status: string
  detail: string
  created_at: string
}

export interface AutomationLogOption {
  value: string
  label: string
}

type AutomationLogsTabProps = {
  logs: AutomationLogItem[]
  total: number
  loading: boolean
  type: string
  status: string
  startDate: string
  endDate: string
  query: string
  page: number
  pageSize: number
  onTypeChange: (value: string) => void
  onStatusChange: (value: string) => void
  onDateRangeChange: (startDate: string, endDate: string) => void
  onQueryChange: (value: string) => void
  onPageChange: (page: number) => void
  typeOptions?: AutomationLogOption[]
  statusOptions?: AutomationLogOption[]
  filterExtension?: ReactNode
  renderSource?: (log: AutomationLogItem) => ReactNode
  renderStatus?: (log: AutomationLogItem) => ReactNode
  footerActions?: ReactNode
  height?: number | string
  showDateFilter?: boolean
}

const DEFAULT_TYPE_OPTIONS: AutomationLogOption[] = [
  { value: 'restart_baseband', label: '重启基带' },
  { value: 'reboot_device', label: '重启设备' },
  { value: 'backup_data', label: '备份数据' },
  { value: 'send_sms', label: '发送短信' },
]

const DEFAULT_STATUS_OPTIONS: AutomationLogOption[] = [
  { value: 'success', label: '成功' },
  { value: 'failed', label: '失败' },
]

const filterTextFieldSx = {
  '& .MuiInputBase-input': { fontSize: '14px' },
  '& .MuiInputBase-input::placeholder': { fontSize: '14px' },
  '& .MuiInputLabel-root': { fontSize: '14px' },
  '& .MuiSelect-select': { fontSize: '14px' },
} as const

export default function AutomationLogsTab({
  logs,
  total,
  loading,
  type,
  status,
  startDate,
  endDate,
  query,
  page,
  pageSize,
  onTypeChange,
  onStatusChange,
  onDateRangeChange,
  onQueryChange,
  onPageChange,
  typeOptions = DEFAULT_TYPE_OPTIONS,
  statusOptions = DEFAULT_STATUS_OPTIONS,
  filterExtension,
  renderSource,
  renderStatus,
  footerActions,
  height = 'calc(100vh - 220px)',
  showDateFilter = true,
}: AutomationLogsTabProps) {
  const pageCount = Math.max(1, Math.ceil(total / pageSize))
  const startRecord = total === 0 ? 0 : page * pageSize + 1
  const endRecord = Math.min(total, (page + 1) * pageSize)
  const canGoPrev = page > 0
  const canGoNext = page < pageCount - 1
  const [pageInput, setPageInput] = useState(() => String(page + 1))
  const [syncedPage, setSyncedPage] = useState(page)

  if (syncedPage !== page) {
    setSyncedPage(page)
    setPageInput(String(page + 1))
  }

  const commitPageInput = () => {
    const parsed = Number(pageInput)
    if (!Number.isFinite(parsed) || parsed < 1) {
      setPageInput(String(page + 1))
      return
    }
    const nextPage = Math.min(pageCount, Math.max(1, Math.trunc(parsed))) - 1
    setPageInput(String(nextPage + 1))
    if (nextPage !== page) onPageChange(nextPage)
  }

  const handlePageInputKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.currentTarget.blur()
      commitPageInput()
    }
  }

  const typeLabel = (value: string) => typeOptions.find((option) => option.value === value)?.label ?? value
  const statusLabel = (value: string) => statusOptions.find((option) => option.value === value)?.label ?? value

  return (
    <Card sx={{ height, minHeight: 520, borderRadius: 1.5 }}>
      <CardContent sx={{ height: '100%', display: 'flex', flexDirection: 'column', p: 2, pb: 0, '&:last-child': { pb: 0 } }}>
        <Box display="flex" gap={1.5} flexWrap="wrap" mb={2}>
          {filterExtension}
          <TextField
            select
            size="small"
            label="任务类型"
            value={type}
            onChange={(event: ChangeEvent<HTMLInputElement>) => onTypeChange(event.target.value)}
            sx={[{ minWidth: 160 }, filterTextFieldSx]}
          >
            <MenuItem value="">所有任务类型</MenuItem>
            {typeOptions.map((option) => <MenuItem key={option.value} value={option.value}>{option.label}</MenuItem>)}
          </TextField>
          <TextField
            select
            size="small"
            label="执行状态"
            value={status}
            onChange={(event: ChangeEvent<HTMLInputElement>) => onStatusChange(event.target.value)}
            sx={[{ minWidth: 140 }, filterTextFieldSx]}
          >
            <MenuItem value="">所有状态</MenuItem>
            {statusOptions.map((option) => <MenuItem key={option.value} value={option.value}>{option.label}</MenuItem>)}
          </TextField>
          {showDateFilter && (
            <DateRangePicker startDate={startDate} endDate={endDate} onChange={onDateRangeChange} minWidth={280} />
          )}
          <TextField
            size="small"
            placeholder="搜索关键字..."
            value={query}
            onChange={(event: ChangeEvent<HTMLInputElement>) => onQueryChange(event.target.value)}
            sx={[{ flexGrow: 1, minWidth: { xs: '100%', sm: 260 } }, filterTextFieldSx]}
            slotProps={{
              input: {
                startAdornment: <InputAdornment position="start"><Search fontSize="small" /></InputAdornment>,
                endAdornment: query
                  ? <InputAdornment position="end"><IconButton size="small" onClick={() => onQueryChange('')}><Clear fontSize="small" /></IconButton></InputAdornment>
                  : undefined,
              },
            }}
          />
        </Box>

        <TableContainer component={Paper} variant="outlined" sx={{ flex: 1, minHeight: 0 }}>
          <Table size="small" stickyHeader sx={{ minWidth: renderSource ? 840 : 690 }}>
            <TableHead>
              <TableRow>
                <TableCell sx={{ width: 150, fontWeight: 400 }}>时间</TableCell>
                <TableCell sx={{ width: 150, fontWeight: 400 }}>任务名称</TableCell>
                <TableCell sx={{ width: 120, fontWeight: 400 }}>任务类型</TableCell>
                <TableCell sx={{ width: 100, fontWeight: 400 }}>执行结果</TableCell>
                {renderSource && <TableCell sx={{ width: 150, fontWeight: 400 }}>执行设备</TableCell>}
                <TableCell sx={{ fontWeight: 400 }}>执行详情</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {loading ? (
                <TableRow><TableCell colSpan={renderSource ? 6 : 5} align="center" sx={{ py: 5 }}><CircularProgress size={24} /></TableCell></TableRow>
              ) : logs.length === 0 ? (
                <TableRow><TableCell colSpan={renderSource ? 6 : 5} align="center" sx={{ py: 5, color: 'text.secondary' }}>暂无运行日志记录</TableCell></TableRow>
              ) : logs.map((log) => (
                <TableRow key={log.id} sx={{ height: 40, '& .MuiTableCell-root': { py: 0.5 } }}>
                  <TableCell sx={{ width: 150, whiteSpace: 'nowrap', fontWeight: 400 }}>{log.created_at}</TableCell>
                  <TableCell sx={{ width: 150, fontWeight: 400 }}>{log.task_name}</TableCell>
                  <TableCell sx={{ width: 120, fontWeight: 400 }}>{typeLabel(log.task_type)}</TableCell>
                  <TableCell sx={{ width: 100, fontWeight: 400, color: log.status === 'success' || log.status === 'succeeded' ? 'primary.main' : log.status === 'failed' ? 'error.main' : 'text.secondary' }}>
                    {renderStatus ? renderStatus(log) : statusLabel(log.status)}
                  </TableCell>
                  {renderSource && <TableCell sx={{ width: 150, fontWeight: 400 }}>{renderSource(log)}</TableCell>}
                  <TableCell sx={{ fontWeight: 400, wordBreak: 'break-word' }} title={log.detail}>{log.detail || '-'}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </TableContainer>

        <Box sx={{ height: 56, minHeight: 56, display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 1.5, overflow: 'hidden' }}>
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, minWidth: 0, flex: '1 1 auto', overflow: 'hidden' }}>
            <Typography variant="body2" color="text.secondary" noWrap sx={{ flexShrink: 0 }}>
              {total === 0 ? '共 0 条记录' : `${startRecord}-${endRecord} / 共 ${total} 条`}
            </Typography>
            {footerActions && <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, minWidth: 0 }}>{footerActions}</Box>}
            {loading && <CircularProgress size={16} sx={{ flexShrink: 0 }} />}
          </Box>
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5, flexShrink: 0 }}>
            <IconButton size="small" disabled={!canGoPrev} onClick={() => onPageChange(0)} aria-label="第一页"><FirstPage fontSize="small" /></IconButton>
            <IconButton size="small" disabled={!canGoPrev} onClick={() => onPageChange(page - 1)} aria-label="上一页"><KeyboardArrowLeft fontSize="small" /></IconButton>
            <TextField
              size="small"
              value={pageInput}
              onChange={(event: ChangeEvent<HTMLInputElement>) => {
                if (/^\d{0,4}$/.test(event.target.value)) setPageInput(event.target.value)
              }}
              onBlur={commitPageInput}
              onKeyDown={handlePageInputKeyDown}
              slotProps={{ htmlInput: { inputMode: 'numeric', 'aria-label': '页码' } }}
              sx={{ width: 48, '& .MuiInputBase-input': { py: 0.5, px: 0.75, textAlign: 'center', fontSize: '0.875rem' } }}
            />
            <Typography variant="body2" color="text.secondary">/ {pageCount}</Typography>
            <IconButton size="small" disabled={!canGoNext} onClick={() => onPageChange(page + 1)} aria-label="下一页"><KeyboardArrowRight fontSize="small" /></IconButton>
            <IconButton size="small" disabled={!canGoNext} onClick={() => onPageChange(pageCount - 1)} aria-label="最后一页"><LastPage fontSize="small" /></IconButton>
          </Box>
        </Box>
      </CardContent>
    </Card>
  )
}
