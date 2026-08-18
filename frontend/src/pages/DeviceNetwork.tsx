import { useCallback, useEffect, useMemo, useRef, useState, type ChangeEvent, type SyntheticEvent } from 'react'
import {
  Alert,
  Box,
  Button,
  Card,
  CardContent,
  CardHeader,
  Chip,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  Divider,
  FormControl,
  FormControlLabel,
  IconButton,
  InputAdornment,
  InputLabel,
  List,
  ListItem,
  ListItemButton,
  ListItemText,
  MenuItem,
  Select,
  Snackbar,
  Stack,
  Switch,
  Tab,
  Tabs,
  TextField,
  Toolbar,
  Tooltip,
  Typography,
} from '@mui/material'
import {
  CloudSync,
  Close,
  DeleteOutline,
  Dns,
  Lock,
  Public,
  Refresh,
  Save,
  Settings,
  SettingsEthernet,
  Terminal,
  Visibility,
  VisibilityOff,
  Wifi,
  WifiOff,
} from '@mui/icons-material'
import { type DdnsConfig, type DdnsLogEntry, type DdnsStatusResponse, type NetworkInterfaceInfo, type WlanNetwork, type WlanSavedNetwork, type WlanStatusResponse } from '../api/current'
import { useSimAdminApi } from '../contexts/ApiContext'
import ErrorSnackbar from '../components/ErrorSnackbar'
import { useRefreshInterval } from '../contexts/RefreshContext'
import { publicIpv6AddressEntries } from '@/utils/ip'

const CONFIRM_CLOSE_WLAN = '确认关闭 WLAN'
const CARD_TITLE_TYPOGRAPHY = { variant: 'subtitle1', fontWeight: 700 } as const

function defaultDdnsConfig(): DdnsConfig {
  return {
    enabled: false,
    provider: 'tencentcloud',
    access_id: '',
    access_secret: '',
    interval_seconds: 300,
    ttl: 600,
    ipv4: {
      enabled: true,
      get_type: 'interface',
      interface_name: '',
      urls: ['https://api.ipify.org', 'https://ip.3322.net', 'https://4.ident.me', 'https://ddns.oray.com/checkip', 'https://4.ipw.cn'],
      domains: [],
    },
    ipv6: {
      enabled: false,
      get_type: 'interface',
      interface_name: '',
      urls: ['https://api6.ipify.org', 'https://speed.neu6.edu.cn/getIP.php', 'https://v6.ident.me', 'https://myip6.ipip.net', 'https://6.ipw.cn'],
      domains: [],
    },
  }
}

interface TabPanelProps {
  children?: React.ReactNode
  index: number
  value: number
}

function TabPanel({ children, value, index }: TabPanelProps) {
  return (
    <div role="tabpanel" hidden={value !== index}>
      {value === index && children}
    </div>
  )
}

function textToLines(value: string) {
  return value
    .split('\n')
    .map((item) => item.trim())
    .filter(Boolean)
}

function linesToText(value: string[]) {
  return value.join('\n')
}

function hasText(value: string) {
  return value.trim().length > 0
}

function providerName(provider: string) {
  switch (provider) {
    case 'alidns':
      return '阿里云 AliDNS'
    case 'tencentcloud':
    case 'dnspod':
    case 'tencent':
      return '腾讯云 DNSPod'
    case 'cloudflare':
      return 'Cloudflare'
    default:
      return provider
  }
}

function signalLabel(signal: number) {
  if (signal >= 75) return '强'
  if (signal >= 45) return '中'
  return '弱'
}

function RuntimeStatusDot({ active }: { active: boolean }) {
  return (
    <Box sx={{ position: 'relative', width: 12, height: 12, flex: '0 0 auto' }}>
      <Box
        sx={{
          position: 'absolute',
          inset: 0,
          borderRadius: '50%',
          bgcolor: active ? 'success.main' : 'error.main',
          opacity: 0.3,
          animation: active ? 'pulse 1.8s infinite' : 'none',
          '@keyframes pulse': {
            '0%': { transform: 'scale(1)', opacity: 0.45 },
            '70%': { transform: 'scale(2.1)', opacity: 0 },
            '100%': { transform: 'scale(2.1)', opacity: 0 },
          },
        }}
      />
      <Box
        sx={{
          position: 'absolute',
          inset: 2,
          borderRadius: '50%',
          bgcolor: active ? 'success.main' : 'error.main',
        }}
      />
    </Box>
  )
}

function mergeDdnsConfig(config?: DdnsConfig): DdnsConfig {
  const defaults = defaultDdnsConfig()
  if (!config) return defaults
  return {
    ...defaults,
    ...config,
    ipv4: {
      ...defaults.ipv4,
      ...config.ipv4,
      urls: config.ipv4.urls.length > 0 ? config.ipv4.urls : defaults.ipv4.urls,
    },
    ipv6: {
      ...defaults.ipv6,
      ...config.ipv6,
      urls: config.ipv6.urls.length > 0 ? config.ipv6.urls : defaults.ipv6.urls,
    },
  }
}

function isDdnsIpConfigComplete(config: DdnsConfig['ipv4']) {
  if (!config.enabled) return true
  if (config.domains.length === 0) return false
  if (config.get_type === 'interface') return hasText(config.interface_name)
  return config.urls.length > 0
}

function isDdnsConfigComplete(config: DdnsConfig, accessSecretConfigured: boolean) {
  const providerComplete = hasText(config.access_id)
    && (hasText(config.access_secret) || accessSecretConfigured)
    && config.interval_seconds > 0
    && config.ttl > 0
  const hasEnabledRecord = config.ipv4.enabled || config.ipv6.enabled
  return providerComplete
    && hasEnabledRecord
    && isDdnsIpConfigComplete(config.ipv4)
    && isDdnsIpConfigComplete(config.ipv6)
}

function interfaceAddressesForFamily(iface: NetworkInterfaceInfo, family: 'ipv4' | 'ipv6') {
  if (family === 'ipv6') {
    return publicIpv6AddressEntries(iface.ip_addresses)
  }

  return iface.ip_addresses.filter((addr) => {
    if (addr.ip_type !== family) return false
    return addr.scope !== 'loopback' && addr.scope !== 'link-local'
  })
}

function ddnsInterfacePriority(iface: NetworkInterfaceInfo, family: 'ipv4' | 'ipv6') {
  const isDefault = family === 'ipv4' ? iface.is_default_ipv4 : iface.is_default_ipv6
  if (iface.is_wireless && isDefault) return 0
  if (isDefault) return 1
  if (iface.is_wireless) return 2
  if (iface.is_cellular) return 3
  if (iface.status.toLowerCase() === 'up') return 4
  return 5
}

function ddnsInterfaceOptions(interfaces: NetworkInterfaceInfo[], family: 'ipv4' | 'ipv6') {
  return interfaces
    .filter((iface) => iface.name !== 'lo' && interfaceAddressesForFamily(iface, family).length > 0)
    .sort((a, b) => {
      const priorityDiff = ddnsInterfacePriority(a, family) - ddnsInterfacePriority(b, family)
      if (priorityDiff !== 0) return priorityDiff
      return a.name.localeCompare(b.name)
    })
}

function interfaceOptionLabel(iface: NetworkInterfaceInfo, family: 'ipv4' | 'ipv6') {
  const addresses = interfaceAddressesForFamily(iface, family).map((addr) => addr.address)
  return addresses.length > 0 ? `${iface.name}(${addresses.join(', ')})` : iface.name
}

