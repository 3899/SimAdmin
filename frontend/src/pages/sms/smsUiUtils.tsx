import { Box } from '@mui/material'
import type { ReactNode } from 'react'
import type { BaseSmsMessage } from './smsTypes'

/**
 * 解析各种格式的短信时间戳为 Date 对象
 */
export function parseSmsTimestamp(timestamp: string): Date | null {
  if (!timestamp) return null
  const normalized = timestamp.includes(' ') ? timestamp.replace(' ', 'T') : timestamp
  const date = new Date(normalized)
  return Number.isNaN(date.getTime()) ? null : date
}

export function smsTimestampMillis(timestamp: string): number {
  return parseSmsTimestamp(timestamp)?.getTime() ?? 0
}

export function compareSmsChronological(a: BaseSmsMessage, b: BaseSmsMessage): number {
  const diff = smsTimestampMillis(a.timestamp) - smsTimestampMillis(b.timestamp)
  if (diff !== 0) return diff
  return String(a.id).localeCompare(String(b.id))
}

export function compareSmsNewestFirst(a: BaseSmsMessage, b: BaseSmsMessage): number {
  const diff = smsTimestampMillis(b.timestamp) - smsTimestampMillis(a.timestamp)
  if (diff !== 0) return diff
  return String(b.id).localeCompare(String(a.id))
}

/**
 * 格式化详细时间（消息气泡展示）
 */
export function formatTime(timestamp: string): string {
  try {
    const date = parseSmsTimestamp(timestamp)
    if (!date) return timestamp
    const now = new Date()
    const isToday = date.toDateString() === now.toDateString()
    if (isToday) {
      return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })
    }
    const isSameYear = date.getFullYear() === now.getFullYear()
    if (isSameYear) {
      return date.toLocaleDateString('zh-CN', {
        month: '2-digit',
        day: '2-digit',
        hour: '2-digit',
        minute: '2-digit',
      })
    }
    return date.toLocaleDateString('zh-CN', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    })
  } catch {
    return timestamp
  }
}

/**
 * 格式化简短时间（会话列表展示）
 */
export function formatShortTime(timestamp: string): string {
  try {
    const date = parseSmsTimestamp(timestamp)
    if (!date) return timestamp
    const now = new Date()
    const isToday = date.toDateString() === now.toDateString()
    if (isToday) {
      return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })
    }
    return date.toLocaleDateString('zh-CN', { month: '2-digit', day: '2-digit' })
  } catch {
    return timestamp
  }
}

/**
 * 渲染搜索关键字高亮
 */
export function renderHighlightedText(text: string, query: string): ReactNode {
  const trimmedQuery = query.trim()
  if (!trimmedQuery) {
    return text
  }

  const lowerText = text.toLocaleLowerCase()
  const lowerQuery = trimmedQuery.toLocaleLowerCase()
  const nodes: ReactNode[] = []
  let cursor = 0
  let matchIndex = lowerText.indexOf(lowerQuery)

  while (matchIndex !== -1) {
    if (matchIndex > cursor) {
      nodes.push(text.slice(cursor, matchIndex))
    }
    const end = matchIndex + trimmedQuery.length
    nodes.push(
      <Box
        key={`${matchIndex}-${end}`}
        component="mark"
        sx={{
          px: 0.25,
          borderRadius: 0.5,
          bgcolor: 'primary.main',
          color: 'primary.contrastText',
        }}
      >
        {text.slice(matchIndex, end)}
      </Box>,
    )
    cursor = end
    matchIndex = lowerText.indexOf(lowerQuery, cursor)
  }

  if (cursor < text.length) {
    nodes.push(text.slice(cursor))
  }

  return nodes
}

/**
 * 3GPP 规范短信分包与字符计算
 */
export function calculateSmsSegments(text: string): {
  chars: number
  segments: number
  maxCharsPerSegment: number
} {
  const chars = text.length
  if (chars === 0) {
    return { chars: 0, segments: 0, maxCharsPerSegment: 70 }
  }

  // 检测是否包含非 GSM-7 字符（如中文、特殊符号）
  const isUnicode = /[^\u0020-\u007E\r\n]/.test(text)

  if (isUnicode) {
    // UCS-2 编码：单包 70，长短信多包时每包扣除 6 字节 UDH 头后为 67 字
    const segments = chars <= 70 ? 1 : Math.ceil(chars / 67)
    return {
      chars,
      segments,
      maxCharsPerSegment: chars <= 70 ? 70 : 67,
    }
  } else {
    // GSM 7-bit 纯英文字符：单包 160，多包 153
    const segments = chars <= 160 ? 1 : Math.ceil(chars / 153)
    return {
      chars,
      segments,
      maxCharsPerSegment: chars <= 160 ? 160 : 153,
    }
  }
}

/**
 * 短信验证码提取（轻量客户端规则嗅探）
 */
export function extractVerificationCode(text: string): string | null {
  if (!text) return null

  // 1. 验证码关键字强匹配
  const keywordRegex = /(?:验证码|校验码|动态码|随机码|确认码|安全码|code|verification\s*code)[^\d]{0,10}([0-9]{4,8})/i
  const match1 = keywordRegex.exec(text)
  if (match1?.[1]) {
    return match1[1]
  }

  // 2. 模式匹配：以冒号或空格跟数字
  const colonRegex = /(?:code|码)[：:\s是为]{1,4}([0-9]{4,8})/i
  const match2 = colonRegex.exec(text)
  if (match2?.[1]) {
    return match2[1]
  }

  // 3. 兜底匹配：前后有明确验证语境的独立 4~6 位数字
  if (/(?:登录|注册|绑定|支付|修改密码|身份验证|转账|确认|退订)/.test(text)) {
    const standaloneMatch = /(?:^|[^\d])([0-9]{4,6})(?=[^\d]|$)/.exec(text)
    if (standaloneMatch?.[1]) {
      return standaloneMatch[1]
    }
  }

  return null
}

/**
 * 搜索文本匹配（不区分大小写）
 */
export function includesSearchText(value: string, query: string): boolean {
  return value.toLocaleLowerCase().includes(query.toLocaleLowerCase())
}
