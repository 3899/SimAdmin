import { useState, useCallback, type MouseEvent } from 'react'
import {
  Box,
  Paper,
  Typography,
  Chip,
  IconButton,
  Tooltip,
  Checkbox,
} from '@mui/material'
import type { Theme } from '@mui/material/styles'
import { DeleteOutline } from '@mui/icons-material'
import type { BaseSmsMessage } from './smsTypes'
import {
  formatTime,
  renderHighlightedText,
  extractVerificationCode,
} from './smsUiUtils'

export interface SmsMessageBubbleProps {
  message: BaseSmsMessage
  searchQuery?: string
  batchMode?: boolean
  checked?: boolean
  onToggleSelect?: () => void
  onDelete?: (message: BaseSmsMessage) => void
  onCopySuccess?: (code: string) => void
}

export function SmsMessageBubble({
  message,
  searchQuery = '',
  batchMode = false,
  checked = false,
  onToggleSelect,
  onDelete,
  onCopySuccess,
}: SmsMessageBubbleProps) {
  const [copied, setCopied] = useState(false)
  const isOutgoing = message.direction === 'outgoing'

  // 验证码检测（仅对收件短信检测）
  const verificationCode = !isOutgoing ? extractVerificationCode(message.content) : null

  // 传输链路检测（VoLTE / VoWiFi）
  const isVolte = message.transport === 'volte_ims' || message.transport === 'volte'
  const isVowifi = message.transport === 'vowifi_ims' || message.transport === 'vowifi'

  // 方案 C: 点击复制验证码
  const handleCopyCode = useCallback(
    (e: MouseEvent<HTMLDivElement>) => {
      e.stopPropagation()
      if (!verificationCode) return

      if (navigator.clipboard) {
        navigator.clipboard.writeText(verificationCode).then(() => {
          setCopied(true)
          onCopySuccess?.(verificationCode)
          setTimeout(() => setCopied(false), 1800)
        }).catch(() => {
          // 剪贴板降级处理
          setCopied(true)
          onCopySuccess?.(verificationCode)
          setTimeout(() => setCopied(false), 1800)
        })
      } else {
        setCopied(true)
        onCopySuccess?.(verificationCode)
        setTimeout(() => setCopied(false), 1800)
      }
    },
    [verificationCode, onCopySuccess],
  )

  const handleDeleteClick = useCallback(
    (e: MouseEvent<HTMLButtonElement>) => {
      e.stopPropagation()
      onDelete?.(message)
    },
    [message, onDelete],
  )

  return (
    <Box
      id={`sms-message-${message.id}`}
      data-sms-direction={message.direction}
      display="flex"
      justifyContent={isOutgoing ? 'flex-end' : 'flex-start'}
      alignItems="center"
      gap={0.75}
      mb={1.5}
      onClick={batchMode ? onToggleSelect : undefined}
      sx={{
        width: '100%',
        cursor: batchMode ? 'pointer' : 'default',
        '&:hover .message-delete, &:focus-within .message-delete': {
          opacity: 1,
        },
      }}
    >
      {batchMode && (
        <Checkbox
          size="small"
          checked={checked}
          onClick={(e) => e.stopPropagation()}
          onChange={onToggleSelect}
          inputProps={{ 'aria-label': '选择短信' }}
          sx={{ order: isOutgoing ? 1 : 0 }}
        />
      )}

      <Paper
        elevation={0}
        sx={{
          maxWidth: '75%',
          bgcolor: isOutgoing
            ? 'primary.main'
            : (theme: Theme) => theme.palette.mode === 'dark' ? 'grey.800' : 'background.paper',
          color: isOutgoing ? 'white' : 'text.primary',
          border: 1,
          borderColor: isOutgoing ? 'primary.main' : 'divider',
          borderRadius: 2,
          borderTopRightRadius: isOutgoing ? 0 : 16,
          borderTopLeftRadius: isOutgoing ? 16 : 0,
          overflow: 'hidden',
        }}
      >
        {/* 正文区域 */}
        <Box sx={{ p: 1.5, pb: 0.8 }}>
          <Typography
            variant="body2"
            sx={{
              wordBreak: 'break-word',
              whiteSpace: 'pre-wrap',
              lineHeight: 1.55,
            }}
          >
            {renderHighlightedText(message.content, searchQuery)}
          </Typography>

          {/* 底部时间与徽章状态 */}
          <Box display="flex" alignItems="center" justifyContent="flex-end" gap={0.75} mt={0.5}>
            <Typography variant="caption" sx={{ opacity: 0.75, fontSize: '0.7rem' }}>
              {formatTime(message.timestamp)}
            </Typography>

            {isVolte && (
              <Chip
                label="VoLTE"
                color="success"
                size="small"
                sx={{
                  height: 18,
                  fontSize: '0.68rem',
                  fontWeight: 600,
                  borderRadius: '4px',
                  '& .MuiChip-label': { px: 0.75 },
                }}
              />
            )}

            {isVowifi && (
              <Chip
                label="VoWiFi"
                color="secondary"
                size="small"
                sx={{
                  height: 18,
                  fontSize: '0.68rem',
                  fontWeight: 600,
                  borderRadius: '4px',
                  '& .MuiChip-label': { px: 0.75 },
                }}
              />
            )}

            {isOutgoing && (
              message.status === 'sent' ? (
                <Chip
                  label="已发送"
                  color="success"
                  size="small"
                  sx={{
                    height: 18,
                    fontSize: '0.68rem',
                    fontWeight: 600,
                    borderRadius: '4px',
                    '& .MuiChip-label': { px: 0.75 },
                  }}
                />
              ) : message.status === 'failed' ? (
                <Chip
                  label="失败"
                  color="error"
                  size="small"
                  sx={{
                    height: 18,
                    fontSize: '0.68rem',
                    fontWeight: 600,
                    borderRadius: '4px',
                    '& .MuiChip-label': { px: 0.75 },
                  }}
                />
              ) : message.status === 'pending' ? (
                <Chip
                  label="等待设备"
                  color="warning"
                  size="small"
                  sx={{
                    height: 18,
                    fontSize: '0.68rem',
                    fontWeight: 600,
                    borderRadius: '4px',
                    '& .MuiChip-label': { px: 0.75 },
                  }}
                />
              ) : null
            )}
          </Box>
        </Box>

        {/* 方案 C: 底部独立全宽验证码快捷操作栏 */}
        {verificationCode && (
          <Box
            onClick={handleCopyCode}
            sx={{
              borderTop: 1,
              borderColor: 'divider',
              bgcolor: (theme: Theme) => theme.palette.mode === 'dark' ? 'rgba(255,255,255,0.03)' : 'rgba(0,0,0,0.02)',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              py: 0.85,
              px: 1.5,
              gap: 0.75,
              cursor: 'pointer',
              userSelect: 'none',
              color: copied ? 'success.main' : 'primary.main',
              transition: 'all 0.15s ease',
              '&:hover': {
                bgcolor: (theme: Theme) => theme.palette.mode === 'dark' ? 'rgba(59, 130, 246, 0.15)' : '#eff6ff',
              },
            }}
          >
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2.2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
              <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
            </svg>
            <Typography variant="body2" fontWeight={600} sx={{ fontSize: '0.82rem' }}>
              {copied ? '已复制验证码' : '复制验证码'}
            </Typography>
            <Typography
              variant="body2"
              fontWeight={800}
              sx={{
                fontFamily: 'SF Mono, Menlo, Monaco, Consolas, monospace',
                fontSize: '0.92rem',
                letterSpacing: 1.2,
              }}
            >
              {verificationCode}
            </Typography>
          </Box>
        )}
      </Paper>

      {/* 悬浮删除小垃圾桶 */}
      {!batchMode && (
        <Tooltip title="删除短信">
          <IconButton
            className="message-delete"
            size="small"
            onClick={handleDeleteClick}
            sx={{
              opacity: 0,
              color: 'text.secondary',
              transition: 'opacity 0.15s ease',
              '&:hover': { color: 'error.main' },
              order: isOutgoing ? 0 : 1,
            }}
          >
            <DeleteOutline fontSize="small" />
          </IconButton>
        </Tooltip>
      )}
    </Box>
  )
}