function applyDefaultDdnsInterfaces(config: DdnsConfig, interfaces: NetworkInterfaceInfo[]): DdnsConfig {
  const ipv4Options = ddnsInterfaceOptions(interfaces, 'ipv4')
  const ipv6Options = ddnsInterfaceOptions(interfaces, 'ipv6')
  return {
    ...config,
    ipv4: {
      ...config.ipv4,
      interface_name: config.ipv4.get_type === 'interface'
        ? config.ipv4.interface_name || ipv4Options[0]?.name || ''
        : config.ipv4.interface_name,
    },
    ipv6: {
      ...config.ipv6,
      interface_name: config.ipv6.get_type === 'interface'
        ? config.ipv6.interface_name || ipv6Options[0]?.name || ''
        : config.ipv6.interface_name,
    },
  }
}

function dedupeNetworksBySsid(networks: WlanNetwork[]) {
  const bySsid = new Map<string, WlanNetwork>()
  networks.forEach((network) => {
    const existing = bySsid.get(network.ssid)
    if (!existing || network.connected || network.signal > existing.signal) {
      bySsid.set(network.ssid, network)
    }
  })
  return Array.from(bySsid.values())
}

function wlanNetworkSecondary(network?: WlanNetwork) {
  if (!network) return '已保存'
  return `${signalLabel(network.signal)} · ${network.signal}% · ${network.security || '开放网络'}`
}

function WlanSignalIcon({ signal }: { signal?: number }) {
  const activeLevels = signal === undefined ? 0 : signal >= 75 ? 3 : signal >= 45 ? 2 : signal > 0 ? 1 : 0
  const paths = [
    'M0 352.832l93.12 98.752c231.296-245.44 606.464-245.44 837.76 0L1024 352.832C741.44 53.056 283.008 53.056 0 352.832z',
    'M186.24 550.4l93.12 98.752c128.448-136.32 336.96-136.32 465.408 0L837.824 550.4c-179.648-190.592-471.488-190.592-651.648 0z',
    'M372.352 747.84L512 896l139.648-148.16c-76.8-81.92-202.048-81.92-279.296 0z',
  ]

  return (
    <Box
      component="svg"
      viewBox="0 0 1024 1024"
      aria-hidden
      sx={{
        display: 'block',
        width: 22,
        height: 22,
        color: signal === undefined ? 'text.disabled' : 'text.primary',
      }}
    >
      {paths.map((path, index) => (
        <Box
          key={path}
          component="path"
          d={path}
          sx={{
            fill: index >= paths.length - activeLevels
              ? 'currentColor'
              : (theme) => theme.palette.mode === 'light' ? '#d8d8d8' : 'rgba(148, 163, 184, 0.34)',
          }}
        />
      ))}
    </Box>
  )
}

function formatDdnsLogTimestamp(timestamp: string) {
  if (!timestamp.includes('T') && !timestamp.endsWith('Z')) return timestamp
  const date = new Date(timestamp)
  if (Number.isNaN(date.getTime())) return timestamp
  const parts = new Intl.DateTimeFormat('zh-CN', {
    timeZone: 'Asia/Shanghai',
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  }).formatToParts(date)
  const get = (type: string) => parts.find((part) => part.type === type)?.value ?? ''
  return `${get('year')}-${get('month')}-${get('day')} ${get('hour')}:${get('minute')}:${get('second')}`
}

function translateDdnsLogMessage(message: string) {
  if (message === 'DDNS is disabled') return 'DDNS 已停用'
  if (message === 'DDNS sync is already running') return 'DDNS 正在同步，请稍后再试'
  if (message === 'DDNS disabled or no enabled records') return 'DDNS 已停用或没有启用的解析记录'

  const noDomains = message.match(/^(A|AAAA) has no domains configured$/)
  if (noDomains) return `${noDomains[1]} 记录未配置解析域名`

  const getIpFailed = message.match(/^Failed to get (A|AAAA) address: (.+)$/)
  if (getIpFailed) return `获取 ${getIpFailed[1]} 地址失败：${getIpFailed[2]}`

  const noAddress = message.match(/^no (A|AAAA) address found on (.+)$/)
  if (noAddress) return `网卡 ${noAddress[2]} 未找到 ${noAddress[1]} 地址`

  const ipSource = message.match(/^unsupported IP source: (.+)$/)
  if (ipSource) return `不支持的 IP 获取方式：${ipSource[1]}`

  const provider = message.match(/^unsupported DDNS provider: (.+)$/)
  if (provider) return `不支持的 DDNS 服务商：${provider[1]}`

  const notifyFailed = message.match(/^DDNS notification failed: (.+)$/)
  if (notifyFailed) return `DDNS 通知发送失败：${notifyFailed[1]}`

  const providerUpdated = message.match(/^(Cloudflare|AliDNS|Tencent Cloud|DNSPod) (A|AAAA) records updated to (.+)$/)
  if (providerUpdated) return `${providerUpdated[1]} ${providerUpdated[2]} 记录已更新为 ${providerUpdated[3]}`

  const providerUnchanged = message.match(/^(Cloudflare|AliDNS|Tencent Cloud|DNSPod) (A|AAAA) records unchanged$/)
  if (providerUnchanged) return `${providerUnchanged[1]} ${providerUnchanged[2]} 记录无变化`

  const requestFailed = message.match(/^(Cloudflare|AliDNS|Tencent Cloud|DNSPod) (.+)$/)
  if (requestFailed) return `${requestFailed[1]} 请求失败：${requestFailed[2]}`

  return message
}

function prefixToSubnetMask(prefix: number) {
  const normalized = Math.min(32, Math.max(0, Math.floor(prefix)))
  const mask = normalized === 0 ? 0 : (0xffffffff << (32 - normalized)) >>> 0
  return [24, 16, 8, 0].map((shift) => String((mask >>> shift) & 255)).join('.')
}

function subnetMaskToPrefix(mask: string) {
  const parts = mask.split('.').map((part) => Number(part.trim()))
  if (parts.length !== 4 || parts.some((part) => !Number.isInteger(part) || part < 0 || part > 255)) {
    return null
  }
  const bits = parts.map((part) => part.toString(2).padStart(8, '0')).join('')
  if (!/^1*0*$/.test(bits)) return null
  return bits.indexOf('0') === -1 ? 32 : bits.indexOf('0')
}

function isPublicIpv6Address(address: string) {
  const normalized = address.split('/')[0].split('%')[0].trim().toLowerCase()
  if (!normalized || normalized === '::1' || normalized === '::') return false
  if (normalized.startsWith('fe80:')) return false
  if (/^f[cd][0-9a-f]{0,2}:/.test(normalized)) return false
  if (/^ff[0-9a-f]{0,2}:/.test(normalized)) return false
  return normalized.includes(':')
}

