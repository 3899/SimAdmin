import { useState, useEffect, useRef } from 'react'
import {
  Alert,
  Box,
  Button,
  Checkbox,
  Chip,
  Divider,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  FormControlLabel,
  FormHelperText,
  MenuItem,
  TextField,
  Typography,
} from '@mui/material'
import BackupStorageSelector, { type BackupDestination } from '../../components/backup/BackupStorageSelector'
import type {
  AutomationTask,
  AutomationAction,
  AutomationTrigger,
  BackupComponentKey,
} from '../../api/contracts'

type AutomationTaskDialogProps = {
  open: boolean
  onClose: () => void
  editingTask: AutomationTask | null
  onSave: (task: AutomationTask) => Promise<void>
  defaultBackupLocalDir?: string
}

const BACKUP_COMPONENT_LABELS: Record<BackupComponentKey, string> = {
  config: '系统配置',
  sms: '短信记录',
  notification_config: '通知配置',
  notification_logs: '通知日志',
  notification_queue: '通知队列',
  automation_config: '自动化配置',
  automation_logs: '自动化日志',
  sim_cache: 'SIM 缓存',
  esim_cache: 'eSIM 缓存',
  auth: '登录凭据',
}

const DEFAULT_BACKUP_COMPONENTS: BackupComponentKey[] = [
  'config',
  'sms',
  'notification_config',
  'automation_config',
  'sim_cache',
  'esim_cache',
]

const CONFIG_AND_DATA_COMPONENTS: BackupComponentKey[] = [
  'config',
  'sms',
  'notification_config',
  'automation_config',
  'sim_cache',
  'esim_cache',
  'auth',
]

const LOG_COMPONENTS: BackupComponentKey[] = [
  'notification_queue',
  'notification_logs',
  'automation_logs',
]


