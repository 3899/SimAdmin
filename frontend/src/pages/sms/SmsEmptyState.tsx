import { Box, Stack, Typography } from '@mui/material'
import { Sms as SmsIcon, SearchOff } from '@mui/icons-material'

export interface SmsEmptyStateProps {
  type: 'no_conversations' | 'no_messages' | 'search_empty' | 'unselected'
  title?: string
  subtitle?: string
}

export function SmsEmptyState({
  type,
  title,
  subtitle,
}: SmsEmptyStateProps) {
  if (type === 'unselected') {
    return (
      <Box
        sx={{
          height: '100%',
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          p: 4,
          textAlign: 'center',
        }}
      >
        <SmsIcon sx={{ fontSize: 64, color: 'text.disabled', mb: 1.5 }} />
        <Typography variant="h6" color="text.secondary">
          {title || '选择一个会话开始查看'}
        </Typography>
        <Typography variant="caption" color="text.secondary" sx={{ mt: 0.5 }}>
          {subtitle || '可在左侧搜索联系人或正文后打开完整会话'}
        </Typography>
      </Box>
    )
  }

  if (type === 'search_empty') {
    return (
      <Stack alignItems="center" spacing={0.8} sx={{ py: 6, px: 2, textAlign: 'center' }}>
        <SearchOff color="disabled" sx={{ fontSize: 36 }} />
        <Typography variant="body2" color="text.secondary">
          {title || '未找到匹配的对话或短信'}
        </Typography>
        <Typography variant="caption" color="text.secondary">
          {subtitle || '请尝试更换搜索关键字'}
        </Typography>
      </Stack>
    )
  }

  if (type === 'no_messages') {
    return (
      <Box display="flex" justifyContent="center" alignItems="center" height="100%" minHeight={200}>
        <Typography color="text.secondary">
          {title || '暂无消息记录'}
        </Typography>
      </Box>
    )
  }

  return (
    <Stack alignItems="center" spacing={0.8} sx={{ py: 6, px: 2, textAlign: 'center' }}>
      <SmsIcon color="disabled" sx={{ fontSize: 36 }} />
      <Typography variant="body2" color="text.secondary">
        {title || '暂无短信会话'}
      </Typography>
      <Typography variant="caption" color="text.secondary">
        {subtitle || '收到的短信将按号码与设备自动归类聚合展示'}
      </Typography>
    </Stack>
  )
}