export default function DeviceNetworkPage() {
  const api = useSimAdminApi()
  const { refreshInterval, refreshKey } = useRefreshInterval()
  const [tabValue, setTabValue] = useState(0)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState<string | null>(null)
  const wlanScanInFlightRef = useRef(false)

  const [ddnsConfig, setDdnsConfig] = useState<DdnsConfig>(() => defaultDdnsConfig())
  const [savedDdnsConfig, setSavedDdnsConfig] = useState<DdnsConfig>(() => defaultDdnsConfig())
  const [ddnsAccessSecretConfigured, setDdnsAccessSecretConfigured] = useState(false)
  const [ddnsStatus, setDdnsStatus] = useState<DdnsStatusResponse | null>(null)
  const [ddnsLogs, setDdnsLogs] = useState<DdnsLogEntry[]>([])
  const [ddnsSaving, setDdnsSaving] = useState(false)
  const [ddnsSyncing, setDdnsSyncing] = useState(false)
  const [ddnsToggling, setDdnsToggling] = useState(false)
  const [ddnsDirty, setDdnsDirty] = useState(false)
  const [networkInterfaces, setNetworkInterfaces] = useState<NetworkInterfaceInfo[]>([])

  const [wlanStatus, setWlanStatus] = useState<WlanStatusResponse | null>(null)
  const [dataActive, setDataActive] = useState(false)
  const [networks, setNetworks] = useState<WlanNetwork[]>([])
  const [savedNetworks, setSavedNetworks] = useState<WlanSavedNetwork[]>([])
  const [wlanScanning, setWlanScanning] = useState(false)
  const [wlanSwitching, setWlanSwitching] = useState(false)
  const [forgettingNetwork, setForgettingNetwork] = useState<string | null>(null)
  const [selectedNetwork, setSelectedNetwork] = useState<WlanNetwork | null>(null)
  const [wifiPassword, setWifiPassword] = useState('')
  const [showWifiPassword, setShowWifiPassword] = useState(false)
  const [connectOpen, setConnectOpen] = useState(false)
  const [disconnecting, setDisconnecting] = useState(false)
  const [closeConfirmOpen, setCloseConfirmOpen] = useState(false)
  const [closeConfirmText, setCloseConfirmText] = useState('')
  const [clearDdnsLogsOpen, setClearDdnsLogsOpen] = useState(false)
  const [profileAutoJoin, setProfileAutoJoin] = useState(true)
  const [profileIpv4Mode, setProfileIpv4Mode] = useState<'dhcp' | 'manual'>('dhcp')
  const [profileIpv4Address, setProfileIpv4Address] = useState('')
  const [profileIpv4Prefix, setProfileIpv4Prefix] = useState(24)
  const [profileIpv4Mask, setProfileIpv4Mask] = useState(prefixToSubnetMask(24))
  const [profileIpv4Gateway, setProfileIpv4Gateway] = useState('')
  const [profileSaving, setProfileSaving] = useState(false)
  const [wlanProfileOpen, setWlanProfileOpen] = useState(false)

  const loadAll = async () => {
    setLoading(true)
    setError(null)
    setWlanProfileOpen(false)
    try {
      const [ddnsConfigRes, ddnsStatusRes, ddnsLogsRes, wlanStatusRes, wlanProfilesRes, dataRes, interfacesRes] = await Promise.allSettled([
        api.getDdnsConfig(),
        api.getDdnsStatus(),
        api.getDdnsLogs(),
        api.getWlanStatus(),
        api.getWlanProfiles(),
        api.getDataStatus(),
        api.getNetworkInterfaces(),
      ])
      const loadedInterfaces = interfacesRes.status === 'fulfilled' ? interfacesRes.value.data?.interfaces ?? [] : []
      setNetworkInterfaces(loadedInterfaces)
      if (ddnsConfigRes.status === 'fulfilled' && ddnsConfigRes.value.data) {
        const loadedDdnsConfig = applyDefaultDdnsInterfaces(mergeDdnsConfig(ddnsConfigRes.value.data), loadedInterfaces)
        setDdnsConfig(loadedDdnsConfig)
        setSavedDdnsConfig(loadedDdnsConfig)
        setDdnsAccessSecretConfigured(Boolean(ddnsConfigRes.value.data.access_secret_set || ddnsConfigRes.value.data.access_secret))
      } else {
        const fallbackDdnsConfig = applyDefaultDdnsInterfaces(defaultDdnsConfig(), loadedInterfaces)
        setDdnsConfig(fallbackDdnsConfig)
        setSavedDdnsConfig(fallbackDdnsConfig)
        setDdnsAccessSecretConfigured(false)
      }
      if (ddnsStatusRes.status === 'fulfilled' && ddnsStatusRes.value.data) {
        setDdnsStatus(ddnsStatusRes.value.data)
      }
      if (ddnsLogsRes.status === 'fulfilled' && ddnsLogsRes.value.data) {
        setDdnsLogs(ddnsLogsRes.value.data.entries)
      }
      if (wlanStatusRes.status === 'fulfilled' && wlanStatusRes.value.data) {
        applyWlanStatus(wlanStatusRes.value.data)
      }
      if (wlanProfilesRes.status === 'fulfilled' && wlanProfilesRes.value.data) {
        setSavedNetworks(wlanProfilesRes.value.data.profiles)
      }
      if (dataRes.status === 'fulfilled' && dataRes.value.data) {
        setDataActive(dataRes.value.data.active)
      }
      setDdnsDirty(false)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    void loadAll()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    if (tabValue !== 1) return undefined

    let cancelled = false
    const loadDdnsRuntime = async () => {
      try {
        const [statusRes, logsRes] = await Promise.all([api.getDdnsStatus(), api.getDdnsLogs()])
        if (cancelled) return
        if (statusRes.data) setDdnsStatus(statusRes.data)
        if (logsRes.data) setDdnsLogs(logsRes.data.entries)
      } catch {
        // Runtime status refresh is best-effort; user actions still surface errors.
      }
    }

    void loadDdnsRuntime()
    if (refreshInterval <= 0) return () => {
      cancelled = true
    }

    const timer = window.setInterval(() => void loadDdnsRuntime(), refreshInterval)
    return () => {
      cancelled = true
      window.clearInterval(timer)
    }
  }, [api, refreshInterval, refreshKey, tabValue])

  const applyWlanStatus = (status: WlanStatusResponse) => {
    setWlanStatus(status)
    setProfileAutoJoin(true)
    setProfileIpv4Mode('dhcp')
    setProfileIpv4Address(status.ipv4_addresses[0]?.split('/')[0] ?? '')
    const prefix = Number(status.ipv4_addresses[0]?.split('/')[1] ?? 24)
    const normalizedPrefix = Number.isFinite(prefix) ? prefix : 24
    setProfileIpv4Prefix(normalizedPrefix)
    setProfileIpv4Mask(prefixToSubnetMask(normalizedPrefix))
    setProfileIpv4Gateway(status.ipv4_gateway ?? '')
    if (!status.connected) setWlanProfileOpen(false)
  }

  const refreshWlanProfiles = useCallback(async () => {
    try {
      const response = await api.getWlanProfiles()
      setSavedNetworks(response.data?.profiles ?? [])
    } catch {
      setSavedNetworks([])
    }
  }, [api])

  const handleTabChange = (_event: SyntheticEvent, value: number) => setTabValue(value)

  const patchDdns = (patch: Partial<DdnsConfig>) => {
    setDdnsDirty(true)
    setDdnsConfig((prev) => ({ ...prev, ...patch }))
  }

  const patchDdnsIp = (key: 'ipv4' | 'ipv6', patch: Partial<DdnsConfig['ipv4']>) => {
    setDdnsDirty(true)
    setDdnsConfig((prev) => ({
      ...prev,
      [key]: {
        ...prev[key],
        ...patch,
      },
    }))
  }

  const handleDdnsIpGetTypeChange = (key: 'ipv4' | 'ipv6', getType: DdnsConfig['ipv4']['get_type']) => {
    if (getType === 'interface' && !ddnsConfig[key].interface_name) {
      const defaultInterface = ddnsInterfaceOptions(networkInterfaces, key)[0]?.name ?? ''
      patchDdnsIp(key, { get_type: getType, interface_name: defaultInterface })
      return
    }
    patchDdnsIp(key, { get_type: getType })
  }

  const setDdnsEnabled = async (enabled: boolean) => {
    const previousEnabled = ddnsConfig.enabled
    setDdnsConfig((prev) => ({ ...prev, enabled }))
    setDdnsToggling(true)
    setError(null)
    try {
      const response = await api.setDdnsConfig({ ...savedDdnsConfig, enabled })
      if (response.data) {
        const nextSavedConfig = mergeDdnsConfig(response.data)
        setSavedDdnsConfig(nextSavedConfig)
        setDdnsConfig((prev) => ({ ...prev, enabled: nextSavedConfig.enabled }))
        setDdnsStatus((prev) => (prev ? { ...prev, enabled: nextSavedConfig.enabled, running: nextSavedConfig.enabled ? prev.running : false } : prev))
        setDdnsAccessSecretConfigured(Boolean(response.data.access_secret_set || ddnsAccessSecretConfigured))
      }
    } catch (err) {
      setDdnsConfig((prev) => ({ ...prev, enabled: previousEnabled }))
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setDdnsToggling(false)
    }
  }

  const saveDdnsConfig = async () => {
    setDdnsSaving(true)
    setError(null)
    try {
      const response = await api.setDdnsConfig({ ...ddnsConfig, enabled: savedDdnsConfig.enabled })
      if (response.data) {
        const nextSavedConfig = mergeDdnsConfig(response.data)
        setSavedDdnsConfig(nextSavedConfig)
        setDdnsConfig(nextSavedConfig)
        setDdnsStatus((prev) => (prev ? { ...prev, enabled: nextSavedConfig.enabled, running: false } : prev))
        setDdnsAccessSecretConfigured(Boolean(response.data.access_secret_set || ddnsConfig.access_secret))
      }
      setDdnsDirty(false)
      setSuccess('DDNS 配置已保存')
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setDdnsSaving(false)
    }
  }

  const syncDdns = async () => {
    setDdnsSyncing(true)
    setError(null)
    try {
      const response = await api.syncDdnsNow()
      const messages = response.data?.records.map((record) => record.message).join('；') || 'DDNS 同步完成'
      setSuccess(messages)
      const [statusRes, logsRes] = await Promise.all([api.getDdnsStatus(), api.getDdnsLogs()])
      if (statusRes.data) setDdnsStatus(statusRes.data)
      if (logsRes.data) setDdnsLogs(logsRes.data.entries)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setDdnsSyncing(false)
    }
  }

  const clearDdnsLogs = async () => {
    setError(null)
    try {
      await api.clearDdnsLogs()
      setDdnsLogs([])
      setClearDdnsLogsOpen(false)
      setSuccess('DDNS 运行日志已清空')
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const scanWlan = useCallback(async () => {
    if (wlanScanInFlightRef.current) return
    wlanScanInFlightRef.current = true
    setWlanScanning(true)
    setError(null)
    try {
      const response = await api.scanWlan()
      setNetworks(response.data?.networks ?? [])
      void refreshWlanProfiles()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      wlanScanInFlightRef.current = false
      setWlanScanning(false)
    }
  }, [api, refreshWlanProfiles])

  useEffect(() => {
    if (tabValue !== 0 || !wlanStatus?.enabled) return
    void scanWlan()
  }, [scanWlan, tabValue, wlanStatus?.enabled])

  const handleWlanToggle = (event: ChangeEvent<HTMLInputElement>) => {
    if (event.target.checked) {
      void enableWlan()
    } else {
      setCloseConfirmText('')
      setCloseConfirmOpen(true)
    }
  }

  const enableWlan = async () => {
    setWlanSwitching(true)
    setError(null)
    setWlanProfileOpen(false)
    try {
      const response = await api.setWlanEnabled(true)
      if (response.data) applyWlanStatus(response.data)
      setSuccess('WLAN 已开启')
      void refreshWlanProfiles()
      void scanWlan()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setWlanSwitching(false)
    }
  }

  const confirmDisableWlan = async () => {
    setWlanSwitching(true)
    setError(null)
    setWlanProfileOpen(false)
    try {
      const response = await api.setWlanEnabled(false)
      if (response.data) applyWlanStatus(response.data)
      setNetworks([])
      setSavedNetworks([])
      setCloseConfirmOpen(false)
      setSuccess('WLAN 已关闭')
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setWlanSwitching(false)
    }
  }

  const openConnectDialog = (network: WlanNetwork) => {
    if (network.connected) return
    setSelectedNetwork(network)
    setWifiPassword('')
    setShowWifiPassword(false)
    if (network.secure) {
      setConnectOpen(true)
    } else {
      void connectWlan(network, '')
    }
  }

  const connectWlan = async (network = selectedNetwork, password = wifiPassword) => {
    if (!network) return
    setError(null)
    setWlanProfileOpen(false)
    try {
      const response = await api.connectWlan({
        ssid: network.ssid,
        password,
        auto_join: true,
      })
      if (response.data) applyWlanStatus(response.data)
      setConnectOpen(false)
      setSuccess(`已连接 ${network.ssid}`)
      void refreshWlanProfiles()
      void scanWlan()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const connectSavedWlan = async (profile: WlanSavedNetwork) => {
    const ssid = profile.ssid || profile.id
    if (!ssid) return
    setError(null)
    setWlanProfileOpen(false)
    try {
      const response = await api.connectWlan({
        ssid,
        auto_join: profile.auto_join,
      })
      if (response.data) applyWlanStatus(response.data)
      setSuccess(`已连接 ${ssid}`)
      void refreshWlanProfiles()
      void scanWlan()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const disconnectWlan = async () => {
    setDisconnecting(true)
    setError(null)
    try {
      const response = await api.disconnectWlan()
      if (response.data) applyWlanStatus(response.data)
      setSuccess('WLAN 已断开')
      void refreshWlanProfiles()
      void scanWlan()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setDisconnecting(false)
    }
  }

  const saveWlanProfile = async () => {
    if (!wlanStatus?.connection_id) return
    const manualPrefix = subnetMaskToPrefix(profileIpv4Mask)
    setProfileSaving(true)
    setError(null)
    try {
      const response = await api.saveWlanProfile({
        connection_id: wlanStatus.connection_id,
        auto_join: profileAutoJoin,
        ipv4_mode: profileIpv4Mode,
        ipv4_address: profileIpv4Mode === 'manual' ? profileIpv4Address : undefined,
        ipv4_prefix: profileIpv4Mode === 'manual' ? manualPrefix ?? profileIpv4Prefix : undefined,
        ipv4_gateway: profileIpv4Mode === 'manual' ? profileIpv4Gateway : undefined,
      })
      if (response.data) applyWlanStatus(response.data)
      void refreshWlanProfiles()
      setSuccess('WLAN 配置已保存')
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setProfileSaving(false)
    }
  }

  const forgetWlanProfile = async (profile: WlanSavedNetwork) => {
    setForgettingNetwork(profile.uuid)
    setError(null)
    try {
      const response = await api.forgetWlan({ uuid: profile.uuid, connection_id: profile.id })
      setSavedNetworks(response.data?.profiles ?? [])
      const status = await api.getWlanStatus()
      if (status.data) applyWlanStatus(status.data)
      setSuccess(`已忘记 ${profile.ssid || profile.id}`)
      if (wlanStatus?.enabled) void scanWlan()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setForgettingNetwork(null)
    }
  }

  const closeWlanPrompt = dataActive
    ? '蜂窝网络已开启，若继续关闭 WLAN 后可依托蜂窝 + DDNS 正常远程管理设备。请输入「确认关闭 WLAN」继续操作。'
    : '蜂窝网络已关闭，若继续关闭 WLAN 后将无法远程管理设备，仅可通过 ADB 或本地恢复。请输入「确认关闭 WLAN」继续操作。'

  const ddnsProviderHelp = useMemo(() => {
    if (ddnsConfig.provider === 'cloudflare') return 'Cloudflare：ID 填写 Zone ID，Token 填写 API Token。'
    if (ddnsConfig.provider === 'alidns') return '阿里云：ID 填写 AccessKey ID，Token 填写 AccessKey Secret。'
    return '腾讯云 DNSPod：支持 DNSPod ID/Token；如使用腾讯云 API 密钥，ID 填写 SecretId，Token 填写 SecretKey。'
  }, [ddnsConfig.provider])

  const ddnsConfigComplete = useMemo(
    () => isDdnsConfigComplete(ddnsConfig, ddnsAccessSecretConfigured),
    [ddnsAccessSecretConfigured, ddnsConfig],
  )
  const ddnsControlsDisabled = !ddnsConfig.enabled

  const ddnsRuntimeStatus = useMemo(() => {
    if (!ddnsConfig.enabled) {
      return { label: '已停用', color: '#64748b' }
    }
    if (ddnsSyncing || ddnsStatus?.running) {
      return { label: '同步中', color: '#f59e0b' }
    }
    if (ddnsDirty) {
      return { label: '待保存', color: '#3b82f6' }
    }
    if (!ddnsConfigComplete) {
      return { label: '待配置', color: '#f59e0b' }
    }
    return { label: '监听中', color: '#10b981' }
  }, [ddnsConfig.enabled, ddnsConfigComplete, ddnsDirty, ddnsStatus?.running, ddnsSyncing])
  const latestDdnsLog = ddnsLogs[ddnsLogs.length - 1]

  const visibleNetworks = useMemo(() => dedupeNetworksBySsid(networks), [networks])
  const connectedSsid = wlanStatus?.connected
    ? wlanStatus.ssid || visibleNetworks.find((network) => network.connected)?.ssid
    : undefined
  const connectedScanNetwork = connectedSsid
    ? visibleNetworks.find((network) => network.ssid === connectedSsid) ?? visibleNetworks.find((network) => network.connected)
    : undefined
  const connectedSavedNetwork = connectedSsid
    ? savedNetworks.find((profile) => profile.active || profile.ssid === connectedSsid || profile.id === connectedSsid)
    : savedNetworks.find((profile) => profile.active)
  const savedNetworkRows = savedNetworks.filter((profile) => profile.uuid !== connectedSavedNetwork?.uuid)
  const savedSsidSet = new Set(savedNetworks.flatMap((profile) => [profile.ssid, profile.id].filter(Boolean)))
  const otherNetworkRows = visibleNetworks.filter((network) => network.ssid !== connectedSsid && !savedSsidSet.has(network.ssid))
  const wlanDetailOpen = Boolean(wlanProfileOpen && wlanStatus?.connected)
  const wlanDetailStatus = wlanDetailOpen ? wlanStatus : null
  const wlanDetailPublicIpv6Addresses = wlanDetailStatus?.ipv6_addresses.filter(isPublicIpv6Address) ?? []

  const renderNetworkIndicators = (network?: WlanNetwork, showProperties = false) => (
    <Box display="flex" alignItems="center" justifyContent="flex-end" gap={1} sx={{ minWidth: { xs: 72, sm: showProperties ? 112 : 72 } }}>
      {network?.secure ? <Lock fontSize="small" color="disabled" /> : <Box sx={{ width: 20 }} />}
      {network ? (
        <Tooltip title={`信号 ${network.signal}%`}>
          <Box component="span" display="inline-flex" alignItems="center">
            <WlanSignalIcon signal={network.signal} />
          </Box>
        </Tooltip>
      ) : (
        <WlanSignalIcon />
      )}
      {showProperties && (
        <Tooltip title="属性">
          <IconButton size="small" onClick={() => setWlanProfileOpen(true)} disabled={!wlanStatus?.connected}>
            <Settings fontSize="small" />
          </IconButton>
        </Tooltip>
      )}
    </Box>
  )

  const renderWlanActionDivider = () => (
    <Box
      aria-hidden
      sx={{
        width: 1,
        height: 18,
        ml: 0.5,
        mr: 0.25,
        bgcolor: 'divider',
        opacity: 0.55,
      }}
    />
  )

  const renderCurrentNetworkActions = () => (
    <Box
      className="wlan-row-actions"
      display="flex"
      alignItems="center"
      gap={0.5}
      sx={{
        opacity: 0,
        pointerEvents: 'none',
        transition: 'opacity 120ms ease',
      }}
    >
      {connectedSavedNetwork && (
        <Tooltip title="删除">
          <span>
            <IconButton
              size="small"
              color="error"
              onClick={() => void forgetWlanProfile(connectedSavedNetwork)}
              disabled={forgettingNetwork === connectedSavedNetwork.uuid}
            >
              {forgettingNetwork === connectedSavedNetwork.uuid ? <CircularProgress size={16} /> : <DeleteOutline fontSize="small" />}
            </IconButton>
          </span>
        </Tooltip>
      )}
      <Tooltip title="断开">
        <span>
          <IconButton size="small" color="error" onClick={() => void disconnectWlan()} disabled={disconnecting}>
            <WifiOff fontSize="small" />
          </IconButton>
        </span>
      </Tooltip>
      {renderWlanActionDivider()}
    </Box>
  )

  const renderDdnsIpCard = (key: 'ipv4' | 'ipv6') => {
    const item = ddnsConfig[key]
    const recordLabel = key === 'ipv4' ? 'IPv4' : 'IPv6'
    const interfaceOptions = ddnsInterfaceOptions(networkInterfaces, key)
    const selectedInterfaceMissing = Boolean(
      item.interface_name && !interfaceOptions.some((iface) => iface.name === item.interface_name),
    )
    const cardDisabled = ddnsControlsDisabled
    const fieldDisabled = cardDisabled || !item.enabled
    return (
      <Card sx={{ opacity: cardDisabled ? 0.55 : 1 }}>
        <CardHeader
          avatar={<Public color={key === 'ipv4' ? 'primary' : 'secondary'} />}
          title={`${recordLabel} 解析配置`}
          titleTypographyProps={CARD_TITLE_TYPOGRAPHY}
          action={
            <Switch
              checked={item.enabled}
              onChange={(event: ChangeEvent<HTMLInputElement>) => patchDdnsIp(key, { enabled: event.target.checked })}
              disabled={cardDisabled}
            />
          }
        />
        <CardContent>
          <Stack spacing={2}>
            <FormControl fullWidth disabled={fieldDisabled}>
              <InputLabel>获取 IP 方式</InputLabel>
              <Select
                value={item.get_type}
                label="获取 IP 方式"
                onChange={(event) => handleDdnsIpGetTypeChange(key, event.target.value as DdnsConfig['ipv4']['get_type'])}
              >
                <MenuItem value="api">接口获取 (API)</MenuItem>
                <MenuItem value="interface">网卡获取 (NIC)</MenuItem>
              </Select>
            </FormControl>
            {item.get_type === 'interface' ? (
              <FormControl fullWidth disabled={fieldDisabled}>
                <InputLabel>网卡</InputLabel>
                <Select
                  value={item.interface_name}
                  label="网卡"
                  onChange={(event) => patchDdnsIp(key, { interface_name: event.target.value })}
                >
                  {selectedInterfaceMissing && (
                    <MenuItem value={item.interface_name}>
                      {item.interface_name}(当前配置，未检测到匹配 IP)
                    </MenuItem>
                  )}
                  {interfaceOptions.map((iface) => (
                    <MenuItem key={`${key}-${iface.name}`} value={iface.name}>
                      {interfaceOptionLabel(iface, key)}
                    </MenuItem>
                  ))}
                  {interfaceOptions.length === 0 && !selectedInterfaceMissing && (
                    <MenuItem value="">未发现可用网卡</MenuItem>
                  )}
                </Select>
              </FormControl>
            ) : (
              <TextField
                label="获取 IP 接口"
                value={linesToText(item.urls)}
                onChange={(event: ChangeEvent<HTMLInputElement>) => patchDdnsIp(key, { urls: textToLines(event.target.value) })}
                disabled={fieldDisabled}
                multiline
                minRows={2}
              />
            )}
            <TextField
              label="解析域名，一行一个"
              placeholder="每行一个域名，例如 home.example.com"
              value={linesToText(item.domains)}
              onChange={(event: ChangeEvent<HTMLInputElement>) => patchDdnsIp(key, { domains: textToLines(event.target.value) })}
              disabled={fieldDisabled}
              multiline
              minRows={4}
            />
          </Stack>
        </CardContent>
      </Card>
    )
  }

  if (loading) {
    return (
      <Box display="flex" justifyContent="center" alignItems="center" minHeight="60vh">
        <CircularProgress />
      </Box>
    )
  }

  return (
    <Box>
      <Box mb={2}>
        <Typography variant="h5" gutterBottom fontWeight={700}>
          设备网络
        </Typography>
        <Typography variant="body2" color="text.secondary">
          管理设备 WLAN 联网、DDNS 动态解析和远程管理网络出口
        </Typography>
      </Box>

      <ErrorSnackbar error={error} onClose={() => setError(null)} />
      <Snackbar
        open={!!success}
        autoHideDuration={3000}
        resumeHideDuration={3000}
        onClose={() => setSuccess(null)}
        anchorOrigin={{ vertical: 'top', horizontal: 'center' }}
      >
        <Alert severity="success" variant="filled" onClose={() => setSuccess(null)}>{success}</Alert>
      </Snackbar>

      <Box sx={{ borderBottom: 1, borderColor: 'divider', mb: 2 }}>
        <Tabs value={tabValue} onChange={handleTabChange} variant="scrollable" scrollButtons="auto">
          <Tab label="WLAN" icon={<Wifi />} iconPosition="start" />
          <Tab label="DDNS" icon={<Dns />} iconPosition="start" />
        </Tabs>
      </Box>

      <TabPanel value={tabValue} index={0}>
            <Card sx={{ mb: 3 }}>
              <Toolbar sx={{ minHeight: 64, px: { xs: 2, sm: 3 }, gap: 2, flexWrap: 'wrap' }}>
                <Box display="flex" alignItems="center" gap={1}>
                  <RuntimeStatusDot active={!!wlanStatus?.enabled} />
                  <Typography variant="subtitle1" fontWeight={700}>无线局域网 (WLAN)</Typography>
                </Box>
                <Box sx={{ width: '1px', height: 20, bgcolor: 'divider', display: { xs: 'none', sm: 'block' } }} />
                <Typography variant="body2" color="text.secondary">
                  {wlanStatus?.enabled && connectedSsid ? (
                    <>
                      已连接：<Typography component="span" color="text.primary" fontWeight={600}>{connectedSsid}</Typography>
                      {connectedScanNetwork && (
                        <Chip
                          label={`${signalLabel(connectedScanNetwork.signal)} · ${connectedScanNetwork.signal}%`}
                          color="primary"
                          variant="outlined"
                          size="small"
                          sx={{ ml: 1 }}
                        />
                      )}
                    </>
                  ) : '未连接'}
                </Typography>
                <Box flexGrow={1} />
                <Box display="flex" alignItems="center" gap={1}>
                  {wlanSwitching && <CircularProgress size={20} />}
                  <Switch
                    checked={!!wlanStatus?.enabled}
                    onChange={handleWlanToggle}
                    disabled={wlanSwitching || !wlanStatus?.available}
                  />
                </Box>
              </Toolbar>
              {!wlanStatus?.available && (
                <CardContent sx={{ pt: 0 }}>
                  <Alert severity="warning">未检测到可由 NetworkManager 管理的 WLAN 设备</Alert>
                </CardContent>
              )}
            </Card>

            <Box display="grid" gridTemplateColumns={{ xs: '1fr', lg: wlanDetailOpen ? 'minmax(0, 1fr) 380px' : '1fr' }} gap={3} alignItems="stretch">
              <Box sx={{ display: 'flex', minWidth: 0 }}>
                {wlanStatus?.enabled && (
                  <Card sx={{ flex: 1, minWidth: 0, height: '100%' }}>
                    <CardHeader
                      title="可用网络"
                      titleTypographyProps={CARD_TITLE_TYPOGRAPHY}
                      action={
                        <Button
                          size="small"
                          startIcon={wlanScanning ? <CircularProgress size={16} /> : <Refresh />}
                          onClick={() => void scanWlan()}
                          disabled={wlanScanning}
                        >
                          {wlanScanning ? '扫描中...' : '重新扫描'}
                        </Button>
                      }
                    />
                    <List disablePadding>
                      {(wlanStatus?.connected || savedNetworkRows.length > 0) && (
                        <>
                          <Box px={2} pt={2} pb={1}>
                            <Typography variant="caption" color="text.secondary" fontWeight={700}>我的网络</Typography>
                          </Box>
                          {wlanStatus?.connected && (
                            <ListItem
                              sx={{
                                '&:hover .wlan-row-actions': {
                                  opacity: 1,
                                  pointerEvents: 'auto',
                                },
                              }}
                            >
                              <ListItemText
                                primary={
                                  <Box display="flex" alignItems="center" gap={1}>
                                    <Typography fontWeight={700} color="primary.main">
                                      {connectedSsid || wlanStatus.connection_id || '当前 WLAN'}
                                    </Typography>
                                    <Chip label="已连接" color="primary" size="small" />
                                  </Box>
                                }
                                secondary={`${wlanStatus.interface_name || ''}${connectedScanNetwork ? ` · ${wlanNetworkSecondary(connectedScanNetwork)}` : ''}`}
                              />
                              <Box display="flex" alignItems="center" gap={1.25}>
                                {renderCurrentNetworkActions()}
                                {renderNetworkIndicators(connectedScanNetwork, true)}
                              </Box>
                            </ListItem>
                          )}
                          {savedNetworkRows.map((profile) => {
                            const scanned = visibleNetworks.find((network) => network.ssid === profile.ssid || network.ssid === profile.id)
                            return (
                              <ListItemButton
                                key={profile.uuid}
                                onClick={() => void connectSavedWlan(profile)}
                                sx={{
                                  '&:hover .wlan-row-actions': {
                                    opacity: 1,
                                    pointerEvents: 'auto',
                                  },
                                }}
                              >
                                <ListItemText
                                  primary={
                                    <Box display="flex" alignItems="center" gap={1}>
                                      <Typography fontWeight={600}>{profile.ssid || profile.id}</Typography>
                                      {profile.auto_join && <Chip label="自动加入" size="small" variant="outlined" />}
                                    </Box>
                                  }
                                  secondary={`${profile.interface_name || '已保存'} · ${wlanNetworkSecondary(scanned)}`}
                                />
                                <Box display="flex" alignItems="center" gap={1.25}>
                                  <Box
                                    className="wlan-row-actions"
                                    display="flex"
                                    alignItems="center"
                                    gap={0.5}
                                    sx={{
                                      opacity: 0,
                                      pointerEvents: 'none',
                                      transition: 'opacity 120ms ease',
                                    }}
                                  >
                                    <Tooltip title="删除">
                                      <span>
                                        <IconButton
                                          size="small"
                                          color="error"
                                          onClick={(event) => {
                                            event.stopPropagation()
                                            void forgetWlanProfile(profile)
                                          }}
                                          disabled={forgettingNetwork === profile.uuid}
                                        >
                                          {forgettingNetwork === profile.uuid ? <CircularProgress size={16} /> : <DeleteOutline fontSize="small" />}
                                        </IconButton>
                                      </span>
                                    </Tooltip>
                                    {renderWlanActionDivider()}
                                  </Box>
                                  {renderNetworkIndicators(scanned)}
                                </Box>
                              </ListItemButton>
                            )
                          })}
                          <Divider sx={{ mx: 2, mt: 1 }} />
                        </>
                      )}

                      <Box px={2} pt={2} pb={1}>
                        <Typography variant="caption" color="text.secondary" fontWeight={700}>其他可用网络</Typography>
                      </Box>
                      {otherNetworkRows.map((network) => (
                        <ListItemButton key={`${network.ssid}-${network.bssid}`} onClick={() => openConnectDialog(network)}>
                          <ListItemText
                            primary={
                              <Box display="flex" alignItems="center" gap={1}>
                                <Typography fontWeight={500}>{network.ssid}</Typography>
                              </Box>
                            }
                            secondary={wlanNetworkSecondary(network)}
                          />
                          {renderNetworkIndicators(network)}
                        </ListItemButton>
                      ))}
                      {otherNetworkRows.length === 0 && (
                        <Box py={5} textAlign="center">
                          <Typography variant="body2" color="text.secondary">
                            {wlanScanning ? '正在扫描无线网络...' : '未发现其他可用网络'}
                          </Typography>
                        </Box>
                      )}
                    </List>
                  </Card>
                )}
              </Box>

              {wlanDetailStatus && (
                <Card sx={{ height: '100%' }}>
                  <CardHeader
                    title={`${wlanDetailStatus.ssid || wlanDetailStatus.connection_id || '当前 WLAN'} 属性`}
                    titleTypographyProps={CARD_TITLE_TYPOGRAPHY}
                    subheader={wlanDetailStatus.interface_name}
                    action={
                      <Tooltip title="关闭">
                        <IconButton size="small" onClick={() => setWlanProfileOpen(false)}>
                          <Close fontSize="small" />
                        </IconButton>
                      </Tooltip>
                    }
                  />
                  <CardContent>
                    <Stack spacing={2}>
                      <FormControlLabel
                        control={<Switch checked={profileAutoJoin} onChange={(event) => setProfileAutoJoin(event.target.checked)} />}
                        label="自动加入"
                      />
                      <FormControl fullWidth>
                        <InputLabel>配置 IPv4</InputLabel>
                        <Select
                          value={profileIpv4Mode}
                          label="配置 IPv4"
                          onChange={(event) => setProfileIpv4Mode(event.target.value)}
                        >
                          <MenuItem value="dhcp">自动 (DHCP)</MenuItem>
                          <MenuItem value="manual">手动</MenuItem>
                        </Select>
                      </FormControl>
                      <TextField
                        label="IP 地址"
                        value={profileIpv4Address}
                        onChange={(event) => setProfileIpv4Address(event.target.value)}
                        disabled={profileIpv4Mode !== 'manual'}
                      />
                      <TextField
                        label="子网掩码"
                        value={profileIpv4Mask}
                        onChange={(event) => {
                          setProfileIpv4Mask(event.target.value)
                          const prefix = subnetMaskToPrefix(event.target.value)
                          if (prefix !== null) setProfileIpv4Prefix(prefix)
                        }}
                        disabled={profileIpv4Mode !== 'manual'}
                      />
                      <TextField
                        label="路由器"
                        value={profileIpv4Gateway}
                        onChange={(event) => setProfileIpv4Gateway(event.target.value)}
                        disabled={profileIpv4Mode !== 'manual'}
                      />
                      <Box>
                        <Typography variant="caption" color="text.secondary">IPv6 公网地址</Typography>
                        <Stack spacing={0.5} mt={0.5}>
                          {wlanDetailPublicIpv6Addresses.length > 0 ? wlanDetailPublicIpv6Addresses.map((address) => (
                            <Typography key={address} variant="body2" sx={{ wordBreak: 'break-all' }}>
                              {address}
                            </Typography>
                          )) : (
                            <Typography variant="body2" color="text.secondary">-</Typography>
                          )}
                        </Stack>
                      </Box>
                      <Button
                        variant="contained"
                        startIcon={profileSaving ? <CircularProgress size={18} /> : <Save />}
                        onClick={() => void saveWlanProfile()}
                        disabled={profileSaving}
                      >
                        保存
                      </Button>
                    </Stack>
                  </CardContent>
                </Card>
              )}
            </Box>
          </TabPanel>

      <TabPanel value={tabValue} index={1}>
        <Box display="grid" gridTemplateColumns={{ xs: '1fr', lg: '360px minmax(0, 1fr)' }} gap={3} alignItems="stretch">
          <Card sx={{ gridColumn: { xs: 'auto', lg: '1 / span 2' }, gridRow: { xs: 'auto', lg: '1' } }}>
            <CardContent sx={{ py: 2 }}>
              <Box display="flex" alignItems="center" gap={2} flexWrap="wrap">
                <Box display="flex" alignItems="center" gap={1}>
                  <RuntimeStatusDot active={ddnsConfig.enabled} />
                  <Typography variant="subtitle1" fontWeight={700}>DDNS 服务</Typography>
                  <Switch
                    checked={ddnsConfig.enabled}
                    onChange={(event: ChangeEvent<HTMLInputElement>) => void setDdnsEnabled(event.target.checked)}
                    disabled={ddnsToggling || ddnsSaving}
                  />
                </Box>
                <Divider orientation="vertical" flexItem sx={{ display: { xs: 'none', md: 'block' } }} />
                <Typography variant="body2" color="text.secondary">
                  服务商: <Typography component="span" color="text.primary" fontWeight={600}>{providerName(ddnsConfig.provider)}</Typography>
                </Typography>
                <Box display="flex" alignItems="center" gap={1}>
                  <Box sx={{ width: 8, height: 8, borderRadius: '50%', bgcolor: ddnsRuntimeStatus.color }} />
                  <Typography variant="body2" color="text.secondary">
                    监听状态: <Typography component="span" color="text.primary" fontWeight={600}>{ddnsRuntimeStatus.label}</Typography>
                  </Typography>
                </Box>
                {ddnsStatus?.last_ipv4 && (
                  <Chip
                    label={`IPv4: ${ddnsStatus.last_ipv4}`}
                    color="primary"
                    variant="outlined"
                    size="small"
                    sx={{ fontWeight: 600, borderColor: 'primary.light', color: 'primary.main' }}
                  />
                )}
                {ddnsStatus?.last_ipv6 && (
                  <Chip
                    label={`IPv6: ${ddnsStatus.last_ipv6}`}
                    color="primary"
                    variant="outlined"
                    size="small"
                    sx={{ fontWeight: 600, borderColor: 'primary.light', color: 'primary.main' }}
                  />
                )}
                <Box flexGrow={1} />
                <Button
                  variant="outlined"
                  startIcon={ddnsSyncing ? <CircularProgress size={18} /> : <SettingsEthernet />}
                  onClick={() => void syncDdns()}
                  disabled={ddnsControlsDisabled || ddnsSyncing || ddnsSaving}
                >
                  立即同步
                </Button>
                <Button
                  variant="contained"
                  startIcon={ddnsSaving ? <CircularProgress size={18} /> : <Save />}
                  onClick={() => void saveDdnsConfig()}
                  disabled={ddnsControlsDisabled || ddnsSaving || ddnsSyncing}
                >
                  保存配置
                </Button>
              </Box>
              {latestDdnsLog && (
                <Box display="flex" alignItems="center" gap={1} flexWrap="wrap" mt={1.5}>
                  <Typography variant="body2" color="text.secondary">最新日志:</Typography>
                  <Typography variant="body2" color="text.secondary">{formatDdnsLogTimestamp(latestDdnsLog.timestamp)}</Typography>
                  <Typography variant="body2" color="text.secondary">{latestDdnsLog.level}</Typography>
                  <Typography variant="body2" color="text.secondary">{latestDdnsLog.record_type || '-'}</Typography>
                  {latestDdnsLog.domains.length > 0 && (
                    <Typography variant="body2" color="text.secondary" sx={{ overflowWrap: 'anywhere' }}>
                      [{latestDdnsLog.domains.join(', ')}]
                    </Typography>
                  )}
                  <Typography variant="body2" color="text.secondary" sx={{ overflowWrap: 'anywhere' }}>
                    {translateDdnsLogMessage(latestDdnsLog.message)}
                  </Typography>
                </Box>
              )}
            </CardContent>
          </Card>

          <Card sx={{ height: '100%', gridColumn: { xs: 'auto', lg: '1' }, gridRow: { xs: 'auto', lg: '2' }, opacity: ddnsControlsDisabled ? 0.55 : 1 }}>
            <CardHeader
              avatar={<CloudSync color="primary" />}
              title="DNS服务商配置"
              titleTypographyProps={CARD_TITLE_TYPOGRAPHY}
            />
            <CardContent>
              <Stack spacing={2}>
                <FormControl fullWidth disabled={ddnsControlsDisabled}>
                  <InputLabel>服务商</InputLabel>
                  <Select value={ddnsConfig.provider} label="服务商" onChange={(event) => patchDdns({ provider: event.target.value })}>
                    <MenuItem value="tencentcloud">腾讯云 DNSPod</MenuItem>
                    <MenuItem value="alidns">阿里云 AliDNS</MenuItem>
                    <MenuItem value="cloudflare">Cloudflare</MenuItem>
                  </Select>
                </FormControl>
                <TextField
                  label="ID"
                  value={ddnsConfig.access_id}
                  onChange={(event) => patchDdns({ access_id: event.target.value })}
                  disabled={ddnsControlsDisabled}
                />
                <TextField
                  label="Token"
                  value={ddnsConfig.access_secret}
                  onChange={(event) => patchDdns({ access_secret: event.target.value })}
                  placeholder="留空或保留星号表示沿用已保存 Token"
                  disabled={ddnsControlsDisabled}
                />
                <TextField
                  label="同步间隔（秒）"
                  type="number"
                  value={ddnsConfig.interval_seconds}
                  onChange={(event) => patchDdns({ interval_seconds: Number(event.target.value) })}
                  disabled={ddnsControlsDisabled}
                />
                <TextField
                  label="TTL"
                  type="number"
                  value={ddnsConfig.ttl}
                  onChange={(event) => patchDdns({ ttl: Number(event.target.value) })}
                  disabled={ddnsControlsDisabled}
                />
                <Alert severity="info">{ddnsProviderHelp}</Alert>
              </Stack>
            </CardContent>
          </Card>

          <Stack spacing={3} sx={{ gridColumn: { xs: 'auto', lg: '2' }, gridRow: { xs: 'auto', lg: '2' } }}>
            {renderDdnsIpCard('ipv4')}
            {renderDdnsIpCard('ipv6')}
          </Stack>

          <Card sx={{ gridColumn: { xs: 'auto', lg: '1 / span 2' }, gridRow: { xs: 'auto', lg: '3' } }}>
            <CardHeader
              avatar={<Terminal color="primary" />}
              title="运行日志"
              titleTypographyProps={CARD_TITLE_TYPOGRAPHY}
              action={
                <Button
                  size="small"
                  color="inherit"
                  startIcon={<DeleteOutline />}
                  onClick={() => setClearDdnsLogsOpen(true)}
                  disabled={ddnsLogs.length === 0}
                  sx={{
                    fontWeight: 400,
                    '&:hover': {
                      color: 'error.main',
                    },
                  }}
                >
                  清空日志
                </Button>
              }
            />
            <CardContent>
              <Box
                sx={{
                  bgcolor: '#0b1020',
                  border: '1px solid rgba(148, 163, 184, 0.24)',
                  borderRadius: 1,
                  boxShadow: 'inset 0 1px 0 rgba(255,255,255,0.04)',
                  color: '#dbeafe',
                  fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace',
                  fontSize: 13,
                  lineHeight: 1.7,
                  maxHeight: 360,
                  minHeight: 240,
                  overflow: 'auto',
                  p: 2,
                }}
              >
                {ddnsLogs.length === 0 ? (
                  <Typography variant="body2" sx={{ color: '#64748b', fontFamily: 'inherit' }}>
                    $ 暂无运行日志
                  </Typography>
                ) : (
                  <Stack spacing={0.75}>
                    {ddnsLogs.map((entry) => (
                      <Box key={`${entry.timestamp}-${entry.message}`} display="grid" gridTemplateColumns={{ xs: '1fr', md: '190px 64px 64px minmax(0, 1fr)' }} gap={1}>
                        <Box component="span" sx={{ color: '#64748b', whiteSpace: 'nowrap' }}>{formatDdnsLogTimestamp(entry.timestamp)}</Box>
                        <Box
                          component="span"
                          sx={{
                            color: entry.level === 'error' ? '#f87171' : entry.level === 'warn' ? '#facc15' : '#22c55e',
                            fontWeight: 700,
                            textTransform: 'uppercase',
                          }}
                        >
                          {entry.level}
                        </Box>
                        <Box component="span" sx={{ color: '#38bdf8', fontWeight: 700 }}>{entry.record_type || '-'}</Box>
                        <Box component="span" sx={{ color: '#e2e8f0', minWidth: 0, overflowWrap: 'anywhere' }}>
                          {entry.domains.length > 0 && (
                            <Box component="span" sx={{ color: '#93c5fd' }}>
                              [{entry.domains.join(', ')}]{' '}
                            </Box>
                          )}
                          {translateDdnsLogMessage(entry.message)}
                        </Box>
                      </Box>
                    ))}
                  </Stack>
                )}
              </Box>
            </CardContent>
          </Card>
        </Box>
      </TabPanel>

      <Dialog open={clearDdnsLogsOpen} onClose={() => setClearDdnsLogsOpen(false)} fullWidth maxWidth="xs">
        <DialogTitle display="flex" alignItems="center" gap={1}>
          <DeleteOutline color="error" />
          清空 DDNS 运行日志
        </DialogTitle>
        <DialogContent>
          <Typography variant="body2" color="text.secondary">
            确定要清空当前 DDNS 运行日志吗？此操作不会影响 DDNS 配置。
          </Typography>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setClearDdnsLogsOpen(false)}>取消</Button>
          <Button color="error" variant="contained" onClick={() => void clearDdnsLogs()}>
            确认清空
          </Button>
        </DialogActions>
      </Dialog>

      <Dialog open={connectOpen} onClose={() => setConnectOpen(false)} fullWidth maxWidth="xs">
        <DialogTitle>输入网络密码</DialogTitle>
        <DialogContent>
          <Typography variant="body2" color="text.secondary" mb={2}>
            输入 "{selectedNetwork?.ssid}" 的密码
          </Typography>
          <TextField
            autoFocus
            fullWidth
            type={showWifiPassword ? 'text' : 'password'}
            label="密码"
            value={wifiPassword}
            onChange={(event) => setWifiPassword(event.target.value)}
            slotProps={{
              input: {
                endAdornment: (
                  <InputAdornment position="end">
                    <IconButton
                      size="small"
                      onClick={() => setShowWifiPassword((prev) => !prev)}
                      edge="end"
                      aria-label={showWifiPassword ? '隐藏密码' : '显示密码'}
                    >
                      {showWifiPassword ? <VisibilityOff fontSize="small" /> : <Visibility fontSize="small" />}
                    </IconButton>
                  </InputAdornment>
                ),
              },
            }}
          />
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setConnectOpen(false)}>取消</Button>
          <Button variant="contained" onClick={() => void connectWlan()}>连接</Button>
        </DialogActions>
      </Dialog>

      <Dialog open={closeConfirmOpen} onClose={() => setCloseConfirmOpen(false)} fullWidth maxWidth="sm">
        <DialogTitle display="flex" alignItems="center" gap={1}>
          <WifiOff color="error" />
          关闭 WLAN
        </DialogTitle>
        <DialogContent>
          <Alert severity="error" sx={{ mb: 2 }}>{closeWlanPrompt}</Alert>
          <TextField
            fullWidth
            label={`请输入 ${CONFIRM_CLOSE_WLAN}`}
            value={closeConfirmText}
            onChange={(event) => setCloseConfirmText(event.target.value)}
          />
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setCloseConfirmOpen(false)}>取消</Button>
          <Button
            color="error"
            variant="contained"
            disabled={closeConfirmText !== CONFIRM_CLOSE_WLAN || wlanSwitching}
            onClick={() => void confirmDisableWlan()}
          >
            确认关闭 WLAN
          </Button>
        </DialogActions>
      </Dialog>
    </Box>
  )
}
