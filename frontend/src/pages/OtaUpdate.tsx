import { useState, useEffect, useCallback, useRef, useMemo } from 'react'
import type { ChangeEvent, ReactNode, RefObject } from 'react'
import {
  Box,
  Typography,
  Card,
  CardContent,
  Button,
  CircularProgress,
  Alert,
  AlertTitle,
  Chip,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableRow,
  Paper,
  LinearProgress,
  Dialog,
  DialogTitle,
  DialogContent,
  DialogContentText,
  DialogActions,
  Divider,
  FormControl,
  FormControlLabel,
  InputLabel,
  Link,
  MenuItem,
  Select,
  Switch,
  TextField,
  Radio,
} from '@mui/material'
import Grid from '@mui/material/Grid'
import {
  CloudUpload,
  CheckCircle,
  Error as ErrorIcon,
  Warning,
  Info,
  Refresh,
  SystemUpdateAlt,
  Cancel,
  RestartAlt,
  Public,
  Search,
  Download,
  Bolt,
  Memory,
} from '@mui/icons-material'
import { useSimAdminApi } from '../contexts/ApiContext'
import type {
  OtaLatestReleaseResponse,
  OtaReleaseAsset,
  OtaStatusResponse,
  OtaUploadResponse,
} from '../api/types'

type ProxyPreset = 'https://gh-proxy.com/' | 'https://ghproxy.net/' | 'https://githubproxy.cc/' | 'custom'
type OnlineUpdateState = 'idle' | 'checking' | 'available' | 'latest' | 'downloading'
type MarkdownHeadingLevel = 1 | 2 | 3 | 4 | 5 | 6
type MarkdownListItem = { text: string; indent: number }
type MarkdownBlock =
  | { type: 'heading'; level: MarkdownHeadingLevel; text: string }
  | { type: 'paragraph'; text: string }
  | { type: 'list'; ordered: boolean; items: MarkdownListItem[] }
  | { type: 'code'; code: string; language?: string }
  | { type: 'quote'; text: string }
  | { type: 'rule' }

const GITHUB_LATEST_RELEASE_PAGE = 'https://github.com/3899/SimAdmin/releases/latest'
const GITHUB_LATEST_RELEASE_API = 'https://api.github.com/repos/3899/SimAdmin/releases/latest'
const BEIJING_TIME_ZONE = 'Asia/Shanghai'

function normalizeVersion(version: string) {
  return version.trim().replace(/^v/i, '')
}

function compareVersions(a: string, b: string) {
  const aParts = normalizeVersion(a).split(/[.-]/)
  const bParts = normalizeVersion(b).split(/[.-]/)
  const length = Math.max(aParts.length, bParts.length)

  for (let i = 0; i < length; i += 1) {
    const aPart = aParts[i] ?? '0'
    const bPart = bParts[i] ?? '0'
    const aNum = Number(aPart)
    const bNum = Number(bPart)

    if (Number.isFinite(aNum) && Number.isFinite(bNum)) {
      if (aNum !== bNum) return aNum - bNum
      continue
    }

    const textCompare = aPart.localeCompare(bPart)
    if (textCompare !== 0) return textCompare
  }

  return 0
}

function formatDateTime(value?: string) {
  if (!value) return 'N/A'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return `${date.toLocaleString('zh-CN', {
    timeZone: BEIJING_TIME_ZONE,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  })}`
}

function formatBytes(size?: number) {
  if (!size) return '未知'
  const mb = size / 1024 / 1024
  return `${mb.toFixed(1)} MB`
}

function inferArch(assetName?: string) {
  if (!assetName) return '未知'
  if (/aarch64|arm64/i.test(assetName)) return 'aarch64-unknown-linux-musl'
  if (/x86_64|amd64/i.test(assetName)) return 'x86_64-unknown-linux-musl'
  if (/armv7|armhf/i.test(assetName)) return 'armv7-unknown-linux-musleabihf'
  return '未知'
}

function resolveTargetArch(rawArch?: string | null): 'arm64' | 'x86_64' | 'unknown' {
  if (!rawArch) return 'unknown'
  const lower = rawArch.toLowerCase()
  if (lower.includes('x86_64') || lower.includes('amd64')) return 'x86_64'
  if (lower.includes('aarch64') || lower.includes('arm64')) return 'arm64'
  return 'unknown'
}

function formatArchLabel(rawArch?: string | null): string {
  const type = resolveTargetArch(rawArch)
  if (type === 'arm64') return 'aarch64 / arm64'
  if (type === 'x86_64') return 'x86_64 / amd64'
  return rawArch || '检测中...'
}

function extractReleaseCommit(release?: OtaLatestReleaseResponse | null): string {
  if (!release) return 'N/A'
  if (release.body) {
    const match = release.body.match(/-\s*\*\*Commit\*\*:\s*([a-f0-9]{7,40})/i)
    if (match && match[1]) {
      return match[1].slice(0, 7)
    }
  }
  if (release.target_commitish && /^[a-f0-9]{7,40}$/i.test(release.target_commitish)) {
    return release.target_commitish.slice(0, 7)
  }
  return 'N/A'
}

interface ClassifiedAsset {
  asset: OtaReleaseAsset
  edition: 'standard' | 'wfc'
  editionLabel: string
  isCurrentMatch: boolean
  arch: string
  sizeStr: string
}

function isWfcStatus(status: OtaStatusResponse | null) {
  const edition = (status?.current_edition || status?.installed_meta?.edition || '').toLowerCase()
  return edition.includes('wfc') || status?.installed_meta?.wificalling === true
}

function isWfcMeta(meta: OtaUploadResponse['meta']) {
  return meta.wificalling === true || (meta.edition || '').toLowerCase().includes('wfc')
}

async function fetchPublicReleaseAssets(tagName: string): Promise<OtaReleaseAsset[]> {
  const controller = new AbortController()
  const timeout = window.setTimeout(() => controller.abort(), 10_000)

  try {
    const response = await fetch(GITHUB_LATEST_RELEASE_API, {
      headers: {
        Accept: 'application/vnd.github+json',
        'X-GitHub-Api-Version': '2022-11-28',
      },
      signal: controller.signal,
    })
    if (!response.ok) return []

    const release = await response.json() as OtaLatestReleaseResponse
    if (normalizeVersion(release.tag_name) !== normalizeVersion(tagName)) return []
    return release.assets ?? []
  } catch {
    return []
  } finally {
    window.clearTimeout(timeout)
  }
}

function deriveSiblingReleaseAsset(
  release: OtaLatestReleaseResponse,
  assets: OtaReleaseAsset[],
): OtaReleaseAsset | null {
  const source = assets[0]
  if (!source) return null

  const lower = source.name.toLowerCase()
  const isWfc = lower.includes('wfc')
  const arch = resolveTargetArch(source.name)
  if (arch === 'unknown') return null

  const siblingName = `simadmin-${isWfc ? '' : 'wfc-'}${arch === 'arm64' ? 'aarch64' : 'x86_64'}.tar.gz`
  const lastSlash = source.browser_download_url.lastIndexOf('/')
  const baseUrl = lastSlash > 0
    ? source.browser_download_url.slice(0, lastSlash)
    : `https://github.com/3899/SimAdmin/releases/download/${release.tag_name}`

  return {
    name: siblingName,
    size: 0,
    browser_download_url: `${baseUrl}/${siblingName}`,
  }
}