export default function AutomationTaskDialog({
  open,
  onClose,
  editingTask,
  onSave,
  defaultBackupLocalDir = '/opt/simadmin/backups',
}: AutomationTaskDialogProps) {
  const [formName, setFormName] = useState('')
  const [formEnabled, setFormEnabled] = useState(true)
  const [formActionType, setFormActionType] = useState<'restart_baseband' | 'reboot_device' | 'send_sms' | 'backup_data'>('restart_baseband')
  const [formRebootDelay, setFormRebootDelay] = useState(5)
  const [formSmsPhone, setFormSmsPhone] = useState('')
  const [formSmsContent, setFormSmsContent] = useState('')
  const [formSmsDelay, setFormSmsDelay] = useState(120)
  const [formSmsRetries, setFormSmsRetries] = useState(3)
  const [formBackupComponents, setFormBackupComponents] = useState<BackupComponentKey[]>(DEFAULT_BACKUP_COMPONENTS)
  const [formBackupLocalDir, setFormBackupLocalDir] = useState(defaultBackupLocalDir)
  const [formBackupDestination, setFormBackupDestination] = useState<BackupDestination>('local')

  const [formTriggerType, setFormTriggerType] = useState<'fixed' | 'interval'>('fixed')
  const [formWeekdays, setFormWeekdays] = useState<number[]>([1, 2, 3, 4, 5, 6, 7])
  const [formTriggerTime, setFormTriggerTime] = useState('04:00')
  const [formIntervalVal, setFormIntervalVal] = useState(180)
  const [formIntervalUnit, setFormIntervalUnit] = useState('days')

  const [dialogError, setDialogError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)

  const smsContentRef = useRef<HTMLTextAreaElement | null>(null)

  useEffect(() => {
    if (open) {
      setDialogError(null)
      if (editingTask) {
        setFormName(editingTask.name)
        setFormEnabled(editingTask.enabled)
        setFormActionType(editingTask.action.type)
        setFormBackupComponents(DEFAULT_BACKUP_COMPONENTS)
        setFormBackupLocalDir(defaultBackupLocalDir)
        setFormBackupDestination('local')
        if (editingTask.action.type === 'reboot_device') {
          setFormRebootDelay(editingTask.action.config.delay_seconds)
        } else if (editingTask.action.type === 'backup_data') {
          setFormBackupComponents(editingTask.action.config.components?.length
            ? editingTask.action.config.components
            : DEFAULT_BACKUP_COMPONENTS)
          setFormBackupLocalDir(editingTask.action.config.storage?.local_dir || defaultBackupLocalDir)
          setFormBackupDestination('local')
        } else if (editingTask.action.type === 'send_sms') {
          setFormSmsPhone(editingTask.action.config.phone_number)
          setFormSmsContent(editingTask.action.config.content)
          setFormSmsDelay(editingTask.action.config.random_delay_seconds ?? 0)
          setFormSmsRetries(editingTask.action.config.retry_limit ?? 0)
        }
        setFormTriggerType(editingTask.trigger.type)
        if (editingTask.trigger.type === 'fixed') {
          setFormWeekdays(editingTask.trigger.config.weekdays || [1, 2, 3, 4, 5, 6, 7])
          setFormTriggerTime((editingTask.trigger.config.times || []).join(', '))
        } else {
          setFormIntervalVal(editingTask.trigger.config.interval_value)
          setFormIntervalUnit(editingTask.trigger.config.interval_unit)
        }
      } else {
        setFormName('')
        setFormEnabled(true)
        setFormActionType('restart_baseband')
        setFormRebootDelay(5)
        setFormSmsPhone('')
        setFormSmsContent('')
        setFormSmsDelay(120)
        setFormSmsRetries(3)
        setFormBackupComponents(DEFAULT_BACKUP_COMPONENTS)
        setFormBackupLocalDir(defaultBackupLocalDir)
        setFormBackupDestination('local')
        setFormTriggerType('fixed')
        setFormWeekdays([1, 2, 3, 4, 5, 6, 7])
        setFormTriggerTime('04:00')
        setFormIntervalVal(180)
        setFormIntervalUnit('days')
      }
    }
  }, [open, editingTask, defaultBackupLocalDir])

  const insertVariable = (token: string) => {
    const el = smsContentRef.current
    if (!el) {
      setFormSmsContent((prev) => prev + token)
      return
    }
    const start = el.selectionStart ?? formSmsContent.length
    const end = el.selectionEnd ?? formSmsContent.length
    const nextValue = formSmsContent.slice(0, start) + token + formSmsContent.slice(end)
    setFormSmsContent(nextValue)
    setTimeout(() => {
      el.focus()
      const newCursorPos = start + token.length
      el.setSelectionRange(newCursorPos, newCursorPos)
    }, 0)
  }

  const handleToggleWeekday = (day: number) => {
    setFormWeekdays((prev) =>
      prev.includes(day) ? prev.filter((d) => d !== day) : [...prev, day].sort()
    )
  }

  const handleSave = async () => {
    setDialogError(null)

    if (!formName.trim()) {
      setDialogError('请输入任务名称')
      return
    }

    let action: AutomationAction
    if (formActionType === 'restart_baseband') {
      action = { type: 'restart_baseband', config: null }
    } else if (formActionType === 'reboot_device') {
      action = { type: 'reboot_device', config: { delay_seconds: Number(formRebootDelay) || 5 } }
    } else if (formActionType === 'backup_data') {
      if (formBackupComponents.length === 0) {
        setDialogError('请至少选择一个备份组件')
        return
      }
      action = {
        type: 'backup_data',
        config: {
          components: formBackupComponents,
          storage: {
            local_dir: formBackupLocalDir.trim() || '/opt/simadmin/backups',
          },
        },
      }
    } else {
      const phoneClean = formSmsPhone.trim()
      if (!phoneClean) {
        setDialogError('请输入接收短信的手机号码')
        return
      }
      if (!/^[0-9+]+$/.test(phoneClean)) {
        setDialogError('接收号码格式不正确（只能包含数字和“+”号）')
        return
      }
      action = {
        type: 'send_sms',
        config: {
          phone_number: phoneClean,
          content: formSmsContent,
          random_delay_seconds: Number(formSmsDelay) || 0,
          retry_limit: Number(formSmsRetries) || 0,
        },
      }
    }

    let trigger: AutomationTrigger
    if (formTriggerType === 'fixed') {
      const rawTimes = formTriggerTime
        .replace(/：/g, ':')
        .replace(/，/g, ',')
        .split(',')
        .map((t) => t.trim())
        .filter((t) => t.length > 0)

      const times: string[] = []
      for (const t of rawTimes) {
        const match = t.match(/^(\d{1,2}):(\d{1,2})$/)
        if (match) {
          const hour = parseInt(match[1], 10)
          const minute = parseInt(match[2], 10)
          if (hour >= 0 && hour <= 23 && minute >= 0 && minute <= 59) {
            const paddedHour = hour.toString().padStart(2, '0')
            const paddedMinute = minute.toString().padStart(2, '0')
            times.push(`${paddedHour}:${paddedMinute}`)
            continue
          }
        }
        setDialogError(`请输入合法的触发时间: "${t}"（格式如 04:00，多个用逗号隔开）`)
        return
      }

      if (times.length === 0) {
        setDialogError('请输入合法的触发时间（格式如 04:00，多个用逗号隔开）')
        return
      }

      if (formWeekdays.length === 0) {
        setDialogError('请至少选择一个触发星期')
        return
      }

      trigger = {
        type: 'fixed',
        config: {
          weekdays: formWeekdays,
          times,
        },
      }
    } else {
      trigger = {
        type: 'interval',
        config: {
          interval_value: Number(formIntervalVal) || 1,
          interval_unit: formIntervalUnit as 'mins' | 'hours' | 'days',
        },
      }
    }

    const newTask: AutomationTask = {
      id: editingTask?.id || `task-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
      name: formName.trim(),
      enabled: formEnabled,
      trigger,
      action,
    }

    setSaving(true)
    try {
      await onSave(newTask)
      onClose()
    } catch (err) {
      setDialogError(err instanceof Error ? err.message : String(err))
    } finally {
      setSaving(false)
    }
  }

  const renderBackupComponentGroup = (title: string, components: BackupComponentKey[]) => (
    <Box>
      <Typography variant="body2" fontWeight={600} color="text.secondary" mb={1}>
        {title}
      </Typography>
      <Box sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', sm: 'repeat(3, minmax(0, 1fr))' }, columnGap: 1.5, rowGap: 0.5 }}>
        {components.map((key) => {
          const checked = formBackupComponents.includes(key)
          return (
            <FormControlLabel
              key={key}
              control={
                <Checkbox
                  checked={checked}
                  disabled={checked && formBackupComponents.length === 1}
                  onChange={() => {
                    setFormBackupComponents((prev) =>
                      prev.includes(key)
                        ? prev.filter((item) => item !== key)
                        : [...prev, key]
                    )
                  }}
                />
              }
              label={<Typography variant="body2">{BACKUP_COMPONENT_LABELS[key]}</Typography>}
              sx={{
                m: 0,
                minWidth: 0,
                '& .MuiFormControlLabel-label': {
                  minWidth: 0,
                },
              }}
            />
          )
        })}
      </Box>
    </Box>
  )

  return (
    <Dialog
      open={open}
      onClose={onClose}
      maxWidth="sm"
      fullWidth
      slotProps={{
        paper: {
          sx: { borderRadius: 2.5 },
        },
      }}
    >
      <DialogTitle sx={{ fontWeight: 700, pb: 1 }}>
        {editingTask ? '编辑自动化任务' : '添加自动化任务'}
      </DialogTitle>
      <DialogContent>
        <Box display="flex" flexDirection="column" gap={2.5} mt={1}>
          {dialogError && (
            <Alert severity="error" onClose={() => setDialogError(null)}>
              {dialogError}
            </Alert>
          )}
          <TextField
            label="任务名称"
            placeholder="例如：每日凌晨基带自动重启"
            fullWidth
            value={formName}
            onChange={(e) => {
              setFormName(e.target.value)
              setDialogError(null)
            }}
          />

          <TextField
            select
            label="执行动作"
            fullWidth
            value={formActionType}
            onChange={(e) => setFormActionType(e.target.value as 'restart_baseband' | 'reboot_device' | 'send_sms' | 'backup_data')}
          >
            <MenuItem value="restart_baseband">重启基带</MenuItem>
            <MenuItem value="reboot_device">重启设备</MenuItem>
            <MenuItem value="backup_data">备份数据</MenuItem>
            <MenuItem value="send_sms">发送短信</MenuItem>
          </TextField>

          {/* 重启设备特有字段 */}
          {formActionType === 'reboot_device' && (
            <TextField
              label="重启延迟时间 (秒)"
              type="number"
              fullWidth
              value={formRebootDelay}
              onChange={(e) => setFormRebootDelay(Math.max(2, parseInt(e.target.value, 10) || 2))}
              slotProps={{ htmlInput: { min: 2, max: 60 } }}
            />
          )}

          {/* 发送短信特有字段 */}
          {formActionType === 'send_sms' && (
            <Box display="flex" flexDirection="column" gap={2.5}>
              <TextField
                label="接收号码"
                placeholder="如：10010 或其他号码"
                fullWidth
                value={formSmsPhone}
                onChange={(e) => setFormSmsPhone(e.target.value)}
              />

              <Box>
                <Box display="flex" justifyContent="space-between" alignItems="center" mb={1}>
                  <Typography variant="body2" fontWeight={600} color="text.secondary">
                    短信内容
                  </Typography>
                  <Box display="flex" gap={0.5}>
                    <Chip
                      size="small"
                      label="+ 时间"
                      variant="outlined"
                      onClick={() => insertVariable('{{时间}}')}
                      sx={{ cursor: 'pointer' }}
                    />
                    <Chip
                      size="small"
                      label="+ 随机字符串"
                      variant="outlined"
                      onClick={() => insertVariable('{{随机字符串}}')}
                      sx={{ cursor: 'pointer' }}
                    />
                  </Box>
                </Box>
                <TextField
                  multiline
                  rows={3}
                  placeholder="发送内容，如：开源项目 SimAdmin {{时间}}"
                  fullWidth
                  value={formSmsContent}
                  onChange={(e) => setFormSmsContent(e.target.value)}
                  inputRef={smsContentRef}
                />
                <FormHelperText>
                  可在内容中插入变量，短信发送时会自动替换。
                </FormHelperText>
              </Box>

              <Box display="grid" gridTemplateColumns="1fr 1fr" gap={2}>
                <TextField
                  label="随机延迟范围 (秒)"
                  type="number"
                  value={formSmsDelay}
                  onChange={(e) => setFormSmsDelay(Math.max(0, parseInt(e.target.value, 10) || 0))}
                  helperText="从0到设定值随机延迟后发送"
                />
                <TextField
                  label="失败重试次数"
                  type="number"
                  value={formSmsRetries}
                  onChange={(e) => setFormSmsRetries(Math.max(0, parseInt(e.target.value, 10) || 0))}
                  helperText="发送失败后每5秒自动重试"
                />
              </Box>
            </Box>
          )}

          {formActionType === 'backup_data' && (
            <Box display="flex" flexDirection="column" gap={2.5}>
              <BackupStorageSelector
                destination={formBackupDestination}
                localDir={formBackupLocalDir}
                onDestinationChange={(destination) => {
                  if (destination === 'local') setFormBackupDestination(destination)
                }}
                disableDownload
                hideDownload
                localDirDisabled={true}
                localDirReadOnly={true}
              />

              <Divider />

              {renderBackupComponentGroup('配置与数据组件', CONFIG_AND_DATA_COMPONENTS)}
              {renderBackupComponentGroup('运行日志组件', LOG_COMPONENTS)}
            </Box>
          )}

          <TextField
            select
            label="触发机制"
            fullWidth
            value={formTriggerType}
            onChange={(e) => setFormTriggerType(e.target.value as 'fixed' | 'interval')}
          >
            <MenuItem value="fixed">定点定时</MenuItem>
            <MenuItem value="interval">时间间隔</MenuItem>
          </TextField>

          {/* 定点定时配置 */}
          {formTriggerType === 'fixed' && (
            <Box display="flex" flexDirection="column" gap={2.5}>
              <Box>
                <Typography variant="body2" fontWeight={600} color="text.secondary" mb={1}>
                  重复星期
                </Typography>
                <Box display="flex" gap={0.5} flexWrap="wrap">
                  {[
                    { val: 1, label: '一' },
                    { val: 2, label: '二' },
                    { val: 3, label: '三' },
                    { val: 4, label: '四' },
                    { val: 5, label: '五' },
                    { val: 6, label: '六' },
                    { val: 7, label: '日' },
                  ].map((day) => {
                    const active = formWeekdays.includes(day.val)
                    return (
                      <Button
                        key={day.val}
                        size="small"
                        variant={active ? 'contained' : 'outlined'}
                        sx={{ minWidth: 36, px: 0 }}
                        onClick={() => handleToggleWeekday(day.val)}
                      >
                        {day.label}
                      </Button>
                    )
                  })}
                </Box>
              </Box>

              <TextField
                label="触发时刻 (HH:MM，多个用逗号隔开)"
                placeholder="例如：04:00, 12:00"
                fullWidth
                value={formTriggerTime}
                onChange={(e) => setFormTriggerTime(e.target.value)}
                helperText="输入英文或中文逗号隔开的HH:MM时刻，例如 04:00, 16:30"
              />
            </Box>
          )}

          {/* 时间间隔配置 */}
          {formTriggerType === 'interval' && (
            <Box display="grid" gridTemplateColumns="1fr 1fr" gap={2}>
              <TextField
                label="间隔时长"
                type="number"
                value={formIntervalVal}
                onChange={(e) => setFormIntervalVal(Math.max(1, parseInt(e.target.value, 10) || 1))}
              />
              <TextField
                select
                label="时间单位"
                value={formIntervalUnit}
                onChange={(e) => setFormIntervalUnit(e.target.value)}
              >
                <MenuItem value="mins">分钟</MenuItem>
                <MenuItem value="hours">小时</MenuItem>
                <MenuItem value="days">天</MenuItem>
              </TextField>
            </Box>
          )}
        </Box>
      </DialogContent>
      <DialogActions sx={{ px: 3, pb: 2.5 }}>
        <Button variant="outlined" onClick={onClose} disabled={saving}>
          取消
        </Button>
        <Button variant="contained" onClick={() => void handleSave()} disabled={saving}>
          保存
        </Button>
      </DialogActions>
    </Dialog>
  )
}
