import { useState, useEffect, useCallback, useRef } from 'react'
import type { ReactNode } from 'react'
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
} from '@mui/material'
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
  NewReleases,
} from '@mui/icons-material'
import { api } from '../api/current'
import type { OtaLatestReleaseResponse, OtaStatusResponse, OtaUploadResponse } from '../api/types'

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
  })} (北京时间)`
}

function formatBytes(size?: number) {
  if (!size) return '未知'
  const mb = size / 1024 / 1024
  return `${mb.toFixed(1)} MB`
}

function getReleaseAsset(release: OtaLatestReleaseResponse) {
  return release.assets?.find(asset => /\.(tar\.gz|tgz|zip)$/i.test(asset.name)) ?? release.assets?.[0]
}

function inferArch(assetName?: string) {
  if (!assetName) return '未知'
  if (/aarch64|arm64/i.test(assetName)) return 'aarch64-unknown-linux-musl'
  if (/x86_64|amd64/i.test(assetName)) return 'x86_64-unknown-linux-musl'
  if (/armv7|armhf/i.test(assetName)) return 'armv7-unknown-linux-musleabihf'
  return '未知'
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

export default function OtaUpdate() {
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
  const [downloadProgress, setDownloadProgress] = useState(0)

  const fileInputRef = useRef<HTMLInputElement>(null)

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
  }, [])

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
    setDownloadProgress(0)

    try {
      const proxyPrefix = getProxyPrefix()
      if (proxyEnabled && proxyPreset === 'custom' && !proxyPrefix) {
        throw new Error('请输入自定义加速节点地址，或关闭 GitHub 下载加速')
      }

      const res = await api.getLatestOtaRelease({ proxy_prefix: proxyPrefix || undefined })
      if (res.status !== 'ok' || !res.data) {
        throw new Error(res.message || 'GitHub Releases 请求失败')
      }

      const release = res.data
      setLatestRelease(release)
      const currentVersion = status?.current_version || '0.0.0'
      setOnlineState(compareVersions(release.tag_name, currentVersion) > 0 ? 'available' : 'latest')
    } catch (err) {
      setOnlineState('idle')
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const handlePrepareOnlineUpdate = async () => {
    if (!latestRelease) return

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
      const res = await api.prepareOnlineOta({ proxy_prefix: proxyPrefix || undefined })
      window.clearInterval(timer)
      setDownloadProgress(100)

      const prepared = res.data
      if (res.status === 'ok' && prepared) {
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

  const handleFileSelect = async (event: React.ChangeEvent<HTMLInputElement>) => {
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

  const asset = latestRelease ? getReleaseAsset(latestRelease) : undefined
  const pendingMeta = status?.pending_meta
  const hasPendingUpdate = Boolean(pendingMeta)
  const proxyPrefix = getProxyPrefix()
  const downloadUrl = asset?.browser_download_url
    ? `${proxyPrefix}${asset.browser_download_url}`
    : undefined

  return (
    <Box>
      <Box display="flex" justifyContent="space-between" alignItems="center" mb={2}>
        <Box>
          <Typography variant="h5" gutterBottom fontWeight={700}>
            OTA 更新
          </Typography>
          <Typography variant="body2" color="text.secondary">
            上传并安装系统更新包 / 在线获取最新版本
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
        <Card>
          <CardContent>
            <Box display="flex" alignItems="center" gap={1} mb={2}>
              <Info color="primary" />
              <Typography variant="h6">当前版本</Typography>
            </Box>
            <TableContainer>
              <Table size="small">
                <TableBody>
                  <TableRow>
                    <TableCell component="th" sx={{ width: 150 }}>版本号</TableCell>
                    <TableCell>
                      <Chip label={status?.current_version || 'N/A'} color="primary" size="small" />
                    </TableCell>
                  </TableRow>
                  <TableRow>
                    <TableCell component="th">Commit</TableCell>
                    <TableCell sx={{ fontFamily: 'monospace' }}>
                      {status?.current_commit || 'N/A'}
                    </TableCell>
                  </TableRow>
                </TableBody>
              </Table>
            </TableContainer>
          </CardContent>
        </Card>

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

        <Stack direction={{ xs: 'column', md: 'row' }} spacing={3} alignItems="stretch">
          <Card sx={{ flex: 1, minWidth: 0, display: 'flex' }}>
            <CardContent sx={{ flex: 1 }}>
              <Box display="flex" justifyContent="space-between" alignItems="center" gap={2} mb={2}>
                <Box display="flex" alignItems="center" gap={1}>
                  <Public color="primary" />
                  <Typography variant="h6">在线更新</Typography>
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
                    onChange={event => setProxyEnabled(event.target.checked)}
                  />
                }
                label="启用 GitHub 下载加速"
              />
              {proxyEnabled && (
                <Stack spacing={2} direction={{ xs: 'column', sm: 'row' }}>
                  <FormControl fullWidth size="small">
                    <InputLabel id="proxy-preset-label">加速节点</InputLabel>
                    <Select
                      labelId="proxy-preset-label"
                      label="加速节点"
                      value={proxyPreset}
                      onChange={event => setProxyPreset(event.target.value as ProxyPreset)}
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
                      onChange={event => setCustomProxy(event.target.value)}
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
                onClick={() => void handleCheckOnlineUpdate()}
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

            {onlineState === 'available' && latestRelease && (
              <Alert severity="info" icon={<NewReleases />} sx={{ mt: 2 }}>
                <AlertTitle>发现可用更新</AlertTitle>
                <Stack spacing={2}>
                  <Stack direction="row" spacing={1} alignItems="center" flexWrap="wrap">
                    <Chip label={status?.current_version || 'N/A'} size="small" variant="outlined" />
                    <Typography variant="body2">→</Typography>
                    <Chip label={latestRelease.tag_name} size="small" color="primary" />
                    <Typography variant="body2" color="text.secondary">
                      发布时间：{formatDateTime(latestRelease.published_at)}
                    </Typography>
                  </Stack>
                  <TableContainer component={Paper} variant="outlined">
                    <Table size="small">
                      <TableBody>
                        <TableRow>
                          <TableCell component="th" sx={{ width: 120 }}>更新包</TableCell>
                          <TableCell>{asset?.name || '未找到 Release Asset'}</TableCell>
                        </TableRow>
                        <TableRow>
                          <TableCell component="th">架构</TableCell>
                          <TableCell>{inferArch(asset?.name)}</TableCell>
                        </TableRow>
                        <TableRow>
                          <TableCell component="th">大小</TableCell>
                          <TableCell>{formatBytes(asset?.size)}</TableCell>
                        </TableRow>
                        <TableRow>
                          <TableCell component="th">Commit</TableCell>
                          <TableCell sx={{ fontFamily: 'monospace' }}>
                            {(latestRelease.target_commitish || '').slice(0, 12) || 'N/A'}
                          </TableCell>
                        </TableRow>
                      </TableBody>
                    </Table>
                  </TableContainer>
                  <Box>
                    <Typography variant="subtitle2" mb={1}>更新日志 (Release Notes)</Typography>
                    <Paper
                      variant="outlined"
                      sx={{ p: 2, maxHeight: 220, overflow: 'auto', bgcolor: 'background.default' }}
                    >
                      <MarkdownPreview source={latestRelease.body} />
                    </Paper>
                  </Box>
                  {downloadUrl && (
                    <Typography variant="caption" color="text.secondary" sx={{ wordBreak: 'break-all' }}>
                      下载地址：{downloadUrl}
                    </Typography>
                  )}
                  <Box>
                    <Button
                      variant="contained"
                      startIcon={<Download />}
                      onClick={() => void handlePrepareOnlineUpdate()}
                      disabled={!asset}
                    >
                      下载并准备更新
                    </Button>
                  </Box>
                </Stack>
              </Alert>
            )}

            {onlineState === 'downloading' && (
              <Box sx={{ mt: 2 }}>
                <Box display="flex" justifyContent="space-between" mb={1}>
                  <Typography variant="body2" color="text.secondary">
                    {proxyPrefix ? `正在通过 ${proxyPreset === 'custom' ? proxyPrefix : new URL(proxyPrefix).hostname} 下载更新包...` : '正在直连下载更新包...'}
                  </Typography>
                  <Typography variant="body2" color="text.secondary">{downloadProgress}%</Typography>
                </Box>
                <LinearProgress variant="determinate" value={downloadProgress} />
              </Box>
            )}

            {onlineState === 'latest' && (
              <Alert severity="success" sx={{ mt: 2 }}>
                当前版本 {status?.current_version || 'N/A'} 已经是最新发布的稳定版。
              </Alert>
            )}
            </CardContent>
          </Card>

          <Card sx={{ flex: 1, minWidth: 0, display: 'flex' }}>
            <CardContent sx={{ flex: 1 }}>
              <Box display="flex" alignItems="center" gap={1} mb={2}>
                <CloudUpload color="primary" />
                <Typography variant="h6">上传更新包</Typography>
              </Box>

              <Typography variant="body2" color="text.secondary" mb={2}>
                前往 GitHub Releases 或 QQ 群下载安装包，手动上传即可完成升级或降级。
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
              onChange={(event) => void handleFileSelect(event)}
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
        </Stack>

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
