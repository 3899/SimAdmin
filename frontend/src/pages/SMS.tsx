import { useState, useEffect, useRef, useCallback, useMemo, type ChangeEvent } from 'react'
import {
  Box,
  Card,
  CardContent,
  Typography,
  Button,
  TextField,
  List,
  Alert,
  CircularProgress,
  IconButton,
  Divider,
  Paper,
  Avatar,
  Snackbar,
  useMediaQuery,
  InputAdornment,
  Checkbox,
  Tooltip,
} from '@mui/material'
import type { Theme } from '@mui/material/styles'
import {
  Refresh,
  Person,
  ArrowBack,
  Add,
  Checklist,
  Close,
  Search,
  SelectAll,
} from '@mui/icons-material'
import { api, type SmsMessage, type SmsStats } from '../api/current'
import {
  SmsMessageBubble,
  SmsComposer,
  SmsConversationItem,
  SmsDeleteConfirmDialog,
  SmsNewChatDialog,
  SmsBatchBar,
  SmsEmptyState,
  compareSmsChronological,
  compareSmsNewestFirst,
  includesSearchText,
} from './sms/index'

interface ConversationGroup {
  phoneNumber: string
  messages: SmsMessage[]
  lastMessage: SmsMessage
  unreadCount: number
}

type ConversationSearchResult = ConversationGroup & {
  matchedMessage: SmsMessage | null
}

type DeleteTarget =
  | { type: 'batch' }
  | { type: 'conversation'; phoneNumber: string; messageCount: number }
  | { type: 'message'; message: SmsMessage }

function buildConversations(msgs: SmsMessage[]): ConversationGroup[] {
  const groups = new Map<string, SmsMessage[]>()

  msgs.forEach((msg) => {
    const key = msg.phone_number
    if (!groups.has(key)) {
      groups.set(key, [])
    }
    groups.get(key)?.push(msg)
  })

  const conversationList: ConversationGroup[] = []
  groups.forEach((groupMessages, phoneNumber) => {
    groupMessages.sort(compareSmsNewestFirst)
    conversationList.push({
      phoneNumber,
      messages: groupMessages,
      lastMessage: groupMessages[0],
      unreadCount: groupMessages.filter((m) => m.direction === 'incoming' && m.status === 'received').length,
    })
  })

  conversationList.sort((a, b) => compareSmsNewestFirst(a.lastMessage, b.lastMessage))

  return conversationList
}

