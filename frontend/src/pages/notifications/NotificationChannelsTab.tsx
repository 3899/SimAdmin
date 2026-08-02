import { useState, type ChangeEvent, type MouseEvent, type ReactNode } from 'react'
import {
  Avatar,
  Box,
  Button,
  Card,
  CardContent,
  CircularProgress,
  Divider,
  FormControlLabel,
  IconButton,
  InputAdornment,
  List,
  ListItemButton,
  ListItemText,
  Menu,
  MenuItem,
  Paper,
  Switch,
  TextField,
  Tooltip,
  Typography,
  useMediaQuery,
} from '@mui/material'
import { alpha, type Theme } from '@mui/material/styles'
import { Add, DeleteOutline, HelpOutline, PlayArrow, Save } from '@mui/icons-material'
import type { NotificationChannelInstance, NotificationChannelKey, NotificationConfig } from '../../api/current'
import {
  CHANNEL_DEFS,
  channelDef,
  getBool,
  getString,
  headersToText,
  textToHeaders,
} from './notificationModel'

const EMAIL_PROVIDER_PRESETS = [
  { value: 'custom', label: '自定义', smtp_host: '', smtp_port: 465, smtp_security: 'implicit_tls' },
  { value: 'qq', label: 'QQ / Foxmail', smtp_host: 'smtp.qq.com', smtp_port: 465, smtp_security: 'implicit_tls' },
  { value: 'exmail_qq', label: '腾讯企业邮箱', smtp_host: 'smtp.exmail.qq.com', smtp_port: 465, smtp_security: 'implicit_tls' },
  { value: 'gmail', label: 'Gmail', smtp_host: 'smtp.gmail.com', smtp_port: 465, smtp_security: 'implicit_tls' },
  { value: 'netease_163', label: '163 邮箱', smtp_host: 'smtp.163.com', smtp_port: 465, smtp_security: 'implicit_tls' },
  { value: 'netease_126', label: '126 邮箱', smtp_host: 'smtp.126.com', smtp_port: 465, smtp_security: 'implicit_tls' },
  { value: 'icloud', label: 'iCloud', smtp_host: 'smtp.mail.me.com', smtp_port: 587, smtp_security: 'starttls' },
  { value: 'outlook', label: 'Outlook / Office365', smtp_host: 'smtp.office365.com', smtp_port: 587, smtp_security: 'starttls' },
] as const

const channelTextFieldSx = {
  '& .MuiInputBase-input': {
    fontSize: '14px',
  },
  '& .MuiInputBase-input::placeholder': {
    fontSize: '14px',
  },
  '& .MuiInputLabel-root': {
    fontSize: '14px',
  },
  '& .MuiSelect-select': {
    fontSize: '14px',
  },
  '& .MuiFormControlLabel-label': {
    fontSize: '14px',
  },
} as const

type ApiBaseUrlHelpKind = 'wecom_app' | 'telegram'

