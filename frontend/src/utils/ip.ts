import type { IpAddress } from '@/api/types'

export function publicIpv6Addresses(addresses: IpAddress[]): string[] {
  return addresses
    .filter((ip) => ip.ip_type === 'ipv6' && ip.scope === 'public')
    .sort((a, b) => Number(b.prefix_len === 128) - Number(a.prefix_len === 128))
    .map((ip) => ip.address)
}

export function publicIpv6AddressEntries(addresses: IpAddress[]): IpAddress[] {
  return addresses
    .filter((ip) => ip.ip_type === 'ipv6' && ip.scope === 'public')
    .sort((a, b) => Number(b.prefix_len === 128) - Number(a.prefix_len === 128))
}

export function isVolteOrImsInterface(name: string): boolean {
  const lower = name.toLowerCase()
  if (lower.includes('ims') || lower.includes('volte') || lower.includes('sip')) {
    return true
  }
  if (lower.includes('.')) {
    const parts = lower.split('.')
    if (
      parts.length === 2 &&
      ['wwan', 'wwp', 'rmnet', 'mbim', 'mhi'].some((p) => parts[0].startsWith(p)) &&
      /^\d+$/.test(parts[1])
    ) {
      return true
    }
  }
  const prefixes = ['rmnet_data', 'rmnet_mhi', 'rmnet', 'wwan', 'mbim']
  for (const prefix of prefixes) {
    if (lower.startsWith(prefix)) {
      const suffix = lower.slice(prefix.length)
      if (/^\d+$/.test(suffix) && Number.parseInt(suffix, 10) > 0) {
        return true
      }
    }
  }
  return false
}
