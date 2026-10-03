import {
  type MouseEvent,
  type ReactNode,
} from 'react'
import {
  Box,
  ListItemButton,
  ListItemText,
  Typography,
  Avatar,
  // Badge, // 预留未读红标组件，后续恢复未读角标时启用
  IconButton,
  Tooltip,
  Checkbox,
} from '@mui/material'
import { Person, DeleteOutline } from '@mui/icons-material'
import { formatShortTime, renderHighlightedText } from './smsUiUtils'

export interface SmsConversationItemProps {
  phoneNumber: string
  lastMessageContent: string
  timestamp: string
  messageCount?: number
  unreadCount?: number // 预留未读数量字段，后续若需展示可解开恢复
  isSelected: boolean
  onClick: () => void
  searchQuery?: string
  batchMode?: boolean
  checked?: boolean
  onToggleSelect?: () => void
  onDelete?: () => void
  deviceSlot?: ReactNode
  countSlot?: ReactNode
}

export function SmsConversationItem({
  phoneNumber,
  lastMessageContent,
  timestamp,
  messageCount = 0,
  // unreadCount = 0, // 预留未读数参数，后续启用 Badge 时解开
  isSelected,
  onClick,
  searchQuery = '',
  batchMode = false,
  checked = false,
  onToggleSelect,
  onDelete,
  deviceSlot,
  countSlot,
}: SmsConversationItemProps) {
  const handleDeleteClick = (e: MouseEvent<HTMLButtonElement>) => {
    e.stopPropagation()
    onDelete?.()
  }

  return (
    <ListItemButton
      selected={isSelected}
      onClick={batchMode ? onToggleSelect : onClick}
      sx={{
        py: 1.2,
        px: 1.5,
        gap: 1.2,
        '&:hover .conversation-delete, &:focus-within .conversation-delete': {
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
          inputProps={{ 'aria-label': `选择对话 ${phoneNumber}` }}
        />
      )}

      <Avatar sx={{ bgcolor: 'primary.main', width: 38, height: 38 }}>
        <Person fontSize="small" />
      </Avatar>

      <ListItemText
        sx={{ my: 0 }}
        primary={
          <Box display="flex" alignItems="center" justifyContent="space-between" gap={1}>
            <Box display="flex" alignItems="center" gap={0.6} minWidth={0} sx={{ flex: 1 }}>
              <Typography fontWeight={600} fontSize="0.9rem" noWrap>
                {renderHighlightedText(phoneNumber, searchQuery)}
              </Typography>
              {countSlot !== undefined ? (
                countSlot
              ) : messageCount > 0 ? (
                <Typography
                  component="span"
                  variant="caption"
                  color="text.secondary"
                  sx={{
                    fontSize: '0.75rem',
                    fontWeight: 500,
                    opacity: 0.8,
                    flexShrink: 0,
                  }}
                >
                  ({messageCount})
                </Typography>
              ) : null}
            </Box>
            {/* 预留未读红色角标代码，后续若需恢复未读数量呈现可解开注释：
            {unreadCount > 0 && (
              <Badge badgeContent={unreadCount} color="error" max={99} sx={{ flexShrink: 0 }} />
            )}
            */}
          </Box>
        }
        secondary={
          <>
            {deviceSlot}
            <Typography
              variant="caption"
              color="text.secondary"
              noWrap
              display="block"
              sx={{ maxWidth: '100%' }}
            >
              {renderHighlightedText(lastMessageContent, searchQuery)}
            </Typography>
          </>
        }
      />

      <Typography
        variant="caption"
        color="text.secondary"
        sx={{ minWidth: 44, textAlign: 'right', fontSize: '0.7rem' }}
      >
        {formatShortTime(timestamp)}
      </Typography>

      {!batchMode && onDelete && (
        <Tooltip title="删除对话">
          <IconButton
            className="conversation-delete"
            size="small"
            onClick={handleDeleteClick}
            sx={{
              opacity: 0,
              color: 'text.secondary',
              transition: 'opacity 0.15s ease',
              '&:hover': { color: 'error.main' },
            }}
          >
            <DeleteOutline fontSize="small" />
          </IconButton>
        </Tooltip>
      )}
    </ListItemButton>
  )
}