function buildLegacyAssetProxyPrefix(assetUrl: string, proxyPrefix: string) {
  // Legacy backends prepend proxy_prefix to their own selected URL. Point that
  // prefix at the requested asset and absorb the appended URL as a query value.
  const selectedAssetUrl = proxyPrefix ? `${proxyPrefix}${assetUrl}` : assetUrl
  const separator = selectedAssetUrl.includes('?') ? '&' : '?'
  return `${selectedAssetUrl}${separator}simadmin_selected_asset=`
}

function CurrentEnvironmentStrip({ status }: { status: OtaStatusResponse | null }) {
  const isWfc = isWfcStatus(status)
  const currentVersion = status?.current_version || status?.installed_meta?.version || ''
  const currentArch = status?.current_arch || status?.installed_meta?.arch || '检测中...'
  const currentCommit = (status?.current_commit || status?.installed_meta?.commit || 'unknown').slice(0, 10)
  const currentBuildTime = formatDateTime(status?.current_build_time || status?.installed_meta?.build_time)

  return (
    <Card sx={{ p: { xs: 2, sm: 2.5 } }}>
      <Grid container spacing={2} alignItems="center">
        {/* Current Version */}
        <Grid size={{ xs: 12, sm: 6, md: 3 }}>
          <Box display="flex" alignItems="center" gap={1.5}>
            <Box
              sx={{
                width: 40,
                height: 40,
                borderRadius: 2.5,
                bgcolor: 'rgba(18, 150, 219, 0.12)',
                color: 'primary.main',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                flexShrink: 0,
              }}
            >
              <Bolt fontSize="small" />
            </Box>
            <Box minWidth={0}>
              <Typography variant="caption" color="text.secondary" sx={{ fontWeight: 400, letterSpacing: '0.04em' }}>
                当前版本
              </Typography>
              <Box display="flex" alignItems="center" gap={0.75} mt={0.25} flexWrap="wrap">
                <Typography variant="subtitle2" sx={{ fontWeight: 400 }}>
                  {normalizeVersion(currentVersion) ? `v${normalizeVersion(currentVersion)}` : '检测中...'}
                </Typography>
                <Chip
                  label={isWfc ? 'WiFi Calling' : '标准版'}
                  color={isWfc ? 'secondary' : 'primary'}
                  size="small"
                  sx={{ height: 20, fontSize: '0.72rem', fontWeight: 400 }}
                />
              </Box>
            </Box>
          </Box>
        </Grid>

        {/* Arch */}
        <Grid size={{ xs: 12, sm: 6, md: 3 }}>
          <Box display="flex" alignItems="center" gap={1.5}>
            <Box
              sx={{
                width: 40,
                height: 40,
                borderRadius: 2.5,
                bgcolor: 'rgba(16, 185, 129, 0.12)',
                color: 'success.main',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                flexShrink: 0,
              }}
            >
              <Memory fontSize="small" />
            </Box>
            <Box minWidth={0}>
              <Typography variant="caption" color="text.secondary" sx={{ fontWeight: 400, letterSpacing: '0.04em' }}>
                架构
              </Typography>
              <Typography variant="body2" sx={{ fontWeight: 400, fontSize: '0.85rem', mt: 0.25, wordBreak: 'break-all' }}>
                {currentArch}
              </Typography>
            </Box>
          </Box>
        </Grid>

        {/* Commit */}
        <Grid size={{ xs: 12, sm: 6, md: 3 }}>
          <Box display="flex" alignItems="center" gap={1.5}>
            <Box
              sx={{
                width: 40,
                height: 40,
                borderRadius: 2.5,
                bgcolor: 'rgba(124, 58, 237, 0.12)',
                color: 'secondary.main',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                flexShrink: 0,
              }}
            >
              <Info fontSize="small" />
            </Box>
            <Box minWidth={0}>
              <Typography variant="caption" color="text.secondary" sx={{ fontWeight: 400, letterSpacing: '0.04em' }}>
                Commit
              </Typography>
              <Typography variant="body2" sx={{ fontWeight: 400, fontSize: '0.85rem', mt: 0.25 }}>
                {currentCommit}
              </Typography>
            </Box>
          </Box>
        </Grid>

        {/* Build Time */}
        <Grid size={{ xs: 12, sm: 6, md: 3 }}>
          <Box display="flex" alignItems="center" gap={1.5}>
            <Box
              sx={{
                width: 40,
                height: 40,
                borderRadius: 2.5,
                bgcolor: 'rgba(245, 158, 11, 0.12)',
                color: 'warning.main',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                flexShrink: 0,
              }}
            >
              <Refresh fontSize="small" />
            </Box>
            <Box minWidth={0}>
              <Typography variant="caption" color="text.secondary" sx={{ fontWeight: 400, letterSpacing: '0.04em' }}>
                构建时间
              </Typography>
              <Typography variant="body2" sx={{ fontWeight: 400, fontSize: '0.825rem', mt: 0.25, color: 'text.primary' }}>
                {currentBuildTime}
              </Typography>
            </Box>
          </Box>
        </Grid>
      </Grid>
    </Card>
  )
}

