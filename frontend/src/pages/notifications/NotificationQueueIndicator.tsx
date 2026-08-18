import { useMemo } from 'react'
import {
  Box,
  Button,
  Chip,
  Divider,
  Drawer,
  IconButton,
  Stack,
  Tooltip,
  Typography,
} from '@mui/material'
import {
  Close,
  DeleteOutline,
  Layers,
  Replay,
  Send,
} from '@mui/icons-material'

export type NotificationQueueItemStatus = 'pending' | 'scheduled' | 'retrying' | 'sending' | 'failed'

export type NotificationQueueItem = {
  id: number | string
  status: NotificationQueueItemStatus
  event_type: string
  event_label: string
  summary: string
  reason: string
  channel_name: string
  device_name?: string
  next_attempt_at: string
  attempt_count: number
  max_attempts: number
}

type NotificationQueueIndicatorProps = {
  items: NotificationQueueItem[]
  open: boolean
  onOpen: () => void
  onClose: () => void
  onRetry: (id: NotificationQueueItem['id']) => void
  onDelete: (id: NotificationQueueItem['id']) => void
  onRetryAll: () => void
  onDeleteAll: () => void
}

const statusMeta: Record<NotificationQueueItemStatus, { label: string; color: string; dot: string }> = {
  pending: { label: '排队中', color: '#f97316', dot: '#facc15' },
  scheduled: { label: '等待中', color: '#f97316', dot: '#facc15' },
  retrying: { label: '重试中', color: '#f97316', dot: '#f97316' },
  sending: { label: '发送中', color: '#0284c7', dot: '#0ea5e9' },
  failed: { label: '已失败', color: '#dc2626', dot: '#ef4444' },
}

function QueuePulseDot() {
  return (
    <Box sx={{ position: 'relative', width: 10, height: 10, flex: '0 0 auto' }}>
      <Box
        sx={{
          position: 'absolute',
          inset: 0,
          borderRadius: '50%',
          bgcolor: '#ff7a1a',
          opacity: 0.36,
          animation: 'notificationQueuePulse 1.6s infinite',
          '@keyframes notificationQueuePulse': {
            '0%': { transform: 'scale(1)', opacity: 0.42 },
            '70%': { transform: 'scale(2.15)', opacity: 0 },
            '100%': { transform: 'scale(2.15)', opacity: 0 },
          },
        }}
      />
      <Box sx={{ position: 'absolute', inset: 2, borderRadius: '50%', bgcolor: '#ff7a1a' }} />
    </Box>
  )
}

function formatQueueTime(value: string) {
  if (!value) return '-'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })
}

function NotificationQueueTimelineItem({
  item,
  onRetry,
  onDelete,
}: {
  item: NotificationQueueItem
  onRetry: (id: NotificationQueueItem['id']) => void
  onDelete: (id: NotificationQueueItem['id']) => void
}) {
  const meta = statusMeta[item.status] ?? statusMeta.pending

  return (
    <Box
      sx={{
        position: 'relative',
        pl: 4,
        pb: 2.5,
        minWidth: 0,
      }}
    >
      <Box sx={{ position: 'absolute', left: 0, top: 4, zIndex: 1 }}>
        <Box
          sx={{
            width: 10,
            height: 10,
            borderRadius: '50%',
            bgcolor: meta.dot,
            border: '2px solid',
            borderColor: 'background.paper',
            boxShadow: '0 0 0 1px rgba(148, 163, 184, 0.42)',
          }}
        />
      </Box>

      <Box sx={{ minWidth: 0, overflow: 'hidden' }}>
        <Typography
          variant="caption"
          color="text.secondary"
          sx={{ display: 'block', mb: 0.75, fontFamily: 'monospace' }}
        >
          {formatQueueTime(item.next_attempt_at)}
        </Typography>

        <Box
          sx={{
            position: 'relative',
            bgcolor: '#fbfcfe',
            border: '1px solid',
            borderColor: 'rgba(148, 163, 184, 0.28)',
            borderRadius: 1.5,
            px: 2,
            py: 1.6,
            width: '100%',
            boxSizing: 'border-box',
            overflow: 'hidden',
            '&:hover .queue-row-actions, &:focus-within .queue-row-actions': {
              opacity: 1,
              pointerEvents: 'auto',
            },
          }}
        >
          <Stack direction="row" alignItems="center" spacing={1} sx={{ pr: 4, minWidth: 0 }}>
            <Typography variant="subtitle2" fontWeight={800} sx={{ flexShrink: 0 }}>
              {item.event_label}
            </Typography>
            <Chip
              label={`${meta.label} (${item.attempt_count}/${item.max_attempts})`}
              size="small"
              sx={{
                height: 23,
                borderRadius: 1,
                bgcolor: 'rgba(255, 237, 213, 0.86)',
                color: meta.color,
                fontSize: 12,
                fontWeight: 700,
              }}
            />
          </Stack>

          <Typography
            variant="body2"
            sx={{
              mt: 1.1,
              lineHeight: 1.65,
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-word',
            }}
          >
            {item.summary}
          </Typography>

          {item.reason && (
            <Typography
              variant="caption"
              sx={{
                display: 'block',
                mt: 0.85,
                color: '#ef4444',
                fontStyle: 'italic',
                wordBreak: 'break-word',
              }}
            >
              X {item.reason}
            </Typography>
          )}

          <Divider sx={{ mt: 1.3, mb: 0.9 }} />

          <Stack direction="row" alignItems="center" justifyContent="space-between" spacing={1}>
            <Typography variant="caption" color="text.secondary" sx={{ minWidth: 0, wordBreak: 'break-word' }}>
              {item.device_name ? `来源：${item.device_name} · ` : ''}通道：{item.channel_name || '-'}
            </Typography>
            <Typography variant="caption" color="text.secondary" sx={{ flexShrink: 0 }}>
              下次：{formatQueueTime(item.next_attempt_at)}
            </Typography>
          </Stack>

          <Stack
            className="queue-row-actions"
            direction="row"
            spacing={0.75}
            sx={{
              position: 'absolute',
              right: 8,
              top: 8,
              opacity: 0,
              pointerEvents: 'none',
              transition: 'opacity 140ms ease',
            }}
          >
            <Tooltip title="重试">
              <IconButton
                size="small"
                onClick={() => onRetry(item.id)}
                aria-label="重试排队通知"
                sx={{ bgcolor: 'transparent', '&:hover': { bgcolor: 'transparent', color: 'primary.main' } }}
              >
                <Replay fontSize="small" />
              </IconButton>
            </Tooltip>
            <Tooltip title="删除">
              <IconButton
                size="small"
                color="error"
                onClick={() => onDelete(item.id)}
                aria-label="删除排队通知"
                sx={{ bgcolor: 'transparent', '&:hover': { bgcolor: 'transparent' } }}
              >
                <DeleteOutline fontSize="small" />
              </IconButton>
            </Tooltip>
          </Stack>
        </Box>
      </Box>
    </Box>
  )
}

