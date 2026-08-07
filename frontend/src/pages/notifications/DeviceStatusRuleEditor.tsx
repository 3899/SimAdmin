import {
  Box,
  Button,
  Checkbox,
  FormControlLabel,
  MenuItem,
  Paper,
  TextField,
  Typography,
} from '@mui/material'
import type { ChangeEvent } from 'react'
import type { DeviceStatusSchedule } from '../../api/current'
import { WEEKDAYS } from './notificationModel'
import {
  DEVICE_STATUS_GROUPS,
  allDeviceStatusItems,
  defaultDeviceStatusItems,
} from './deviceStatusModel'

type DeviceStatusRuleEditorProps = {
  items: string[]
  schedule: DeviceStatusSchedule
  smsPeriod: string
  onItemsChange: (items: string[]) => void
  onScheduleChange: (schedule: DeviceStatusSchedule) => void
  onSmsPeriodChange: (period: string) => void
}

const PERIODS = [
  { value: 'today', label: '今日' },
  { value: 'last_24h', label: '最近 24 小时' },
  { value: 'last_7d', label: '最近 7 天' },
  { value: 'all', label: '累计' },
]

export default function DeviceStatusRuleEditor({
  items,
  schedule,
  smsPeriod,
  onItemsChange,
  onScheduleChange,
  onSmsPeriodChange,
}: DeviceStatusRuleEditorProps) {
  const selected = new Set(items)

  const toggleItem = (key: string, checked: boolean) => {
    const next = new Set(selected)
    if (checked) {
      next.add(key)
    } else {
      next.delete(key)
    }
    onItemsChange([...next])
  }

  const toggleGroup = (keys: string[], checked: boolean) => {
    const next = new Set(selected)
    keys.forEach((key) => {
      if (checked) {
        next.add(key)
      } else {
        next.delete(key)
      }
    })
    onItemsChange([...next])
  }

  const patchSchedule = (patch: Partial<DeviceStatusSchedule>) => {
    onScheduleChange({ ...schedule, ...patch })
  }

  return (
    <Box mt={2}>
      <Box display="grid" gridTemplateColumns={{ xs: '1fr', md: '220px 1fr' }} gap={1.5} mb={2}>
        <TextField
          select
          label="推送方式"
          value={schedule.mode}
          onChange={(event: ChangeEvent<HTMLInputElement>) => patchSchedule({ mode: event.target.value as DeviceStatusSchedule['mode'] })}
          SelectProps={{ MenuProps: { PaperProps: { sx: { '& .MuiMenuItem-root': { fontSize: '14px' } } } } }}
        >
          <MenuItem value="fixed" sx={{ fontSize: '14px' }}>定时</MenuItem>
          <MenuItem value="interval" sx={{ fontSize: '14px' }}>间隔</MenuItem>
        </TextField>
        {schedule.mode === 'interval' ? (
          <TextField
            type="number"
            label="间隔分钟"
            value={schedule.interval_minutes}
            onChange={(event: ChangeEvent<HTMLInputElement>) => {
              const value = Number(event.target.value)
              patchSchedule({ interval_minutes: Number.isFinite(value) ? Math.max(30, Math.trunc(value)) : 1440 })
            }}
            slotProps={{ htmlInput: { min: 30, step: 30 } }}
          />
        ) : (
          <Box display="flex" gap={1} flexWrap="wrap" alignItems="center">
            <TextField
              type="time"
              label="推送时间"
              value={schedule.times[0] ?? '09:00'}
              onChange={(event: ChangeEvent<HTMLInputElement>) => patchSchedule({ times: [event.target.value] })}
              sx={{ minWidth: 170 }}
            />
            <Box display="flex" gap={0.5} flexWrap="wrap">
              {WEEKDAYS.map((day) => {
                const active = schedule.weekdays.includes(day.value)
                return (
                  <Button
                    key={day.value}
                    size="small"
                    variant={active ? 'contained' : 'outlined'}
                    sx={{ minWidth: 34, px: 0.5 }}
                    onClick={() => {
                      const weekdays = active
                        ? schedule.weekdays.filter((value) => value !== day.value)
                        : [...schedule.weekdays, day.value].sort((a, b) => a - b)
                      patchSchedule({ weekdays })
                    }}
                  >
                    {day.label}
                  </Button>
                )
              })}
            </Box>
          </Box>
        )}
      </Box>

      <Box display="flex" alignItems="center" gap={1} mb={1} flexWrap="wrap">
        <Typography variant="subtitle2">状态内容</Typography>
        <Button size="small" onClick={() => onItemsChange(defaultDeviceStatusItems())}>恢复默认</Button>
        <Button size="small" onClick={() => onItemsChange(allDeviceStatusItems())}>全选</Button>
        <Button size="small" onClick={() => onItemsChange([])}>清空</Button>
      </Box>

      <Box display="grid" gridTemplateColumns={{ xs: '1fr', lg: 'repeat(2, minmax(0, 1fr))' }} gap={1.25}>
        {DEVICE_STATUS_GROUPS.map((group) => {
          const keys = group.items.map((item) => item.key)
          const checkedCount = keys.filter((key) => selected.has(key)).length
          return (
            <Paper key={group.key} variant="outlined" sx={{ p: 1.5, borderRadius: 1 }}>
              <Box display="flex" alignItems="center" justifyContent="space-between" mb={0.75}>
                <Typography variant="body2" fontWeight={700}>{group.label}</Typography>
                <FormControlLabel
                  sx={{ m: 0 }}
                  control={
                    <Checkbox
                      size="small"
                      checked={checkedCount === keys.length}
                      indeterminate={checkedCount > 0 && checkedCount < keys.length}
                      onChange={(event) => toggleGroup(keys, event.target.checked)}
                    />
                  }
                  label={<Typography variant="caption">{checkedCount}/{keys.length}</Typography>}
                />
              </Box>
              <Box display="grid" gridTemplateColumns={{ xs: '1fr', sm: 'repeat(2, minmax(0, 1fr))' }} columnGap={1.25}>
                {group.items.map((item) => (
                  <FormControlLabel
                    key={item.key}
                    sx={{ m: 0, minHeight: item.key === 'sms_forwarding_stats' && selected.has(item.key) ? 44 : 30, alignItems: 'flex-start' }}
                    control={
                      <Checkbox
                        size="small"
                        sx={{ pt: 0.5 }}
                        checked={selected.has(item.key)}
                        onChange={(event) => toggleItem(item.key, event.target.checked)}
                      />
                    }
                    label={(
                      <Box display="flex" alignItems="center" gap={1} flexWrap="wrap">
                        <Typography variant="body2">{item.label}</Typography>
                        {item.key === 'sms_forwarding_stats' && selected.has(item.key) && (
                          <TextField
                            select
                            size="small"
                            label="周期"
                            value={smsPeriod}
                            onClick={(event) => event.stopPropagation()}
                            onChange={(event: ChangeEvent<HTMLInputElement>) => onSmsPeriodChange(event.target.value)}
                            sx={{ minWidth: 132 }}
                            SelectProps={{ MenuProps: { PaperProps: { sx: { '& .MuiMenuItem-root': { fontSize: '14px' } } } } }}
                          >
                            {PERIODS.map((period) => <MenuItem key={period.value} value={period.value} sx={{ fontSize: '14px' }}>{period.label}</MenuItem>)}
                          </TextField>
                        )}
                      </Box>
                    )}
                  />
                ))}
              </Box>
            </Paper>
          )
        })}
      </Box>
    </Box>
  )
}