function isMarkdownBlockStart(line: string) {
  const trimmed = line.trim()
  return (
    /^#{1,6}\s+/.test(trimmed) ||
    /^(```|~~~)/.test(trimmed) ||
    /^>\s?/.test(trimmed) ||
    /^([-*_])(?:\s*\1){2,}$/.test(trimmed) ||
    /^\s*[-*+]\s+/.test(line) ||
    /^\s*\d+[.)]\s+/.test(line)
  )
}

function parseMarkdownBlocks(markdown: string) {
  const lines = markdown.replace(/\r\n?/g, '\n').split('\n')
  const blocks: MarkdownBlock[] = []
  let index = 0

  while (index < lines.length) {
    const line = lines[index]
    const trimmed = line.trim()

    if (!trimmed) {
      index += 1
      continue
    }

    const fenceMatch = trimmed.match(/^(```|~~~)\s*([^`]*)$/)
    if (fenceMatch) {
      const fence = fenceMatch[1]
      const language = fenceMatch[2]?.trim() || undefined
      const codeLines: string[] = []
      index += 1

      while (index < lines.length && !lines[index].trim().startsWith(fence)) {
        codeLines.push(lines[index])
        index += 1
      }

      if (index < lines.length) {
        index += 1
      }

      blocks.push({ type: 'code', code: codeLines.join('\n'), language })
      continue
    }

    if (/^([-*_])(?:\s*\1){2,}$/.test(trimmed)) {
      blocks.push({ type: 'rule' })
      index += 1
      continue
    }

    const headingMatch = trimmed.match(/^(#{1,6})\s+(.+)$/)
    if (headingMatch) {
      blocks.push({
        type: 'heading',
        level: Math.min(headingMatch[1].length, 6) as MarkdownHeadingLevel,
        text: headingMatch[2].replace(/\s+#+$/, ''),
      })
      index += 1
      continue
    }

    if (/^>\s?/.test(trimmed)) {
      const quoteLines: string[] = []

      while (index < lines.length && /^>\s?/.test(lines[index].trim())) {
        quoteLines.push(lines[index].trim().replace(/^>\s?/, ''))
        index += 1
      }

      blocks.push({ type: 'quote', text: quoteLines.join('\n') })
      continue
    }

    const unorderedListMatch = line.match(/^(\s*)[-*+]\s+(.+)$/)
    const orderedListMatch = line.match(/^(\s*)\d+[.)]\s+(.+)$/)
    if (unorderedListMatch || orderedListMatch) {
      const ordered = Boolean(orderedListMatch)
      const items: MarkdownListItem[] = []

      while (index < lines.length) {
        const currentLine = lines[index]
        const currentMatch = ordered
          ? currentLine.match(/^(\s*)\d+[.)]\s+(.+)$/)
          : currentLine.match(/^(\s*)[-*+]\s+(.+)$/)

        if (!currentMatch) break

        const indent = Math.floor(currentMatch[1].replace(/\t/g, '    ').length / 2)
        items.push({ indent, text: currentMatch[2] })
        index += 1
      }

      blocks.push({ type: 'list', ordered, items })
      continue
    }

    const paragraphLines: string[] = []

    while (index < lines.length && lines[index].trim()) {
      if (paragraphLines.length > 0 && isMarkdownBlockStart(lines[index])) break
      paragraphLines.push(lines[index].trim())
      index += 1
    }

    blocks.push({ type: 'paragraph', text: paragraphLines.join(' ') })
  }

  return blocks
}

function isSafeMarkdownHref(href: string) {
  if (href.startsWith('#') || href.startsWith('/')) return true

  try {
    const url = new URL(href)
    return ['http:', 'https:', 'mailto:', 'tel:'].includes(url.protocol)
  } catch {
    return false
  }
}

function renderInlineMarkdown(text: string, keyPrefix: string): ReactNode[] {
  const pattern = /(`[^`]+`|\*\*[^*]+\*\*|__[^_]+__|~~[^~]+~~|\[[^\]]+\]\([^)]+\))/g
  const nodes: ReactNode[] = []
  let lastIndex = 0
  let match: RegExpExecArray | null
  let tokenIndex = 0

  while ((match = pattern.exec(text)) !== null) {
    if (match.index > lastIndex) {
      nodes.push(text.slice(lastIndex, match.index))
    }

    const token = match[0]
    const key = `${keyPrefix}-inline-${tokenIndex}`
    tokenIndex += 1

    if (token.startsWith('`')) {
      nodes.push(
        <Box
          component="code"
          key={key}
          sx={{
            px: 0.5,
            py: 0.125,
            borderRadius: 0.5,
            bgcolor: 'action.hover',
            fontFamily: 'monospace',
            fontSize: '0.875em',
          }}
        >
          {token.slice(1, -1)}
        </Box>,
      )
    } else if (token.startsWith('**') || token.startsWith('__')) {
      nodes.push(
        <Box component="strong" key={key} sx={{ fontWeight: 700 }}>
          {token.slice(2, -2)}
        </Box>,
      )
    } else if (token.startsWith('~~')) {
      nodes.push(
        <Box component="del" key={key}>
          {token.slice(2, -2)}
        </Box>,
      )
    } else {
      const linkMatch = token.match(/^\[([^\]]+)\]\(([^)]+)\)$/)
      const label = linkMatch?.[1] ?? token
      const href = linkMatch?.[2]?.trim()

      if (href && isSafeMarkdownHref(href)) {
        nodes.push(
          <Link key={key} href={href} target="_blank" rel="noreferrer" underline="hover">
            {label}
          </Link>,
        )
      } else {
        nodes.push(label)
      }
    }

    lastIndex = pattern.lastIndex
  }

  if (lastIndex < text.length) {
    nodes.push(text.slice(lastIndex))
  }

  return nodes
}

function MarkdownPreview({ source }: { source?: string }) {
  const blocks = parseMarkdownBlocks(source?.trim() || '无更新日志')

  return (
    <Box
      sx={{
        color: 'text.primary',
        '& > :first-of-type': { mt: 0 },
        '& > :last-child': { mb: 0 },
        '& a': { wordBreak: 'break-all' },
      }}
    >
      {blocks.map((block, index) => {
        const key = `${block.type}-${index}`

        if (block.type === 'heading') {
          return (
            <Typography
              key={key}
              component="div"
              role="heading"
              aria-level={block.level}
              variant={block.level <= 2 ? 'subtitle1' : 'subtitle2'}
              sx={{ mt: index === 0 ? 0 : 1.5, mb: 0.75, fontWeight: 700 }}
            >
              {renderInlineMarkdown(block.text, key)}
            </Typography>
          )
        }

        if (block.type === 'paragraph') {
          return (
            <Typography key={key} variant="body2" sx={{ my: 1, lineHeight: 1.7 }}>
              {renderInlineMarkdown(block.text, key)}
            </Typography>
          )
        }

        if (block.type === 'list') {
          return (
            <Box
              key={key}
              component={block.ordered ? 'ol' : 'ul'}
              sx={{ my: 1, pl: 3, lineHeight: 1.7 }}
            >
              {block.items.map((item, itemIndex) => (
                <Box
                  component="li"
                  key={`${key}-item-${itemIndex}`}
                  sx={{ ml: item.indent * 2, mb: 0.5, '&::marker': { color: 'text.secondary' } }}
                >
                  <Typography component="span" variant="body2">
                    {renderInlineMarkdown(item.text, `${key}-item-${itemIndex}`)}
                  </Typography>
                </Box>
              ))}
            </Box>
          )
        }

        if (block.type === 'quote') {
          return (
            <Box
              key={key}
              sx={{
                my: 1,
                pl: 1.5,
                borderLeft: 3,
                borderColor: 'divider',
                color: 'text.secondary',
              }}
            >
              <Typography variant="body2" sx={{ whiteSpace: 'pre-wrap', lineHeight: 1.7 }}>
                {renderInlineMarkdown(block.text, key)}
              </Typography>
            </Box>
          )
        }

        if (block.type === 'code') {
          return (
            <Box
              key={key}
              component="pre"
              sx={{
                my: 1,
                p: 1.5,
                overflow: 'auto',
                borderRadius: 1,
                bgcolor: 'action.hover',
                fontFamily: 'monospace',
                fontSize: '0.8125rem',
                whiteSpace: 'pre-wrap',
              }}
            >
              {block.language && (
                <Box component="span" sx={{ display: 'block', mb: 1, color: 'text.secondary' }}>
                  {block.language}
                </Box>
              )}
              <Box component="code">{block.code}</Box>
            </Box>
          )
        }

        return <Divider key={key} sx={{ my: 1.5 }} />
      })}
    </Box>
  )
}

interface OnlineUpdateCardProps {
  expanded: boolean
  supportsOtaUpload: boolean
  proxyEnabled: boolean
  proxyPreset: ProxyPreset
  customProxy: string
  proxyPrefix: string
  onlineState: OnlineUpdateState
  downloadProgress: number
  latestRelease: OtaLatestReleaseResponse | null
  currentVersion?: string
  manualShowReleaseCard: boolean
  onProxyEnabledChange: (enabled: boolean) => void
  onProxyPresetChange: (preset: ProxyPreset) => void
  onCustomProxyChange: (proxy: string) => void
  onCheck: () => void
  onShowReleaseCard: () => void
}

