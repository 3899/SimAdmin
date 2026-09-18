import { useState, type ReactNode } from 'react'
import {
  Dialog,
  DialogTitle,
  DialogContent,
  DialogActions,
  Stack,
  TextField,
  Typography,
  Button,
  Alert,
  CircularProgress,
} from '@mui/material'
import { Send } from '@mui/icons-material'
import { calculateSmsSegments } from './smsUiUtils'

export interface SmsNewChatDialogProps {
  open: boolean
  onClose: () => void
  onSend: (data: { phoneNumber: string; content: string }) => void
  loading?: boolean
  error?: string | null
  deviceSelectSlot?: ReactNode
  isDeviceSelected?: boolean
}

export function SmsNewChatDialog({
  open,
  onClose,
  onSend,
  loading = false,
  error = null,
  deviceSelectSlot,
  isDeviceSelected = true,
}: SmsNewChatDialogProps) {
  const [phoneNumber, setPhoneNumber] = useState('')
  const [content, setContent] = useState('')

  const handleReset = () => {
    setPhoneNumber('')
    setContent('')
  }

  const { chars, segments } = calculateSmsSegments(content)
  const canSubmit = isDeviceSelected && phoneNumber.trim().length > 0 && content.trim().length > 0 && !loading

  const handleSend = () => {
    if (!canSubmit) return
    onSend({ phoneNumber: phoneNumber.trim(), content })
  }

  const handleClose = () => {
    handleReset()
    onClose()
  }

  return (
    <Dialog
      open={open}
      onClose={loading ? undefined : handleClose}
      TransitionProps={{ onEnter: handleReset }}
      fullWidth
      maxWidth="sm"
    >
      <DialogTitle sx={{ fontWeight: 700 }}>发送新短信</DialogTitle>
      <DialogContent dividers>
        <Stack spacing={1.8}>
          {deviceSelectSlot}

          <TextField
            size="small"
            label="收件号码"
            value={phoneNumber}
            onChange={(e) => setPhoneNumber(e.target.value)}
            placeholder="输入手机号码"
            disabled={loading}
            fullWidth
          />

          <TextField
            multiline
            minRows={4}
            maxRows={8}
            label="短信正文"
            value={content}
            onChange={(e) => setContent(e.target.value)}
            placeholder="输入短信内容..."
            disabled={loading}
            fullWidth
          />

          <Typography variant="caption" color="text.secondary" textAlign="right">
            {chars > 0
              ? `${chars} 字符${segments > 1 ? ` (共 ${segments} 条)` : ''}`
              : '0 字符'}
          </Typography>

          {error && <Alert severity="error">{error}</Alert>}
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose} disabled={loading}>
          取消
        </Button>
        <Button
          variant="contained"
          onClick={handleSend}
          disabled={!canSubmit}
          startIcon={loading ? <CircularProgress size={16} color="inherit" /> : <Send />}
        >
          {loading ? '正在发送...' : '立即发送'}
        </Button>
      </DialogActions>
    </Dialog>
  )
}
