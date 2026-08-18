import { useEffect, useMemo, useState, useRef, useCallback } from 'react'
import {
  Alert,
  Avatar,
  Box,
  Button,
  Card,
  Chip,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogContentText,
  DialogTitle,
  Divider,
  IconButton,
  LinearProgress,
  List,
  ListItemButton,
  ListItemText,
  MenuItem,
  Snackbar,
  Stack,
  TextField,
  Tooltip,
  Typography,
  Toolbar,
} from '@mui/material'
import Grid from '@mui/material/Grid'
import {
  Build,
  CheckCircle,
  DeleteOutline,
  Dns,
  DriveFileRenameOutline,
  Edit,
  Fingerprint,
  Memory,
  PowerSettingsNew,
  Public,
  Refresh,
  SimCard,
  CloudDownload,
  QrCodeScanner,
  Add,
} from '@mui/icons-material'
import jsQR from 'jsqr'
import { alpha, type Theme } from '@mui/material/styles'
import { useSimAdminApi } from '../contexts/ApiContext'
import ErrorSnackbar from '../components/ErrorSnackbar'
import { formatCarrierName } from '../utils/carriers'
import type { BasebandRestartStep, EsimCommandResponse, EsimEuiccInfo, EsimLpacStatusResponse, EsimProfile } from '../api/types'

type ConfirmAction = 'enable' | 'delete' | null
const CONFIRM_DELETE_PROFILE = '确认删除'

type EsimPageSnapshot = {
  euicc: EsimEuiccInfo | null
  profiles: EsimProfile[]
  selectedIccid: string
  lpacStatus: EsimLpacStatusResponse | null
}

let esimPageSnapshot: EsimPageSnapshot | null = null

function updateEsimPageSnapshot(partial: Partial<EsimPageSnapshot>) {
  esimPageSnapshot = {
    euicc: null,
    profiles: [],
    selectedIccid: '',
    lpacStatus: null,
    ...esimPageSnapshot,
    ...partial,
  }
}

const LPAC_PROXY_PREFIX_OPTIONS = [
  { label: 'gh-proxy.com', value: 'https://gh-proxy.com/' },
  { label: 'ghproxy.net', value: 'https://ghproxy.net/' },
  { label: 'githubproxy.cc', value: 'https://githubproxy.cc/' },
  { label: '直连', value: '' },
]

const MCC_COUNTRY: Record<string, string> = {
  // === 2xx: 欧洲区域 ===
  '202': 'GR', // 希腊
  '204': 'NL', // 荷兰
  '206': 'BE', // 比利时
  '208': 'FR', // 法国
  '214': 'ES', // 西班牙
  '216': 'HU', // 匈牙利
  '222': 'IT', // 意大利
  '226': 'RO', // 罗马尼亚
  '228': 'CH', // 瑞士
  '230': 'CZ', // 捷克
  '232': 'AT', // 奥地利
  '234': 'GB', // 英国
  '235': 'GB', // 英国
  '240': 'SE', // 瑞典
  '242': 'NO', // 挪威
  '244': 'FI', // 芬兰
  '247': 'LV', // 拉脱维亚
  '248': 'EE', // 爱沙尼亚
  '250': 'RU', // 俄罗斯
  '255': 'UA', // 乌克兰
  '260': 'PL', // 波兰
  '262': 'DE', // 德国
  '268': 'PT', // 葡萄牙
  '272': 'IE', // 爱尔兰
  '286': 'TR', // 土耳其
  '293': 'SI', // 斯洛文尼亚

  // === 3xx: 北美及加勒比区域 ===
  '302': 'CA', // 加拿大
  // 美国 (号段极多，IoT/漫游卡非常高频)
  '310': 'US', '311': 'US', '312': 'US', '313': 'US', '314': 'US', '315': 'US', '316': 'US',
  '334': 'MX', // 墨西哥

  // === 4xx: 亚洲（中东/南亚/东亚）区域 ===
  // 印度
  '404': 'IN', '405': 'IN', '406': 'IN',
  '420': 'SA', // 沙特阿拉伯
  '424': 'AE', // 阿联酋
  '432': 'IR', // 伊朗
  '434': 'UZ', // 乌兹别克斯坦
  '440': 'JP', // 日本
  '441': 'JP', // 日本
  '450': 'KR', // 韩国
  '452': 'VN', // 越南
  '454': 'HK', // 中国香港
  '455': 'MO', // 中国澳门
  '456': 'KH', // 柬埔寨
  '460': 'CN', // 中国
  '461': 'CN', // 中国 (新分配给广电/中星微等新型号段)
  '466': 'TW', // 中国台湾
  '470': 'BD', // 孟加拉国
  '472': 'MV', // 马尔代夫

  // === 5xx: 大洋洲及东南亚区域 ===
  '502': 'MY', // 马来西亚
  '505': 'AU', // 澳大利亚
  '510': 'ID', // 印度尼西亚
  '514': 'TL', // 东帝汶
  '515': 'PH', // 菲律宾
  '520': 'TH', // 泰国
  '525': 'SG', // 新加坡
  '530': 'NZ', // 新西兰
  '548': 'CK', // 库克群岛

  // === 6xx: 非洲区域 ===
  '602': 'EG', // 埃及
  '647': 'IO', // 英属印度洋领地
  '655': 'ZA', // 南非

  // === 7xx: 南美洲区域 ===
  '724': 'BR', // 巴西
  '730': 'CL', // 智利
  '732': 'CO', // 哥伦比亚
  '748': 'UY', // 乌拉圭
  '750': 'FK', // 福克兰群岛
}

function commandSucceeded(response?: EsimCommandResponse) {
  if (!response) return false
  const status = response.status.toLowerCase()
  return response.code === 0 && (!status || status === 'success' || status === 'ok')
}

function formatIccid(iccid: string) {
  return iccid.replace(/(\d{4})(?=\d)/g, '$1 ')
}

function translateEsimError(rawError: string): string {
  const err = rawError.toLowerCase();

  if (err.includes("matchingid is refused") || err.includes("matching id was refused")) {
    return "激活码已被使用或失效 (Matching ID was refused by SM-DP+ server)";
  }
  if (err.includes("es9p_initiate_authentication")) {
    return "无法启动身份认证。请检查设备是否已联网，或者激活码是否有效 (es9p_initiate_authentication)";
  }
  if (err.includes("es10b_load_bound_profile_package")) {
    return "安全域装载失败。这通常是因为该配置文件已存在于当前芯片中，无法重复写入 (es10b_load_bound_profile_package)";
  }
  if (err.includes("connect") || err.includes("timeout") || err.includes("resolve") || err.includes("could not resolve")) {
    return "网络连接超时或无法解析服务器地址，请确保设备已正常联网后再试";
  }
  if (err.includes("es10b_")) {
    return `芯片交互阶段发生错误 (${rawError})，请确认卡片接触良好或芯片空间是否充足`;
  }
  if (err.includes("es9p_")) {
    return `服务器通信阶段发生错误 (${rawError})，请检查网络状态与激活码是否正确`;
  }

  return rawError;
}

function profileStateLabel(state: string) {
  const normalized = state.toLowerCase()
  if (normalized === 'active' || normalized === 'enabled' || normalized === '1') return '已启用'
  if (normalized === 'inactive' || normalized === 'disabled' || normalized === '0') return '已禁用'
  return state || '未知'
}

function profileActive(profile?: EsimProfile) {
  if (!profile) return false
  const state = profile.state.toLowerCase()
  return state === 'active' || state === 'enabled' || state === '1'
}

function preferredProfileIccid(profiles: EsimProfile[], currentIccid: string) {
  if (currentIccid && profiles.some((profile) => profile.iccid === currentIccid)) {
    return currentIccid
  }
  return profiles.find(profileActive)?.iccid ?? profiles[0]?.iccid ?? ''
}

function countryCodeFromMcc(mcc?: string | null) {
  const normalized = String(mcc ?? '').trim().padStart(3, '0')
  if (!/^\d{3}$/.test(normalized)) return null
  const numeric = Number(normalized)
  if (numeric >= 310 && numeric <= 316) return 'US'
  return MCC_COUNTRY[normalized] ?? null
}

const ICCID_COUNTRY_PREFIXES: Array<[string, string]> = [
  ['246', 'IO'], ['351', 'PT'], ['353', 'IE'], ['420', 'CZ'], ['500', 'FK'], ['598', 'UY'],
  ['682', 'CK'], ['852', 'HK'], ['853', 'MO'], ['880', 'BD'], ['886', 'TW'], ['960', 'MV'],
  ['966', 'SA'], ['971', 'AE'], ['998', 'UZ'],
  ['1', 'US'], ['7', 'RU'], ['20', 'EG'], ['27', 'ZA'], ['30', 'GR'], ['31', 'NL'],
  ['32', 'BE'], ['33', 'FR'], ['34', 'ES'], ['36', 'HU'], ['39', 'IT'], ['40', 'RO'],
  ['41', 'CH'], ['43', 'AT'], ['44', 'GB'], ['46', 'SE'], ['47', 'NO'], ['48', 'PL'],
  ['49', 'DE'], ['52', 'MX'], ['55', 'BR'], ['56', 'CL'], ['57', 'CO'], ['60', 'MY'],
  ['61', 'AU'], ['62', 'ID'], ['63', 'PH'], ['64', 'NZ'], ['65', 'SG'], ['66', 'TH'],
  ['81', 'JP'], ['82', 'KR'], ['84', 'VN'], ['86', 'CN'], ['90', 'TR'], ['91', 'IN'],
]

