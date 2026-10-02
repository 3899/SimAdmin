import wechatQrUrl from '../../../static/Sponsor/wechat.png'
import alipayQrUrl from '../../../static/Sponsor/alipay.png'
import sponsorsData from '../../../static/Sponsor/sponsors.json'

export interface Contributor {
  name: string
  github: string
  avatar: string
  role?: string
}

export interface SponsorItem {
  name: string
  date?: string
  message?: string
}

export const CONTRIBUTORS: Contributor[] = [
  {
    name: '3899',
    github: 'https://github.com/3899',
    avatar: 'https://wsrv.nl/?url=https://github.com/3899.png&w=96&h=96&fit=cover&mask=circle',
    role: '发起人 & 维护者',
  },
  {
    name: 'crossgg',
    github: 'https://github.com/crossgg',
    avatar: 'https://wsrv.nl/?url=https://github.com/crossgg.png&w=96&h=96&fit=cover&mask=circle',
    role: '特别贡献者',
  },
  {
    name: 'enjin1314',
    github: 'https://github.com/enjin1314',
    avatar: 'https://wsrv.nl/?url=https://github.com/enjin1314.png&w=96&h=96&fit=cover&mask=circle',
    role: '代码贡献者',
  },
  {
    name: 'nkguo',
    github: 'https://github.com/nkguo',
    avatar: 'https://wsrv.nl/?url=https://github.com/nkguo.png&w=96&h=96&fit=cover&mask=circle',
    role: '贡献者',
  },
  {
    name: 'arimitx',
    github: 'https://github.com/arimitx',
    avatar: 'https://wsrv.nl/?url=https://github.com/arimitx.png&w=96&h=96&fit=cover&mask=circle',
    role: '贡献者',
  },
  {
    name: 'Espirak',
    github: 'https://github.com/Espirak',
    avatar: 'https://wsrv.nl/?url=https://github.com/Espirak.png&w=96&h=96&fit=cover&mask=circle',
    role: '贡献者',
  },
  {
    name: 'monlor',
    github: 'https://github.com/monlor',
    avatar: 'https://wsrv.nl/?url=https://github.com/monlor.png&w=96&h=96&fit=cover&mask=circle',
    role: '贡献者',
  },
  {
    name: 'small2star',
    github: 'https://github.com/small2star',
    avatar: 'https://wsrv.nl/?url=https://github.com/small2star.png&w=96&h=96&fit=cover&mask=circle',
    role: '贡献者',
  },
  {
    name: 'zzo0',
    github: 'https://github.com/zzo0',
    avatar: 'https://wsrv.nl/?url=https://github.com/zzo0.png&w=96&h=96&fit=cover&mask=circle',
    role: '贡献者',
  },
]

function parseSponsorsData(raw: unknown): SponsorItem[] {
  const sanitize = (item: Record<string, unknown>): SponsorItem => ({
    name: typeof item.name === 'string' ? item.name.trim() : '',
    date: typeof item.date === 'string' && item.date.trim() ? item.date.trim() : undefined,
    message: typeof item.message === 'string' && item.message.trim() ? item.message.trim() : undefined,
  })

  const isValid = (item: unknown): item is Record<string, unknown> =>
    Boolean(
      item &&
      typeof item === 'object' &&
      typeof (item as Record<string, unknown>).name === 'string' &&
      ((item as Record<string, unknown>).name as string).trim().length > 0,
    )

  if (Array.isArray(raw)) {
    return raw.filter(isValid).map(sanitize)
  }
  if (raw && typeof raw === 'object' && 'sponsors' in raw && Array.isArray((raw as { sponsors: unknown }).sponsors)) {
    return (raw as { sponsors: unknown[] }).sponsors.filter(isValid).map(sanitize)
  }
  return []
}

export const SPONSORS_LIST: SponsorItem[] = parseSponsorsData(sponsorsData)

export const RAW_SPONSORS_URL = 'https://raw.githubusercontent.com/3899/SimAdmin/main/static/Sponsor/sponsors.json'
export const CONTRIBUTORS_URL = 'https://github.com/3899/SimAdmin/graphs/contributors'

// 国内代理前缀逐个尝试，成功即止，直连兜底
const PROXY_PREFIX_CANDIDATES = [
  'https://gh-proxy.com/',
  'https://ghproxy.net/',
  'https://githubproxy.cc/',
  '', // 直连兜底
]

const SPONSORS_CACHE_KEY = 'simadmin_sponsors_cache'

export async function fetchLatestSponsors(): Promise<SponsorItem[]> {
  for (const proxy of PROXY_PREFIX_CANDIDATES) {
    const targetUrl = proxy ? `${proxy}${RAW_SPONSORS_URL}` : RAW_SPONSORS_URL
    try {
      const controller = new AbortController()
      const timer = setTimeout(() => controller.abort(), 2500)
      const res = await fetch(targetUrl, {
        signal: controller.signal,
        cache: 'no-cache',
      })
      clearTimeout(timer)
      if (res.ok) {
        const rawJson = await res.json()
        const list = parseSponsorsData(rawJson)
        if (Array.isArray(list)) {
          localStorage.setItem(SPONSORS_CACHE_KEY, JSON.stringify(list))
          return list
        }
      }
    } catch {
      // 当前代理超时或不可达，继续尝试下一个候选节点
      continue
    }
  }

  // 若所有网络请求均不可用，尝试读取 localStorage 本地持久化缓存
  try {
    const cached = localStorage.getItem(SPONSORS_CACHE_KEY)
    if (cached) {
      const parsed = JSON.parse(cached)
      if (Array.isArray(parsed)) return parsed as SponsorItem[]
    }
  } catch {
    // ignore
  }

  return SPONSORS_LIST
}

export const SPONSOR_QR_CODES = {
  wechat: wechatQrUrl,
  alipay: alipayQrUrl,
}