const apiBaseUrlHelpContent = (kind: ApiBaseUrlHelpKind) => {
  const isTelegram = kind === 'telegram'
  return (
    <Box sx={{ display: 'flex', flexDirection: 'column', gap: 1.5, py: 0.5 }}>
      <Typography variant="subtitle2" fontWeight={600} color="text.primary" sx={{ fontSize: '14px', display: 'flex', alignItems: 'center', gap: 0.5 }}>
        API 反代地址配置
      </Typography>

      <Typography variant="body2" color="text.secondary" sx={{ fontSize: '13px', lineHeight: 1.5 }}>
        当服务器无法直接连接官方 API 时，可在此填写代理服务地址。<strong>留空表示直连官方 API。</strong>
      </Typography>

      <Box sx={{ bgcolor: (theme) => theme.palette.mode === 'light' ? 'rgba(0, 0, 0, 0.02)' : 'rgba(255, 255, 255, 0.02)', p: 1.25, borderRadius: 1, border: (theme) => `1px dashed ${theme.palette.divider}` }}>
        <Typography variant="caption" display="block" color="text.secondary" sx={{ fontWeight: 600, mb: 0.5 }}>
          填写规范：
        </Typography>
        <Typography variant="caption" display="block" color="text.secondary" component="ul" sx={{ pl: 2, m: 0, '& li': { mb: 0.25 } }}>
          <li>必须包含 <code>http://</code> 或 <code>https://</code> 协议头</li>
          <li>可携带自定义端口及路径前缀（如 <code>/tg</code>）</li>
          <li>不要填写具体的 API 接口动作路径</li>
        </Typography>
      </Box>

      <Box sx={{ width: '100%' }}>
        <Typography variant="caption" display="block" color="text.secondary" sx={{ fontWeight: 600, mb: 0.5 }}>
          填写示例：
        </Typography>
        <Box sx={{
          fontFamily: 'Consolas, Monaco, monospace',
          fontSize: '11px',
          bgcolor: (theme) => theme.palette.mode === 'light' ? '#f1f5f9' : '#1e293b',
          color: (theme) => theme.palette.mode === 'light' ? '#0f172a' : '#cbd5e1',
          p: 1.25,
          borderRadius: 1,
          border: (theme) => `1px solid ${theme.palette.divider}`,
          whiteSpace: 'pre-wrap',
          wordBreak: 'break-all'
        }}>
          {isTelegram
            ? 'https://relay.example.com/telegram\nhttp://192.168.1.10:8080/tg'
            : 'https://relay.example.com/wecom\nhttp://192.168.1.10:8080/wecom'}
        </Box>
      </Box>

      <Box>
        <Typography variant="caption" display="block" color="text.secondary" sx={{ fontWeight: 600, mb: 0.5 }}>
          实际请求路径：
        </Typography>
        <Box sx={{
          fontSize: '11px',
          color: 'primary.main',
          bgcolor: (theme) => theme.palette.mode === 'light' ? 'rgba(18, 150, 219, 0.06)' : 'rgba(66, 183, 245, 0.08)',
          p: 1,
          borderRadius: 1,
          border: (theme) => `1px solid ${theme.palette.mode === 'light' ? 'rgba(18, 150, 219, 0.2)' : 'rgba(66, 183, 245, 0.2)'}`,
          fontFamily: 'Consolas, Monaco, monospace',
          wordBreak: 'break-all',
          lineHeight: 1.4
        }}>
          {isTelegram ? (
            <>
              [反代地址]<strong>/bot&lt;TOKEN&gt;/sendMessage</strong>
            </>
          ) : (
            <>
              [反代地址]<strong>/cgi-bin/gettoken</strong> 等
            </>
          )}
        </Box>
      </Box>
    </Box>
  )
}

type NotificationChannelsTabProps = {
  config: NotificationConfig
  selectedChannel?: NotificationChannelInstance
  saving: boolean
  testing: boolean
  onSelectChannel: (id: string) => void
  onAddChannel: (type: NotificationChannelKey) => void
  onDeleteChannel: (id: string) => void
  onPatchChannel: (id: string, patch: Partial<NotificationChannelInstance>) => void
  onPatchChannelConfig: (id: string, patch: Record<string, unknown>) => void
  onSave: () => void
  onTest: () => void
}

