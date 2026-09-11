/**
 * Modem 暂态错误识别与节流日志工具。
 *
 * 设备开机、SIM 搜网注册、Modem 初始化阶段，后端依赖 ModemManager D-Bus
 * 的接口会返回 WrongState / IncompatibleState 等预期暂态错误。
 * 这些错误不应向用户弹窗展示，但后端真正宕机、HTTP 网络故障等硬错误
 * 仍然需要被感知。
 */

/** 已知的 Modem 暂态错误关键词（全部小写匹配） */
const TRANSIENT_PATTERNS: string[] = [
  'no modemmanager modem found',
  'wrongstate',
  'incompatiblestate',
  'invalidtransition',
  'core.retry',
  'not registered',
  'no sim',
  'sim not inserted',
  'modemmanager1.error.core',
  'org.freedesktop.dbus.error',
  'the name org.freedesktop.modemmanager1 was not provided',
  'getcellinfo is unsupported',
  'no sim object found',
  'failed to find modem',
  'modem not found',
  'request timed out',
  'timed out',
  'timeout',
]

/**
 * 判断一个错误是否为 Modem 初始化/搜网阶段的预期暂态错误。
 * 这些错误在设备开机、SIM 注册过程中属于正常现象，不应向用户弹窗展示。
 */
export function isTransientModemError(error: unknown): boolean {
  if (error === undefined || error === null) return false
  const message = error instanceof Error
    ? error.message
    : typeof error === 'string'
      ? error
      : ''
  if (!message) return false
  const lower = message.toLowerCase()
  return TRANSIENT_PATTERNS.some((pattern) => lower.includes(pattern))
}

/**
 * 创建一个节流控制台警告器。
 * 相同 (label + detail) 在 intervalMs 时间窗口内只输出一次，
 * 避免 1s 轮询失败导致控制台刷屏不可用。
 */
export function createThrottledWarner(intervalMs = 10_000) {
  const lastWarned = new Map<string, number>()

  return (label: string, detail: string) => {
    const now = Date.now()
    const key = `${label}::${detail}`
    const last = lastWarned.get(key)
    if (last !== undefined && now - last < intervalMs) return

    lastWarned.set(key, now)
    console.warn(`[${label}]`, detail)

    // 避免 Map 无限增长
    if (lastWarned.size > 50) {
      for (const [k, t] of lastWarned) {
        if (now - t > intervalMs * 3) lastWarned.delete(k)
      }
    }
  }
}