export default function SMSPage() {
  const isMobile = useMediaQuery<Theme>((theme: Theme) => theme.breakpoints.down('md'))

  const [messages, setMessages] = useState<SmsMessage[]>([])
  const [stats, setStats] = useState<SmsStats | null>(null)
  const [loading, setLoading] = useState(false)
  const [sendLoading, setSendLoading] = useState(false)
  const [deleteLoading, setDeleteLoading] = useState(false)
  const [phoneNumber, setPhoneNumber] = useState('')
  const [content, setContent] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState<string | null>(null)
  const [newChatDialogOpen, setNewChatDialogOpen] = useState(false)

  // 对话状态
  const [conversations, setConversations] = useState<ConversationGroup[]>([])
  const [selectedConversation, setSelectedConversation] = useState<string | null>(null)
  const [conversationMessages, setConversationMessages] = useState<SmsMessage[]>([])
  const [conversationLoading, setConversationLoading] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')

  // 批量管理状态
  const [batchMode, setBatchMode] = useState(false)
  const [selectedConversationPhones, setSelectedConversationPhones] = useState<Set<string>>(() => new Set())
  const [selectedMessageIds, setSelectedMessageIds] = useState<Set<number>>(() => new Set())
  const [deleteTarget, setDeleteTarget] = useState<DeleteTarget | null>(null)

  // 聊天区域滚动引用
  const chatEndRef = useRef<HTMLDivElement>(null)
  // 输入框焦点状态 - 有焦点时暂停刷新避免失焦
  const inputFocusedRef = useRef(false)

  const scrollToBottom = useCallback(() => {
    chatEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [])

  const scrollToMessage = useCallback((messageId: number) => {
    const target = document.getElementById(`sms-message-${messageId}`)
    if (target) {
      target.scrollIntoView({ behavior: 'smooth', block: 'center' })
      return
    }
    scrollToBottom()
  }, [scrollToBottom])

  const fetchMessages = useCallback(async (isBackground = false) => {
    if (!isBackground) {
      setLoading(true)
      setError(null)
    }
    try {
      const response = await api.getSmsList({ limit: 1000, offset: 0 })
      if (response.status === 'ok' && response.data) {
        setMessages(response.data.messages)
        setConversations(buildConversations(response.data.messages))
      } else {
        if (!isBackground) setError(response.message)
      }
    } catch (err) {
      if (!isBackground) {
        setError(err instanceof Error ? err.message : String(err))
      } else {
        console.warn('Background SMS fetch warning:', err)
      }
    } finally {
      if (!isBackground) setLoading(false)
    }
  }, [])

  const fetchConversation = useCallback(async (phone: string, scrollTargetId?: number) => {
    setConversationLoading(true)
    try {
      const response = await api.getSmsConversation({ phone_number: phone, limit: 1000 })
      if (response.status === 'ok' && response.data) {
        const sorted = [...response.data.messages].sort(compareSmsChronological)
        setConversationMessages(sorted)
        setTimeout(() => {
          if (scrollTargetId !== undefined) {
            scrollToMessage(scrollTargetId)
          } else {
            scrollToBottom()
          }
        }, 100)
      }
    } catch {
      const localMsgs = messages.filter((m) => m.phone_number === phone)
      const sorted = [...localMsgs].sort(compareSmsChronological)
      setConversationMessages(sorted)
      setTimeout(() => {
        if (scrollTargetId !== undefined) {
          scrollToMessage(scrollTargetId)
        } else {
          scrollToBottom()
        }
      }, 100)
    } finally {
      setConversationLoading(false)
    }
  }, [messages, scrollToBottom, scrollToMessage])

  const fetchStats = useCallback(async () => {
    try {
      const response = await api.getSmsStats()
      if (response.status === 'ok' && response.data) {
        setStats(response.data)
      }
    } catch (err) {
      console.error('获取短信统计失败:', err)
    }
  }, [])

  useEffect(() => {
    void fetchMessages(false)
    void fetchStats()
    const interval = setInterval(() => {
      if (inputFocusedRef.current) {
        return
      }
      void fetchMessages(true)
      void fetchStats()
    }, 10000)
    return () => clearInterval(interval)
  }, [fetchMessages, fetchStats])

  const messageById = useMemo(() => {
    const map = new Map<number, SmsMessage>()
    messages.forEach((msg) => map.set(msg.id, msg))
    conversationMessages.forEach((msg) => map.set(msg.id, msg))
    return map
  }, [messages, conversationMessages])

  const searchTerm = searchQuery.trim()

  const visibleConversations = useMemo<ConversationSearchResult[]>(() => {
    if (!searchTerm) {
      return conversations.map((conv) => ({ ...conv, matchedMessage: null }))
    }

    return conversations
      .map((conv) => {
        const phoneMatched = includesSearchText(conv.phoneNumber, searchTerm)
        const matchedMessage = conv.messages.find((msg) => includesSearchText(msg.content, searchTerm)) ?? null

        if (!phoneMatched && !matchedMessage) {
          return null
        }

        return {
          ...conv,
          matchedMessage,
        }
      })
      .filter((conv): conv is ConversationSearchResult => conv !== null)
  }, [conversations, searchTerm])

  const batchSelection = useMemo(() => {
    const visiblePhones = new Set(visibleConversations.map((conv) => conv.phoneNumber))
    const phoneNumbers = Array.from(selectedConversationPhones).filter((phone) => visiblePhones.has(phone))
    const phoneNumberSet = new Set(phoneNumbers)
    const selectedConversationNumbers = new Set(phoneNumbers)
    let messageCount = 0

    visibleConversations.forEach((conv) => {
      if (phoneNumberSet.has(conv.phoneNumber)) {
        messageCount += conv.messages.length
      }
    })

    const ids = Array.from(selectedMessageIds).filter((id) => {
      const msg = messageById.get(id)
      if (!msg || phoneNumberSet.has(msg.phone_number)) {
        return false
      }
      selectedConversationNumbers.add(msg.phone_number)
      messageCount += 1
      return true
    })

    return {
      ids,
      phoneNumbers,
      conversationCount: selectedConversationNumbers.size,
      messageCount,
    }
  }, [visibleConversations, messageById, selectedConversationPhones, selectedMessageIds])

  const hasBatchSelection = batchSelection.messageCount > 0
  const smsStats = stats ?? { total: 0, incoming: 0, outgoing: 0, pushed: 0, push_attempted: 0 }
  const pushCount = smsStats.pushed ?? 0
  const pushAttemptedCount = smsStats.push_attempted ?? 0
  const allConversationsSelected = visibleConversations.length > 0
    && visibleConversations.every((conv) => selectedConversationPhones.has(conv.phoneNumber))
  const currentMessagesSomeSelected = conversationMessages.some(
    (msg) => selectedConversationPhones.has(msg.phone_number) || selectedMessageIds.has(msg.id),
  )
  const currentMessagesAllSelected = conversationMessages.length > 0
    && conversationMessages.every((msg) => selectedConversationPhones.has(msg.phone_number) || selectedMessageIds.has(msg.id))

  const resetBatchSelection = () => {
    setSelectedConversationPhones(new Set())
    setSelectedMessageIds(new Set())
  }

  const handleEnterBatchMode = () => {
    setBatchMode(true)
  }

  const handleExitBatchMode = () => {
    setBatchMode(false)
    resetBatchSelection()
  }

  const handleSelectConversation = (phone: string, scrollTargetId?: number) => {
    setSelectedConversation(phone)
    setPhoneNumber(phone)
    void fetchConversation(phone, scrollTargetId)
  }

  const handleBackToList = () => {
    setSelectedConversation(null)
    setConversationMessages([])
  }

  const handleStartNewChat = async (phone: string, text: string) => {
    if (!phone.trim()) {
      setError('请输入电话号码')
      return
    }
    setNewChatDialogOpen(false)
    setSendLoading(true)
    setError(null)
    setSuccess(null)

    try {
      const response = await api.sendSms(phone, text)
      if (response.status === 'ok') {
        setSuccess(`短信已发送到 ${phone}`)
        setSelectedConversation(phone)
        setPhoneNumber(phone)
        setContent('')
        setTimeout(() => {
          void fetchMessages()
          void fetchStats()
          void fetchConversation(phone)
        }, 1000)
      } else {
        setError(response.message)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setSendLoading(false)
    }
  }

  const handleSendMessage = async () => {
    if (!phoneNumber.trim()) {
      setError('请输入电话号码')
      return
    }
    if (!content.trim()) {
      setError('请输入短信内容')
      return
    }

    setSendLoading(true)
    setError(null)
    setSuccess(null)

    try {
      const response = await api.sendSms(phoneNumber, content)
      if (response.status === 'ok') {
        setSuccess(`短信已发送到 ${phoneNumber}`)
        setContent('')
        setTimeout(() => {
          void fetchMessages()
          void fetchStats()
          if (selectedConversation) {
            void fetchConversation(selectedConversation)
          }
        }, 1000)
      } else {
        setError(response.message)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setSendLoading(false)
    }
  }

  const toggleConversationSelection = (phone: string) => {
    const selected = selectedConversationPhones.has(phone)
    setSelectedConversationPhones((prev) => {
      const next = new Set(prev)
      if (selected) {
        next.delete(phone)
      } else {
        next.add(phone)
      }
      return next
    })
    setSelectedMessageIds((prev) => {
      const next = new Set(prev)
      messageById.forEach((msg) => {
        if (msg.phone_number === phone) {
          next.delete(msg.id)
        }
      })
      return next
    })
  }

  const toggleAllConversations = () => {
    if (allConversationsSelected) {
      resetBatchSelection()
      return
    }

    const nextPhones = new Set<string>()
    const nextIds = new Set<number>()

    visibleConversations.forEach((conv) => {
      nextPhones.add(conv.phoneNumber)
      conv.messages.forEach((msg) => nextIds.delete(msg.id))
    })

    setSelectedConversationPhones(nextPhones)
    setSelectedMessageIds(nextIds)
  }

  const toggleMessageSelection = (msg: SmsMessage) => {
    if (selectedConversationPhones.has(msg.phone_number)) {
      const nextPhones = new Set(selectedConversationPhones)
      nextPhones.delete(msg.phone_number)
      const nextIds = new Set(selectedMessageIds)
      const currentConv = conversations.find((conv) => conv.phoneNumber === msg.phone_number)
      currentConv?.messages.forEach((item) => {
        if (item.id !== msg.id) {
          nextIds.add(item.id)
        }
      })
      setSelectedConversationPhones(nextPhones)
      setSelectedMessageIds(nextIds)
      return
    }

    setSelectedMessageIds((prev) => {
      const next = new Set(prev)
      if (next.has(msg.id)) {
        next.delete(msg.id)
      } else {
        next.add(msg.id)
      }
      return next
    })
  }

  const toggleAllCurrentMessages = () => {
    if (!selectedConversation) {
      return
    }

    if (currentMessagesAllSelected) {
      setSelectedConversationPhones((prev) => {
        const next = new Set(prev)
        next.delete(selectedConversation)
        return next
      })
      setSelectedMessageIds((prev) => {
        const next = new Set(prev)
        conversationMessages.forEach((msg) => next.delete(msg.id))
        return next
      })
      return
    }

    setSelectedConversationPhones((prev) => {
      const next = new Set(prev)
      next.delete(selectedConversation)
      return next
    })
    setSelectedMessageIds((prev) => {
      const next = new Set(prev)
      conversationMessages.forEach((msg) => next.add(msg.id))
      return next
    })
  }

  const isMessageSelected = (msg: SmsMessage) => (
    selectedConversationPhones.has(msg.phone_number) || selectedMessageIds.has(msg.id)
  )

  const requestConversationDelete = (conv: ConversationGroup) => {
    setDeleteTarget({
      type: 'conversation',
      phoneNumber: conv.phoneNumber,
      messageCount: conv.messages.length,
    })
  }

  const refreshAfterDelete = (clearConversation: boolean) => {
    void fetchMessages()
    void fetchStats()
    if (clearConversation) {
      setSelectedConversation(null)
      setConversationMessages([])
      return
    }
    if (selectedConversation) {
      void fetchConversation(selectedConversation)
    }
  }

  const handleConfirmDelete = async () => {
    if (!deleteTarget) {
      return
    }

    setDeleteLoading(true)
    setError(null)
    setSuccess(null)

    try {
      let deleted = 0
      let clearCurrentConversation = false

      if (deleteTarget.type === 'batch') {
        const response = await api.deleteSmsBatch({
          ids: batchSelection.ids,
          phone_numbers: batchSelection.phoneNumbers,
        })
        deleted = response.data?.deleted ?? batchSelection.messageCount
        clearCurrentConversation = Boolean(
          selectedConversation
          && (
            batchSelection.phoneNumbers.includes(selectedConversation)
            || (
              conversationMessages.length > 0
              && conversationMessages.every((msg) => batchSelection.ids.includes(msg.id))
            )
          ),
        )
        setSuccess(`已删除 ${deleted} 条短信`)
        handleExitBatchMode()
      } else if (deleteTarget.type === 'conversation') {
        const response = await api.deleteSmsConversation(deleteTarget.phoneNumber)
        deleted = response.data?.deleted ?? deleteTarget.messageCount
        clearCurrentConversation = selectedConversation === deleteTarget.phoneNumber
        setSuccess(`已删除对话 ${deleteTarget.phoneNumber}（${deleted} 条短信）`)
      } else {
        const response = await api.deleteSmsMessage(deleteTarget.message.id)
        deleted = response.data?.deleted ?? 1
        clearCurrentConversation = selectedConversation === deleteTarget.message.phone_number
          && conversationMessages.length <= 1
        setSuccess(deleted > 0 ? '短信已删除' : '短信不存在或已被删除')
      }

      setDeleteTarget(null)
      refreshAfterDelete(clearCurrentConversation)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setDeleteLoading(false)
    }
  }

  const renderBatchSelectionBar = () => (
    batchMode && hasBatchSelection ? (
      <SmsBatchBar
        selectedText={`已选 ${batchSelection.conversationCount} 个对话共 ${batchSelection.messageCount} 条短信`}
        onDelete={() => setDeleteTarget({ type: 'batch' })}
        deleting={deleteLoading}
      />
    ) : null
  )

  const conversationListContent = (
    <Box sx={{ height: '100%', display: 'flex', flexDirection: 'column' }}>
      <Box display="flex" gap={1} p={2} flexWrap="wrap">
        <Paper sx={{ p: 1, flex: 1, minWidth: 60, textAlign: 'center' }}>
          <Typography variant="h6" color="success.main" fontWeight={600}>{smsStats.incoming}</Typography>
          <Typography variant="caption" color="text.secondary">接收</Typography>
        </Paper>
        <Paper sx={{ p: 1, flex: 1, minWidth: 60, textAlign: 'center' }}>
          <Typography variant="h6" color="info.main" fontWeight={600}>{smsStats.outgoing}</Typography>
          <Typography variant="caption" color="text.secondary">发送</Typography>
        </Paper>
        <Paper sx={{ p: 1, flex: 1, minWidth: 60, textAlign: 'center' }}>
          <Tooltip title={`推送成功 ${pushCount} 条，尝试推送 ${pushAttemptedCount} 条`}>
            <Typography variant="h6" component="div">
              <Box component="span" sx={{ color: 'warning.main', fontWeight: 600 }}>
                {pushCount}
              </Box>
              <Box component="span" sx={{ mx: 0.5, color: 'text.secondary', fontWeight: 400 }}>
                /
              </Box>
              <Box component="span" sx={{ color: 'success.main', fontWeight: 600 }}>
                {pushAttemptedCount}
              </Box>
            </Typography>
          </Tooltip>
          <Typography variant="caption" color="text.secondary">推送</Typography>
        </Paper>
      </Box>

      <Box display="flex" justifyContent="space-between" alignItems="center" px={2} pb={1} gap={1}>
        <Typography variant="subtitle1" fontWeight={600}>
          对话 ({visibleConversations.length})
        </Typography>
        <Box display="flex" gap={0.5} alignItems="center">
          {batchMode ? (
            <>
              <Button
                size="small"
                startIcon={<SelectAll />}
                onClick={toggleAllConversations}
                disabled={visibleConversations.length === 0}
              >
                {allConversationsSelected ? '取消全选' : '全选对话'}
              </Button>
              <Tooltip title="退出批量管理">
                <IconButton size="small" onClick={handleExitBatchMode}>
                  <Close />
                </IconButton>
              </Tooltip>
            </>
          ) : (
            <>
              <Tooltip title="新建对话">
                <IconButton size="small" color="primary" onClick={() => setNewChatDialogOpen(true)}>
                  <Add />
                </IconButton>
              </Tooltip>
              <Tooltip title="刷新">
                <IconButton size="small" color="primary" onClick={() => void fetchMessages()} disabled={loading}>
                  <Refresh />
                </IconButton>
              </Tooltip>
              <Tooltip title="批量管理">
                <IconButton size="small" color="primary" onClick={handleEnterBatchMode}>
                  <Checklist />
                </IconButton>
              </Tooltip>
            </>
          )}
        </Box>
      </Box>

      <Box px={2} pb={1}>
        <TextField
          fullWidth
          size="small"
          value={searchQuery}
          onChange={(event: ChangeEvent<HTMLInputElement>) => setSearchQuery(event.target.value)}
          onFocus={() => { inputFocusedRef.current = true }}
          onBlur={() => { inputFocusedRef.current = false }}
          placeholder="搜索联系人或内容..."
          slotProps={{
            input: {
              startAdornment: (
                <InputAdornment position="start">
                  <Search fontSize="small" />
                </InputAdornment>
              ),
              endAdornment: searchQuery ? (
                <InputAdornment position="end">
                  <IconButton
                    size="small"
                    onClick={() => setSearchQuery('')}
                    edge="end"
                  >
                    <Close fontSize="small" />
                  </IconButton>
                </InputAdornment>
              ) : null,
            },
          }}
        />
      </Box>

      {!isMobile && renderBatchSelectionBar()}

      {loading && conversations.length === 0 ? (
        <Box display="flex" justifyContent="center" py={4}>
          <CircularProgress />
        </Box>
      ) : visibleConversations.length === 0 ? (
        <SmsEmptyState type={searchTerm ? 'search_empty' : 'no_conversations'} />
      ) : (
        <List sx={{ flex: 1, overflow: 'auto', p: 0 }}>
          {visibleConversations.map((conv, idx) => (
            <Box key={conv.phoneNumber}>
              <SmsConversationItem
                phoneNumber={conv.phoneNumber}
                lastMessageContent={conv.lastMessage.content}
                timestamp={conv.lastMessage.timestamp}
                messageCount={conv.messages.length}
                unreadCount={conv.unreadCount}
                isSelected={selectedConversation === conv.phoneNumber}
                onClick={() => handleSelectConversation(conv.phoneNumber)}
                searchQuery={searchQuery}
                batchMode={batchMode}
                checked={selectedConversationPhones.has(conv.phoneNumber)}
                onToggleSelect={() => toggleConversationSelection(conv.phoneNumber)}
                onDelete={() => requestConversationDelete(conv)}
              />
              {idx < visibleConversations.length - 1 && <Divider />}
            </Box>
          ))}
        </List>
      )}
    </Box>
  )

  const chatAreaContent = (
    <Box sx={{ height: '100%', display: 'flex', flexDirection: 'column' }}>
      <Box
        sx={{
          p: 2,
          borderBottom: 1,
          borderColor: 'divider',
          display: 'flex',
          alignItems: 'center',
          gap: 1,
        }}
      >
        {isMobile && (
          <IconButton onClick={handleBackToList} edge="start">
            <ArrowBack />
          </IconButton>
        )}
        <Avatar sx={{ bgcolor: 'primary.main' }}><Person /></Avatar>
        <Typography variant="h6" fontWeight={600}>{selectedConversation}</Typography>
        {batchMode && conversationMessages.length > 0 && (
          <Box sx={{ ml: 'auto', display: 'flex', alignItems: 'center' }}>
            <Checkbox
              size="small"
              checked={currentMessagesAllSelected}
              indeterminate={currentMessagesSomeSelected && !currentMessagesAllSelected}
              onChange={toggleAllCurrentMessages}
              inputProps={{ 'aria-label': '全选当前对话短信' }}
            />
            <Typography variant="body2" color="text.secondary">全选短信</Typography>
          </Box>
        )}
      </Box>

      {isMobile && renderBatchSelectionBar()}

      <Box
        sx={{
          flex: 1,
          overflow: 'auto',
          p: 2,
          bgcolor: (theme: Theme) => theme.palette.mode === 'dark' ? 'grey.900' : 'grey.50',
        }}
      >
        {conversationLoading ? (
          <Box display="flex" justifyContent="center" py={4}><CircularProgress /></Box>
        ) : conversationMessages.length === 0 ? (
          <SmsEmptyState type="no_messages" title="开始发送第一条消息" />
        ) : (
          <>
            {conversationMessages.map((msg, idx) => (
              <SmsMessageBubble
                key={msg.id || idx}
                message={msg}
                searchQuery={searchTerm}
                batchMode={batchMode}
                checked={isMessageSelected(msg)}
                onToggleSelect={() => toggleMessageSelection(msg)}
                onDelete={(m) => setDeleteTarget({ type: 'message', message: m as any })}
                onCopySuccess={(code) => setSuccess(`验证码 [${code}] 已复制`)}
              />
            ))}
            <div ref={chatEndRef} />
          </>
        )}
      </Box>

      {/* 底部输入框 */}
      <SmsComposer
        value={content}
        onChange={setContent}
        onSend={handleSendMessage}
        disabled={sendLoading}
        sending={sendLoading}
        onFocus={() => { inputFocusedRef.current = true }}
        onBlur={() => { inputFocusedRef.current = false }}
      />
    </Box>
  )

  return (
    <Box sx={{ p: { xs: 1, sm: 2, md: 3 } }}>
      <Card sx={{ height: 'calc(100vh - 120px)', minHeight: 500, display: 'flex', flexDirection: 'column' }}>
        <CardContent sx={{ flex: 1, p: '0 !important', display: 'flex', overflow: 'hidden' }}>
          {isMobile ? (
            selectedConversation ? (
              <Box sx={{ width: '100%', height: '100%' }}>{chatAreaContent}</Box>
            ) : (
              <Box sx={{ width: '100%', height: '100%' }}>{conversationListContent}</Box>
            )
          ) : (
            <>
              <Box sx={{ width: { sm: '40%', md: '35%' }, borderRight: 1, borderColor: 'divider', height: '100%' }}>
                {conversationListContent}
              </Box>
              <Box sx={{ flex: 1, height: '100%' }}>
                {selectedConversation ? (
                  chatAreaContent
                ) : (
                  <SmsEmptyState type="unselected" />
                )}
              </Box>
            </>
          )}
        </CardContent>
      </Card>

      {/* 新建短信对话框 */}
      <SmsNewChatDialog
        open={newChatDialogOpen}
        onClose={() => setNewChatDialogOpen(false)}
        onSend={({ phoneNumber: phone, content: text }) => handleStartNewChat(phone, text)}
        loading={sendLoading}
        error={error}
      />

      {/* 删除确认弹窗 */}
      <SmsDeleteConfirmDialog
        open={Boolean(deleteTarget)}
        target={deleteTarget ? {
          type: deleteTarget.type,
          phone_number: (deleteTarget as any).phoneNumber,
          message_count: (deleteTarget as any).messageCount,
          message: (deleteTarget as any).message,
        } : null}
        onClose={() => setDeleteTarget(null)}
        onConfirm={handleConfirmDelete}
        loading={deleteLoading}
        error={error}
        batchCount={batchSelection.messageCount}
      />

      <Snackbar
        open={Boolean(success)}
        autoHideDuration={4000}
        onClose={() => setSuccess(null)}
        anchorOrigin={{ vertical: 'bottom', horizontal: 'center' }}
      >
        <Alert onClose={() => setSuccess(null)} severity="success" sx={{ width: '100%' }}>
          {success}
        </Alert>
      </Snackbar>

      <Snackbar
        open={Boolean(error)}
        autoHideDuration={6000}
        onClose={() => setError(null)}
        anchorOrigin={{ vertical: 'bottom', horizontal: 'center' }}
      >
        <Alert onClose={() => setError(null)} severity="error" sx={{ width: '100%' }}>
          {error}
        </Alert>
      </Snackbar>
    </Box>
  )
}
