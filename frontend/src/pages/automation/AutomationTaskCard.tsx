import {
  Box,
  Card,
  CardContent,
  Chip,
  CircularProgress,
  IconButton,
  Switch,
  Typography,
} from '@mui/material'
import {
  PlayArrow,
  Edit,
  Delete,
  Autorenew,
  PowerSettingsNew,
  Sms as SmsIcon,
  Timer,
  CheckCircle,
  Error as ErrorIcon,
  SettingsBackupRestore,
} from '@mui/icons-material'
import type { AutomationTask, AutomationLogEntry } from '../../api/contracts'

type AutomationTaskCardProps = {
  task: AutomationTask
  latestLog: AutomationLogEntry | undefined
  testingTaskId: string | null
  onTest: (taskId: string) => void
  onEdit: (task: AutomationTask) => void
  onDelete: (task: AutomationTask) => void
  onToggle: (taskId: string, checked: boolean) => void
}

export default function AutomationTaskCard({
  task,
  latestLog,
  testingTaskId,
  onTest,
  onEdit,
  onDelete,
  onToggle,
}: AutomationTaskCardProps) {

  // Next run display helper calculation
  const getNextRunDisplay = () => {
    if (!task.enabled) return '已停用'

    if (task.trigger.type === 'fixed') {
      const weekdays = task.trigger.config.weekdays || []
      const times = task.trigger.config.times || []
      if (weekdays.length === 0 || times.length === 0) return '未配置触发时间'

      const now = new Date()
      let minDiff = Infinity
      let nextDate: Date | null = null

      for (let d = 0; d <= 8; d++) {
        const checkDate = new Date(now.getTime() + d * 24 * 60 * 60 * 1000)
        const jsDay = checkDate.getDay()
        const weekdayVal = jsDay === 0 ? 7 : jsDay
        if (!weekdays.includes(weekdayVal)) continue

        for (const timeStr of times) {
          const [hStr, mStr] = timeStr.split(':')
          const h = parseInt(hStr, 10)
          const m = parseInt(mStr, 10)
          if (isNaN(h) || isNaN(m)) continue

          const candidate = new Date(
            checkDate.getFullYear(),
            checkDate.getMonth(),
            checkDate.getDate(),
            h,
            m,
            0,
            0
          )
          if (candidate.getTime() > now.getTime()) {
            const diff = candidate.getTime() - now.getTime()
            if (diff < minDiff) {
              minDiff = diff
              nextDate = candidate
            }
          }
        }
      }
      if (!nextDate) return '计算失败'

      // Format as YYYY-MM-DD HH:MM
      const pad = (n: number) => n.toString().padStart(2, '0')
      return `${nextDate.getFullYear()}-${pad(nextDate.getMonth() + 1)}-${pad(nextDate.getDate())} ${pad(nextDate.getHours())}:${pad(nextDate.getMinutes())}`
    } else {
      // Interval
      const val = task.trigger.config.interval_value
      const unit = task.trigger.config.interval_unit

      if (!latestLog) return '无运行历史，即将触发'

      // Parse last run time
      // Expected format: YYYY-MM-DD HH:MM:SS
      const parts = latestLog.created_at.match(/^(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2}):(\d{2})$/)
      if (!parts) return '无运行历史，即将触发'

      const lastRunDate = new Date(
        parseInt(parts[1], 10),
        parseInt(parts[2], 10) - 1,
        parseInt(parts[3], 10),
        parseInt(parts[4], 10),
        parseInt(parts[5], 10),
        parseInt(parts[6], 10)
      )

      let multiplier = 60 * 1000
      if (unit === 'hours') multiplier = 60 * 60 * 1000
      else if (unit === 'days') multiplier = 24 * 60 * 60 * 1000
      const nextTime = lastRunDate.getTime() + val * multiplier
      const nextDate = new Date(nextTime)

      const pad = (n: number) => n.toString().padStart(2, '0')
      return `${nextDate.getFullYear()}-${pad(nextDate.getMonth() + 1)}-${pad(nextDate.getDate())} ${pad(nextDate.getHours())}:${pad(nextDate.getMinutes())}`
    }
  }

  const nextRun = getNextRunDisplay()

  return (
    <Card sx={{ height: '100%', display: 'flex', flexDirection: 'column' }}>
      <CardContent sx={{ p: 2.5, flexGrow: 1, display: 'flex', flexDirection: 'column', '&:last-child': { pb: 2.5 } }}>
        {/* 卡片头部 */}
        <Box display="flex" justifyContent="space-between" alignItems="flex-start" mb={2}>
          <Box sx={{ minWidth: 0, flexGrow: 1 }}>
            <Typography variant="subtitle1" fontWeight={700} noWrap>
              {task.name}
            </Typography>
            <Box display="flex" alignItems="center" gap={1} mt={0.5}>
              {task.action.type === 'restart_baseband' && (
                <Chip
                  size="small"
                  icon={<Autorenew fontSize="small" />}
                  label="重启基带"
                  color="primary"
                  variant="outlined"
                  sx={{ height: 20, fontSize: '0.72rem', '& .MuiChip-label': { px: 0.75 }, '& .MuiChip-icon': { fontSize: '0.85rem' } }}
                />
              )}
              {task.action.type === 'reboot_device' && (
                <Chip
                  size="small"
                  icon={<PowerSettingsNew fontSize="small" />}
                  label="重启设备"
                  color="secondary"
                  variant="outlined"
                  sx={{ height: 20, fontSize: '0.72rem', '& .MuiChip-label': { px: 0.75 }, '& .MuiChip-icon': { fontSize: '0.85rem' } }}
                />
              )}
              {task.action.type === 'send_sms' && (
                <Chip
                  size="small"
                  icon={<SmsIcon fontSize="small" />}
                  label="发送短信"
                  color="warning"
                  variant="outlined"
                  sx={{ height: 20, fontSize: '0.72rem', '& .MuiChip-label': { px: 0.75 }, '& .MuiChip-icon': { fontSize: '0.85rem' } }}
                />
              )}
              {task.action.type === 'backup_data' && (
                <Chip
                  size="small"
                  icon={<SettingsBackupRestore fontSize="small" />}
                  label="备份数据"
                  color="success"
                  variant="outlined"
                  sx={{ height: 20, fontSize: '0.72rem', '& .MuiChip-label': { px: 0.75 }, '& .MuiChip-icon': { fontSize: '0.85rem' } }}
                />
              )}
            </Box>
          </Box>
          <Switch
            size="small"
            checked={task.enabled}
            onChange={(e) => onToggle(task.id, e.target.checked)}
          />
        </Box>

        {/* 卡片参数/触发详情 */}
        <Box
          sx={{
            mb: 2,
            fontSize: '0.85rem',
          }}
        >
          <Box display="flex" justifyContent="space-between" mb={0.75}>
            <Typography variant="body2" color="text.secondary">触发机制:</Typography>
            <Typography variant="body2">
              {task.trigger.type === 'fixed'
                ? `每周[${task.trigger.config.weekdays.join('')}] ${task.trigger.config.times.join(',')}`
                : `每隔 ${task.trigger.config.interval_value} ${task.trigger.config.interval_unit === 'mins' ? '分钟' : task.trigger.config.interval_unit === 'hours' ? '小时' : '天'}`}
            </Typography>
          </Box>

          {task.action.type === 'send_sms' && (
            <>
              <Box display="flex" justifyContent="space-between" mb={0.75}>
                <Typography variant="body2" color="text.secondary">接收号码:</Typography>
                <Typography variant="body2">
                  {task.action.config.phone_number}
                </Typography>
              </Box>
              <Box display="flex" justifyContent="space-between" mb={0.75}>
                <Typography variant="body2" color="text.secondary">短信内容:</Typography>
                <Typography variant="body2" sx={{
                  maxWidth: '180px',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                }} title={task.action.config.content}>
                  {task.action.config.content}
                </Typography>
              </Box>
            </>
          )}
          {task.action.type === 'reboot_device' && (
            <Box display="flex" justifyContent="space-between" mb={0.75}>
              <Typography variant="body2" color="text.secondary">重启延迟:</Typography>
              <Typography variant="body2">
                {task.action.config.delay_seconds} 秒
              </Typography>
            </Box>
          )}
          {task.action.type === 'backup_data' && (
            <>
              <Box display="flex" justifyContent="space-between" mb={0.75}>
                <Typography variant="body2" color="text.secondary">备份组件:</Typography>
                <Typography variant="body2">
                  {task.action.config.components.length} 项
                </Typography>
              </Box>
              <Box display="flex" justifyContent="space-between" mb={0.75} gap={1}>
                <Typography variant="body2" color="text.secondary" sx={{ flexShrink: 0 }}>存储目录:</Typography>
                <Typography
                  variant="body2"
                  title={task.action.config.storage.local_dir}
                  sx={{
                    minWidth: 0,
                    maxWidth: 190,
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                  }}
                >
                  {task.action.config.storage.local_dir}
                </Typography>
              </Box>
            </>
          )}

          <Box display="flex" justifyContent="space-between" mt={0.75}>
            <Typography variant="body2" color="text.secondary" display="flex" alignItems="center" gap={0.5}>
              <Timer fontSize="inherit" />
              下次运行:
            </Typography>
            <Typography variant="body2" color={task.enabled ? 'primary.main' : 'text.disabled'}>
              {nextRun}
            </Typography>
          </Box>
        </Box>

        {/* 卡片底部执行结果及操作 */}
        <Box
          display="flex"
          alignItems="center"
          justifyContent="space-between"
          mt="auto"
          pt={1.5}
          sx={{ borderTop: '1px solid', borderColor: 'divider' }}
        >
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5, minWidth: 0, flexGrow: 1 }}>
            {latestLog ? (
              <>
                {latestLog.status === 'success' ? (
                  <CheckCircle color="success" sx={{ fontSize: 16, flexShrink: 0 }} />
                ) : (
                  <ErrorIcon color="error" sx={{ fontSize: 16, flexShrink: 0 }} />
                )}
                <Typography variant="caption" color="text.secondary" noWrap sx={{ maxWidth: 140 }} title={latestLog.detail}>
                  {latestLog.status === 'success' ? '上次成功' : `失败: ${latestLog.detail}`}
                </Typography>
              </>
            ) : (
              <Typography variant="caption" color="text.disabled">
                未曾运行
              </Typography>
            )}
          </Box>

          <Box display="flex" gap={0.5} sx={{ flexShrink: 0 }}>
            <IconButton
              size="small"
              color="primary"
              title="立即执行"
              disabled={testingTaskId === task.id}
              onClick={() => onTest(task.id)}
            >
              {testingTaskId === task.id ? <CircularProgress size={18} /> : <PlayArrow />}
            </IconButton>
            <IconButton
              size="small"
              onClick={() => onEdit(task)}
              title="编辑"
            >
              <Edit fontSize="small" />
            </IconButton>
            <IconButton
              size="small"
              color="error"
              onClick={() => onDelete(task)}
              title="删除"
            >
              <Delete fontSize="small" />
            </IconButton>
          </Box>
        </Box>
      </CardContent>
    </Card>
  )
}
