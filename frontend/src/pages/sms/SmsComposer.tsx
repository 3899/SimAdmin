import {
  type ChangeEvent,
  type KeyboardEvent,
  type ReactNode,
} from 'react'
import {
  Box,
  TextField,
  InputAdornment,
  IconButton,
  CircularProgress,
  Typography,
} from '@mui/material'
import { Send } from '@mui/icons-material'
import { calculateSmsSegments } from './smsUiUtils'

export interface SmsComposerProps {
  value: string
  onChange: (value: string) => void
  onSend: () => void
  placeholder?: string
  disabled?: boolean
  sending?: boolean
  deviceSlot?: ReactNode
  onFocus?: () => void
  onBlur?: () => void
}

export function SmsComposer({
  value,
  onChange,
  onSend,
  placeholder = '输入短信内容...',
  disabled = false,
  sending = false,
  deviceSlot,
  onFocus,
  onBlur,
}: SmsComposerProps) {
  const { chars, segments } = calculateSmsSegments(value)

  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      if (!disabled && !sending && value.trim()) {
        onSend()
      }
    }
  }

  return (
    <Box
      sx={{
        p: 1.5,
        borderTop: 1,
        borderColor: 'divider',
        bgcolor: 'background.paper',
        flex: 'none',
      }}
    >
      <Box sx={{ width: '100%' }}>
        <TextField
          fullWidth
          multiline
          maxRows={4}
          value={value}
          onChange={(e: ChangeEvent<HTMLInputElement>) => onChange(e.target.value)}
          placeholder={placeholder}
          disabled={disabled || sending}
          onFocus={onFocus}
          onBlur={onBlur}
          onKeyDown={handleKeyDown}
          slotProps={{
            input: {
              endAdornment: (
                <InputAdornment position="end">
                  <IconButton
                    color="primary"
                    onClick={onSend}
                    disabled={disabled || sending || !value.trim()}
                  >
                    {sending ? <CircularProgress size={20} /> : <Send />}
                  </IconButton>
                </InputAdornment>
              ),
            },
          }}
        />

        <Box
          display="flex"
          alignItems="center"
          justifyContent="space-between"
          mt={0.5}
          gap={1}
          flexWrap="wrap"
        >
          {deviceSlot ? <Box>{deviceSlot}</Box> : <Box />}
          <Typography
            variant="caption"
            color="text.secondary"
            sx={{ display: 'block', textAlign: 'right' }}
          >
            {chars > 0
              ? `${chars} 字符${segments > 1 ? ` (长短信 ${segments} 条)` : ''} | Enter 发送，Shift+Enter 换行`
              : 'Enter 发送，Shift+Enter 换行'}
          </Typography>
        </Box>
      </Box>
    </Box>
  )
}