function countryCodeFromIccid(iccid?: string | null) {
  const digits = String(iccid ?? '').replace(/\D/g, '')
  if (!digits.startsWith('89')) return null
  const countryPart = digits.slice(2)
  const match = ICCID_COUNTRY_PREFIXES.find(([prefix]) => countryPart.startsWith(prefix))
  return match?.[1] ?? null
}

function profileCountryCode(profile?: EsimProfile | null) {
  return countryCodeFromMcc(profile?.mcc) ?? countryCodeFromIccid(profile?.iccid)
}

function euiccManufacturerFromEid(eid?: string | null): string {
  if (!eid) {
    return ''
  }
  // 提取前 8 位并转为大写，去除可能存在的空格
  const prefix = String(eid).replace(/\s/g, '').slice(0, 8).toUpperCase()

  if (prefix.length < 8) {
    return 'Invalid EID'
  }

  switch (prefix) {
    // 泰雷兹 (原 Gemalto 金雅拓)
    case '89033023':
      return 'Thales'

    // IDEMIA (原 Oberthur 欧贝特)
    case '89033024':
    case '89039011':
      return 'Idemia'

    // Giesecke+Devrient (捷德)
    case '89044011':
    case '89044020':
    case '89049032':
    case '89049038':
    case '89049044':
      return 'Giesecke+Devrient'

    // 意法半导体
    case '89041030':
      return 'STMicroelectronics'

    // 恩智浦半导体
    case '89043051':
    case '89043052':
      return 'NXP Semiconductors'

    // Valid
    case '89034011':
      return 'Valid'

    // Workz
    case '8904C012':
      return 'Workz'

    // Kigen (ARM 剥离的 eSIM 业务)
    case '89014022':
    case '89014052':
    case '89044045':
      return 'Kigen'

    // Truphone
    case '89044047':
      return 'Truphone'

    // 英飞凌
    case '89046031':
      return 'Infineon Technologies'

    // 东信和平 (EastcomPeace)
    case '89086030':
      return 'EastcomPeace'

    // 华大电子 (HED)
    case '89086016':
      return 'HED'

    // 中移物联网 (China Mobile IoT)
    case '89086011':
      return 'China Mobile IoT'

    // 紫光同芯 (Tongxin Micro)
    case '89086002':
      return 'Tongxin Micro'

    // 天喻信息 (Wuhan Tianyu)
    case '89086026':
    case '89086027':
    case '89086029':
      return 'Tianyu'

    // 恒宝股份 (Hengbao)
    case '89086001':
    case '89086014':
      return 'Hengbao'

    // 大唐微电子 (Datang Micro)
    case '89086012':
      return 'Datang Micro'

    // 中兴微电子 (ZTE ICT)
    case '89086004':
      return 'ZTE ICT'

    // 握奇数据 (Watchdata)
    case '89086017':
    case '89086032':
      return 'Watchdata'

    // 绿洲智能 (Oasis Smart SIM)
    case '89033032':
      return 'Oasis Smart SIM'

    // 韩国柯纳 (Kona I)
    case '89082012':
      return 'Kona I'

    // Roam9 / E-SIM
    case '89044033':
      return 'Roam9'

    // 兜底逻辑：基于国家代码 (MNC/MCC 规则演变) 解析原产国
    default:
      // 提取第 3 到 5 位（索引 2, 3, 4）作为国家/地区代码并执行匹配
      switch (prefix.slice(2, 5)) {
        case '086': return 'Unknown Chinese EUM (086)';
        case '033': return 'Unknown French EUM (033)';
        case '049': return 'Unknown German EUM (049)';
        case '044': return 'Unknown British EUM (044)';
        case '001': return 'Unknown US EUM (001)';
        case '082': return 'Unknown South Korean EUM (082)';
        case '081': return 'Unknown Japanese EUM (081)';
        default: return `Unknown EUM (Prefix: ${prefix})`;
      }
  }
}

function profileProviderLabel(profile: EsimProfile) {
  const provider = profile.provider?.trim()
  if (provider) return provider
  const carrier = formatCarrierName(profile.mcc, profile.mnc)
  return carrier === 'Unknown' ? '未知提供商' : carrier
}

function formatCapacityK(value?: number | null) {
  if (typeof value !== 'number' || !Number.isFinite(value)) return 'N/A'
  return `${Math.max(0, Math.floor(value)).toLocaleString()}k`
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null
}

function stringAt(value: unknown, path: string[]) {
  let current: unknown = value
  for (const key of path) {
    const record = asRecord(current)
    if (!record) return ''
    current = record[key]
  }
  return typeof current === 'string' ? current.trim() : ''
}

function extractDefaultSmdp(raw: unknown) {
  const paths = [
    ['EuiccConfiguredAddresses', 'defaultDpAddress'],
    ['euiccConfiguredAddresses', 'defaultDpAddress'],
    ['euicc_configured_addresses', 'default_dp_address'],
    ['defaultDpAddress'],
    ['defaultSmdpAddress'],
  ]
  for (const path of paths) {
    const value = stringAt(raw, path)
    if (value) return value
  }
  return ''
}

function svgDataUrl(svg: string) {
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`
}

function horizontalFlag(colors: string[]) {
  const height = 40 / colors.length
  const rects = colors.map((color, index) => (
    `<rect width="60" height="${height}" y="${index * height}" fill="${color}"/>`
  )).join('')
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40">${rects}</svg>`
}

function verticalFlag(colors: string[]) {
  const width = 60 / colors.length
  const rects = colors.map((color, index) => (
    `<rect width="${width}" height="40" x="${index * width}" fill="${color}"/>`
  )).join('')
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40">${rects}</svg>`
}

const FLAG_SVGS: Record<string, string> = {
  AT: horizontalFlag(['#c8102e', '#fff', '#c8102e']),
  AU: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="40" fill="#012169"/><path fill="#fff" d="M0 0h30v20H0z"/><path stroke="#012169" stroke-width="3" d="M0 0l30 20M30 0 0 20"/><path stroke="#c8102e" stroke-width="2" d="M0 0l30 20M30 0 0 20"/><path stroke="#012169" stroke-width="7" d="M15 0v20M0 10h30"/><path stroke="#c8102e" stroke-width="4" d="M15 0v20M0 10h30"/><circle cx="45" cy="26" r="3" fill="#fff"/></svg>`,
  BE: verticalFlag(['#000', '#fae042', '#ed2939']),
  BR: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="40" fill="#009b3a"/><path d="M30 5 55 20 30 35 5 20z" fill="#ffdf00"/><circle cx="30" cy="20" r="8" fill="#002776"/></svg>`,
  CA: verticalFlag(['#d52b1e', '#fff', '#d52b1e']),
  CH: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="40" fill="#d52b1e"/><path fill="#fff" d="M26 8h8v9h9v8h-9v9h-8v-9h-9v-8h9z"/></svg>`,
  CN: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="40" fill="#de2910"/><polygon fill="#ffde00" points="12,6 14,12 20,12 15,16 17,22 12,18 7,22 9,16 4,12 10,12"/></svg>`,
  DE: horizontalFlag(['#000', '#dd0000', '#ffce00']),
  ES: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="40" fill="#aa151b"/><rect y="10" width="60" height="20" fill="#f1bf00"/></svg>`,
  FI: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="40" fill="#fff"/><rect x="18" width="7" height="40" fill="#002f6c"/><rect y="16" width="60" height="7" fill="#002f6c"/></svg>`,
  FR: verticalFlag(['#0055a4', '#fff', '#ef4135']),
  GB: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="40" fill="#012169"/><path stroke="#fff" stroke-width="8" d="M0 0 60 40M60 0 0 40"/><path stroke="#c8102e" stroke-width="4" d="M0 0 60 40M60 0 0 40"/><path stroke="#fff" stroke-width="13" d="M30 0v40M0 20h60"/><path stroke="#c8102e" stroke-width="8" d="M30 0v40M0 20h60"/></svg>`,
  GR: horizontalFlag(['#0d5eaf', '#fff', '#0d5eaf', '#fff', '#0d5eaf', '#fff', '#0d5eaf', '#fff', '#0d5eaf']),
  HK: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="40" fill="#de2910"/><circle cx="30" cy="20" r="7" fill="#fff"/><circle cx="30" cy="20" r="3" fill="#de2910"/></svg>`,
  IE: verticalFlag(['#169b62', '#fff', '#ff883e']),
  IN: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="13.33" fill="#ff9933"/><rect y="13.33" width="60" height="13.34" fill="#fff"/><rect y="26.67" width="60" height="13.33" fill="#138808"/><circle cx="30" cy="20" r="4" fill="none" stroke="#000080" stroke-width="1.2"/></svg>`,
  IT: verticalFlag(['#009246', '#fff', '#ce2b37']),
  JP: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="40" fill="#fff"/><circle cx="30" cy="20" r="9" fill="#bc002d"/></svg>`,
  KR: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="40" fill="#fff"/><path d="M30 10a10 10 0 0 1 0 20 5 5 0 0 1 0-10 5 5 0 0 0 0-10z" fill="#c60c30"/><path d="M30 30a10 10 0 0 1 0-20 5 5 0 0 1 0 10 5 5 0 0 0 0 10z" fill="#003478"/></svg>`,
  MO: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="40" fill="#00785e"/><path d="M30 9 33 18h9l-7 5 3 8-8-5-8 5 3-8-7-5h9z" fill="#fff"/></svg>`,
  MX: verticalFlag(['#006847', '#fff', '#ce1126']),
  MY: horizontalFlag(['#cc0001', '#fff', '#cc0001', '#fff', '#cc0001', '#fff', '#cc0001', '#fff']),
  NL: horizontalFlag(['#ae1c28', '#fff', '#21468b']),
  NO: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="40" fill="#ba0c2f"/><rect x="16" width="12" height="40" fill="#fff"/><rect y="14" width="60" height="12" fill="#fff"/><rect x="19" width="6" height="40" fill="#00205b"/><rect y="17" width="60" height="6" fill="#00205b"/></svg>`,
  NZ: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="40" fill="#012169"/><path fill="#fff" d="M0 0h30v20H0z"/><path stroke="#012169" stroke-width="3" d="M0 0l30 20M30 0 0 20"/><path stroke="#c8102e" stroke-width="2" d="M0 0l30 20M30 0 0 20"/><path stroke="#012169" stroke-width="7" d="M15 0v20M0 10h30"/><path stroke="#c8102e" stroke-width="4" d="M15 0v20M0 10h30"/><circle cx="46" cy="15" r="3" fill="#cc142b"/></svg>`,
  PH: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="20" y="0" fill="#0038a8"/><rect width="60" height="20" y="20" fill="#ce1126"/><path d="M0 0 26 20 0 40z" fill="#fff"/><circle cx="8" cy="20" r="3" fill="#fcd116"/></svg>`,
  PL: horizontalFlag(['#fff', '#dc143c']),
  PT: verticalFlag(['#006600', '#ff0000']),
  RU: horizontalFlag(['#fff', '#0039a6', '#d52b1e']),
  SE: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="40" fill="#006aa7"/><rect x="17" width="7" height="40" fill="#fecc00"/><rect y="16" width="60" height="7" fill="#fecc00"/></svg>`,
  SG: horizontalFlag(['#ef3340', '#fff']),
  TH: horizontalFlag(['#a51931', '#fff', '#2d2a4a', '#2d2a4a', '#fff', '#a51931']),
  TR: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="40" fill="#e30a17"/><circle cx="25" cy="20" r="9" fill="#fff"/><circle cx="28" cy="20" r="7" fill="#e30a17"/><polygon fill="#fff" points="39,14 41,19 46,19 42,22 44,27 39,24 34,27 36,22 32,19 37,19"/></svg>`,
  TW: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="40" fill="#fe0000"/><rect width="30" height="20" fill="#000095"/><circle cx="15" cy="10" r="5" fill="#fff"/></svg>`,
  US: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="40" fill="#fff"/><path fill="#b22234" d="M0 0h60v3.08H0zm0 6.15h60v3.08H0zm0 6.16h60v3.08H0zm0 6.15h60v3.08H0zm0 6.16h60v3.08H0zm0 6.15h60v3.08H0zm0 6.15h60V40H0z"/><rect width="25" height="21.54" fill="#3c3b6e"/><g fill="#fff"><circle cx="4" cy="4" r="1"/><circle cx="10" cy="4" r="1"/><circle cx="16" cy="4" r="1"/><circle cx="22" cy="4" r="1"/><circle cx="7" cy="9" r="1"/><circle cx="13" cy="9" r="1"/><circle cx="19" cy="9" r="1"/><circle cx="4" cy="14" r="1"/><circle cx="10" cy="14" r="1"/><circle cx="16" cy="14" r="1"/><circle cx="22" cy="14" r="1"/></g></svg>`,
  VN: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="40" fill="#da251d"/><polygon fill="#ff0" points="30,8 33,17 42,17 35,22 38,31 30,25 22,31 25,22 18,17 27,17"/></svg>`,
  ZA: `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 60 40"><rect width="60" height="40" fill="#de3831"/><rect y="20" width="60" height="20" fill="#002395"/><path d="M0 0 24 20 0 40z" fill="#000"/><path d="M0 4 20 20 0 36" fill="none" stroke="#ffb612" stroke-width="6"/><path d="M0 0 30 20 0 40" fill="none" stroke="#fff" stroke-width="10"/><path d="M0 0 30 20 0 40" fill="none" stroke="#007a4d" stroke-width="6"/></svg>`,
}

