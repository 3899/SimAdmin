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

const SERVICE_HOTLINES = new Set([
  '10086', '10010', '10000', '10001', '12306', '12315', '12345',
  '95588', '95533', '95566', '95599', '95555', '95559', '95511',
])

/**
 * 短信验证码提取（轻量客户端保底规则）
 * 优先建议直接使用后端通过 simadmin-sms-core 权威提取下发的 message.verification_code。
 */
export function extractVerificationCode(rawText: string): string | null {
  if (!rawText || typeof rawText !== 'string') return null

  // 1. 全角数字转半角
  let text = rawText.replace(/[０-９]/g, (ch) => String.fromCharCode(ch.charCodeAt(0) - 65248))

  // 2. 脱敏 URL 与常见域名，避免网址中的 10086.cn、189.cn 或路径哈希干扰
  text = text.replace(/https?:\/\/[^\s，。；、]+|[a-zA-Z0-9-]+\.(?:com|cn|net|org|edu|gov|vip|cc|top)\b[^\s，。；、]*/gi, (url) =>
    ' '.repeat(url.length)
  )

  // 3. 负向规则判定：排除日期、货币、单位或业务单号
  const isFalsePositive = (numStr: string, matchIndex: number): boolean => {
    if (SERVICE_HOTLINES.has(numStr)) {
      const immediateBefore = text.slice(Math.max(0, matchIndex - 8), matchIndex)
      if (!/(?:验证码|校验码|code)/i.test(immediateBefore)) {
        return true
      }
    }

    const afterChar = text.charAt(matchIndex + numStr.length)
    if (/[年月日号时分秒点元角分块¥$]|[kKmMgGtT][bB]|[mM][bB]|次|折|%/i.test(afterChar)) {
      return true
    }

    const beforeSegment = text.slice(Math.max(0, matchIndex - 6), matchIndex)
    if (/(?:订单|运单|流水|工单|卡|尾|手机|账|日期|编号|序号|温度|湿度)[号号码值]?$/i.test(beforeSegment.trim())) {
      return true
    }

    return false
  }

  // 4.1 强模式 A：关键词在前，数字紧随其后 (0~8字符内)
  const prefixRegex =
    /(?:验证码|校验码|动态码|动态验证码|认证码|安全码|短信码|确认码|verification\s*code|verify\s*code|\bcode\b|\botp\b|\bpasscode\b|\bpin\b)[^\d\n\r]{0,8}?([0-9]{4,8}|[0-9]{3}-[0-9]{3}|G-[0-9]{6})(?!\d)/gi

  let match: RegExpExecArray | null
  while ((match = prefixRegex.exec(text)) !== null) {
    const rawCode = match[1]
    const cleanCode = rawCode.replace(/[-G]/g, '')
    const startIndex = match.index + match[0].indexOf(rawCode)
    if (!isFalsePositive(cleanCode, startIndex)) {
      return cleanCode
    }
  }

  // 4.2 强模式 B：数字在前，关键词在后紧密说明 (0~10字符内)
  const suffixRegex =
    /(?<!\d)([0-9]{4,8})[^\d\n\r]{0,10}?(?:为|是)?(?:您的|你的|本次|申请的)?(?:登录|注册|绑定|换绑|安全|身份)?(?:验证码|校验码|动态码|认证码|安全码|动态密码)/gi

  while ((match = suffixRegex.exec(text)) !== null) {
    const code = match[1]
    if (!isFalsePositive(code, match.index)) {
      return code
    }
  }

  // 4.3 强模式 C：英文固定格式
  const englishRegex =
    /(?<!\d)([0-9]{4,8})\s+(?:is\s+your\s+(?:verification\s+)?code|is\s+the\s+code)/i
  const matchEng = englishRegex.exec(text)
  if (matchEng?.[1] && !isFalsePositive(matchEng[1], matchEng.index)) {
    return matchEng[1]
  }

  return null
}

/**
 * 搜索文本匹配（不区分大小写）
 */
export function includesSearchText(value: string, query: string): boolean {
  return value.toLocaleLowerCase().includes(query.toLocaleLowerCase())
}