export default function NotificationChannelsTab({
  config,
  selectedChannel,
  saving,
  testing,
  onSelectChannel,
  onAddChannel,
  onDeleteChannel,
  onPatchChannel,
  onPatchChannelConfig,
  onSave,
  onTest,
}: NotificationChannelsTabProps) {
  const isCompact = useMediaQuery<Theme>((theme: Theme) => theme.breakpoints.down('md'))
  const [addMenuAnchor, setAddMenuAnchor] = useState<HTMLElement | null>(null)
  const selectedChannelDef = selectedChannel ? channelDef(selectedChannel.type) : undefined
  const SelectedChannelIcon = selectedChannelDef?.icon
  const enabledChannelCount = config.channels.filter((channel) => channel.enabled).length
  const disabledChannelCount = config.channels.length - enabledChannelCount
  const selectedChannelTitle = selectedChannel?.name.trim() || selectedChannelDef?.label || '通知通道'

  const [prevChannelId, setPrevChannelId] = useState<string | undefined>(selectedChannel?.id)
  const [headersText, setHeadersText] = useState(() => {
    if (selectedChannel && selectedChannel.type === 'webhook') {
      return headersToText(selectedChannel.config.headers)
    }
    return ''
  })

  if (selectedChannel?.id !== prevChannelId) {
    setPrevChannelId(selectedChannel?.id)
    setHeadersText(selectedChannel && selectedChannel.type === 'webhook' ? headersToText(selectedChannel.config.headers) : '')
  }

  const renderStringField = (
    channel: NotificationChannelInstance,
    key: string,
    label: string,
    extra?: { password?: boolean; select?: string[]; multiline?: boolean; endAdornment?: ReactNode },
  ) => (
    <TextField
      key={key}
      select={!!extra?.select}
      label={label}
      type={extra?.password ? 'password' : 'text'}
      value={getString(channel.config, key)}
      onChange={(event: ChangeEvent<HTMLInputElement>) => onPatchChannelConfig(channel.id, { [key]: event.target.value })}
      multiline={extra?.multiline}
      minRows={extra?.multiline ? 3 : undefined}
      fullWidth
      sx={channelTextFieldSx}
      slotProps={extra?.endAdornment ? {
        input: {
          endAdornment: extra.endAdornment,
        },
      } : undefined}
    >
      {extra?.select?.map((option) => (
        <MenuItem key={option} value={option}>{option || '默认'}</MenuItem>
      ))}
    </TextField>
  )

  const renderApiBaseUrlHelp = (kind: ApiBaseUrlHelpKind) => (
    <InputAdornment position="end">
      <Tooltip
        title={apiBaseUrlHelpContent(kind)}
        arrow
        placement="top"
        slotProps={{
          tooltip: {
            sx: {
              bgcolor: (theme) => theme.palette.mode === 'light' ? '#ffffff' : '#0f172a',
              color: 'text.primary',
              boxShadow: (theme) => theme.palette.mode === 'light'
                ? '0 10px 25px -5px rgba(0,0,0,0.1), 0 8px 10px -6px rgba(0,0,0,0.05), 0 0 0 1px rgba(0,0,0,0.05)'
                : '0 10px 25px -5px rgba(0,0,0,0.5), 0 8px 10px -6px rgba(0,0,0,0.3), 0 0 0 1px rgba(255,255,255,0.08)',
              borderRadius: 2,
              p: 2.25,
              border: (theme) => `1px solid ${theme.palette.divider}`,
              maxWidth: 380,
            },
          },
          arrow: {
            sx: {
              color: (theme) => theme.palette.mode === 'light' ? '#ffffff' : '#0f172a',
              '&::before': {
                border: (theme) => `1px solid ${theme.palette.divider}`,
              },
            },
          },
        }}
      >
        <IconButton size="small" edge="end" aria-label="API 反代地址填写说明">
          <HelpOutline fontSize="small" />
        </IconButton>
      </Tooltip>
    </InputAdornment>
  )

  const renderBoolField = (channel: NotificationChannelInstance, key: string, label: string) => (
    <FormControlLabel
      control={
        <Switch
          checked={getBool(channel.config, key)}
          onChange={(event: ChangeEvent<HTMLInputElement>) => onPatchChannelConfig(channel.id, { [key]: event.target.checked })}
        />
      }
      label={label}
    />
  )

  const renderNumberConfigField = (
    channel: NotificationChannelInstance,
    key: string,
    label: string,
    min = 0,
  ) => (
    <TextField
      type="number"
      label={label}
      value={Number(channel.config[key]) || min}
      onChange={(event: ChangeEvent<HTMLInputElement>) => {
        const value = Math.max(min, Math.trunc(Number(event.target.value) || min))
        onPatchChannelConfig(channel.id, { [key]: value })
      }}
      inputProps={{ min }}
      fullWidth
      sx={channelTextFieldSx}
    />
  )

  const emailPresetValue = (channel: NotificationChannelInstance) => {
    const smtpHost = getString(channel.config, 'smtp_host')
    const smtpPort = Number(channel.config.smtp_port) || 0
    const smtpSecurity = getString(channel.config, 'smtp_security')
    return EMAIL_PROVIDER_PRESETS.find((preset) => (
      preset.value !== 'custom'
      && preset.smtp_host === smtpHost
      && preset.smtp_port === smtpPort
      && preset.smtp_security === smtpSecurity
    ))?.value ?? 'custom'
  }

  const applyEmailPreset = (channel: NotificationChannelInstance, value: string) => {
    const preset = EMAIL_PROVIDER_PRESETS.find((item) => item.value === value)
    if (!preset || preset.value === 'custom') return
    onPatchChannelConfig(channel.id, {
      smtp_host: preset.smtp_host,
      smtp_port: preset.smtp_port,
      smtp_security: preset.smtp_security,
    })
  }

  const patchRateLimit = (channel: NotificationChannelInstance, patch: Partial<NotificationChannelInstance['rate_limit']>) => {
    onPatchChannel(channel.id, {
      rate_limit: {
        ...channel.rate_limit,
        ...patch,
      },
    })
  }

  const renderRateNumberField = (
    channel: NotificationChannelInstance,
    key: keyof NotificationChannelInstance['rate_limit'],
    label: string,
    min = 0,
  ) => (
    <TextField
      type="number"
      size="small"
      label={label}
      value={channel.rate_limit[key]}
      onChange={(event: ChangeEvent<HTMLInputElement>) => {
        const value = Math.max(min, Math.trunc(Number(event.target.value) || min))
        patchRateLimit(channel, { [key]: value })
      }}
      inputProps={{ min }}
      fullWidth
      sx={channelTextFieldSx}
    />
  )

  const renderRateLimitFields = () => {
    if (!selectedChannel) return null
    const channel = selectedChannel
    return (
      <Box
        sx={{
          p: 2,
          mb: 0,
          border: '1px solid',
          borderColor: 'rgba(0, 0, 0, 0.23)',
          borderRadius: 1,
          bgcolor: 'transparent',
        }}
      >
        <Box display="flex" alignItems="flex-start" justifyContent="space-between" gap={2} flexWrap="wrap">
          <Box sx={{ minWidth: 0 }}>
            <Typography variant="subtitle2" fontWeight={700}>队列保护</Typography>
            <Typography variant="caption" color="text.secondary">
              推荐保持开启。超过平台频率时先排队，稍后自动继续推送，避免消息丢失。
            </Typography>
          </Box>
          <FormControlLabel
            sx={{ mr: 0 }}
            control={(
              <Switch
                checked={channel.rate_limit.enabled}
                onChange={(event: ChangeEvent<HTMLInputElement>) => {
                  patchRateLimit(channel, {
                    enabled: event.target.checked,
                  })
                }}
              />
            )}
            label={channel.rate_limit.enabled ? '已开启' : '已关闭'}
          />
        </Box>

        {channel.rate_limit.enabled && (
          <>
            <Box
              display="grid"
              gridTemplateColumns={{ xs: '1fr', sm: 'repeat(2, minmax(0, 1fr))' }}
              gap={1.5}
              mt={2}
            >
              {renderRateNumberField(channel, 'window_seconds', '统计周期（秒）', 1)}
              {renderRateNumberField(channel, 'max_messages', '最多发送条数', 1)}
            </Box>

            <Typography variant="caption" color="text.secondary" sx={{ display: 'block', mt: 1 }}>
              当前限制：每 {channel.rate_limit.window_seconds} 秒最多发送 {channel.rate_limit.max_messages} 条，超出部分将进入通知队列，等待下一个时间窗口自动重试。
            </Typography>
          </>
        )}
      </Box>
    )
  }

  const renderChannelFields = () => {
    if (!selectedChannel) {
      return (
        <Box display="flex" alignItems="center" justifyContent="center" height="100%" color="text.secondary">
          <Typography>点击左侧 + 创建通知渠道</Typography>
        </Box>
      )
    }

    const channel = selectedChannel
    const fieldStackSx = {
      display: 'flex',
      flexDirection: 'column',
      gap: 2,
    }

    switch (channel.type) {
      case 'webhook':
        return (
          <Box sx={fieldStackSx}>
            {renderStringField(channel, 'url', 'Webhook URL')}
            {renderStringField(channel, 'secret', '签名密钥', { password: true })}
            <TextField
              label="请求头"
              value={headersText}
              onChange={(event: ChangeEvent<HTMLInputElement>) => {
                const text = event.target.value
                setHeadersText(text)
                onPatchChannelConfig(channel.id, { headers: textToHeaders(text) })
              }}
              placeholder="Content-Type: text/plain"
              multiline
              minRows={4}
              fullWidth
              sx={channelTextFieldSx}
            />
          </Box>
        )
      case 'bark':
        return (
          <Box sx={fieldStackSx}>
            {renderStringField(channel, 'server_url', 'Server URL')}
            {renderStringField(channel, 'device_key', 'Device Key', { password: true })}
            {renderStringField(channel, 'group', '分组')}
            {renderStringField(channel, 'sound', '铃声')}
            {renderStringField(channel, 'level', '推送等级', { select: ['', 'active', 'timeSensitive', 'passive'] })}
            {renderStringField(channel, 'icon', '图标 URL')}
            <Box display="flex" gap={2} flexWrap="wrap">
              {renderBoolField(channel, 'auto_copy', '自动复制')}
              {renderBoolField(channel, 'save_history', '保存历史记录')}
            </Box>
          </Box>
        )
      case 'pushplus':
        return (
          <Box sx={fieldStackSx}>
            {renderStringField(channel, 'token', 'Token', { password: true })}
            {renderStringField(channel, 'topic', 'Topic')}
            {renderStringField(channel, 'template', 'Template', { select: ['', 'txt', 'html', 'markdown'] })}
            {renderStringField(channel, 'channel', 'Channel', { select: ['', 'wechat', 'webhook', 'cp', 'mail', 'sms', 'bark', 'gotify'] })}
            {renderStringField(channel, 'option', 'Option')}
            {renderStringField(channel, 'callback_url', 'Callback URL')}
          </Box>
        )
      case 'wecom_app':
        return (
          <Box sx={fieldStackSx}>
            {renderStringField(channel, 'corp_id', 'CorpID')}
            {renderStringField(channel, 'agent_id', 'AgentID')}
            {renderStringField(channel, 'secret', 'Secret', { password: true })}
            {renderStringField(channel, 'to_user', 'ToUser')}
            {renderStringField(channel, 'to_party', 'ToParty')}
            {renderStringField(channel, 'to_tag', 'ToTag')}
            {renderStringField(channel, 'api_base_url', 'API 反代地址', { endAdornment: renderApiBaseUrlHelp('wecom_app') })}
            {renderBoolField(channel, 'safe', '保密消息')}
          </Box>
        )
      case 'wecom_robot':
        return (
          <Box sx={fieldStackSx}>
            {renderStringField(channel, 'webhook_url', 'Webhook URL')}
            {renderStringField(channel, 'key', 'Webhook Key', { password: true })}
          </Box>
        )
      case 'dingtalk_robot':
        return (
          <Box sx={fieldStackSx}>
            {renderStringField(channel, 'webhook_url', 'Webhook URL')}
            {renderStringField(channel, 'access_token', 'Access Token', { password: true })}
            {renderStringField(channel, 'secret', '加签 Secret', { password: true })}
            {renderStringField(channel, 'at_mobiles', 'At Mobiles')}
            {renderBoolField(channel, 'at_all', '@ 所有人')}
          </Box>
        )
      case 'dingtalk_app':
        return (
          <Box sx={fieldStackSx}>
            {renderStringField(channel, 'app_key', 'AppKey')}
            {renderStringField(channel, 'app_secret', 'AppSecret', { password: true })}
            {renderStringField(channel, 'robot_code', 'RobotCode')}
            {renderStringField(channel, 'open_conversation_id', 'OpenConversationId')}
            {renderStringField(channel, 'msg_key', 'MsgKey')}
          </Box>
        )
      case 'feishu_robot':
        return (
          <Box sx={fieldStackSx}>
            {renderStringField(channel, 'webhook_url', 'Webhook URL')}
            {renderStringField(channel, 'token', 'Token', { password: true })}
            {renderStringField(channel, 'secret', '加签 Secret', { password: true })}
          </Box>
        )
      case 'telegram':
        return (
          <Box sx={fieldStackSx}>
            {renderStringField(channel, 'bot_token', 'Bot Token', { password: true })}
            {renderStringField(channel, 'chat_id', 'Chat ID')}
            {renderStringField(channel, 'parse_mode', 'Parse Mode', { select: ['', 'MarkdownV2', 'HTML'] })}
            {renderStringField(channel, 'api_base_url', 'API 反代地址', { endAdornment: renderApiBaseUrlHelp('telegram') })}
            {renderBoolField(channel, 'disable_web_page_preview', '禁用链接预览')}
          </Box>
        )
      case 'email':
        return (
          <Box sx={fieldStackSx}>
            <TextField
              select
              label="服务商预设"
              value={emailPresetValue(channel)}
              onChange={(event: ChangeEvent<HTMLInputElement>) => applyEmailPreset(channel, event.target.value)}
              fullWidth
              sx={channelTextFieldSx}
            >
              {EMAIL_PROVIDER_PRESETS.map((preset) => (
                <MenuItem key={preset.value} value={preset.value}>{preset.label}</MenuItem>
              ))}
            </TextField>
            <Box display="grid" gridTemplateColumns={{ xs: '1fr', sm: 'minmax(0, 1fr) 160px' }} gap={2}>
              {renderStringField(channel, 'smtp_host', 'SMTP 服务器')}
              {renderNumberConfigField(channel, 'smtp_port', 'SMTP 端口', 1)}
            </Box>
            {renderStringField(channel, 'smtp_security', '安全模式', { select: ['implicit_tls', 'starttls', 'none'] })}
            <Box display="flex" gap={2} flexWrap="wrap">
              {renderBoolField(channel, 'allow_insecure_tls', '允许不安全证书')}
            </Box>
            {renderStringField(channel, 'username', '用户名')}
            {renderStringField(channel, 'password', '密码 / 授权码', { password: true })}
            {renderStringField(channel, 'sender_address', '发件人邮箱')}
            {renderStringField(channel, 'sender_name', '发件人名称')}
            {renderStringField(channel, 'receiver_addresses', '收件人邮箱', { multiline: true })}
            {renderStringField(channel, 'message_format', '消息格式', { select: ['plain', 'html'] })}
          </Box>
        )
      case 'serverchan3':
        return (
          <Box sx={fieldStackSx}>
            {renderStringField(channel, 'send_key', 'SendKey / AppKey', { password: true })}
            {renderStringField(channel, 'uid', 'UID（可选）')}
            {renderStringField(channel, 'channel', '发送通道（可选）')}
            {renderStringField(channel, 'openid', 'OpenID / Group（可选）')}
          </Box>
        )
      default:
        return null
    }
  }

  return (
    <Card sx={{ height: 'calc(100vh - 220px)', minHeight: 520, ...channelTextFieldSx }}>
      <CardContent sx={{ height: '100%', p: 0, '&:last-child': { pb: 0 } }}>
        <Box display="flex" height="100%">
          {!isCompact && (
            <Box sx={{ width: 288, borderRight: 1, borderColor: 'divider', flexShrink: 0, display: 'flex', flexDirection: 'column' }}>
              <Box p={2}>
                <Box display="flex" gap={1} flexWrap="wrap">
                  <Paper sx={{ p: 1, flex: 1, minWidth: 60, textAlign: 'center' }}>
                    <Typography variant="h6" color="primary" fontWeight={600}>{enabledChannelCount}</Typography>
                    <Typography variant="caption" color="text.secondary">启用</Typography>
                  </Paper>
                  <Paper sx={{ p: 1, flex: 1, minWidth: 60, textAlign: 'center' }}>
                    <Typography variant="h6" color="text.secondary" fontWeight={600}>{disabledChannelCount}</Typography>
                    <Typography variant="caption" color="text.secondary">停用</Typography>
                  </Paper>
                </Box>
                <Box display="flex" alignItems="center" justifyContent="space-between" mt={1.5}>
                  <Typography variant="subtitle1" fontWeight={600}>通知渠道 ({config.channels.length})</Typography>
                  <Tooltip title="新增通知渠道">
                    <IconButton size="small" color="primary" onClick={(event: MouseEvent<HTMLElement>) => setAddMenuAnchor(event.currentTarget)}>
                      <Add />
                    </IconButton>
                  </Tooltip>
                </Box>
              </Box>
              <Divider />
              <List sx={{ flex: 1, overflow: 'auto' }}>
                {config.channels.map((channel) => {
                  const def = channelDef(channel.type)
                  const Icon = def.icon
                  return (
                    <ListItemButton
                      key={channel.id}
                      selected={selectedChannel?.id === channel.id}
                      onClick={() => onSelectChannel(channel.id)}
                      sx={{ gap: 1.25, py: 1.25 }}
                    >
                      <Avatar sx={{
                        width: 32,
                        height: 32,
                        bgcolor: (theme: Theme) => channel.enabled ? alpha(theme.palette.primary.main, 0.1) : theme.palette.action.hover,
                        color: channel.enabled ? 'primary.main' : 'text.disabled',
                      }}>
                        <Icon fontSize="small" />
                      </Avatar>
                      <ListItemText
                        primary={<Typography variant="body2" fontWeight={600} noWrap>{channel.name}</Typography>}
                        secondary={<Typography variant="caption" color="text.secondary" noWrap>{def.label}</Typography>}
                      />
                    </ListItemButton>
                  )
                })}
              </List>
            </Box>
          )}

          <Box sx={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column' }}>
            {isCompact && (
              <Box sx={{ p: 2, pb: 0 }}>
                <Box display="flex" gap={1} mb={1.5}>
                  <Paper sx={{ p: 1, flex: 1, minWidth: 60, textAlign: 'center' }}>
                    <Typography variant="h6" color="primary" fontWeight={600}>{enabledChannelCount}</Typography>
                    <Typography variant="caption" color="text.secondary">启用</Typography>
                  </Paper>
                  <Paper sx={{ p: 1, flex: 1, minWidth: 60, textAlign: 'center' }}>
                    <Typography variant="h6" color="text.secondary" fontWeight={600}>{disabledChannelCount}</Typography>
                    <Typography variant="caption" color="text.secondary">停用</Typography>
                  </Paper>
                </Box>
                <Box display="flex" gap={1} alignItems="center" mb={2}>
                  <TextField
                    select
                    size="small"
                    label={`通知渠道 (${config.channels.length})`}
                    value={selectedChannel?.id ?? ''}
                    onChange={(event: ChangeEvent<HTMLInputElement>) => onSelectChannel(event.target.value)}
                    sx={{ flex: 1, minWidth: 0, ...channelTextFieldSx }}
                  >
                    {config.channels.length === 0 && <MenuItem value="">暂无通知渠道</MenuItem>}
                    {config.channels.map((channel) => {
                      const def = channelDef(channel.type)
                      return (
                        <MenuItem key={channel.id} value={channel.id}>
                          {channel.name} / {def.label}
                        </MenuItem>
                      )
                    })}
                  </TextField>
                  <Tooltip title="新增通知渠道">
                    <IconButton size="small" color="primary" onClick={(event: MouseEvent<HTMLElement>) => setAddMenuAnchor(event.currentTarget)}>
                      <Add />
                    </IconButton>
                  </Tooltip>
                </Box>
              </Box>
            )}
            <Box display="flex" alignItems="center" gap={1.5} p={2} borderBottom={1} borderColor="divider" flexWrap="wrap">
              {selectedChannel && SelectedChannelIcon ? (
                <>
                  <Avatar
                    sx={{
                      width: 36,
                      height: 36,
                      bgcolor: (theme: Theme) => selectedChannel.enabled ? alpha(theme.palette.primary.main, 0.1) : theme.palette.action.hover,
                      color: selectedChannel.enabled ? 'primary.main' : 'text.disabled',
                    }}
                  >
                    <SelectedChannelIcon fontSize="small" />
                  </Avatar>
                  <Typography variant="subtitle1" fontWeight={600} noWrap sx={{ minWidth: 0, maxWidth: { xs: 180, sm: 280 } }}>
                    {selectedChannelTitle}
                  </Typography>
                  <Box sx={{ width: '1px', height: 20, bgcolor: 'divider' }} />
                  <Button variant="outlined" startIcon={testing ? <CircularProgress size={18} /> : <PlayArrow />} disabled={testing} onClick={onTest} sx={{ whiteSpace: 'nowrap' }}>
                    发送测试
                  </Button>
                  <Button variant="contained" startIcon={saving ? <CircularProgress size={18} /> : <Save />} disabled={saving} onClick={onSave} sx={{ whiteSpace: 'nowrap' }}>
                    保存配置
                  </Button>
                  <Tooltip title="删除通道">
                    <IconButton color="error" onClick={() => onDeleteChannel(selectedChannel.id)}>
                      <DeleteOutline />
                    </IconButton>
                  </Tooltip>
                  <Box flexGrow={1} />
                  <FormControlLabel
                    sx={{ ml: 0, whiteSpace: 'nowrap' }}
                    control={<Switch checked={selectedChannel.enabled} onChange={(event: ChangeEvent<HTMLInputElement>) => onPatchChannel(selectedChannel.id, { enabled: event.target.checked })} />}
                    label={selectedChannel.enabled ? '已启用' : '已停用'}
                  />
                </>
              ) : (
                <Typography color="text.secondary">暂无通知渠道</Typography>
              )}
            </Box>
            <Box sx={{ flex: 1, overflow: 'auto', p: 2 }}>
              {selectedChannel && (
                <TextField
                  label="通道名称"
                  value={selectedChannel.name}
                  onChange={(event: ChangeEvent<HTMLInputElement>) => onPatchChannel(selectedChannel.id, { name: event.target.value })}
                  fullWidth
                  sx={{ mb: 2, ...channelTextFieldSx }}
                />
              )}
              {renderChannelFields()}
              {selectedChannel && <Box sx={{ mt: 2 }}>{renderRateLimitFields()}</Box>}
            </Box>
          </Box>
        </Box>
      </CardContent>
      <Menu anchorEl={addMenuAnchor} open={!!addMenuAnchor} onClose={() => setAddMenuAnchor(null)}>
        <MenuItem disabled>选择通道类型</MenuItem>
        {CHANNEL_DEFS.map((item) => {
          const Icon = item.icon
          return (
            <MenuItem
              key={item.key}
              onClick={() => {
                onAddChannel(item.key)
                setAddMenuAnchor(null)
              }}
            >
              <Icon fontSize="small" style={{ marginRight: 10 }} />
              {item.label}
            </MenuItem>
          )
        })}
      </Menu>
    </Card>
  )
}