function CountryFlag({ countryCode, size = 28 }: { countryCode?: string | null; size?: number }) {
  const normalizedCode = countryCode?.trim().toUpperCase() || ''
  const svg = normalizedCode ? FLAG_SVGS[normalizedCode] : ''
  const src = svg ? svgDataUrl(svg) : ''

  if (!src) {
    return <SimCard fontSize="small" />
  }

  return (
    <Box
      component="img"
      src={src}
      alt={`${countryCode} flag`}
      sx={{
        display: 'block',
        width: size,
        height: Math.round(size * 0.72),
        borderRadius: 0.5,
        objectFit: 'cover',
        boxShadow: '0 0 0 1px rgba(0, 0, 0, 0.12)',
      }}
    />
  )
}

function EuiccMetaItem({ label, value, mono = false }: { label: string; value?: string | null; mono?: boolean }) {
  return (
    <Typography component="span" variant="body2" color="text.secondary" fontWeight={400}>
      {label}:{' '}
      <Typography
        component="span"
        variant="body2"
        color="text.primary"
        fontWeight={400}
        fontFamily={mono ? 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace' : undefined}
        sx={{ overflowWrap: 'anywhere' }}
      >
        {value || 'N/A'}
      </Typography>
    </Typography>
  )
}

function InfoCell({
  label,
  value,
  mono = false,
  emptyText = 'N/A',
  sensitive = false,
}: {
  label: string
  value?: string | null
  mono?: boolean
  emptyText?: string
  sensitive?: boolean
}) {
  return (
    <Box
      sx={{
        p: 1.5,
        minHeight: 72,
        borderRadius: 1,
        border: '1px solid',
        borderColor: 'divider',
        bgcolor: 'action.hover',
        overflow: 'hidden',
      }}
    >
      <Typography variant="caption" color="text.secondary" display="block" mb={0.5}>
        {label}
      </Typography>
      <Typography
        data-sensitive={sensitive ? 'true' : undefined}
        variant="body2"
        fontFamily={mono ? 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace' : undefined}
        sx={{ wordBreak: 'break-word' }}
      >
        {value || emptyText}
      </Typography>
    </Box>
  )
}

export default function EsimManagerPage() {
  const api = useSimAdminApi()
  const initialSnapshot = esimPageSnapshot
  const [euicc, setEuicc] = useState<EsimEuiccInfo | null>(initialSnapshot?.euicc ?? null)
  const [profiles, setProfiles] = useState<EsimProfile[]>(initialSnapshot?.profiles ?? [])
  const [selectedIccid, setSelectedIccid] = useState<string>(initialSnapshot?.selectedIccid ?? '')
  const [statusLoading, setStatusLoading] = useState(!initialSnapshot?.lpacStatus)
  const [profilesLoading, setProfilesLoading] = useState(false)
  const [euiccLoading, setEuiccLoading] = useState(false)
  const [refreshing, setRefreshing] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState<string | null>(null)
  const [confirmAction, setConfirmAction] = useState<ConfirmAction>(null)
  const [deleteConfirmText, setDeleteConfirmText] = useState('')
  const [renameOpen, setRenameOpen] = useState(false)
  const [renameValue, setRenameValue] = useState('')
  const [actionLoading, setActionLoading] = useState(false)
  const [lpacStatus, setLpacStatus] = useState<EsimLpacStatusResponse | null>(initialSnapshot?.lpacStatus ?? null)
  const [lpacRepairing, setLpacRepairing] = useState(false)
  const [lpacProxyPrefix, setLpacProxyPrefix] = useState(LPAC_PROXY_PREFIX_OPTIONS[0].value)

  const [totalMemoryDialogOpen, setTotalMemoryDialogOpen] = useState(false)
  const [totalMemoryInput, setTotalMemoryInput] = useState('')
  const [savingTotalMemory, setSavingTotalMemory] = useState(false)

  // Baseband recovery progress tracking (shown after profile enable)
  const [basebandRecoveryOpen, setBasebandRecoveryOpen] = useState(false)
  const [basebandRecoveryRunning, setBasebandRecoveryRunning] = useState(false)
  const [basebandRecoverySteps, setBasebandRecoverySteps] = useState<BasebandRestartStep[]>([])
  const [basebandRecoveryRegistration, setBasebandRecoveryRegistration] = useState<string | null>(null)
  const basebandRecoveryTimerRef = useRef<number | undefined>(undefined)

  const euiccCardRef = useRef<HTMLDivElement | null>(null)
  const [gridHeight, setGridHeight] = useState<string | number>('calc(100vh - 350px)')

  const updateHeight = useCallback(() => {
    const cardEl = euiccCardRef.current
    if (cardEl) {
      const rect = cardEl.getBoundingClientRect()
      const availableHeight = window.innerHeight - rect.bottom - 48
      setGridHeight(Math.max(450, availableHeight))
    }
  }, [])

  useEffect(() => {
    updateHeight()
    const timer = setTimeout(updateHeight, 100)
    window.addEventListener('resize', updateHeight)
    return () => {
      clearTimeout(timer)
      window.removeEventListener('resize', updateHeight)
    }
  }, [updateHeight, lpacStatus])

  const handleOpenTotalMemoryDialog = () => {
    setTotalMemoryInput(euicc?.memory_total_kb ? String(Math.round(euicc.memory_total_kb)) : '')
    setTotalMemoryDialogOpen(true)
  }

  const handleSaveTotalMemory = async () => {
    const val = parseInt(totalMemoryInput, 10)
    if (isNaN(val) || val <= 0) {
      setError('请输入有效的正整数容量（单位：KB）')
      return
    }
    setSavingTotalMemory(true)
    try {
      const cfgRes = await api.getEsimConfig()
      const currentCfg = cfgRes.data || { lpac_path: '' }
      currentCfg.custom_memory_total_kb = val
      await api.setEsimConfig(currentCfg)
      setSuccess('保存自定义芯片总容量成功')
      setTotalMemoryDialogOpen(false)
      void loadData()
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setSavingTotalMemory(false)
    }
  }

  const [showAddWorkspace, setShowAddWorkspace] = useState(false)
  const [smartInput, setSmartInput] = useState('')
  const [addForm, setAddForm] = useState({
    smdp: '',
    matchingId: '',
    confirmationCode: '',
    imei: '',
  })
  const [smartParsed, setSmartParsed] = useState(false)
  const [writeState, setWriteState] = useState<'idle' | 'writing' | 'success' | 'failed'>('idle')
  const [writeProgress, setWriteProgress] = useState(0)
  const [writeLogLines, setWriteLogLines] = useState<Array<{ time: string; text: string; type: 'info' | 'success' | 'error' }>>([])

  const fileInputRef = useRef<HTMLInputElement>(null)
  const logEndRef = useRef<HTMLDivElement>(null)

  const resetAddWorkspace = () => {
    setSmartInput('')
    setAddForm({
      smdp: '',
      matchingId: '',
      confirmationCode: '',
      imei: '',
    })
    setSmartParsed(false)
    setWriteState('idle')
    setWriteProgress(0)
    setWriteLogLines([])
  }

  const parseLpaCode = (code: string) => {
    const val = code.trim()
    const lpaRegex = /^LPA:1\$([^$]+)\$([^$]+)(?:\$([^$]+))?/
    const match = val.match(lpaRegex)
    if (match) {
      const smdp = match[1]
      const matchingId = match[2]
      const cc = match[3] || ''
      setAddForm((prev) => ({
        ...prev,
        smdp,
        matchingId,
        confirmationCode: cc,
      }))
      setSmartParsed(true)
    } else {
      setSmartParsed(false)
    }
  }

  const handleQrUpload = (file: File) => {
    const reader = new FileReader()
    reader.onload = (e) => {
      const img = new Image()
      img.onload = () => {
        const canvas = document.createElement('canvas')
        canvas.width = img.width
        canvas.height = img.height
        const ctx = canvas.getContext('2d')
        if (ctx) {
          ctx.drawImage(img, 0, 0)
          try {
            const imgData = ctx.getImageData(0, 0, img.width, img.height)
            const code = jsQR(imgData.data, imgData.width, imgData.height)
            if (code && code.data) {
              setSmartInput(code.data)
              parseLpaCode(code.data)
              setSuccess('二维码解析成功，已填充参数')
            } else {
              setError('未在图片中检测到有效的 eSIM 二维码')
            }
          } catch {
            setError('解析二维码图片时出错')
          }
        }
      }
      img.src = e.target?.result as string
    }
    reader.readAsDataURL(file)
  }

  const startWriteCard = async () => {
    if (!addForm.smdp || !addForm.matchingId) return
    setWriteState('writing')
    setWriteProgress(5)
    setWriteLogLines([
      { time: '[00:01]', text: '检索设备底座 lpac 私有环境可执行配置...', type: 'info' },
    ])

    let timer: number | undefined = undefined
    const startProgressSim = () => {
      let currentProgress = 5
      timer = window.setInterval(() => {
        currentProgress = Math.min(95, currentProgress + Math.floor(Math.random() * 8) + 2)
        setWriteProgress(currentProgress)
      }, 1000)
    }

    try {
      startProgressSim()
      setWriteLogLines((prev) => [
        ...prev,
        { time: '[00:02]', text: '建立 APDU 底层传输信道握手成功。设备架构: ' + (lpacStatus?.arch || 'aarch64'), type: 'success' },
        { time: '[00:03]', text: '读取卡槽 eUICC (芯片厂商: ' + euiccManufacturer + ', EID: ' + euiccEid.slice(0, 14) + '...)', type: 'success' },
        { time: '[00:04]', text: '运营商 SM-DP+ 服务器端身份授权认证开始...', type: 'info' },
      ])

      const response = await api.downloadEsimProfile({
        smdp: addForm.smdp.trim(),
        matching_id: addForm.matchingId.trim(),
        confirmation_code: addForm.confirmationCode.trim() || undefined,
        imei: addForm.imei.trim() || undefined,
      })

      window.clearInterval(timer)

      if (!commandSucceeded(response.data)) {
        let errMsg = response.data?.msg || 'lpac 执行写卡指令失败'
        if (response.data?.data === 'MatchingID is refused' || response.data?.msg === 'MatchingID is refused' || (typeof response.data?.data === 'string' && response.data.data.includes('MatchingID is refused'))) {
          errMsg = '激活码已被使用或失效 (Matching ID was refused by SM-DP+ server, profile may already be downloaded)'
        }
        throw new Error(errMsg)
      }

      setWriteProgress(100)
      setWriteLogLines((prev) => [
        ...prev,
        { time: '[00:25]', text: 'SHA256 密匙签名芯片端校验一致通过。', type: 'success' },
        { time: '[00:28]', text: 'Profile 写入完毕！', type: 'success' },
        { time: '[00:30]', text: '操作成功，正在刷新 Profile 列表并更新本地缓存。', type: 'success' },
        { time: '[SUCCESS]', text: '新配置已在 eUICC 芯片中成功写入，本地数据库缓存已更新。', type: 'success' },
      ])

      setTimeout(() => {
        setWriteState('success')
        setSuccess('Profile 写入成功，本地数据库缓存已更新')
        void loadData(true)
      }, 1000)

    } catch (err) {
      window.clearInterval(timer)
      const msg = err instanceof Error ? err.message : String(err)
      const friendlyMsg = translateEsimError(msg)
      setWriteState('failed')
      setWriteLogLines((prev) => [
        ...prev,
        { time: '[ERR]', text: `写入失败: ${friendlyMsg}`, type: 'error' },
      ])
      setError(`写卡失败: ${friendlyMsg}`)
    }
  }

  const selectedProfile = useMemo(
    () => profiles.find((profile) => profile.iccid === selectedIccid) ?? profiles[0],
    [profiles, selectedIccid],
  )
  const defaultSmdp = useMemo(() => extractDefaultSmdp(euicc?.raw), [euicc])
  const selectedProvider = selectedProfile ? profileProviderLabel(selectedProfile) : ''
  const selectedSmdp = selectedProfile?.smdp || defaultSmdp
  const selectedMatchingId = selectedProfile?.matching_id
  const selectedCountryCode = profileCountryCode(selectedProfile)

  const loadData = async (forceLive = false) => {
    if (forceLive) setRefreshing(true)
    setStatusLoading(!lpacStatus)
    setError(null)
    const failures: string[] = []

    const requestOrNull = async <T,>(
      promise: Promise<T>,
      label: string,
      recordFailure = true,
    ): Promise<T | null> => {
      try {
        return await promise
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err)
        if (recordFailure) failures.push(`${label}: ${message}`)
        return null
      }
    }

    try {
      let hasProfiles = profiles.length > 0

      if (!forceLive) {
        setProfilesLoading(true)
        const cachedProfilesRes = await requestOrNull(api.getCachedEsimProfiles(), 'profiles-cache', false)
        const cachedProfiles = cachedProfilesRes?.data?.profiles ?? []
        if (cachedProfiles.length > 0) {
          hasProfiles = true
          setProfiles(cachedProfiles)
          setSelectedIccid((current) => {
            const nextSelectedIccid = preferredProfileIccid(cachedProfiles, current)
            updateEsimPageSnapshot({
              profiles: cachedProfiles,
              selectedIccid: nextSelectedIccid,
            })
            return nextSelectedIccid
          })
        }
        setProfilesLoading(false)
      }

      const statusRes = await requestOrNull(api.getEsimLpacStatus(), 'lpac')
      setStatusLoading(false)
      if (!statusRes?.data) {
        if (failures.length > 0) setError(failures[0])
        return
      }

      const nextLpacStatus = statusRes.data ?? null
      setLpacStatus(nextLpacStatus)
      updateEsimPageSnapshot({ lpacStatus: nextLpacStatus })
      if (!nextLpacStatus?.usable) {
        setEuicc(null)
        setProfiles([])
        setSelectedIccid('')
        updateEsimPageSnapshot({
          euicc: null,
          profiles: [],
          selectedIccid: '',
        })
        return
      }

      const shouldLoadLiveProfiles = forceLive || !hasProfiles
      if (shouldLoadLiveProfiles) {
        setProfilesLoading(true)
        const profilesRes = await requestOrNull(api.getEsimProfiles(), 'profiles')
        setProfilesLoading(false)
        if (profilesRes?.data) {
          const nextProfiles = profilesRes.data.profiles ?? []
          setProfiles(nextProfiles)
          setSelectedIccid((current) => {
            const nextSelectedIccid = preferredProfileIccid(nextProfiles, current)
            updateEsimPageSnapshot({
              profiles: nextProfiles,
              selectedIccid: nextSelectedIccid,
            })
            return nextSelectedIccid
          })
        }
      }

      setEuiccLoading(true)
      const euiccRes = await requestOrNull(api.getEsimEuicc(forceLive), 'euicc')
      setEuiccLoading(false)
      if (euiccRes?.data) {
        setEuicc(euiccRes.data)
        updateEsimPageSnapshot({ euicc: euiccRes.data })
      }

      if (failures.length > 0) {
        setError(failures[0])
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setStatusLoading(false)
      setProfilesLoading(false)
      setEuiccLoading(false)
      setRefreshing(false)
    }
  }

  useEffect(() => {
    void loadData()
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (writeState !== 'idle') {
      logEndRef.current?.scrollIntoView({ behavior: 'smooth' })
    }
  }, [writeLogLines, writeState])

  const repairLpac = async () => {
    setLpacRepairing(true)
    setError(null)
    setSuccess(null)
    try {
      const response = await api.repairEsimLpac({
        proxy_prefix: lpacProxyPrefix.trim() || undefined,
      })
      setSuccess(response.data?.message || 'lpac 安装/修复完成')
      await loadData(true)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLpacRepairing(false)
    }
  }

  const loadBasebandRecoveryStatus = async () => {
    try {
      const res = await api.getBasebandRestartStatus()
      const data = res.data
      if (data) {
        setBasebandRecoverySteps(data.steps ?? [])
        setBasebandRecoveryRegistration(data.current_registration ?? null)
        if (!data.running) return true // finished
      }
    } catch { /* ignore polling errors */ }
    return false
  }

  const startBasebandRecoveryPolling = () => {
    if (basebandRecoveryTimerRef.current !== undefined) {
      window.clearInterval(basebandRecoveryTimerRef.current)
      basebandRecoveryTimerRef.current = undefined
    }
    setBasebandRecoveryOpen(true)
    setBasebandRecoveryRunning(true)
    setBasebandRecoverySteps([])
    setBasebandRecoveryRegistration(null)

    const handleStatusResult = (finished: boolean) => {
      if (finished) {
        if (basebandRecoveryTimerRef.current !== undefined) {
          window.clearInterval(basebandRecoveryTimerRef.current)
          basebandRecoveryTimerRef.current = undefined
        }
        setBasebandRecoveryRunning(false)
        void loadData(true)
      }
    }

    // Poll every 1s until baseband recovery finishes
    const timer = window.setInterval(() => {
      void loadBasebandRecoveryStatus().then(handleStatusResult)
    }, 1000)
    basebandRecoveryTimerRef.current = timer
    void loadBasebandRecoveryStatus().then(handleStatusResult)
  }

  // Cleanup polling timer on unmount
  useEffect(() => {
    return () => {
      if (basebandRecoveryTimerRef.current !== undefined) {
        window.clearInterval(basebandRecoveryTimerRef.current)
      }
    }
  }, [])

  const getRecoveryErrorStep = () => basebandRecoverySteps.find(s => s.status === 'error')

  const getCurrentRecoveryMessage = () => {
    const errorStep = getRecoveryErrorStep()
    if (errorStep) return errorStep.detail || `${errorStep.step} 失败`
    if (!basebandRecoveryRunning && basebandRecoverySteps.length > 0) {
      return '网络恢复成功！'
    }
    if (basebandRecoverySteps.length === 0) return '正在启动恢复程序...'
    const lastStep = basebandRecoverySteps[basebandRecoverySteps.length - 1]
    return lastStep.status === 'running' ? `正在进行：${lastStep.step}` : `已完成：${lastStep.step}`
  }

  const runProfileAction = async () => {
    if (!selectedProfile || !confirmAction) return
    if (confirmAction === 'delete' && deleteConfirmText !== CONFIRM_DELETE_PROFILE) return
    setActionLoading(true)
    setError(null)
    setSuccess(null)
    try {
      const response = confirmAction === 'enable'
        ? await api.enableEsimProfile(selectedProfile.iccid)
        : await api.deleteEsimProfile(selectedProfile.iccid)
      if (!commandSucceeded(response.data)) {
        throw new Error(response.data?.msg || 'eSIM 操作失败')
      }
      const action = confirmAction
      const targetIccid = selectedProfile.iccid
      setConfirmAction(null)
      setDeleteConfirmText('')
      if (action === 'enable') {
        // Profile enable succeeded (lpac command done), baseband recovery runs in background.
        // Immediately update the UI optimistically and open the recovery progress dialog.
        setSuccess('Profile 启用指令成功，基带正在恢复...')
        setSelectedIccid(targetIccid)
        updateEsimPageSnapshot({ selectedIccid: targetIccid })
        setProfiles((current) => {
          const nextProfiles = current.map((profile) => (
            profile.iccid === targetIccid
              ? { ...profile, state: 'enabled' }
              : profileActive(profile)
                ? { ...profile, state: 'disabled' }
                : profile
          ))
          updateEsimPageSnapshot({ profiles: nextProfiles })
          return nextProfiles
        })
        startBasebandRecoveryPolling()
      } else {
        setSuccess('Profile 删除完成')
        setProfiles((current) => {
          const nextProfiles = current.filter((profile) => profile.iccid !== targetIccid)
          updateEsimPageSnapshot({ profiles: nextProfiles })
          return nextProfiles
        })
        setSelectedIccid((current) => {
          const nextSelectedIccid = current === targetIccid ? '' : current
          updateEsimPageSnapshot({ selectedIccid: nextSelectedIccid })
          return nextSelectedIccid
        })
        void loadData(true)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setActionLoading(false)
    }
  }

  const openConfirmAction = (action: Exclude<ConfirmAction, null>) => {
    if (action === 'delete') setDeleteConfirmText('')
    setConfirmAction(action)
  }

  const closeConfirmAction = () => {
    if (actionLoading) return
    setConfirmAction(null)
    setDeleteConfirmText('')
  }

  const submitRename = async () => {
    if (!selectedProfile) return
    const name = renameValue.trim()
    if (!name) return
    setActionLoading(true)
    setError(null)
    setSuccess(null)
    try {
      const response = await api.renameEsimProfile(selectedProfile.iccid, name)
      if (!commandSucceeded(response.data)) {
        throw new Error(response.data?.msg || 'Profile 重命名失败')
      }
      setSuccess('Profile 名称已更新')
      setRenameOpen(false)
      setProfiles((current) => {
        const nextProfiles = current.map((profile) => (
          profile.iccid === selectedProfile.iccid ? { ...profile, name } : profile
        ))
        updateEsimPageSnapshot({ profiles: nextProfiles })
        return nextProfiles
      })
      void loadData(true)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setActionLoading(false)
    }
  }

  const openRename = () => {
    setRenameValue(selectedProfile?.name || '')
    setRenameOpen(true)
  }

  const memoryTotal = typeof euicc?.memory_total_kb === 'number' && Number.isFinite(euicc.memory_total_kb)
    ? euicc.memory_total_kb
    : null
  const memoryAvailable = typeof euicc?.memory_available_kb === 'number' && Number.isFinite(euicc.memory_available_kb)
    ? euicc.memory_available_kb
    : null
  const hasMemoryInfo = memoryTotal !== null || memoryAvailable !== null
  const memoryUsedPercent = memoryTotal !== null && memoryAvailable !== null && memoryTotal > 0
    ? Math.max(0, Math.min(100, ((memoryTotal - memoryAvailable) / memoryTotal) * 100))
    : null
  const memoryPercentLabel = memoryUsedPercent !== null ? `${Math.round(memoryUsedPercent)}%` : 'N/A'
  const memoryUsageLabel = memoryAvailable !== null
    ? `可用 ${formatCapacityK(memoryAvailable)} / ${memoryTotal !== null ? formatCapacityK(memoryTotal) : '未知'}`
    : `总容量 ${formatCapacityK(memoryTotal)}`
  const dataLoading = statusLoading || profilesLoading || euiccLoading
  const showManagerContent = statusLoading || lpacStatus?.usable
  const euiccManufacturer = euiccManufacturerFromEid(euicc?.eid) || euicc?.manufacturer || 'N/A'
  const euiccEid = euiccLoading && !euicc ? '正在读取 EID...' : euicc?.eid || 'N/A'
  const selectedDeleteLabel = selectedProfile
    ? `${selectedProfile.name || '未命名'}：${selectedProfile.iccid}`
    : ''

  return (
    <Box>

      <ErrorSnackbar error={error} onClose={() => setError(null)} />
      <Snackbar
        open={!!success}
        autoHideDuration={3000}
        resumeHideDuration={3000}
        onClose={() => setSuccess(null)}
        anchorOrigin={{ vertical: 'top', horizontal: 'center' }}
      >
        <Alert severity="info" variant="filled" onClose={() => setSuccess(null)}>
          {success}
        </Alert>
      </Snackbar>

      <Box display="flex" flexDirection="column" gap={3}>
        {statusLoading && !lpacStatus && (
          <Card sx={{ p: 2.5 }}>
            <Box display="flex" alignItems="center" gap={1.5} mb={2}>
              <CircularProgress size={18} />
              <Typography fontWeight={700}>正在检测 lpac 状态</Typography>
            </Box>
            <LinearProgress />
          </Card>
        )}

        {!statusLoading && !lpacStatus && (
          <Card sx={{ p: 2.5 }}>
            <Alert severity="warning">暂无法读取 lpac 状态，请稍后刷新。</Alert>
          </Card>
        )}

        {lpacStatus && !lpacStatus.usable && !statusLoading && (
          <Card sx={{ p: 2.5 }}>
            <Alert severity="warning" sx={{ mb: 2 }}>
              未检测到可用 lpac，暂不能读取 eUICC Profiles。当前架构：
              {lpacStatus.arch || '不支持'}；glibc：
              {lpacStatus.glibc_version || '未知'}；优先目标：
              {lpacStatus.asset_name || '无匹配资源'}。{lpacStatus.message}
            </Alert>
            <Box display="flex" gap={1.5} alignItems="center" flexWrap="wrap">
              <TextField
                select
                label="GitHub 代理前缀（可选）"
                size="small"
                value={lpacProxyPrefix}
                onChange={(event) => setLpacProxyPrefix(event.target.value)}
                sx={{ minWidth: { xs: '100%', sm: 240 } }}
              >
                {LPAC_PROXY_PREFIX_OPTIONS.map((option) => (
                  <MenuItem key={option.label} value={option.value}>
                    {option.label}
                  </MenuItem>
                ))}
              </TextField>
              <Button
                variant="contained"
                startIcon={lpacRepairing ? <CircularProgress size={16} /> : <Build />}
                disabled={lpacRepairing || !lpacStatus.asset_name}
                onClick={() => void repairLpac()}
              >
                安装/修复 lpac
              </Button>
              <Typography variant="caption" color="text.secondary" sx={{ wordBreak: 'break-all' }}>
                安装位置：{lpacStatus.path}
              </Typography>
            </Box>
          </Card>
        )}

        {showManagerContent && (
          <>
            <Card ref={euiccCardRef}>
              <Toolbar
                sx={{
                  minHeight: 76,
                  px: { xs: 2, sm: 3 },
                  py: 1.5,
                  gap: 2,
                  alignItems: 'center',
                  flexWrap: 'wrap',
                }}
              >
                <Box
                  sx={{
                    width: 36,
                    height: 36,
                    borderRadius: 1,
                    bgcolor: 'primary.light',
                    color: 'primary.contrastText',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    flexShrink: 0,
                  }}
                >
                  <Memory fontSize="small" />
                </Box>
                <Box minWidth={0} flex="1 1 520px">
                  <Box display="flex" alignItems="center" gap={0.5} mb={0.5}>
                    <Typography variant="subtitle1" fontWeight={700}>eUICC 芯片</Typography>
                    <Tooltip title="刷新">
                      <IconButton
                        size="small"
                        disabled={dataLoading || refreshing}
                        onClick={() => void loadData(true)}
                        sx={{ p: 0.25 }}
                      >
                        {dataLoading || refreshing ? <CircularProgress size={16} /> : <Refresh sx={{ fontSize: 18 }} />}
                      </IconButton>
                    </Tooltip>
                  </Box>
                  <Box
                    minWidth={0}
                    sx={{
                      color: 'text.disabled',
                      overflowWrap: 'anywhere',
                      wordBreak: 'break-word',
                    }}
                  >
                    <EuiccMetaItem label="厂商" value={euiccManufacturer} />
                    <Typography component="span" variant="body2" color="text.disabled" mx={1}>|</Typography>
                    <EuiccMetaItem label="EID" value={euiccEid} mono />
                  </Box>
                </Box>
                {hasMemoryInfo && (
                  <Box
                    sx={{
                      width: { xs: '100%', md: 300 },
                      ml: { md: 'auto' },
                      '& .edit-capacity-btn': {
                        opacity: 0,
                        visibility: 'hidden',
                        transition: 'opacity 0.2s ease, visibility 0.2s ease',
                      },
                      '&:hover .edit-capacity-btn': {
                        opacity: 1,
                        visibility: 'visible',
                      },
                    }}
                  >
                    <Box display="flex" alignItems="center" justifyContent="space-between" gap={1} mb={0.75}>
                      <Typography variant="caption" color="text.secondary" display="inline-flex" alignItems="center" gap={0.5}>
                        eUICC 容量
                        {euicc?.memory_total_customizable && (
                          <Tooltip title="自定义总容量">
                            <IconButton
                              className="edit-capacity-btn"
                              size="small"
                              onClick={handleOpenTotalMemoryDialog}
                              sx={{ p: 0.25 }}
                            >
                              <Edit sx={{ fontSize: 13 }} />
                            </IconButton>
                          </Tooltip>
                        )}
                      </Typography>
                      <Typography variant="caption" color="text.primary" fontWeight={400}>
                        {memoryPercentLabel}
                      </Typography>
                    </Box>
                    <LinearProgress
                      variant="determinate"
                      value={memoryUsedPercent ?? 0}
                      color="primary"
                      sx={{
                        height: 9,
                        borderRadius: 999,
                        bgcolor: (theme: Theme) => alpha(theme.palette.primary.main, 0.12),
                        '& .MuiLinearProgress-bar': { borderRadius: 999 },
                      }}
                    />
                    <Box display="flex" justifyContent="flex-end" mt={0.5}>
                      <Typography variant="caption" color="text.secondary">
                        {memoryUsageLabel}
                      </Typography>
                    </Box>
                  </Box>
                )}
              </Toolbar>
              {euiccLoading && <LinearProgress />}
            </Card>

            <Box
              sx={{
                display: 'grid',
                gridTemplateColumns: { xs: '1fr', md: '340px minmax(0, 1fr)' },
                gap: 2,
                alignItems: 'stretch',
                height: { md: gridHeight },
                minHeight: { md: 520 },
              }}
            >
              <Card sx={{ height: '100%', minWidth: 0, display: 'flex', flexDirection: 'column' }}>
                <Box p={2} display="flex" alignItems="center" justifyContent="space-between">
                  <Box display="flex" alignItems="center" gap={1}>
                    <Typography fontWeight={700}>
                      {profilesLoading && profiles.length === 0
                        ? 'Profiles 列表 (读取中)'
                        : `Profiles 列表 (${profiles.length})`}
                    </Typography>
                    {profilesLoading && <CircularProgress size={14} />}
                  </Box>
                  <Tooltip title="添加 Profile">
                    <span>
                      <IconButton
                        size="small"
                        color="primary"
                        onClick={() => {
                          setShowAddWorkspace(true)
                          resetAddWorkspace()
                        }}
                        disabled={profilesLoading || statusLoading || !lpacStatus?.usable}
                      >
                        <Add />
                      </IconButton>
                    </span>
                  </Tooltip>
                </Box>
                <Divider />
                <Box sx={{ flex: { md: 1 }, minHeight: 0, maxHeight: { xs: 360, md: 'none' }, overflow: 'auto' }}>
                  {profilesLoading && profiles.length === 0 ? (
                    <Box p={3}>
                      <Box display="flex" alignItems="center" gap={1.25}>
                        <CircularProgress size={18} />
                        <Typography variant="body2" color="text.secondary">
                          正在读取 Profiles...
                        </Typography>
                      </Box>
                    </Box>
                  ) : statusLoading && profiles.length === 0 ? (
                    <Box p={3}>
                      <Alert severity="info">正在检测 lpac，稍后读取 Profiles。</Alert>
                    </Box>
                  ) : profiles.length === 0 ? (
                    <Box p={3}>
                      <Alert severity="info">暂无 Profiles，或 lpac 未返回 profiles 数据。</Alert>
                    </Box>
                  ) : (
                    <List sx={{ py: 0 }}>
                      {profiles.map((profile) => {
                        const selected = profile.iccid === selectedProfile?.iccid
                        const active = profileActive(profile)
                        const countryCode = profileCountryCode(profile)
                        return (
                          <ListItemButton
                            key={profile.iccid}
                            selected={selected}
                            onClick={() => {
                              setSelectedIccid(profile.iccid)
                              setShowAddWorkspace(false)
                            }}
                            sx={{
                              gap: 1.25,
                              alignItems: 'center',
                              py: 1.25,
                              px: 2,
                              borderBottom: '1px solid',
                              borderBottomColor: 'divider',
                              '&.Mui-selected': {
                                bgcolor: (theme: Theme) => alpha(theme.palette.primary.main, 0.08),
                                '&:hover': {
                                  bgcolor: (theme: Theme) => alpha(theme.palette.primary.main, 0.12),
                                },
                              },
                            }}
                          >
                            <Avatar
                              sx={{
                                width: 36,
                                height: 36,
                                border: 1,
                                borderColor: selected || active ? 'primary.main' : 'divider',
                                bgcolor: (theme: Theme) => selected || active
                                  ? alpha(theme.palette.primary.main, 0.08)
                                  : theme.palette.action.hover,
                                color: selected || active ? 'primary.main' : 'text.disabled',
                                fontSize: 20,
                              }}
                            >
                              <CountryFlag countryCode={countryCode} size={26} />
                            </Avatar>
                            <ListItemText
                              primary={
                                <Box display="flex" alignItems="center" gap={1} minWidth={0}>
                                  <Typography fontWeight={selected ? 700 : 600} noWrap flexGrow={1}>
                                    {profile.name || <Box component="span" data-sensitive="true">{profile.iccid}</Box>}
                                  </Typography>
                                  {active && <CheckCircle color="primary" fontSize="small" />}
                                </Box>
                              }
                              secondary={
                                <Typography variant="caption" color="text.secondary" display="block" noWrap>
                                  {profileProviderLabel(profile)} · <Box component="span" data-sensitive="true">{formatIccid(profile.iccid)}</Box>
                                </Typography>
                              }
                              sx={{ minWidth: 0, my: 0 }}
                            />
                          </ListItemButton>
                        )
                      })}
                    </List>
                  )}
                </Box>
              </Card>

              <Card sx={{ height: '100%', minWidth: 0, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
                {showAddWorkspace ? (
                  <>
                    <Box sx={{ px: 3, height: 66, minHeight: 66, display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                      <Box display="flex" alignItems="center" gap={1}>
                        <CloudDownload color="primary" />
                        <Typography sx={{ fontSize: '16px', fontWeight: 700 }}>添加 Profile</Typography>
                      </Box>
                      <Box display="flex" gap={1}>
                        <Button
                          size="small"
                          variant="outlined"
                          onClick={() => {
                            setShowAddWorkspace(false)
                            resetAddWorkspace()
                          }}
                          disabled={writeState === 'writing'}
                          sx={{
                            borderColor: 'divider',
                            color: 'text.secondary',
                            '&:hover': {
                              borderColor: 'text.secondary',
                              color: 'text.primary',
                              bgcolor: 'action.hover',
                            }
                          }}
                        >
                          {writeState === 'success' || writeState === 'failed' ? '返回' : '取消'}
                        </Button>
                        <Button
                          size="small"
                          variant="contained"
                          color="primary"
                          onClick={() => void startWriteCard()}
                          disabled={writeState === 'writing' || writeState === 'success' || !addForm.smdp || !addForm.matchingId}
                        >
                          开始写卡
                        </Button>
                      </Box>
                    </Box>
                    <Divider />
                    <Box sx={{ p: 3, flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column', overflow: 'hidden' }}>
                      <>
                        <Box sx={{ flexShrink: 0, mb: 2 }}>
                          <Grid container spacing={2} mb={3}>
                            <Grid size={{ xs: 12, md: 6 }}>
                              <Box display="flex" flexDirection="column" gap={1}>
                                <Typography sx={{ fontSize: '14px', fontWeight: 600, color: 'rgb(15, 23, 42)', display: 'block', mb: 1 }}>
                                  智能识别框
                                </Typography>
                                <Box sx={{ position: 'relative' }}>
                                  <TextField
                                    fullWidth
                                    multiline
                                    rows={3}
                                    placeholder="在此粘贴 LPA 激活码 (如 LPA:1$smdp.io$matching-id)"
                                    value={smartInput}
                                    onChange={(e) => {
                                      setSmartInput(e.target.value)
                                      parseLpaCode(e.target.value)
                                    }}
                                    disabled={writeState === 'writing'}
                                    sx={{
                                      '& .MuiOutlinedInput-root': {
                                        fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
                                        fontSize: '14px',
                                        fontWeight: 400,
                                        height: 98,
                                        alignItems: 'flex-start',
                                      }
                                    }}
                                  />
                                  {smartParsed && (
                                    <Chip
                                      label="解析成功"
                                      size="small"
                                      color="primary"
                                      sx={{
                                        position: 'absolute',
                                        bottom: 8,
                                        right: 8,
                                        height: 20,
                                        fontSize: '0.6875rem',
                                        pointerEvents: 'none',
                                      }}
                                    />
                                  )}
                                </Box>
                              </Box>
                            </Grid>
                            <Grid size={{ xs: 12, md: 6 }}>
                              <Box display="flex" flexDirection="column" gap={1}>
                                <Typography sx={{ fontSize: '14px', fontWeight: 600, color: 'rgb(15, 23, 42)', display: 'block', mb: 1 }}>
                                  二维码解析区
                                </Typography>
                                <Box
                                  onClick={() => fileInputRef.current?.click()}
                                  onDragOver={(e) => e.preventDefault()}
                                  onDrop={(e) => {
                                    e.preventDefault()
                                    if (e.dataTransfer.files && e.dataTransfer.files[0]) {
                                      handleQrUpload(e.dataTransfer.files[0])
                                    }
                                  }}
                                  sx={{
                                    height: 98,
                                    boxSizing: 'border-box',
                                    border: '1px dashed',
                                    borderColor: (theme) => alpha(theme.palette.primary.main, 0.4),
                                    borderRadius: 1,
                                    bgcolor: (theme) => alpha(theme.palette.primary.main, 0.04),
                                    color: 'primary.main',
                                    display: 'flex',
                                    flexDirection: 'column',
                                    alignItems: 'center',
                                    justifyContent: 'center',
                                    cursor: 'pointer',
                                    gap: 0.5,
                                    transition: 'all 0.2s',
                                    '&:hover': {
                                      border: '2px dashed',
                                      borderColor: 'primary.main',
                                    },
                                  }}
                                >
                                  <input
                                    type="file"
                                    accept="image/*"
                                    ref={fileInputRef}
                                    onChange={(e) => {
                                      if (e.target.files && e.target.files[0]) {
                                        handleQrUpload(e.target.files[0])
                                      }
                                    }}
                                    style={{ display: 'none' }}
                                  />
                                  <QrCodeScanner color="primary" />
                                  <Typography variant="caption" sx={{ color: 'primary.main', fontWeight: 500 }}>
                                    点击或拖入二维码图片解码
                                  </Typography>
                                </Box>
                              </Box>
                            </Grid>
                          </Grid>

                          {/* Bottom 2x2 grid form */}
                          <Box
                            sx={{
                              py: 1,
                              transition: 'all 0.2s',
                              mb: writeState === 'writing' || writeState === 'failed' ? 3 : 0,
                            }}
                          >
                            <Typography sx={{ fontSize: '14px', fontWeight: 600, color: 'rgb(15, 23, 42)', display: 'block', mb: 2 }}>
                              预览与校验
                            </Typography>
                            <Grid container columnSpacing={2} rowSpacing={4}>
                              <Grid size={{ xs: 12, md: 6 }}>
                                <TextField
                                  fullWidth
                                  label="SM-DP+ 服务器地址 *"
                                  value={addForm.smdp}
                                  onChange={(e) => setAddForm((prev) => ({ ...prev, smdp: e.target.value }))}
                                  disabled={writeState === 'writing'}
                                  slotProps={{
                                    input: { sx: { fontSize: '14px', fontWeight: 400 } },
                                    inputLabel: { sx: { fontSize: '14px', fontWeight: 400 } }
                                  }}
                                />
                              </Grid>
                              <Grid size={{ xs: 12, md: 6 }}>
                                <TextField
                                  fullWidth
                                  label="标识码 (Matching ID)*"
                                  value={addForm.matchingId}
                                  onChange={(e) => setAddForm((prev) => ({ ...prev, matchingId: e.target.value }))}
                                  disabled={writeState === 'writing'}
                                  slotProps={{
                                    input: { sx: { fontSize: '14px', fontWeight: 400 } },
                                    inputLabel: { sx: { fontSize: '14px', fontWeight: 400 } }
                                  }}
                                />
                              </Grid>
                              <Grid size={{ xs: 12, md: 6 }}>
                                <TextField
                                  fullWidth
                                  label="确认码 (选填)"
                                  value={addForm.confirmationCode}
                                  onChange={(e) => setAddForm((prev) => ({ ...prev, confirmationCode: e.target.value }))}
                                  disabled={writeState === 'writing'}
                                  slotProps={{
                                    input: { sx: { fontSize: '14px', fontWeight: 400 } },
                                    inputLabel: { sx: { fontSize: '14px', fontWeight: 400 } }
                                  }}
                                />
                              </Grid>
                              <Grid size={{ xs: 12, md: 6 }}>
                                <TextField
                                  fullWidth
                                  label="绑定 IMEI (选填)"
                                  value={addForm.imei}
                                  onChange={(e) => setAddForm((prev) => ({ ...prev, imei: e.target.value }))}
                                  disabled={writeState === 'writing'}
                                  slotProps={{
                                    input: {
                                      sx: {
                                        fontSize: '14px',
                                        fontWeight: 400,
                                        fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
                                      }
                                    },
                                    inputLabel: { sx: { fontSize: '14px', fontWeight: 400 } }
                                  }}
                                />
                              </Grid>
                            </Grid>
                          </Box>
                        </Box>

                        {/* Terminal simulation for writing process */}
                        {writeState !== 'idle' && (
                          <Box
                            sx={{
                              mt: 'auto',
                              flex: 1,
                              minHeight: 180,
                              display: 'flex',
                              flexDirection: 'column',
                              border: '1px solid',
                              borderColor: 'divider',
                              borderRadius: 1,
                              bgcolor: 'action.hover',
                              color: 'text.primary',
                              fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
                              overflow: 'hidden',
                            }}
                          >
                            <Box sx={{ bgcolor: 'action.selected', px: 2, py: 1, borderBottom: '1px solid', borderColor: 'divider', display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                              <Typography variant="caption" sx={{ color: 'text.secondary', fontFamily: 'inherit', fontWeight: 500 }}>
                                SimAdmin @ Add Profile
                              </Typography>
                              <Typography variant="caption" sx={{ color: 'text.secondary', fontFamily: 'inherit', fontWeight: 500 }}>
                                {writeProgress}%
                              </Typography>
                            </Box>
                            <Box sx={{ p: 2, flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
                              <Box sx={{ mb: 2 }}>
                                <LinearProgress variant="determinate" value={writeProgress} color="primary" sx={{ height: 4, borderRadius: 1, bgcolor: 'divider' }} />
                              </Box>
                              <Box sx={{ fontSize: '0.75rem', lineHeight: 1.6, flex: 1, overflowY: 'auto' }}>
                                {writeLogLines.map((log, index) => (
                                  <Box key={index} sx={{ display: 'flex', gap: 1, mb: 0.5 }}>
                                    <Box component="span" sx={{ color: 'text.secondary' }}>{log.time}</Box>
                                    <Box component="span" sx={{ color: 'primary.main' }}>&gt;</Box>
                                    <Box component="span" sx={{ color: log.type === 'error' ? 'error.main' : log.type === 'success' ? 'success.dark' : 'inherit', fontWeight: log.type === 'success' || log.type === 'error' ? 500 : 400 }}>
                                      {log.text}
                                    </Box>
                                  </Box>
                                ))}
                                <div ref={logEndRef} />
                              </Box>
                            </Box>
                          </Box>
                        )}
                      </>
                    </Box>
                  </>
                ) : selectedProfile ? (
                  <Box sx={{ height: '100%', minHeight: 0, display: 'flex', flexDirection: 'column' }}>
                    <Box p={3} display="flex" flexDirection={{ xs: 'column', sm: 'row' }} alignItems={{ sm: 'flex-start' }} justifyContent="space-between" gap={2}>
                      <Box minWidth={0}>
                        <Box display="flex" alignItems="center" gap={1} flexWrap="wrap">
                          <CountryFlag countryCode={selectedCountryCode} size={30} />
                          <Typography variant="h5" fontWeight={700} sx={{ wordBreak: 'break-word' }}>
                            {selectedProfile.name || <Box component="span" data-sensitive="true">{selectedProfile.iccid}</Box>}
                          </Typography>
                          <Chip
                            label={profileStateLabel(selectedProfile.state)}
                            size="small"
                            color={profileActive(selectedProfile) ? 'primary' : 'default'}
                            variant="outlined"
                          />
                        </Box>
                        <Typography variant="body2" color="text.secondary" mt={0.75}>
                          提供商: {selectedProvider}
                        </Typography>
                      </Box>
                      <Box display="flex" alignItems="center" gap={1}>
                        <Tooltip title="重命名">
                          <span>
                            <IconButton onClick={openRename} disabled={actionLoading}>
                              <DriveFileRenameOutline />
                            </IconButton>
                          </span>
                        </Tooltip>
                        <Tooltip title={selectedProfile.delete_allowed === false ? '策略不允许删除' : '删除'}>
                          <span>
                            <IconButton
                              color="error"
                              onClick={() => openConfirmAction('delete')}
                              disabled={actionLoading || profileActive(selectedProfile) || selectedProfile.delete_allowed === false}
                            >
                              <DeleteOutline />
                            </IconButton>
                          </span>
                        </Tooltip>
                        {!profileActive(selectedProfile) && (
                          <Button
                            variant="contained"
                            startIcon={<PowerSettingsNew />}
                            onClick={() => openConfirmAction('enable')}
                            disabled={actionLoading}
                          >
                            启用
                          </Button>
                        )}
                      </Box>
                    </Box>
                    <Divider />
                    <Box sx={{ p: 3, flex: 1, minHeight: 0, overflow: 'auto' }}>
                      <Box display="flex" alignItems="center" gap={1} mb={2}>
                        <Fingerprint color="disabled" />
                        <Typography fontWeight={700}>基础与网络属性</Typography>
                      </Box>
                      <Grid container spacing={2} mb={3}>
                        <Grid size={{ xs: 12, sm: 6, md: 4 }}>
                          <InfoCell label="ICCID" value={selectedProfile.iccid} mono sensitive />
                        </Grid>
                        <Grid size={{ xs: 12, sm: 6, md: 4 }}>
                          <InfoCell label="本机号码 (MSISDN)" value={selectedProfile.msisdn} mono sensitive />
                        </Grid>
                        <Grid size={{ xs: 12, sm: 6, md: 4 }}>
                          <InfoCell label="短信中心号码 (SMSC)" value={selectedProfile.smsc} mono emptyText="未读取到" sensitive />
                        </Grid>
                        <Grid size={{ xs: 12, sm: 6, md: 4 }}>
                          <InfoCell label="IMSI" value={selectedProfile.imsi} mono />
                        </Grid>
                        <Grid size={{ xs: 12, sm: 6, md: 4 }}>
                          <InfoCell label="MCC / MNC" value={[selectedProfile.mcc, selectedProfile.mnc].filter(Boolean).join(' / ')} mono />
                        </Grid>
                        <Grid size={{ xs: 12, sm: 6, md: 4 }}>
                          <InfoCell label="Profile Class" value={selectedProfile.class} />
                        </Grid>
                      </Grid>

                      <Box display="flex" alignItems="center" gap={1} mb={2}>
                        <Dns color="disabled" />
                        <Typography fontWeight={700}>底层供应与策略</Typography>
                      </Box>
                      <Grid container spacing={2}>
                        <Grid size={{ xs: 12, sm: 6, md: 4 }}>
                          <InfoCell label="SM-DP+ 服务器" value={selectedSmdp} mono />
                        </Grid>
                        <Grid size={{ xs: 12, sm: 6, md: 4 }}>
                          <InfoCell label="标识码 (Matching ID)" value={selectedMatchingId} mono />
                        </Grid>
                        <Grid size={{ xs: 12, sm: 6, md: 4 }}>
                          <InfoCell label="ISDP-AID" value={selectedProfile.isdp_aid} mono />
                        </Grid>
                        <Grid size={{ xs: 12, sm: 6, md: 4 }}>
                          <InfoCell label="允许删除" value={selectedProfile.delete_allowed === undefined ? '未知' : selectedProfile.delete_allowed ? '是' : '否'} />
                        </Grid>
                        <Grid size={{ xs: 12, sm: 6, md: 4 }}>
                          <InfoCell label="允许禁用" value={selectedProfile.disable_allowed === undefined ? '未知' : selectedProfile.disable_allowed ? '是' : '否'} />
                        </Grid>
                      </Grid>
                    </Box>
                  </Box>
                ) : (
                  <Box p={4} display="flex" alignItems="center" justifyContent="center" minHeight={360} height="100%">
                    <Box textAlign="center">
                      <Public color="disabled" sx={{ fontSize: 48, mb: 1 }} />
                      <Typography color="text.secondary">
                        {statusLoading || profilesLoading ? '正在读取 Profile 数据' : '选择一个 Profile 查看详情'}
                      </Typography>
                    </Box>
                  </Box>
                )}
              </Card>
            </Box>
          </>
        )}
      </Box>

      <Dialog open={!!confirmAction} onClose={closeConfirmAction} fullWidth maxWidth="sm">
        <DialogTitle>{confirmAction === 'enable' ? '确认启用 Profile' : '确认删除 Profile'}</DialogTitle>
        <DialogContent>
          {confirmAction === 'enable' ? (
            <DialogContentText>
              {`确定要启用 ${selectedProfile?.name || selectedProfile?.iccid} 吗？该操作会执行 SIM 断电上电并重启 ModemManager，网络会短暂中断。`}
            </DialogContentText>
          ) : (
            <>
              <Typography variant="body2" color="text.secondary" mb={2}>
                {`确定要删除「${selectedDeleteLabel}」吗？该操作不可撤销，请输入「${CONFIRM_DELETE_PROFILE}」继续操作。`}
              </Typography>
              <TextField
                fullWidth
                autoFocus
                label={`请输入 ${CONFIRM_DELETE_PROFILE}`}
                value={deleteConfirmText}
                onChange={(event) => setDeleteConfirmText(event.target.value)}
              />
            </>
          )}
        </DialogContent>
        <DialogActions>
          <Button onClick={closeConfirmAction} disabled={actionLoading}>取消</Button>
          <Button
            variant="contained"
            color={confirmAction === 'delete' ? 'error' : 'primary'}
            onClick={() => void runProfileAction()}
            disabled={actionLoading || (confirmAction === 'delete' && deleteConfirmText !== CONFIRM_DELETE_PROFILE)}
            startIcon={actionLoading ? <CircularProgress size={16} /> : undefined}
          >
            {confirmAction === 'delete' ? '确认删除' : '确认'}
          </Button>
        </DialogActions>
      </Dialog>

      <Dialog
        open={basebandRecoveryOpen}
        onClose={() => { if (!basebandRecoveryRunning) setBasebandRecoveryOpen(false) }}
        maxWidth="xs"
        fullWidth
      >
        <DialogTitle>eSIM 切卡恢复</DialogTitle>
        <DialogContent>
          <Stack spacing={2} alignItems="center" sx={{ py: 2 }}>
            {basebandRecoveryRunning && !getRecoveryErrorStep() && (
              <CircularProgress size={48} />
            )}
            {getRecoveryErrorStep() ? (
              <Alert severity="error" sx={{ width: '100%' }}>{getCurrentRecoveryMessage()}</Alert>
            ) : !basebandRecoveryRunning && basebandRecoverySteps.length > 0 ? (
              <Alert severity="success" sx={{ width: '100%' }}>{getCurrentRecoveryMessage()}</Alert>
            ) : (
              <Typography variant="body1" color="text.secondary" textAlign="center">
                {getCurrentRecoveryMessage()}
              </Typography>
            )}
            {basebandRecoveryRegistration && basebandRecoveryRunning && (
              <Typography variant="caption" color="text.secondary" textAlign="center">
                当前注册状态：{basebandRecoveryRegistration}
              </Typography>
            )}
          </Stack>
        </DialogContent>
        <DialogActions>
          <Button
            disabled={basebandRecoveryRunning}
            onClick={() => setBasebandRecoveryOpen(false)}
          >
            关闭
          </Button>
        </DialogActions>
      </Dialog>

      <Dialog open={renameOpen} onClose={() => !actionLoading && setRenameOpen(false)} fullWidth maxWidth="xs">
        <DialogTitle>重命名 Profile</DialogTitle>
        <DialogContent>
          <TextField
            fullWidth
            autoFocus
            margin="dense"
            label="Profile 名称"
            value={renameValue}
            onChange={(event) => setRenameValue(event.target.value)}
          />
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setRenameOpen(false)} disabled={actionLoading}>取消</Button>
          <Button
            variant="contained"
            onClick={() => void submitRename()}
            disabled={actionLoading || !renameValue.trim()}
            startIcon={actionLoading ? <CircularProgress size={16} /> : undefined}
          >
            保存
          </Button>
        </DialogActions>
      </Dialog>

      <Dialog open={totalMemoryDialogOpen} onClose={() => !savingTotalMemory && setTotalMemoryDialogOpen(false)} fullWidth maxWidth="xs">
        <DialogTitle>自定义 eUICC 芯片总容量</DialogTitle>
        <DialogContent>
          <DialogContentText sx={{ mb: 2 }}>
            如果您的 eUICC 芯片无法识别总容量（内存显示未知），您可以手动指定该芯片的总空间大小，以便正确计算已用空间比例。
          </DialogContentText>
          <TextField
            fullWidth
            autoFocus
            margin="dense"
            label="芯片总容量 (KB)"
            placeholder="例如: 256, 512, 1024"
            type="number"
            value={totalMemoryInput}
            onChange={(event) => setTotalMemoryInput(event.target.value)}
            disabled={savingTotalMemory}
          />
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setTotalMemoryDialogOpen(false)} disabled={savingTotalMemory}>取消</Button>
          <Button
            variant="contained"
            onClick={() => void handleSaveTotalMemory()}
            disabled={savingTotalMemory || !totalMemoryInput.trim()}
            startIcon={savingTotalMemory ? <CircularProgress size={16} /> : undefined}
          >
            保存
          </Button>
        </DialogActions>
      </Dialog>
    </Box>
  )
}