function OnlineUpdateCard({
  expanded,
  supportsOtaUpload,
  proxyEnabled,
  proxyPreset,
  customProxy,
  proxyPrefix,
  onlineState,
  downloadProgress,
  latestRelease,
  currentVersion,
  manualShowReleaseCard,
  onProxyEnabledChange,
  onProxyPresetChange,
  onCustomProxyChange,
  onCheck,
  onShowReleaseCard,
}: OnlineUpdateCardProps) {
  const proxyLabelId = expanded ? 'proxy-preset-label' : 'proxy-preset-label-solo'

  return (
    <Card
      sx={{
        flex: expanded ? (supportsOtaUpload ? '0 0 auto' : 1) : 1,
        ...(!expanded && { minWidth: 0 }),
        display: 'flex',
        flexDirection: 'column',
      }}
    >
      <CardContent sx={{ flex: expanded && supportsOtaUpload ? 'none' : 1 }}>
        <Box display="flex" justifyContent="space-between" alignItems="center" gap={2} mb={2}>
          <Box display="flex" alignItems="center" gap={1}>
            <Public color="primary" />
            <Typography variant="subtitle1" fontWeight={600}>在线更新</Typography>
          </Box>
          <Link href={GITHUB_LATEST_RELEASE_PAGE} target="_blank" rel="noreferrer" variant="caption" underline="hover">
            GitHub Releases
          </Link>
        </Box>

        <Typography variant="body2" color="text.secondary" mb={2}>
          连接到 GitHub 检查是否有可用的 SimAdmin 更新版本。
        </Typography>

        <Stack spacing={2} sx={{ mb: 2 }}>
          <FormControlLabel
            control={
              <Switch
                checked={proxyEnabled}
                onChange={event => onProxyEnabledChange(event.target.checked)}
              />
            }
            label="启用 GitHub 下载加速"
          />
          {proxyEnabled && (
            <Stack spacing={2} direction={{ xs: 'column', sm: 'row' }}>
              <FormControl fullWidth size="small">
                <InputLabel id={proxyLabelId}>加速节点</InputLabel>
                <Select
                  labelId={proxyLabelId}
                  label="加速节点"
                  value={proxyPreset}
                  onChange={event => onProxyPresetChange(event.target.value as ProxyPreset)}
                >
                  <MenuItem value="https://gh-proxy.com/">gh-proxy.com (默认)</MenuItem>
                  <MenuItem value="https://ghproxy.net/">ghproxy.net</MenuItem>
                  <MenuItem value="https://githubproxy.cc/">githubproxy.cc</MenuItem>
                  <MenuItem value="custom">自定义</MenuItem>
                </Select>
              </FormControl>
              {proxyPreset === 'custom' && (
                <TextField
                  fullWidth
                  size="small"
                  label="自定义加速节点"
                  value={customProxy}
                  onChange={event => onCustomProxyChange(event.target.value)}
                  placeholder="https://my-proxy.example.com/"
                />
              )}
            </Stack>
          )}
        </Stack>

        <Stack direction={{ xs: 'column', sm: 'row' }} spacing={2} alignItems={{ xs: 'stretch', sm: 'center' }}>
          <Button
            variant="contained"
            startIcon={onlineState === 'checking' ? <CircularProgress size={20} color="inherit" /> : <Search />}
            onClick={onCheck}
            disabled={onlineState === 'checking' || onlineState === 'downloading'}
          >
            {onlineState === 'checking' ? '检查中...' : '检查更新'}
          </Button>
          {proxyEnabled && proxyPrefix && (
            <Typography variant="caption" color="text.secondary">
              下载加速：{proxyPreset === 'custom' ? proxyPrefix : new URL(proxyPrefix).hostname}
            </Typography>
          )}
        </Stack>

        {!expanded && onlineState === 'latest' && (
          <Alert
            severity="success"
            sx={{ mt: 2 }}
            action={
              !manualShowReleaseCard && latestRelease ? (
                <Button color="inherit" size="small" onClick={onShowReleaseCard}>
                  查看产物包
                </Button>
              ) : undefined
            }
          >
            当前版本 {currentVersion || 'N/A'} 已经是最新发布的稳定版，暂无更新。
          </Alert>
        )}

        {onlineState === 'downloading' && (
          <Box sx={{ mt: 2.5 }}>
            <Box display="flex" justifyContent="space-between" mb={1}>
              <Typography variant="body2" color="text.secondary">
                {proxyPrefix ? '正在通过加速节点下载更新包...' : '正在直连下载更新包...'}
              </Typography>
              <Typography variant="body2" color="text.secondary">{downloadProgress}%</Typography>
            </Box>
            <LinearProgress variant="determinate" value={downloadProgress} sx={{ borderRadius: 1 }} />
          </Box>
        )}
      </CardContent>
    </Card>
  )
}

interface UploadUpdateCardProps {
  expanded: boolean
  uploading: boolean
  fileInputRef: RefObject<HTMLInputElement | null>
  onFileSelect: (event: ChangeEvent<HTMLInputElement>) => void
}

function UploadUpdateCard({
  expanded,
  uploading,
  fileInputRef,
  onFileSelect,
}: UploadUpdateCardProps) {
  return (
    <Card
      sx={{
        flex: 1,
        ...(!expanded && { minWidth: 0 }),
        display: 'flex',
        flexDirection: 'column',
      }}
    >
      <CardContent
        sx={{
          flex: 1,
          ...(expanded && { display: 'flex', flexDirection: 'column' }),
        }}
      >
        <Box display="flex" alignItems="center" gap={1} mb={2}>
          <CloudUpload color="primary" />
          <Typography variant="subtitle1" fontWeight={600}>上传更新包</Typography>
        </Box>

        <Typography variant="body2" color="text.secondary" mb={2}>
          前往 GitHub Releases 下载安装包，手动上传即可完成升级或降级。
        </Typography>

        <Alert severity="info" sx={{ mb: 2 }}>
          <AlertTitle>OTA 更新包格式</AlertTitle>
          请上传 <code>.tar.gz</code> 格式的 OTA 更新包。错误的包会导致系统无法启动。
        </Alert>

        <input
          ref={fileInputRef}
          type="file"
          accept=".gz,.tgz,.zip,application/gzip,application/x-gzip,application/x-tar,application/zip"
          style={{ display: 'none' }}
          onChange={onFileSelect}
        />

        <Button
          variant="contained"
          startIcon={uploading ? <CircularProgress size={20} color="inherit" /> : <CloudUpload />}
          onClick={() => fileInputRef.current?.click()}
          disabled={uploading}
        >
          {uploading ? '上传中...' : '选择更新包'}
        </Button>

        {uploading && (
          <Box sx={{ mt: 2 }}>
            <LinearProgress />
          </Box>
        )}
      </CardContent>
    </Card>
  )
}

