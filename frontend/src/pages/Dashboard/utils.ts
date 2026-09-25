// Dashboard 工具函数

export const getSignalColor = (strength: number) => {
  if (strength >= 75) return 'success'
  if (strength >= 50) return 'primary'
  if (strength >= 25) return 'warning'
  return 'error'
}

export const getTempColor = (temp: number) => {
  if (temp >= 70) return 'error'
  if (temp >= 60) return 'warning'
  return 'success'
}

export const getMemoryColor = (percent: number) => {
  if (percent >= 90) return 'error'
  if (percent >= 70) return 'warning'
  return 'success'
}

export const getCpuColor = (percent: number) => {
  if (percent >= 90) return 'error'
  if (percent >= 70) return 'warning'
  return 'success'
}

export const formatBytes = (bytes: number, decimals = 1): string => {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(decimals)) + ' ' + sizes[i]
}

export const formatSpeed = (bytesPerSec: number): string => {
  if (bytesPerSec === 0) return '0 B/s'
  const k = 1024
  const sizes = ['B/s', 'KB/s', 'MB/s', 'GB/s']
  const i = Math.floor(Math.log(bytesPerSec) / Math.log(k))
  return parseFloat((bytesPerSec / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i]
}

export const convertSignalValue = (value: string | number | undefined): number | null => {
  if (value === undefined || value === null) return null
  if (typeof value === 'string') {
    const trimmed = value.trim()
    if (!trimmed) return null
    const numValue = parseFloat(trimmed)
    if (isNaN(numValue)) return null
    if (trimmed.toLowerCase().includes('db')) {
      return numValue
    }
    return numValue / 100
  }
  const numValue = value
  if (isNaN(numValue)) return null
  return numValue / 100
}

export const formatSignalValue = (value: string | number | undefined): string => {
  const converted = convertSignalValue(value)
  if (converted === null) return '-'
  return converted.toFixed(1)
}

export const getSignalChipColor = (rsrp?: string | number) => {
  const value = convertSignalValue(rsrp)
  if (value === null) return 'default'
  if (value >= -80) return 'success'
  if (value >= -100) return 'primary'
  if (value >= -110) return 'warning'
  return 'error'
}

// 根据不同功能块返回对应的敏感信息样式
export const getSensitiveStyle = (show: boolean) => ({
  filter: show ? 'none' : 'blur(5px)',
  transition: 'filter 0.3s ease',
  userSelect: show ? 'auto' as const : 'none' as const,
})

export const TEMPERATURE_DANGER_THRESHOLD = 100

export const getTempPercent = (temp: number) => {
  return Math.min(Math.max((temp / TEMPERATURE_DANGER_THRESHOLD) * 100, 0), 100)
}

export const getTempBarColor = (currentTemp: number) => {
  const clampedTemp = Math.max(0, currentTemp)
  const steppedTemp = Math.round(clampedTemp / 5) * 5

  let hue: number

  if (steppedTemp <= 50) {
    const ratio = steppedTemp / 50
    hue = Math.round(193 - (193 - 45) * ratio)
  } else if (steppedTemp <= 100) {
    const ratio = (steppedTemp - 50) / 50
    hue = Math.round(45 - 45 * ratio)
  } else {
    hue = 0
  }

  return `hsl(${hue}, 84%, 60%)`
}

export const generateHeatmapGradient = () => {
  const stops = []

  for (let percent = 0; percent <= 100; percent += 5) {
    let hue: number
    if (percent <= 50) {
      hue = Math.round(193 - (193 - 45) * (percent / 50))
    } else {
      hue = Math.round(45 - 45 * ((percent - 50) / 50))
    }
    stops.push(`hsl(${hue}, 84%, 60%) ${percent}%`)
  }

  return `linear-gradient(to right, ${stops.join(', ')})`
}

