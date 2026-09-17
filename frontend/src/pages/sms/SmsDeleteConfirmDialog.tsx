import {
  Dialog,
  DialogTitle,
  DialogContent,
  DialogActions,
  Button,
  Typography,
  Alert,
  CircularProgress,
} from '@mui/material'
import { Delete } from '@mui/icons-material'
import type { DeleteTarget } from './smsTypes'

export interface SmsDeleteConfirmDialogProps {
  open: boolean
  target: DeleteTarget | any
  onClose: () => void
  onConfirm: () => void
  loading?: boolean
  error?: string | null
  batchCount?: number
}

export function SmsDeleteConfirmDialog({
  open,
  target,
  onClose,
  onConfirm,
  loading = false,
  error = null,
  batchCount = 0,
}: SmsDeleteConfirmDialogProps) {
  if (!target) return null

  let title = '删除确认'
  let description = '确定要执行删除操作吗？删除后将无法恢复。'

  if (target.type === 'batch') {
    title = '批量删除短信'
    description = `确定要删除当前已选中的 ${batchCount} 项吗？此操作不可逆。`
  } else if (target.type === 'conversation') {
    const phoneNumber = target.conversation?.phone_number ?? target.phone_number ?? target.phoneNumber ?? '该联系人'
    const count = target.conversation?.message_count ?? target.message_count ?? target.messageCount
    title = `删除与 ${phoneNumber} 的对话`
    description = `确定要删除与 ${phoneNumber} 的完整对话${
      count ? `（共 ${count} 条短信）` : ''
    }吗？删除后将无法恢复。`
  } else if (target.type === 'message') {
    const phone = target.message?.phone_number ?? '该联系人'
    title = '删除单条短信'
    description = `确定要删除来自 ${phone} 的这条短信吗？删除后无法恢复。`
  }

  return (
    <Dialog open={open} onClose={loading ? undefined : onClose} maxWidth="xs" fullWidth>
      <DialogTitle sx={{ fontWeight: 700 }}>{title}</DialogTitle>
      <DialogContent dividers>
        <Typography variant="body2" color="text.secondary" sx={{ lineHeight: 1.6 }}>
          {description}
        </Typography>
        {error && (
          <Alert severity="error" sx={{ mt: 2 }}>
            {error}
          </Alert>
        )}
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose} disabled={loading}>
          取消
        </Button>
        <Button
          color="error"
          variant="contained"
          onClick={onConfirm}
          disabled={loading}
          startIcon={loading ? <CircularProgress size={16} color="inherit" /> : <Delete />}
        >
          {loading ? '正在删除...' : '确定删除'}
        </Button>
      </DialogActions>
    </Dialog>
  )
}