export default function OtaUpdate() {
  const api = useSimAdminApi()
  const supportsOtaUpload = api.supportsOtaUpload
  const [loading, setLoading] = useState(true)
  const [uploading, setUploading] = useState(false)
  const [applying, setApplying] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState<string | null>(null)

  const [status, setStatus] = useState<OtaStatusResponse | null>(null)
  const [uploadResult, setUploadResult] = useState<OtaUploadResponse | null>(null)
  const [confirmDialog, setConfirmDialog] = useState<'apply' | 'cancel' | null>(null)

  const [proxyEnabled, setProxyEnabled] = useState(true)
  const [proxyPreset, setProxyPreset] = useState<ProxyPreset>('https://gh-proxy.com/')
  const [customProxy, setCustomProxy] = useState('')
  const [onlineState, setOnlineState] = useState<OnlineUpdateState>('idle')
  const [latestRelease, setLatestRelease] = useState<OtaLatestReleaseResponse | null>(null)
  const [usesLegacyAssetSelection, setUsesLegacyAssetSelection] = useState(false)
  const [selectedAssetName, setSelectedAssetName] = useState<string | null>(null)
  const [downloadProgress, setDownloadProgress] = useState(0)

  const fileInputRef = useRef<HTMLInputElement>(null)

  const targetArchType = useMemo<'arm64' | 'x86_64' | 'unknown'>(() => {
    const raw = status?.current_arch || status?.installed_meta?.arch
    return resolveTargetArch(raw)
  }, [status])

  const targetArchLabel = useMemo(() => {
    const raw = status?.current_arch || status?.installed_meta?.arch
    return formatArchLabel(raw)
  }, [status])

  const isCurrentWfc = isWfcStatus(status)

  // 解析并筛选出与当前设备硬件架构相匹配的所有产物包（标准版与 WiFi Calling 版）
  const compatibleAssets: ClassifiedAsset[] = useMemo(() => {
    if (!latestRelease) return []

    const rawAssets = latestRelease.assets ? [...latestRelease.assets] : []

    // 确定目标架构类型：优先根据设备 status，若 status 暂未返回则根据返回包推断
    let archType = targetArchType
    if (archType === 'unknown' && rawAssets.length > 0) {
      const first = rawAssets[0].name.toLowerCase()
      if (first.includes('x86_64') || first.includes('amd64')) archType = 'x86_64'
      else if (first.includes('aarch64') || first.includes('arm64')) archType = 'arm64'
    }

    const filtered = rawAssets.filter(asset => {
      const lower = asset.name.toLowerCase()
      if (!lower.startsWith('simadmin') || (!lower.endsWith('.tar.gz') && !lower.endsWith('.tgz') && !lower.endsWith('.zip'))) {
        return false
      }
      if (archType === 'arm64') {
        if (lower.includes('amd64') || lower.includes('x86_64')) return false
        return lower.includes('arm64') || lower.includes('aarch64') || lower === 'simadmin.tar.gz' || lower === 'simadmin.tgz' || lower === 'simadmin.zip'
      }
      if (archType === 'x86_64') {
        if (lower.includes('arm64') || lower.includes('aarch64')) return false
        return lower.includes('amd64') || lower.includes('x86_64')
      }
      return true
    })

    return filtered.map(asset => {
      const lower = asset.name.toLowerCase()
      const isWfc = lower.includes('wfc') || lower.includes('wificalling')
      const edition: 'standard' | 'wfc' = isWfc ? 'wfc' : 'standard'
      return {
        asset,
        edition,
        editionLabel: isWfc ? 'WiFi Calling 版' : '标准版',
        isCurrentMatch: isWfc ? isCurrentWfc : !isCurrentWfc,
        arch: inferArch(asset.name),
        sizeStr: formatBytes(asset.size),
      }
    })
  }, [latestRelease, targetArchType, isCurrentWfc])

  // 默认选择与当前版本相同的产物包（标准版默认选标准版，WFC 版默认选 WFC 版）
  useEffect(() => {
    if (compatibleAssets.length > 0) {
      if (selectedAssetName && compatibleAssets.some(a => a.asset.name === selectedAssetName)) {
        return
      }
      const match = compatibleAssets.find(a => a.isCurrentMatch)
      if (match) {
        setSelectedAssetName(match.asset.name)
      } else {
        setSelectedAssetName(compatibleAssets[0].asset.name)
      }
    } else {
      setSelectedAssetName(null)
    }
  }, [compatibleAssets, selectedAssetName])

  const selectedAssetItem = compatibleAssets.find(a => a.asset.name === selectedAssetName) || compatibleAssets[0]
  const selectedAsset = selectedAssetItem?.asset

  const [manualShowReleaseCard, setManualShowReleaseCard] = useState(false)
  const hasUpdate = Boolean(
    latestRelease && compareVersions(latestRelease.tag_name, status?.current_version || '0.0.0') > 0
  )
  const showReleaseCard = Boolean(
    latestRelease && (hasUpdate || manualShowReleaseCard || onlineState === 'downloading')
  )

  const loadStatus = useCallback(async () => {
    try {
      const res = await api.getOtaStatus()
      if (res.data) {
        setStatus(res.data)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [api])

  useEffect(() => {
    void loadStatus()
  }, [loadStatus])

  const getProxyPrefix = () => {
    if (!proxyEnabled) return ''
    if (proxyPreset !== 'custom') return proxyPreset
    const trimmed = customProxy.trim()
    if (!trimmed) return ''
    return trimmed.endsWith('/') ? trimmed : `${trimmed}/`
  }

  const handleCheckOnlineUpdate = async () => {
    setOnlineState('checking')
    setError(null)
    setSuccess(null)
    setLatestRelease(null)
    setUsesLegacyAssetSelection(false)
    setManualShowReleaseCard(false)
    setDownloadProgress(0)

    try {
      const proxyPrefix = getProxyPrefix()
      if (proxyEnabled && proxyPreset === 'custom' && !proxyPrefix) {
        throw new Error('请输入自定义加速节点地址，或关闭 GitHub 下载加速')
      }

      const res = await api.getLatestOtaRelease({
        proxy_prefix: proxyPrefix || undefined,
        include_variants: true,
      })
      if (res.status !== 'ok' || !res.data) {
        throw new Error(res.message || 'GitHub Releases 请求失败')
      }

      let release = res.data
      const usesLegacySelection = release.supports_asset_selection !== true
      if (usesLegacySelection) {
        const publicAssets = await fetchPublicReleaseAssets(release.tag_name)
        if (publicAssets.length > 0) {
          release = { ...release, assets: publicAssets }
        } else {
          const currentAssets = release.assets ?? []
          const siblingAsset = deriveSiblingReleaseAsset(release, currentAssets)
          if (siblingAsset && !currentAssets.some(asset => asset.name === siblingAsset.name)) {
            release = { ...release, assets: [...currentAssets, siblingAsset] }
          }
        }
      }

      setUsesLegacyAssetSelection(usesLegacySelection)
      setLatestRelease(release)
      const currentVersion = status?.current_version || '0.0.0'
      setOnlineState(compareVersions(release.tag_name, currentVersion) > 0 ? 'available' : 'latest')
    } catch (err) {
      setOnlineState('idle')
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const handlePrepareOnlineUpdate = async () => {
    if (!latestRelease || !selectedAsset) return

    const proxyPrefix = getProxyPrefix()
    if (proxyEnabled && proxyPreset === 'custom' && !proxyPrefix) {
      setError('请输入自定义加速节点地址，或关闭 GitHub 下载加速')
      return
    }

    setOnlineState('downloading')
    setError(null)
    setSuccess(null)
    setDownloadProgress(0)

    const timer = window.setInterval(() => {
      setDownloadProgress(prev => Math.min(prev + 4 + Math.floor(Math.random() * 8), 88))
    }, 260)

    try {
      const needsLegacyOverride = usesLegacyAssetSelection && !selectedAssetItem?.isCurrentMatch
      const prepareProxyPrefix = needsLegacyOverride
        ? buildLegacyAssetProxyPrefix(selectedAsset.browser_download_url, proxyPrefix)
        : proxyPrefix
      const res = await api.prepareOnlineOta({
        proxy_prefix: prepareProxyPrefix || undefined,
        asset_name: selectedAsset.name,
      })
      window.clearInterval(timer)
      setDownloadProgress(100)

      const prepared = res.data
      if (res.status === 'ok' && prepared) {
        const selectedIsWfc = selectedAssetItem?.edition === 'wfc'
        if (isWfcMeta(prepared.meta) !== selectedIsWfc) {
          await api.cancelOta().catch(() => undefined)
          await loadStatus()
          throw new Error(`后台未能下载所选的 ${selectedAssetItem?.editionLabel || 'OTA'}，已取消错误的暂存包，请切换下载节点后重试`)
        }

        window.setTimeout(() => {
          setUploadResult(prepared)
          setOnlineState('idle')
          setDownloadProgress(0)
          if (prepared.validation.valid) {
            setSuccess('在线下载成功，验证通过')
          } else {
            setError('在线 OTA 包验证失败：' + (prepared.validation.error || '未知错误'))
          }
          void loadStatus()
        }, 300)
      } else {
        setOnlineState('available')
        setDownloadProgress(0)
        setError(res.message || '在线下载失败')
      }
    } catch (err) {
      window.clearInterval(timer)
      setOnlineState('available')
      setDownloadProgress(0)
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const handleFileSelect = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]
    if (!file) return

    const validExtensions = ['.tar.gz', '.tgz', '.zip']
    const isValid = validExtensions.some(ext => file.name.endsWith(ext))

    if (!isValid) {
      setError('请上传 .tar.gz 或 .zip 格式的 OTA 更新包')
      return
    }

    setUploading(true)
    setError(null)
    setSuccess(null)
    setUploadResult(null)

    try {
      const res = await api.uploadOta(file)
      if (res.status === 'ok' && res.data) {
        setUploadResult(res.data)
        if (res.data.validation.valid) {
          setSuccess('OTA 包上传成功，验证通过')
        } else {
          setError('OTA 包验证失败：' + (res.data.validation.error || '未知错误'))
        }
        await loadStatus()
      } else {
        setError(res.message || '上传失败')
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setUploading(false)
      if (fileInputRef.current) {
        fileInputRef.current.value = ''
      }
    }
  }

  const handleApply = async (restartNow: boolean) => {
    setConfirmDialog(null)
    setApplying(true)
    setError(null)
    setSuccess(null)

    try {
      const res = await api.applyOta(restartNow)
      if (res.status === 'ok') {
        setSuccess(restartNow ? '更新已应用，系统即将重启...' : '更新已应用，请手动重启服务生效')
        setUploadResult(null)
        await loadStatus()
      } else {
        setError(res.message || '应用更新失败')
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setApplying(false)
    }
  }

  const handleCancel = async () => {
    setConfirmDialog(null)
    setError(null)
    setSuccess(null)

    try {
      const res = await api.cancelOta()
      if (res.status === 'ok') {
        setSuccess('已取消待安装的更新')
        setUploadResult(null)
        await loadStatus()
      } else {
        setError(res.message || '取消失败')
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  if (loading) {
    return (
      <Box display="flex" justifyContent="center" alignItems="center" minHeight="60vh">
        <CircularProgress />
      </Box>
    )
  }

  const pendingMeta = status?.pending_meta
  const hasPendingUpdate = Boolean(pendingMeta)
  const proxyPrefix = getProxyPrefix()

  return (
    <Box>
      <Box display="flex" justifyContent="space-between" alignItems="center" mb={2}>
        <Box>
          <Typography variant="h5" gutterBottom fontWeight={700}>
            OTA 更新
          </Typography>
          <Typography variant="body2" color="text.secondary">
            {supportsOtaUpload ? '上传并安装系统更新包 / 在线获取最新版本' : '在线获取最新版本'}
          </Typography>
        </Box>
        <Button
          variant="outlined"
          startIcon={<Refresh />}
          onClick={() => void loadStatus()}
          disabled={loading}
        >
          刷新状态
        </Button>
      </Box>

      {error && (
        <Alert severity="error" sx={{ mb: 2 }} onClose={() => setError(null)}>
          {error}
        </Alert>
      )}
      {success && (
        <Alert severity="success" sx={{ mb: 2 }} onClose={() => setSuccess(null)}>
          {success}
        </Alert>
      )}

      <Stack spacing={3}>
        <CurrentEnvironmentStrip status={status} />

        {(hasPendingUpdate && pendingMeta) || uploadResult ? (
          <Stack direction={{ xs: 'column', md: 'row' }} spacing={3} alignItems="stretch">
            {hasPendingUpdate && pendingMeta && (
              <Card sx={{ flex: 1, minWidth: 0, borderColor: 'warning.main', borderWidth: 2, borderStyle: 'solid' }}>
                <CardContent>
                  <Box display="flex" alignItems="center" gap={1} mb={2}>
                    <Warning color="warning" />
                    <Typography variant="h6">待安装更新</Typography>
                    <Chip
                      label={pendingMeta.version}
                      color="warning"
                      size="small"
                      sx={{ ml: 1 }}
                    />
                  </Box>
                  <TableContainer>
                    <Table size="small">
                      <TableBody>
                        <TableRow>
                          <TableCell component="th" sx={{ width: 150 }}>版本号</TableCell>
                          <TableCell>{pendingMeta.version}</TableCell>
                        </TableRow>
                        <TableRow>
                          <TableCell component="th">Commit</TableCell>
                          <TableCell sx={{ wordBreak: 'break-all' }}>{pendingMeta.commit}</TableCell>
                        </TableRow>
                        <TableRow>
                          <TableCell component="th">构建时间</TableCell>
                          <TableCell>{formatDateTime(pendingMeta.build_time)}</TableCell>
                        </TableRow>
                        <TableRow>
                          <TableCell component="th">架构</TableCell>
                          <TableCell>{pendingMeta.arch}</TableCell>
                        </TableRow>
                      </TableBody>
                    </Table>
                  </TableContainer>
                  <Divider sx={{ my: 2 }} />
                  <Stack direction="row" spacing={2}>
                    <Button
                      variant="contained"
                      color="success"
                      startIcon={<SystemUpdateAlt />}
                      onClick={() => setConfirmDialog('apply')}
                      disabled={applying}
                    >
                      {applying ? <CircularProgress size={20} /> : '应用更新'}
                    </Button>
                    <Button
                      variant="outlined"
                      color="error"
                      startIcon={<Cancel />}
                      onClick={() => setConfirmDialog('cancel')}
                    >
                      取消更新
                    </Button>
                  </Stack>
                </CardContent>
              </Card>
            )}

            {uploadResult && (
              <Card
                sx={{
                  flex: 1,
                  minWidth: 0,
                  ...(uploadResult.validation.valid
                    ? { borderColor: 'success.main', borderWidth: 2, borderStyle: 'solid' }
                    : {}),
                }}
              >
                <CardContent>
                  <Box display="flex" alignItems="center" gap={1} mb={2}>
                    {uploadResult.validation.valid ? (
                      <CheckCircle color="success" />
                    ) : (
                      <ErrorIcon color="error" />
                    )}
                    <Typography variant="h6">
                      验证结果
                    </Typography>
                    <Chip
                      label={uploadResult.validation.valid ? '通过' : '失败'}
                      color={uploadResult.validation.valid ? 'success' : 'error'}
                      size="small"
                    />
                  </Box>

                  <TableContainer component={Paper} variant="outlined">
                    <Table size="small">
                      <TableBody>
                        <TableRow>
                          <TableCell component="th" sx={{ width: 180 }}>版本号</TableCell>
                          <TableCell>{uploadResult.meta.version}</TableCell>
                          <TableCell align="right">
                            {uploadResult.validation.is_newer ? (
                              <Chip label="新版本" color="success" size="small" />
                            ) : (
                              <Chip label="旧版本或相同" color="warning" size="small" />
                            )}
                          </TableCell>
                        </TableRow>
                        <TableRow>
                          <TableCell component="th">Commit</TableCell>
                          <TableCell sx={{ wordBreak: 'break-all' }} colSpan={2}>
                            {uploadResult.meta.commit}
                          </TableCell>
                        </TableRow>
                        <TableRow>
                          <TableCell component="th">构建时间</TableCell>
                          <TableCell colSpan={2}>{formatDateTime(uploadResult.meta.build_time)}</TableCell>
                        </TableRow>
                        <TableRow>
                          <TableCell component="th">二进制 MD5</TableCell>
                          <TableCell sx={{ wordBreak: 'break-all' }}>
                            {uploadResult.meta.binary_md5}
                          </TableCell>
                          <TableCell align="right">
                            {uploadResult.validation.binary_md5_match ? (
                              <CheckCircle color="success" fontSize="small" />
                            ) : (
                              <ErrorIcon color="error" fontSize="small" />
                            )}
                          </TableCell>
                        </TableRow>
                        <TableRow>
                          <TableCell component="th">前端 MD5</TableCell>
                          <TableCell sx={{ wordBreak: 'break-all' }}>
                            {uploadResult.meta.frontend_md5}
                          </TableCell>
                          <TableCell align="right">
                            {uploadResult.validation.frontend_md5_match ? (
                              <CheckCircle color="success" fontSize="small" />
                            ) : (
                              <ErrorIcon color="error" fontSize="small" />
                            )}
                          </TableCell>
                        </TableRow>
                        <TableRow>
                          <TableCell component="th">架构</TableCell>
                          <TableCell>{uploadResult.meta.arch}</TableCell>
                          <TableCell align="right">
                            {uploadResult.validation.arch_match ? (
                              <CheckCircle color="success" fontSize="small" />
                            ) : (
                              <ErrorIcon color="error" fontSize="small" />
                            )}
                          </TableCell>
                        </TableRow>
                      </TableBody>
                    </Table>
                  </TableContainer>

                  {uploadResult.validation.error && (
                    <Alert severity="error" sx={{ mt: 2 }}>
                      {uploadResult.validation.error}
                    </Alert>
                  )}
                </CardContent>
              </Card>
            )}
          </Stack>
        ) : null}

        {showReleaseCard ? (
          <Grid container spacing={3} alignItems="stretch">
            {/* 左列：在线配置与更新触发 + 上传更新包 */}
            <Grid size={{ xs: 12, md: 5, lg: 4.5 }} sx={{ display: 'flex', flexDirection: 'column' }}>
              <Stack spacing={3} sx={{ flex: 1, display: 'flex', flexDirection: 'column', height: '100%' }}>
                <OnlineUpdateCard
                  expanded
                  supportsOtaUpload={supportsOtaUpload}
                  proxyEnabled={proxyEnabled}
                  proxyPreset={proxyPreset}
                  customProxy={customProxy}
                  proxyPrefix={proxyPrefix}
                  onlineState={onlineState}
                  downloadProgress={downloadProgress}
                  latestRelease={latestRelease}
                  currentVersion={status?.current_version}
                  manualShowReleaseCard={manualShowReleaseCard}
                  onProxyEnabledChange={setProxyEnabled}
                  onProxyPresetChange={setProxyPreset}
                  onCustomProxyChange={setCustomProxy}
                  onCheck={() => void handleCheckOnlineUpdate()}
                  onShowReleaseCard={() => setManualShowReleaseCard(true)}
                />

                {supportsOtaUpload && (
                  <UploadUpdateCard
                    expanded
                    uploading={uploading}
                    fileInputRef={fileInputRef}
                    onFileSelect={event => void handleFileSelect(event)}
                  />
                )}
              </Stack>
            </Grid>

            {/* 右列：Release 交付与产物包选型 + 说明 */}
            <Grid size={{ xs: 12, md: 7, lg: 7.5 }} sx={{ display: 'flex', flexDirection: 'column' }}>
              {latestRelease && (
                <Card sx={{ height: '100%', flex: 1, display: 'flex', flexDirection: 'column' }}>
                  <CardContent sx={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 2.5 }}>
                    {/* Release 基础信息 Header */}
                    <Box display="flex" justifyContent="space-between" alignItems="flex-start" flexWrap="wrap" gap={1.5} pb={2} sx={{ borderBottom: 1, borderColor: 'divider' }}>
                      <Box>
                        <Box display="flex" alignItems="center" gap={1.25} flexWrap="wrap">
                          <Typography variant="h6" fontWeight={600} color="primary">
                            {latestRelease.name || `SimAdmin ${latestRelease.tag_name}`}
                          </Typography>
                          {compareVersions(latestRelease.tag_name, status?.current_version || '0.0.0') > 0 ? (
                            <Chip label="发现可用更新" color="success" size="small" sx={{ fontWeight: 400 }} />
                          ) : (
                            <Chip label="当前已是最新版" color="info" size="small" variant="outlined" sx={{ fontWeight: 400 }} />
                          )}
                          <Chip label="最新正式版" size="small" sx={{ fontWeight: 400 }} />
                        </Box>
                        <Typography variant="caption" color="text.secondary" display="block" mt={0.5}>
                          发布时间：{formatDateTime(latestRelease.published_at)}  •  Commit: {extractReleaseCommit(latestRelease)}  •  目标架构：{targetArchLabel}
                        </Typography>
                      </Box>
                      <Link href={GITHUB_LATEST_RELEASE_PAGE} target="_blank" rel="noreferrer" variant="caption" underline="hover">
                        GitHub Releases
                      </Link>
                    </Box>

                    {/* 状态提醒 */}
                    {onlineState === 'latest' && (
                      <Alert severity="success">
                        当前版本 <strong>{status?.current_version || 'N/A'}</strong> 已经是最新发布的稳定版。您仍可在下方重新下载或在标准版与 WiFi Calling 版之间切换。
                      </Alert>
                    )}

                    {/* 产物包选型区域（标准版 vs WFC 版） */}
                    <Box>
                      <Box display="flex" justifyContent="space-between" alignItems="center" mb={1.5} flexWrap="wrap" gap={1}>
                        <Typography variant="subtitle2" fontWeight={700} sx={{ textTransform: 'uppercase', letterSpacing: '0.04em', color: 'text.secondary' }}>
                          📦 可用产物包 (架构: {targetArchLabel})
                        </Typography>
                      </Box>

                      <Grid container spacing={2}>
                        {compatibleAssets.map((item) => {
                          const isSelected = selectedAssetName === item.asset.name
                          const isWfc = item.edition === 'wfc'

                          return (
                            <Grid
                              key={item.asset.name}
                              size={{ xs: 12, sm: compatibleAssets.length > 1 ? 6 : 12 }}
                              sx={{ display: 'flex' }}
                            >
                              <Paper
                                variant="outlined"
                                onClick={() => setSelectedAssetName(item.asset.name)}
                                sx={{
                                  p: 2,
                                  flex: 1,
                                  display: 'flex',
                                  flexDirection: 'column',
                                  cursor: 'pointer',
                                  borderRadius: 2,
                                  position: 'relative',
                                  transition: 'all 0.2s cubic-bezier(0.4, 0, 0.2, 1)',
                                  ...(isSelected
                                    ? {
                                      borderColor: isWfc ? 'secondary.main' : 'primary.main',
                                      borderWidth: 2,
                                      bgcolor: isWfc ? 'rgba(124, 58, 237, 0.04)' : 'rgba(18, 150, 219, 0.04)',
                                      boxShadow: isWfc
                                        ? '0 6px 20px -6px rgba(124, 58, 237, 0.25)'
                                        : '0 6px 20px -6px rgba(18, 150, 219, 0.25)',
                                    }
                                    : {
                                      '&:hover': {
                                        borderColor: 'primary.light',
                                        transform: 'translateY(-2px)',
                                      },
                                    }),
                                }}
                              >
                                <Box display="flex" justifyContent="space-between" alignItems="flex-start">
                                  <Box minWidth={0} sx={{ pr: 1 }}>
                                    <Box display="flex" alignItems="center" gap={1} flexWrap="wrap">
                                      <Typography variant="subtitle2" fontWeight={700} color={isWfc ? 'secondary.main' : 'text.primary'}>
                                        {item.editionLabel}
                                      </Typography>
                                      {item.isCurrentMatch && (
                                        <Chip
                                          label="当前对应版本"
                                          color={isWfc ? 'secondary' : 'primary'}
                                          size="small"
                                          sx={{ height: 18, fontSize: '0.68rem', fontWeight: 600 }}
                                        />
                                      )}
                                    </Box>
                                    <Typography variant="caption" color="text.secondary" sx={{ fontFamily: 'monospace', display: 'block', mt: 0.25, wordBreak: 'break-all' }}>
                                      {item.asset.name}
                                    </Typography>
                                  </Box>

                                  <Radio
                                    name="ota-release-asset"
                                    checked={isSelected}
                                    onChange={() => setSelectedAssetName(item.asset.name)}
                                    color={isWfc ? 'secondary' : 'primary'}
                                    size="small"
                                    inputProps={{ 'aria-label': item.editionLabel }}
                                    sx={{ p: 0.5 }}
                                  />
                                </Box>

                                <Typography
                                  variant="caption"
                                  color="text.secondary"
                                  sx={{ display: 'block', flexGrow: 1, my: 1, lineHeight: 1.45 }}
                                >
                                  {isWfc
                                    ? '包含标准版全部功能，并内置 WiFi Calling 协议栈、IPsec IKEv2 隧道驱动、EAP-AKA 鉴权与全球多个运营商配置文件。'
                                    : '包含完整的设备管理、短信智能收发、集中管理 Hub 协同通信以及自动化规则中心。'}
                                </Typography>

                                <Box display="flex" alignItems="center" gap={1.5} pt={1} sx={{ borderTop: '1px dashed', borderColor: 'divider' }}>
                                  <Typography variant="caption" color="text.secondary" fontWeight={600}>
                                    {item.sizeStr}
                                  </Typography>
                                  <Typography variant="caption" color="text.secondary">•</Typography>
                                  <Typography variant="caption" color="success.main" fontWeight={600}>
                                    100% 架构匹配
                                  </Typography>
                                  <Typography variant="caption" color="text.secondary">•</Typography>
                                  <Typography variant="caption" color={isWfc ? 'secondary.main' : 'primary.main'} fontWeight={600}>
                                    {item.isCurrentMatch ? '推荐升级' : '一键扩展'}
                                  </Typography>
                                </Box>
                              </Paper>
                            </Grid>
                          )
                        })}
                      </Grid>
                    </Box>

                    {/* 下载与准备操作栏 */}
                    <Paper
                      variant="outlined"
                      sx={{
                        p: 2,
                        borderRadius: 2,
                        bgcolor: 'action.hover',
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'space-between',
                        flexWrap: 'wrap',
                        gap: 2,
                      }}
                    >
                      <Box>
                        <Typography variant="subtitle2" fontWeight={700}>
                          准备安装包：
                          <Box component="span" sx={{ color: selectedAssetItem?.edition === 'wfc' ? 'secondary.main' : 'primary.main', ml: 0.5 }}>
                            {selectedAsset?.name || '未选择'}
                          </Box>
                        </Typography>
                        <Typography variant="caption" color="text.secondary">
                          下载完成后将自动进行 MD5 二进制完整性校验
                        </Typography>
                      </Box>

                      <Button
                        variant="contained"
                        color={selectedAssetItem?.edition === 'wfc' ? 'secondary' : 'primary'}
                        startIcon={onlineState === 'downloading' ? <CircularProgress size={18} color="inherit" /> : <Download />}
                        onClick={() => void handlePrepareOnlineUpdate()}
                        disabled={onlineState === 'downloading' || !selectedAsset}
                      >
                        {onlineState === 'downloading' ? '正在下载更新包...' : '下载并准备更新'}
                      </Button>

                      {onlineState === 'downloading' && (
                        <Box sx={{ width: '100%', mt: 1 }}>
                          <Box display="flex" justifyContent="space-between" mb={0.5}>
                            <Typography variant="caption" color="text.secondary">
                              {proxyPrefix ? `正在通过加速节点下载...` : '正在直连下载...'}
                            </Typography>
                            <Typography variant="caption" color="text.secondary">
                              {downloadProgress}%
                            </Typography>
                          </Box>
                          <LinearProgress variant="determinate" value={downloadProgress} sx={{ borderRadius: 1 }} />
                        </Box>
                      )}
                    </Paper>

                    <Divider />

                    {/* Release 说明（产物包正下方展示） */}
                    <Box>
                      <Typography variant="subtitle2" fontWeight={700} mb={1}>
                        Release 更新说明 ({latestRelease.tag_name})
                      </Typography>
                      <Paper
                        variant="outlined"
                        sx={{
                          p: 2,
                          maxHeight: 340,
                          overflow: 'auto',
                          bgcolor: 'background.default',
                          borderRadius: 2,
                        }}
                      >
                        <MarkdownPreview source={latestRelease.body} />
                      </Paper>
                    </Box>
                  </CardContent>
                </Card>
              )}
            </Grid>
          </Grid>
        ) : (
          <Stack
            direction={{ xs: 'column', md: supportsOtaUpload ? 'row' : 'column' }}
            spacing={3}
            alignItems="stretch"
          >
            <OnlineUpdateCard
              expanded={false}
              supportsOtaUpload={supportsOtaUpload}
              proxyEnabled={proxyEnabled}
              proxyPreset={proxyPreset}
              customProxy={customProxy}
              proxyPrefix={proxyPrefix}
              onlineState={onlineState}
              downloadProgress={downloadProgress}
              latestRelease={latestRelease}
              currentVersion={status?.current_version}
              manualShowReleaseCard={manualShowReleaseCard}
              onProxyEnabledChange={setProxyEnabled}
              onProxyPresetChange={setProxyPreset}
              onCustomProxyChange={setCustomProxy}
              onCheck={() => void handleCheckOnlineUpdate()}
              onShowReleaseCard={() => setManualShowReleaseCard(true)}
            />

            {supportsOtaUpload && (
              <UploadUpdateCard
                expanded={false}
                uploading={uploading}
                fileInputRef={fileInputRef}
                onFileSelect={event => void handleFileSelect(event)}
              />
            )}
          </Stack>
        )}

      </Stack>

      <Dialog open={confirmDialog === 'apply'} onClose={() => setConfirmDialog(null)}>
        <DialogTitle>确认应用更新</DialogTitle>
        <DialogContent>
          <DialogContentText>
            确定要应用此更新吗？更新将替换当前的后端程序和前端文件。
          </DialogContentText>
          <Alert severity="warning" sx={{ mt: 2 }}>
            建议在应用更新后重启服务以确保更新完全生效。
          </Alert>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setConfirmDialog(null)}>取消</Button>
          <Button
            onClick={() => void handleApply(false)}
            variant="outlined"
            color="primary"
          >
            仅应用（稍后重启）
          </Button>
          <Button
            onClick={() => void handleApply(true)}
            variant="contained"
            color="success"
            startIcon={<RestartAlt />}
          >
            应用并重启
          </Button>
        </DialogActions>
      </Dialog>

      <Dialog open={confirmDialog === 'cancel'} onClose={() => setConfirmDialog(null)}>
        <DialogTitle>确认取消更新</DialogTitle>
        <DialogContent>
          <DialogContentText>
            确定要取消待安装的更新吗？这将删除已上传的更新包。
          </DialogContentText>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setConfirmDialog(null)}>返回</Button>
          <Button
            onClick={() => void handleCancel()}
            variant="contained"
            color="error"
          >
            确认取消
          </Button>
        </DialogActions>
      </Dialog>
    </Box>
  )
}