export default function NotificationQueueIndicator({
  items,
  open,
  onOpen,
  onClose,
  onRetry,
  onDelete,
  onRetryAll,
  onDeleteAll,
}: NotificationQueueIndicatorProps) {
  const activeItems = useMemo(() => items, [items])
  const count = activeItems.length

  if (count === 0) return null

  return (
    <>
      <Button
        variant="outlined"
        size="small"
        onClick={onOpen}
        startIcon={<QueuePulseDot />}
        sx={{
          ml: 'auto',
          height: 34,
          px: 1.4,
          borderRadius: 1,
          borderColor: 'rgba(249, 115, 22, 0.35)',
          bgcolor: 'rgba(255, 247, 237, 0.72)',
          color: '#ea580c',
          fontWeight: 700,
          '&:hover': {
            borderColor: 'rgba(249, 115, 22, 0.55)',
            bgcolor: 'rgba(255, 237, 213, 0.82)',
          },
        }}
      >
        通知队列（{count}）
      </Button>

      <Drawer
        anchor="right"
        open={open}
        onClose={onClose}
        PaperProps={{
          sx: {
            width: { xs: '100%', sm: 500 },
            maxWidth: '100%',
            bgcolor: '#ffffff',
            borderRadius: { xs: 0, sm: 2 },
            overflow: 'hidden',
            overflowX: 'hidden',
          },
        }}
      >
        <Box sx={{ display: 'flex', flexDirection: 'column', height: '100%', bgcolor: '#ffffff', overflowX: 'hidden' }}>
          <Box sx={{ px: 3, py: 2.5, bgcolor: '#ffffff' }}>
            <Stack direction="row" alignItems="center" spacing={1.5}>
              <Box
                sx={{
                  width: 34,
                  height: 34,
                  borderRadius: '50%',
                  display: 'grid',
                  placeItems: 'center',
                  bgcolor: 'rgba(255, 237, 213, 0.95)',
                  color: '#f97316',
                }}
              >
                <Layers fontSize="small" />
              </Box>
              <Box sx={{ minWidth: 0, flex: 1 }}>
                <Typography variant="h6" fontWeight={800} lineHeight={1.15}>
                  通知队列
                </Typography>
                <Typography variant="caption" color="text.secondary">
                  当超过接口限流或网络波动时，通知将在此排队等待发送
                </Typography>
              </Box>
              <IconButton size="small" onClick={onClose} aria-label="关闭通知队列">
                <Close />
              </IconButton>
            </Stack>
          </Box>
          <Divider />

          <Box
            sx={{
              position: 'relative',
              flex: 1,
              overflowY: 'auto',
              overflowX: 'hidden',
              bgcolor: '#ffffff',
              px: 3,
              py: 2.5,
              boxSizing: 'border-box',
            }}
          >
            <Box
              aria-hidden
              sx={{
                position: 'absolute',
                left: 29,
                top: 20,
                bottom: 20,
                width: '1px',
                bgcolor: 'rgba(148, 163, 184, 0.45)',
                pointerEvents: 'none',
              }}
            />
            {activeItems.map((item) => (
              <NotificationQueueTimelineItem
                key={item.id}
                item={item}
                onRetry={onRetry}
                onDelete={onDelete}
              />
            ))}
          </Box>

          <Box sx={{ p: 3, borderTop: '1px solid', borderColor: 'divider', bgcolor: '#ffffff' }}>
            <Stack direction="row" spacing={1.5}>
              <Button
                variant="outlined"
                color="inherit"
                fullWidth
                onClick={onDeleteAll}
                sx={{ height: 42, borderRadius: 1, borderColor: 'rgba(148, 163, 184, 0.45)' }}
              >
                全部丢弃
              </Button>
              <Button
                variant="contained"
                fullWidth
                startIcon={<Send />}
                onClick={onRetryAll}
                sx={{ height: 42, borderRadius: 1, boxShadow: 'none' }}
              >
                立即重试
              </Button>
            </Stack>
          </Box>
        </Box>
      </Drawer>
    </>
  )
}
