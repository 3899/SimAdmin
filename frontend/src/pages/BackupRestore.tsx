import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ChangeEvent,
  type DragEvent,
  type ElementType,
} from 'react'
import {
  Alert,
  Box,
  Button,
  ButtonBase,
  Card,
  CardContent,
  CardHeader,
  Checkbox,
  Chip,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogContentText,
  DialogTitle,
  Divider,
  FormControlLabel,
  IconButton,
  InputAdornment,
  Paper,
  Snackbar,
  Stack,
  Switch,
  Tab,
  Tabs,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  TextField,
  Typography,
} from '@mui/material'
import Grid from '@mui/material/Grid'
import {
  Archive,
  AutoMode,
  Delete,
  Download,
  Folder,
  History,
  Key,
  Memory,
  NotificationsActive,
  Restore,
  Save,
  Schedule,
  Settings,
  SettingsBackupRestore,
  SimCard,
  Sms,
  Storage,
  UploadFile,
  WarningAmber,
  CheckCircle,
} from '@mui/icons-material'
import { api } from '../api/current'
import BackupStorageSelector, { type BackupDestination } from '../components/backup/BackupStorageSelector'
import ErrorSnackbar from '../components/ErrorSnackbar'
import type {
  BackupBlobResponse,
  BackupCleanupConfig,
  BackupComponentKey,
  BackupComponentOption,
  BackupConfig,
  BackupImportMode,
  BackupImportPreview,
  BackupKind,
  BackupLocalFile,
  BackupLocalFilesResponse,
  BackupOptionsResponse,
  AutomationTask,
} from '../api/contracts'

const DEFAULT_COMPONENTS: BackupComponentKey[] = [
  'config',
  'sms',
  'notification_config',
  'automation_config',
  'sim_cache',
  'esim_cache',
]

const ALL_COMPONENTS: BackupComponentKey[] = [
  'config',
  'sms',
  'notification_config',
  'notification_queue',
  'notification_logs',
  'automation_config',
  'automation_logs',
  'sim_cache',
  'esim_cache',
  'auth',
]



const BACKUP_PRESETS: Array<{
  key: string
  label: string
  description: string
  components: BackupComponentKey[]
}> = [
    {
      key: 'common',
      label: '常用备份项',
      description: '系统配置、短信、通知配置、自动化配置、SIM 缓存、eSIM 缓存',
      components: ['config', 'sms', 'notification_config', 'automation_config', 'sim_cache', 'esim_cache'],
    },
    {
      key: 'full',
      label: '完整数据',
      description: '默认组件全量打包，不包含管理员登录凭据',
      components: [
        'config',
        'sms',
        'notification_config',
        'notification_queue',
        'notification_logs',
        'automation_config',
        'automation_logs',
        'sim_cache',
        'esim_cache',
      ],
    },
    {
      key: 'sms',
      label: '短信记录',
      description: '仅导出短信收发历史，适合迁移消息记录',
      components: ['sms'],
    },
    {
      key: 'rules',
      label: '配置与规则',
      description: '基础配置、通知配置和自动化配置',
      components: ['config', 'notification_config', 'automation_config'],
    },
    {
      key: 'cache',
      label: '硬件卡缓存',
      description: '仅保存 SIM 与 eSIM 读取缓存',
      components: ['sim_cache', 'esim_cache'],
    },
  ]

const COMPONENT_ICONS: Record<BackupComponentKey, ElementType> = {
  config: Settings,
  sms: Sms,
  notification_config: NotificationsActive,
  notification_logs: NotificationsActive,
  notification_queue: Archive,
  automation_config: AutoMode,
  automation_logs: History,
  sim_cache: SimCard,
  esim_cache: Memory,
  auth: Key,
}

const COMPONENT_FALLBACK: Record<BackupComponentKey, BackupComponentOption> = {
  config: {
    key: 'config',
    label: '系统配置',
    description: 'APN、漫游、数据连接、DDNS、工作模式和备份设置',
    default_selected: true,
    sensitive: true,
  },
  sms: {
    key: 'sms',
    label: '短信记录',
    description: '短信接收和发送的全部历史记录',
    default_selected: true,
    sensitive: true,
  },
  notification_config: {
    key: 'notification_config',
    label: '通知配置',
    description: '通知渠道、规则、模板和日志保留策略',
    default_selected: true,
    sensitive: false,
  },
  notification_logs: {
    key: 'notification_logs',
    label: '通知日志',
    description: '通知发送历史日志',
    default_selected: true,
    sensitive: false,
  },
  notification_queue: {
    key: 'notification_queue',
    label: '通知队列',
    description: '通知待重试和失败队列',
    default_selected: true,
    sensitive: false,
  },
  automation_config: {
    key: 'automation_config',
    label: '自动化配置',
    description: '自动化任务配置',
    default_selected: true,
    sensitive: false,
  },
  automation_logs: {
    key: 'automation_logs',
    label: '自动化日志',
    description: '自动化任务执行日志',
    default_selected: true,
    sensitive: false,
  },
  sim_cache: {
    key: 'sim_cache',
    label: 'SIM 缓存',
    description: 'SMSC、本机号码和短信容量缓存',
    default_selected: true,
    sensitive: true,
  },
  esim_cache: {
    key: 'esim_cache',
    label: 'eSIM 缓存',
    description: 'eSIM Profile 缓存和 eUICC 缓存，不包含运营商 Profile 内容',
    default_selected: true,
    sensitive: true,
  },
  auth: {
    key: 'auth',
    label: '登录凭据',
    description: '管理员密码哈希和安全策略，不包含会话',
    default_selected: false,
    sensitive: true,
  },
}

const EMPTY_LOCAL_FILES: BackupLocalFilesResponse = {
  backups: [],
  pre_restore: [],
}

const filterTextFieldSx = {
  '& .MuiInputBase-input': {
    fontSize: '14px',
  },
  '& .MuiInputBase-input::placeholder': {
    fontSize: '14px',
  },
  '& .MuiInputLabel-root': {
    fontSize: '14px',
  },
  '& .MuiSelect-select': {
    fontSize: '14px',
  },
  '& .MuiFormControlLabel-label': {
    fontSize: '14px',
  },
} as const

function createDefaultBackupConfig(): BackupConfig {
  return {
    enabled: false,
    components: ['config', 'sms', 'notification_config', 'automation_config', 'sim_cache', 'esim_cache'],
    schedule: {
      mode: 'manual',
      weekdays: [1, 2, 3, 4, 5, 6, 7],
      times: ['04:00'],
      interval_value: 7,
      interval_unit: 'days',
    },
    cleanup: {
      retention_days_enabled: true,
      retention_days: 7,
      max_files_enabled: true,
      max_files: 10,
    },
    storage: {
      local_dir: '/opt/simadmin/backups',
    },
    last_run_at: '',
    last_run_key: '',
  }
}

function uniqueComponents(components: BackupComponentKey[]) {
  const valid = new Set<BackupComponentKey>(ALL_COMPONENTS)
  const seen = new Set<BackupComponentKey>()
  const result = components.filter((component) => {
    if (!valid.has(component) || seen.has(component)) return false
    seen.add(component)
    return true
  })
  return result
}

function normalizeBackupConfig(config?: BackupConfig): BackupConfig {
  const fallback = createDefaultBackupConfig()
  if (!config) return fallback
  return {
    ...fallback,
    ...config,
    components: uniqueComponents(config.components ?? fallback.components),
    schedule: {
      ...fallback.schedule,
      ...config.schedule,
    },
    cleanup: {
      ...fallback.cleanup,
      ...config.cleanup,
    },
    storage: {
      ...fallback.storage,
      ...config.storage,
      local_dir: config.storage?.local_dir?.trim() || fallback.storage.local_dir,
    },
  }
}

function backupKind(components: BackupComponentKey[]): BackupKind {
  const selected = new Set(components)
  const fullComponents = ALL_COMPONENTS.filter((component) => component !== 'auth')
  return fullComponents.every((component) => selected.has(component)) ? 'full' : 'slim'
}

function backupKindLabel(kind: BackupKind) {
  return kind === 'full' ? '完整备份' : '精简备份'
}

function generateDefaultFilename(kind: BackupKind) {
  const now = new Date()
  const date = [
    now.getFullYear(),
    String(now.getMonth() + 1).padStart(2, '0'),
    String(now.getDate()).padStart(2, '0'),
  ].join('')
  const time = [
    String(now.getHours()).padStart(2, '0'),
    String(now.getMinutes()).padStart(2, '0'),
    String(now.getSeconds()).padStart(2, '0'),
  ].join('')
  return `simadmin-backup-${kind}-${date}-${time}`
}

function expectedFilename(kind: BackupKind) {
  return `${generateDefaultFilename(kind)}.zip`
}

function formatDateTime(value: string) {
  if (!value) return '无记录'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString()
}

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`
  return `${(value / 1024 / 1024).toFixed(1)} MB`
}

function positiveInt(value: string | number, fallback: number) {
  const parsed = Number(value)
  if (!Number.isFinite(parsed) || parsed < 1) return fallback
  return Math.trunc(parsed)
}

function downloadBlobFile(file: BackupBlobResponse, filenameOverride?: string) {
  const url = URL.createObjectURL(file.blob)
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = filenameOverride ? `${filenameOverride}.zip` : file.filename
  document.body.appendChild(anchor)
  anchor.click()
  anchor.remove()
  window.setTimeout(() => URL.revokeObjectURL(url), 1000)
}

const getComponentPreviewDetail = (key: BackupComponentKey, records: number) => {
  switch (key) {
    case 'config':
      return {
        desc: '1 个 JSON 配置文件',
        conflict: <Typography component="span" variant="body2" sx={{ color: '#ed6c02', fontWeight: 500, fontSize: 13 }}>有冲突</Typography>,
        strategy: '合并模式仅重写有冲突的键，覆盖模式丢弃全局重新加载'
      }
    case 'sms':
      return {
        desc: `${records} 条短信记录`,
        conflict: <Typography component="span" variant="body2" sx={{ color: '#ed6c02', fontWeight: 500, fontSize: 13 }}>{records > 0 ? '需合并' : '无重复'}</Typography>,
        strategy: '去重：按方向、号码、时间、内容指纹比对重复并过滤'
      }
    case 'notification_config':
      return {
        desc: `${records} 个渠道或规则`,
        conflict: <Typography component="span" variant="body2" sx={{ color: 'text.secondary', fontSize: 13 }}>无冲突</Typography>,
        strategy: '-'
      }
    case 'automation_config':
      return {
        desc: `${records} 个自动化规则`,
        conflict: <Typography component="span" variant="body2" sx={{ color: 'text.secondary', fontSize: 13 }}>无冲突</Typography>,
        strategy: '-'
      }
    case 'sim_cache':
      return {
        desc: `${records} 组卡槽信息缓存`,
        conflict: <Typography component="span" variant="body2" sx={{ color: '#ed6c02', fontWeight: 500, fontSize: 13 }}>有冲突</Typography>,
        strategy: '自动覆盖更新缓存条目'
      }
    case 'esim_cache':
      return {
        desc: `${records} 组芯片状态缓存`,
        conflict: <Typography component="span" variant="body2" sx={{ color: '#ed6c02', fontWeight: 500, fontSize: 13 }}>有冲突</Typography>,
        strategy: '覆盖更新芯片状态，不包含运营商 Profile'
      }
    case 'auth':
      return {
        desc: '1 组登录凭据哈希',
        conflict: <Typography component="span" variant="body2" sx={{ color: '#ef4444', fontWeight: 600, fontSize: 13 }}>冲突确认</Typography>,
        strategy: <span style={{ color: '#ef4444', fontWeight: 500 }}>默认忽略（需要下方显式勾选授权导入）</span>
      }
    case 'notification_logs':
      return {
        desc: `${records} 条历史通知日志`,
        conflict: <Typography component="span" variant="body2" sx={{ color: 'text.secondary', fontSize: 13 }}>无冲突</Typography>,
        strategy: '追加写入执行历史记录'
      }
    case 'notification_queue':
      return {
        desc: `${records} 条队列任务`,
        conflict: <Typography component="span" variant="body2" sx={{ color: 'text.secondary', fontSize: 13 }}>无冲突</Typography>,
        strategy: '追加进入待发队列'
      }
    case 'automation_logs':
      return {
        desc: `${records} 条自动化日志`,
        conflict: <Typography component="span" variant="body2" sx={{ color: 'text.secondary', fontSize: 13 }}>无冲突</Typography>,
        strategy: '追加写入执行历史记录'
      }
    default:
      return {
        desc: `${records} 个记录`,
        conflict: <Typography component="span" variant="body2" sx={{ color: 'text.secondary', fontSize: 13 }}>无冲突</Typography>,
        strategy: '-'
      }
  }
}

export default function BackupRestorePage() {
  const fileInputRef = useRef<HTMLInputElement | null>(null)
  const [tab, setTab] = useState(0)
  const [options, setOptions] = useState<BackupOptionsResponse | null>(null)
  const [config, setConfig] = useState<BackupConfig>(() => createDefaultBackupConfig())
  const [files, setFiles] = useState<BackupLocalFilesResponse>(EMPTY_LOCAL_FILES)
  const [destination, setDestination] = useState<BackupDestination>('download')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [running, setRunning] = useState(false)
  const [previewLoading, setPreviewLoading] = useState(false)
  const [applyDialogOpen, setApplyDialogOpen] = useState(false)
  const [importFile, setImportFile] = useState<File | null>(null)
  const [importLocalFilename, setImportLocalFilename] = useState<string | null>(null)
  const [preview, setPreview] = useState<BackupImportPreview | null>(null)
  const [importComponents, setImportComponents] = useState<BackupComponentKey[]>([])
  const [importMode, setImportMode] = useState<BackupImportMode>('merge')
  const [authConfirmed, setAuthConfirmed] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState<string | null>(null)

  // Modifiable Filename State
  const [customFilename, setCustomFilename] = useState('')
  const [isFilenameEdited, setIsFilenameEdited] = useState(false)

  // Progress Simulation State
  const [progressOpen, setProgressOpen] = useState(false)
  const [progressType, setProgressType] = useState<'export' | 'import' | 'rollback' | null>(null)
  const [progressValue, setProgressValue] = useState(0)
  const [progressStatus, setProgressStatus] = useState('')
  const [progressLogs, setProgressLogs] = useState<string[]>([])
  const [progressStep, setProgressStep] = useState(0)
  const [progressComplete, setProgressComplete] = useState(false)
  const [importAuthAppliedGlobal, setImportAuthAppliedGlobal] = useState(false)
  const [automationTasks, setAutomationTasks] = useState<AutomationTask[]>([])

  const componentOptions = useMemo(() => {
    const fromServer = options?.components ?? []
    const byKey = new Map<BackupComponentKey, BackupComponentOption>()
    ALL_COMPONENTS.forEach((component) => byKey.set(component, COMPONENT_FALLBACK[component]))
    fromServer.forEach((component) => byKey.set(component.key, component))
    return ALL_COMPONENTS.map((component) => byKey.get(component) ?? COMPONENT_FALLBACK[component])
  }, [options])

  const optionByKey = useMemo(() => {
    return new Map(componentOptions.map((component) => [component.key, component]))
  }, [componentOptions])

  const selectedComponents = config.components
  const currentKind = backupKind(selectedComponents)
  const previewHasAuth = preview?.components.some((component) => component.key === 'auth') ?? false
  const authSelectedForImport = importComponents.includes('auth')
  const canApplyImport = Boolean(preview && (importFile || importLocalFilename) && importComponents.length > 0 && (!authSelectedForImport || authConfirmed))

  // Synchronize custom filename when currentKind updates if not edited
  useEffect(() => {
    if (!isFilenameEdited) {
      setCustomFilename(generateDefaultFilename(currentKind))
    }
  }, [currentKind, isFilenameEdited])

  const enabledAutomationBackupTasks = useMemo(() => {
    return automationTasks.filter((task) => task.enabled && task.action.type === 'backup_data')
  }, [automationTasks])

  const isPeriodicBackupEnabled = useMemo(() => {
    return config.enabled || enabledAutomationBackupTasks.length > 0
  }, [config.enabled, enabledAutomationBackupTasks])

  const periodicBackupLabel = useMemo(() => {
    const enabledCount = (config.enabled ? 1 : 0) + enabledAutomationBackupTasks.length
    if (enabledCount === 0) {
      return '定期备份：未启用'
    }
    if (enabledCount > 1) {
      return '定期备份：已启用 (多项)'
    }
    if (config.enabled) {
      const mode = config.schedule.mode === 'fixed' ? '定时' : '间隔'
      return `定期备份：已启用 (${mode})`
    }
    const task = enabledAutomationBackupTasks[0]
    const mode = task.trigger.type === 'fixed' ? '定时' : '间隔'
    return `定期备份：已启用 (${mode})`
  }, [config.enabled, config.schedule.mode, enabledAutomationBackupTasks])

  const configAndDataComponents = useMemo(() => {
    return componentOptions.filter(
      (opt) => ['config', 'sms', 'notification_config', 'automation_config', 'sim_cache', 'esim_cache', 'auth'].includes(opt.key)
    )
  }, [componentOptions])

  const logComponents = useMemo(() => {
    return componentOptions.filter(
      (opt) => ['notification_queue', 'notification_logs', 'automation_logs'].includes(opt.key)
    )
  }, [componentOptions])

  const patchConfig = (patch: Partial<BackupConfig>) => {
    setConfig((prev) => ({ ...prev, ...patch }))
  }

  const patchCleanup = (patch: Partial<BackupCleanupConfig>) => {
    setConfig((prev) => ({
      ...prev,
      cleanup: { ...prev.cleanup, ...patch },
    }))
  }

  const loadFiles = useCallback(async (_silent = false) => {
    try {
      const response = await api.getBackupFiles()
      setFiles(response.data ?? EMPTY_LOCAL_FILES)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }, [])

  const loadData = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const [optionsResponse, configResponse, filesResponse, automationResponse] = await Promise.all([
        api.getBackupOptions(),
        api.getBackupConfig(),
        api.getBackupFiles(),
        api.getAutomationConfig(),
      ])
      const nextOptions = optionsResponse.data ?? null
      const nextConfig = normalizeBackupConfig(configResponse.data)
      setOptions(nextOptions)
      setConfig(nextConfig)
      setFiles(filesResponse.data ?? EMPTY_LOCAL_FILES)
      setAutomationTasks(automationResponse.data?.tasks ?? [])
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    void loadData()
  }, [loadData])

  const setComponents = (components: BackupComponentKey[]) => {
    patchConfig({ components: uniqueComponents(components) })
  }

  const toggleComponent = (component: BackupComponentKey) => {
    setConfig((prev) => {
      const selected = prev.components.includes(component)
      const next = selected
        ? prev.components.filter((item) => item !== component)
        : uniqueComponents([...prev.components, component])
      return {
        ...prev,
        components: next.length > 0 ? next : prev.components,
      }
    })
  }

  const selectAllInList = (list: BackupComponentKey[]) => {
    const next = [...config.components]
    list.forEach((item) => {
      if (!next.includes(item)) {
        next.push(item)
      }
    })
    setComponents(next)
  }

  const clearAllInList = (list: BackupComponentKey[]) => {
    setComponents(config.components.filter((item) => !list.includes(item)))
  }



  const saveConfig = async () => {
    const nextConfig = normalizeBackupConfig({
      ...config,
      components: uniqueComponents(selectedComponents),
      cleanup: {
        ...config.cleanup,
        retention_days: positiveInt(config.cleanup.retention_days, 7),
        max_files: positiveInt(config.cleanup.max_files, 10),
      },
      storage: {
        local_dir: config.storage.local_dir.trim() || '/opt/simadmin/backups',
      },
    })
    setSaving(true)
    setError(null)
    setSuccess(null)
    try {
      const response = await api.setBackupConfig(nextConfig)
      const saved = normalizeBackupConfig(response.data)
      setConfig(saved)
      setSuccess('备份设置已保存')
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setSaving(false)
    }
  }

  // Export progress animation with Blue Monospaced terminal logs
  const runExport = async () => {
    if (selectedComponents.length === 0) {
      setError('请至少选择一个备份组件')
      return
    }
    if (destination === 'webdav') {
      setError('WebDAV 为预留能力，当前版本暂不支持')
      return
    }

    setRunning(true)
    setProgressType('export')
    setProgressValue(0)
    setProgressStep(0)
    setProgressComplete(false)
    setProgressOpen(true)
    setProgressStatus('正在初始化备份生成策略...')

    const termLogs = [
      '[INFO] 正在初始化备份生成策略...',
      `[INFO] 当前判定备份类型为: ${backupKindLabel(currentKind)}`,
      '[INFO] 创建导出临时内存镜像事务...',
      '[INFO] 开始序列化选定组件 JSON 数据...'
    ]
    setProgressLogs([...termLogs])

    try {
      let exportResponse: BackupBlobResponse | null = null
      if (destination === 'local') {
        await api.exportBackupLocal(selectedComponents)
      } else {
        exportResponse = await api.exportBackup(selectedComponents)
      }

      selectedComponents.forEach(comp => {
        termLogs.push(`[INFO] 正在转换组件: components/${comp}.json...`)
      })
      termLogs.push('[INFO] 正在校验数据包内部结构并编译描述 manifest.json...')
      termLogs.push('[INFO] 压缩打包归档中...')

      const finalName = customFilename || generateDefaultFilename(currentKind)
      termLogs.push(`[SUCCESS] 压缩包打包成功！包名称: ${finalName}.zip`)

      if (destination === 'local') {
        termLogs.push('[INFO] 正在将备份包写入本地服务器存储路径 (/opt/simadmin/backups)...')
        termLogs.push(`[SUCCESS] 备份包已成功写入 /opt/simadmin/backups/${finalName}.zip`)
      } else {
        termLogs.push('[INFO] 推送浏览器下载数据...')
        termLogs.push('[SUCCESS] 备份包已成功下载到您的电脑中。')
      }

      let currentLogIdx = 4
      const timer = setInterval(() => {
        if (currentLogIdx < termLogs.length) {
          const shownLogs = termLogs.slice(0, currentLogIdx + 1)
          setProgressLogs(shownLogs)
          const pct = Math.min(99, Math.round((currentLogIdx / (termLogs.length - 1)) * 100))
          setProgressValue(pct)

          if (pct < 25) {
            setProgressStep(0)
            setProgressStatus('初始化导出配置中...')
          } else if (pct < 60) {
            setProgressStep(1)
            setProgressStatus('序列化导出数据组件...')
          } else if (pct < 90) {
            setProgressStep(2)
            setProgressStatus('打包归档压缩文件...')
          } else {
            setProgressStep(3)
            setProgressStatus(destination === 'local' ? '写入服务器存储...' : '下载推送流...')
          }
          currentLogIdx++

          setTimeout(() => {
            const el = document.getElementById('terminal-box-logs')
            if (el) el.scrollTop = el.scrollHeight
          }, 10)
        } else {
          clearInterval(timer)
          setProgressValue(100)
          setProgressStep(3)
          setProgressComplete(true)
          setRunning(false)

          if (destination === 'local') {
            setSuccess(`已保存到本地：${finalName}.zip`)
            void loadFiles(true)
          } else if (exportResponse) {
            downloadBlobFile(exportResponse, finalName)
            setSuccess(`已生成下载：${finalName}.zip`)
          }
        }
      }, 150)

    } catch (err) {
      setProgressOpen(false)
      setRunning(false)
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const previewImportFile = async (file: File) => {
    setPreviewLoading(true)
    setError(null)
    setSuccess(null)
    setImportFile(file)
    setImportLocalFilename(null)
    setPreview(null)
    setImportComponents([])
    setAuthConfirmed(false)
    try {
      const response = await api.previewBackupImport(file)
      const nextPreview = response.data ?? null
      setPreview(nextPreview)
      setImportComponents(
        nextPreview?.components
          .map((component) => component.key)
          .filter((component) => component !== 'auth') ?? [],
      )
      setSuccess('备份包校验通过')
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setPreviewLoading(false)
    }
  }

  const handleImportInputChange = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]
    event.target.value = ''
    if (file) void previewImportFile(file)
  }

  const handleImportDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault()
    const file = event.dataTransfer.files?.[0]
    if (file) void previewImportFile(file)
  }

  const toggleImportComponent = (component: BackupComponentKey) => {
    setImportComponents((prev) => (
      prev.includes(component)
        ? prev.filter((item) => item !== component)
        : [...prev, component]
    ))
  }

  // Restore progress animation with Blue Monospaced terminal logs
  const applyImport = async () => {
    if (!preview || (!importFile && !importLocalFilename)) return
    setRunning(true)
    setApplyDialogOpen(false)

    setProgressType('import')
    setProgressValue(0)
    setProgressStep(0)
    setProgressComplete(false)
    setProgressOpen(true)
    setProgressStatus('正在初始化数据导入恢复...')

    const termLogs = [
      '[INFO] 正在初始化数据导入恢复...',
      '[INFO] 挂起当前的 SQL 连接活动句柄...',
      '[INFO] 正在为设备生成恢复前置的 SQLite 快照镜像...'
    ]
    setProgressLogs([...termLogs])

    try {
      const response = importLocalFilename
        ? await api.applyBackupLocalFile(importLocalFilename, importMode, importComponents)
        : await api.applyBackupImport(importFile as File, importMode, importComponents)

      const snapName = files.pre_restore[0]?.name || `snapshot-${expectedFilename(currentKind).replace('simadmin-backup-', '').replace('.zip', '')}.db`
      termLogs.push(`[SUCCESS] 镜像存储成功: /opt/simadmin/backups/pre-restore/${snapName}`)
      termLogs.push('[INFO] 读取包内 components/ 序列 JSON 数据...')
      termLogs.push('[INFO] 开启单个 SQLite 单事务控制段 (Transaction)...')

      importComponents.forEach(comp => {
        if (comp === 'sms') {
          termLogs.push('[INFO] 正在匹配 sms.json 中的指纹去重（时间+号码+内容相同略过）...')
        } else {
          termLogs.push(`[INFO] 正在恢复配置项 components/${comp}.json...`)
        }
      })

      termLogs.push('[INFO] 正在对 sim_cache 与 esim_cache 进行 Upsert 自然键覆盖...')

      const authApply = importComponents.includes('auth')
      termLogs.push(`[INFO] 管理员密码组件 auth: ${authApply ? '确认应用密码哈希覆盖' : '忽略未授权导入'}`)
      if (authApply) {
        termLogs.push('[INFO] 正在清空 session 会话表 auth_sessions...')
      }
      termLogs.push('[INFO] 正在提交 SQLite 事务记录...')
      termLogs.push('[SUCCESS] 数据库事务成功 Commit！通知后端重载系统配置...')

      let currentLogIdx = 3
      const timer = setInterval(() => {
        if (currentLogIdx < termLogs.length) {
          const shownLogs = termLogs.slice(0, currentLogIdx + 1)
          setProgressLogs(shownLogs)
          const pct = Math.min(99, Math.round((currentLogIdx / (termLogs.length - 1)) * 100))
          setProgressValue(pct)

          if (pct < 25) {
            setProgressStep(1)
            setProgressStatus('安全防灾预警快照克隆中...')
          } else if (pct < 75) {
            setProgressStep(2)
            setProgressStatus('执行数据恢复写入中...')
          } else {
            setProgressStep(3)
            setProgressStatus('配置重载并进行会话注销...')
          }
          currentLogIdx++

          setTimeout(() => {
            const el = document.getElementById('terminal-box-logs')
            if (el) el.scrollTop = el.scrollHeight
          }, 10)
        } else {
          clearInterval(timer)
          setProgressValue(100)
          setProgressStep(3)
          setProgressComplete(true)
          setRunning(false)
          setImportAuthAppliedGlobal(authApply)
          setSuccess(`恢复完成：已导入 ${response.data?.imported_components.length ?? importComponents.length} 个组件`)
          void Promise.all([loadData(), loadFiles(true)])
        }
      }, 150)

    } catch (err) {
      setProgressOpen(false)
      setRunning(false)
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const downloadLocalFile = async (file: BackupLocalFile) => {
    setRunning(true)
    setError(null)
    try {
      const download = await api.downloadBackupFile(file.name)
      downloadBlobFile(download)
      setSuccess(`已下载：${download.filename}`)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setRunning(false)
    }
  }

  const previewLocalFile = async (file: BackupLocalFile) => {
    setPreviewLoading(true)
    setError(null)
    setSuccess(null)
    setPreview(null)
    setImportComponents([])
    try {
      const response = await api.previewBackupLocalFile(file.name)
      const nextPreview = response.data ?? null
      setImportFile(null)
      setImportLocalFilename(file.name)
      setPreview(nextPreview)
      setImportComponents(
        nextPreview?.components
          .map((component) => component.key)
          .filter((component) => component !== 'auth') ?? [],
      )
      setAuthConfirmed(false)
      setTab(1)
      setSuccess('本地备份包校验通过')
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setPreviewLoading(false)
    }
  }

  const deleteLocalFile = async (file: BackupLocalFile) => {
    if (!window.confirm(`确认删除备份文件 ${file.name}？`)) return
    setRunning(true)
    setError(null)
    try {
      await api.deleteBackupFile(file.name)
      setSuccess(`已删除：${file.name}`)
      await loadFiles(true)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setRunning(false)
    }
  }

  // System snapshot rollback progress simulation
  const triggerSnapshotRollback = async (file: BackupLocalFile) => {
    if (!window.confirm(`【警告】确定要将系统回滚到快照 ${file.name} 的状态吗？\n当前操作会物理覆写所有组件数据！`)) return
    setRunning(true)
    setProgressType('rollback')
    setProgressValue(0)
    setProgressStep(0)
    setProgressComplete(false)
    setProgressOpen(true)
    setProgressStatus('正在读取防灾快照...')

    const termLogs = [
      `[INFO] 开始读取快照源文件 /opt/simadmin/backups/pre-restore/${file.name}...`,
      '[INFO] 正在设备内部校验快照归档...',
    ]
    setProgressLogs([...termLogs])

    try {
      termLogs.push('[INFO] 快照解析成功。开始物理覆写数据库事务...')

      await api.applyBackupLocalFile(
        file.name,
        'replace',
        file.components.length ? file.components : DEFAULT_COMPONENTS,
      )
      termLogs.push('[INFO] 正在恢复配置项，清空当前组件缓存并覆写...')
      termLogs.push('[INFO] 正在提交 SQLite 事务记录...')
      termLogs.push('[SUCCESS] 数据库状态热回滚成功！正在重新载入配置与自动化规则...')

      let currentLogIdx = 2
      const timer = setInterval(() => {
        if (currentLogIdx < termLogs.length) {
          const shownLogs = termLogs.slice(0, currentLogIdx + 1)
          setProgressLogs(shownLogs)
          const pct = Math.min(99, Math.round((currentLogIdx / (termLogs.length - 1)) * 100))
          setProgressValue(pct)

          if (pct < 40) {
            setProgressStep(1)
            setProgressStatus('读取镜像快照映射中...')
          } else if (pct < 80) {
            setProgressStep(2)
            setProgressStatus('数据覆写中...')
          } else {
            setProgressStep(3)
            setProgressStatus('服务重载状态对齐...')
          }
          currentLogIdx++

          setTimeout(() => {
            const el = document.getElementById('terminal-box-logs')
            if (el) el.scrollTop = el.scrollHeight
          }, 10)
        } else {
          clearInterval(timer)
          setProgressValue(100)
          setProgressStep(3)
          setProgressComplete(true)
          setRunning(false)
          setSuccess(`系统已成功回滚到快照版本：${file.name}`)
          void Promise.all([loadData(), loadFiles(true)])
        }
      }, 200)

    } catch (err) {
      setProgressOpen(false)
      setRunning(false)
      setError(err instanceof Error ? err.message : String(err))
    }
  }

  const handleCloseProgressDialog = () => {
    setProgressOpen(false)
    if (progressType === 'import' && importAuthAppliedGlobal) {
      alert("管理员凭据已覆盖应用。出于安全性考量，当前账户会话已被强制登出，请使用备份中的旧密码重新登录。")
      window.location.reload()
    } else {
      setPreview(null)
      setImportFile(null)
      setImportLocalFilename(null)
      setImportComponents([])
      setAuthConfirmed(false)
      setProgressType(null)
      setProgressComplete(false)
      setProgressLogs([])
      setProgressValue(0)
      setProgressStep(0)
    }
  }

  const componentLabel = (component: BackupComponentKey) => {
    return optionByKey.get(component)?.label ?? COMPONENT_FALLBACK[component].label
  }

  const getSensitivityChip = (key: BackupComponentKey) => {
    if (key === 'auth') {
      return (
        <Chip
          size="small"
          color="error"
          variant="outlined"
          label="高敏"
          sx={{ height: 18, fontSize: 10, px: 0.25, fontWeight: 600 }}
        />
      )
    }
    if (['config', 'sim_cache', 'esim_cache'].includes(key)) {
      return (
        <Chip
          size="small"
          color="warning"
          variant="outlined"
          label="敏感"
          sx={{ height: 18, fontSize: 10, px: 0.25, fontWeight: 600 }}
        />
      )
    }
    if (['sms', 'notification_config'].includes(key)) {
      return (
        <Chip
          size="small"
          color="success"
          variant="outlined"
          label="隐私"
          sx={{ height: 18, fontSize: 10, px: 0.25, fontWeight: 600 }}
        />
      )
    }
    return null
  }

  const renderComponentOption = (option: BackupComponentOption) => {
    const active = selectedComponents.includes(option.key)
    const Icon = COMPONENT_ICONS[option.key]
    return (
      <ButtonBase
        key={option.key}
        onClick={() => toggleComponent(option.key)}
        sx={(theme) => ({
          alignItems: 'flex-start',
          border: '1px solid',
          borderColor: active ? 'primary.main' : 'divider',
          borderRadius: 1.5,
          bgcolor: active
            ? theme.palette.mode === 'light' ? 'rgba(25, 118, 210, 0.04)' : 'rgba(144, 202, 249, 0.08)'
            : 'background.paper',
          display: 'flex',
          flexDirection: 'column',
          justifyContent: 'space-between',
          minHeight: 110,
          p: 1.25,
          textAlign: 'left',
          width: '100%',
          transition: 'all 0.2s',
          '&:hover': {
            borderColor: 'primary.light',
            bgcolor: 'action.hover',
            transform: 'translateY(-1px)',
          },
        })}
      >
        <Box display="flex" justifyContent="space-between" alignItems="flex-start" width="100%" mb={0.5}>
          <Box display="flex" alignItems="center" gap={0.75}>
            <Icon color={active ? 'primary' : 'action'} sx={{ fontSize: 16 }} />
            <Typography variant="body2" fontWeight={700} sx={{ fontSize: 13 }}>
              {option.label}
            </Typography>
            {getSensitivityChip(option.key)}
          </Box>
          <Checkbox checked={active} size="small" sx={{ p: 0 }} />
        </Box>
        <Typography variant="caption" color="text.secondary" sx={{ fontSize: 10.5, lineHeight: 1.45, flexGrow: 1, mb: 1 }}>
          {option.description}
        </Typography>
        {typeof option.records === 'number' && (
          <Box display="flex" justifyContent="space-between" alignItems="center" width="100%" pt={0.5} sx={{ borderTop: '1px dashed', borderColor: 'divider' }}>
            <Typography variant="caption" sx={{ fontSize: 9, color: 'text.disabled', fontFamily: 'monospace' }}>
              组件：{option.key}
            </Typography>
            <Typography variant="caption" sx={{ fontSize: 9, color: 'text.disabled', fontFamily: 'monospace' }}>
              条目：{option.records}
            </Typography>
          </Box>
        )}
      </ButtonBase>
    )
  }



  const renderCleanupRow = (
    title: string,
    description: string,
    checked: boolean,
    value: number,
    unit: string,
    onCheckedChange: (checked: boolean) => void,
    onValueChange: (value: number) => void,
  ) => (
    <Box>
      <Box display="flex" alignItems="center" justifyContent="space-between" gap={2}>
        <Box minWidth={0}>
          <Typography variant="subtitle2" sx={{ fontSize: 13, fontWeight: 600 }}>{title}</Typography>
          <Typography variant="caption" color="text.secondary">
            {description}
          </Typography>
        </Box>
        <Switch checked={checked} onChange={(event) => onCheckedChange(event.target.checked)} />
      </Box>
      <TextField
        type="number"
        value={value}
        onChange={(event) => onValueChange(positiveInt(event.target.value, value))}
        disabled={!checked}
        fullWidth
        sx={{ mt: 1.5, ...filterTextFieldSx }}
        slotProps={{
          input: { endAdornment: <InputAdornment position="end">{unit}</InputAdornment> },
          htmlInput: { min: 1 },
        }}
      />
    </Box>
  )

  const renderExportTab = () => (
    <Grid container spacing={2.5} alignItems="stretch">
      {/* Top Presets horizontal chip bar */}
      <Grid size={12}>
        <Card variant="outlined" sx={{ borderRadius: 1.5, p: 2 }}>
          <Stack direction="row" spacing={1.5} alignItems="center" flexWrap="wrap">
            <Typography variant="body2" fontWeight={700} color="text.secondary">
              快捷选择模板:
            </Typography>
            {BACKUP_PRESETS.map((preset) => {
              const active = preset.components.length === selectedComponents.length
                && preset.components.every((component) => selectedComponents.includes(component))
              return (
                <Chip
                  key={preset.key}
                  label={preset.label}
                  color={active ? 'primary' : 'default'}
                  onClick={() => setComponents(preset.components)}
                  variant={active ? 'filled' : 'outlined'}
                  clickable
                  sx={{ borderRadius: 1.5, fontWeight: 500 }}
                />
              )
            })}
          </Stack>
        </Card>
      </Grid>

      {/* Left Column: Components list cards */}
      <Grid size={{ xs: 12, lg: 7 }}>
        <Stack spacing={2.5}>
          {/* Card 1: Configs & Data */}
          <Card>
            <CardHeader
              title="配置与数据组件"
              titleTypographyProps={{ fontSize: 16, fontWeight: 600 }}
              action={
                <Stack direction="row" spacing={1} sx={{ mt: -0.5 }}>
                  <Button size="small" variant="text" sx={{ fontWeight: 500 }} onClick={() => selectAllInList(configAndDataComponents.map(o => o.key))}>
                    全选
                  </Button>
                  <Button size="small" variant="text" color="error" sx={{ fontWeight: 500 }} onClick={() => clearAllInList(configAndDataComponents.map(o => o.key))}>
                    清空
                  </Button>
                </Stack>
              }
            />
            <CardContent sx={{ pt: 0 }}>
              <Box
                sx={{
                  display: 'grid',
                  gap: 1.25,
                  gridTemplateColumns: { xs: '1fr', sm: 'repeat(3, minmax(0, 1fr))' },
                }}
              >
                {configAndDataComponents.map(renderComponentOption)}
              </Box>
            </CardContent>
          </Card>

          {/* Card 2: Runtime Logs */}
          <Card>
            <CardHeader
              title="运行日志组件"
              titleTypographyProps={{ fontSize: 16, fontWeight: 600 }}
              action={
                <Stack direction="row" spacing={1} sx={{ mt: -0.5 }}>
                  <Button size="small" variant="text" sx={{ fontWeight: 500 }} onClick={() => selectAllInList(logComponents.map(o => o.key))}>
                    全选
                  </Button>
                  <Button size="small" variant="text" color="error" sx={{ fontWeight: 500 }} onClick={() => clearAllInList(logComponents.map(o => o.key))}>
                    清空
                  </Button>
                </Stack>
              }
            />
            <CardContent sx={{ pt: 0 }}>
              <Box
                sx={{
                  display: 'grid',
                  gap: 1.25,
                  gridTemplateColumns: { xs: '1fr', sm: 'repeat(3, minmax(0, 1fr))' },
                }}
              >
                {logComponents.map(renderComponentOption)}
              </Box>
            </CardContent>
          </Card>
        </Stack>
      </Grid>

      {/* Right Column: Settings & Execution Control */}
      <Grid size={{ xs: 12, lg: 5 }} sx={{ display: 'flex' }}>
        <Card sx={{ flexGrow: 1, display: 'flex', flexDirection: 'column' }}>
          <CardHeader
            title="存储设置"
            titleTypographyProps={{ fontSize: 16, fontWeight: 600 }}
          />
          <CardContent sx={{ pt: 1.5, flexGrow: 1, display: 'flex', flexDirection: 'column' }}>
            <Stack spacing={2.5} sx={{ flexGrow: 1 }}>
              <TextField
                label="备份文件名"
                value={customFilename}
                onChange={(event) => {
                  setIsFilenameEdited(true)
                  setCustomFilename(event.target.value)
                }}
                fullWidth
                sx={filterTextFieldSx}
                slotProps={{
                  input: {
                    endAdornment: <InputAdornment position="end">.zip</InputAdornment>
                  }
                }}
              />

              <BackupStorageSelector
                destination={destination}
                localDir={config.storage.local_dir}
                onDestinationChange={setDestination}
                localDirDisabled={true}
                localDirReadOnly={true}
                textFieldSx={filterTextFieldSx}
              />

              <Divider />

              {/* Execution Summary Widget (Scheme E + B combined) */}
              <Card variant="outlined" sx={{ bgcolor: 'action.hover', border: '1px solid', borderColor: 'divider', borderRadius: 2 }}>
                <CardContent sx={{ p: 2 }}>
                  <Stack spacing={1} textAlign="left">
                    <Typography variant="body2" fontWeight={700} sx={{ display: 'flex', alignItems: 'center', gap: 0.5 }}>
                      备份执行摘要
                    </Typography>
                    <Typography variant="caption" color="text.secondary">
                      备份文件: <strong>{customFilename || generateDefaultFilename(currentKind)}.zip</strong>
                    </Typography>
                    <Typography variant="caption" color="text.secondary">
                      备份类型: <strong>{backupKindLabel(currentKind)} ({currentKind.toUpperCase()})</strong>
                    </Typography>
                    <Typography variant="caption" color="text.secondary">
                      选择项数: 已选中 {selectedComponents.length} / {componentOptions.length} 个组件
                    </Typography>
                  </Stack>
                  <Button
                    variant="contained"
                    startIcon={running ? <CircularProgress size={16} color="inherit" /> : <Archive />}
                    onClick={() => void runExport()}
                    disabled={running || selectedComponents.length === 0}
                    fullWidth
                    sx={{ mt: 2, height: '36.5px' }}
                  >
                    开始备份
                  </Button>
                </CardContent>
              </Card>
            </Stack>
          </CardContent>
        </Card>
      </Grid>
    </Grid>
  )

  const renderImportStepper = (activeStep: number) => (
    <Box sx={{ display: 'flex', alignItems: 'center', justifyContent: 'center', mb: 3.5, position: 'relative', width: '100%', maxWidth: 680, mx: 'auto', px: 2, alignSelf: 'center' }}>
      {/* Background Line */}
      <Box sx={{ position: 'absolute', top: 18, left: '12%', right: '12%', height: 2, bgcolor: 'divider', zIndex: 0 }}>
        <Box sx={{ width: activeStep === 0 ? '0%' : activeStep === 1 ? '50%' : '100%', height: '100%', bgcolor: 'success.main', transition: 'width 0.3s' }} />
      </Box>

      {/* Steps */}
      {([
        { step: 1, label: '上传校验' },
        { step: 2, label: '冲突策略' },
        { step: 3, label: '执行应用' },
      ]).map((item, idx) => {
        const completed = idx < activeStep
        const active = idx === activeStep

        return (
          <Box key={item.step} sx={{ display: 'flex', flexDirection: 'column', alignItems: 'center', flex: 1, zIndex: 1, position: 'relative' }}>
            <Box
              sx={{
                width: 36,
                height: 36,
                borderRadius: '50%',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                fontWeight: 600,
                fontSize: 14,
                border: '2px solid',
                borderColor: completed ? 'success.main' : active ? 'primary.main' : 'divider',
                bgcolor: completed ? 'success.main' : active ? 'primary.main' : 'background.default',
                color: completed || active ? 'primary.contrastText' : 'text.disabled',
                boxShadow: active ? (theme) => `0 0 0 4px ${theme.palette.mode === 'light' ? 'rgba(25, 118, 210, 0.15)' : 'rgba(25, 118, 210, 0.25)'}` : 'none',
                transition: 'all 0.3s',
              }}
            >
              {item.step}
            </Box>
            <Typography
              variant="caption"
              sx={{
                mt: 1,
                fontSize: 12,
                fontWeight: active ? 600 : 500,
                color: completed ? 'success.main' : active ? 'text.primary' : 'text.disabled',
              }}
            >
              {item.label}
            </Typography>
          </Box>
        )
      })}
    </Box>
  )

  const renderImportTab = () => {
    const activeStep = preview ? 1 : 0
    return (
      <Stack spacing={2.5}>
        {renderImportStepper(activeStep)}

        {!preview ? (
          <Card>
            <CardHeader
              title="上传备份包"
              titleTypographyProps={{ fontSize: 16, fontWeight: 600 }}
            />
            <CardContent sx={{ pt: 2 }}>
              <Paper
                variant="outlined"
                tabIndex={0}
                onDragOver={(event) => event.preventDefault()}
                onDrop={handleImportDrop}
                sx={(theme) => ({
                  alignItems: 'center',
                  border: '2px dashed',
                  borderColor: 'divider',
                  borderRadius: '12px',
                  bgcolor: theme.palette.mode === 'light' ? 'rgba(25,118,210,0.03)' : 'rgba(144,202,249,0.08)',
                  cursor: 'pointer',
                  display: 'flex',
                  flexDirection: 'column',
                  gap: 1.5,
                  minHeight: 320,
                  justifyContent: 'center',
                  p: 4,
                  textAlign: 'center',
                  transition: 'all 0.2s ease',
                  outline: 'none',
                  '&:hover': {
                    borderColor: 'primary.main',
                    bgcolor: theme.palette.mode === 'light' ? 'rgba(18, 150, 219, 0.02)' : 'rgba(18, 150, 219, 0.04)',
                  },
                  '&:focus-visible': {
                    borderColor: 'primary.main',
                    boxShadow: `0 0 0 3px ${theme.palette.mode === 'light' ? 'rgba(18, 150, 219, 0.15)' : 'rgba(18, 150, 219, 0.25)'}`,
                  }
                })}
                onClick={() => fileInputRef.current?.click()}
              >
                <input
                  ref={fileInputRef}
                  type="file"
                  accept=".zip,application/zip"
                  hidden
                  onChange={handleImportInputChange}
                />
                {previewLoading ? (
                  <CircularProgress size={28} />
                ) : (
                  <UploadFile color="primary" sx={{ fontSize: 42 }} />
                )}
                <Typography fontWeight={700}>
                  {importFile?.name ?? importLocalFilename ?? '点击选择或拖拽备份包至此处'}
                </Typography>
                <Typography variant="body2" color="text.secondary">
                  仅支持 SimAdmin 生成且含 manifest.json 的 ZIP 备份包
                </Typography>
                <Typography variant="body2" color="text.secondary">
                  上传后会先校验格式版本、组件清单、记录数和 SHA-256 摘要
                </Typography>
              </Paper>
            </CardContent>
          </Card>
        ) : (
          <Card>
            <CardHeader
              title="恢复预览"
              titleTypographyProps={{ fontSize: 16, fontWeight: 600 }}
            />
            <CardContent sx={{ pt: 0 }}>
              <Stack spacing={2.5}>
                <Alert
                  severity="success"
                  variant="outlined"
                  icon={<CheckCircle fontSize="inherit" />}
                  sx={{
                    borderRadius: 1.5,
                    borderColor: 'success.main',
                    color: 'success.main',
                    bgcolor: (theme) => theme.palette.mode === 'light' ? 'rgba(46, 125, 50, 0.02)' : 'rgba(46, 125, 50, 0.06)',
                    '& .MuiAlert-icon': {
                      color: 'success.main',
                    }
                  }}
                >
                  <strong>解析包元数据校验成功：</strong>
                  {` 文件名: ${importFile?.name ?? importLocalFilename ?? preview.filename ?? 'manifest.json'}  丨  备份模式: ${backupKindLabel(preview.backup_kind)} (${preview.backup_kind.toUpperCase()})  丨  版本: v${preview.format_version}  丨  备份时间: ${formatDateTime(preview.created_at)}`}
                </Alert>

                <Typography fontSize={14} fontWeight={600} mb={1}>
                  📊 备份包内容预览
                </Typography>

                <TableContainer component={Paper} variant="outlined" sx={{ borderRadius: 1.5 }}>
                  <Table size="small">
                    <TableHead>
                      <TableRow>
                        <TableCell padding="checkbox" sx={{ bgcolor: 'action.hover', py: 1.25, width: 48 }} />
                        <TableCell sx={{ fontWeight: 600, color: 'text.secondary', fontSize: '13px', py: 1.25, bgcolor: 'action.hover', width: 200 }}>组件</TableCell>
                        <TableCell sx={{ fontWeight: 600, color: 'text.secondary', fontSize: '13px', py: 1.25, bgcolor: 'action.hover', width: 180 }}>包内记录描述</TableCell>
                        <TableCell sx={{ fontWeight: 600, color: 'text.secondary', fontSize: '13px', py: 1.25, bgcolor: 'action.hover', width: 120 }}>冲突</TableCell>
                        <TableCell sx={{ fontWeight: 600, color: 'text.secondary', fontSize: '13px', py: 1.25, bgcolor: 'action.hover' }}>恢复策略</TableCell>
                      </TableRow>
                    </TableHead>
                    <TableBody>
                      {preview.components.map((component) => {
                        const selected = importComponents.includes(component.key)
                        const detail = getComponentPreviewDetail(component.key, component.records)
                        return (
                          <TableRow key={component.key} hover>
                            <TableCell padding="checkbox" sx={{ py: 1.5 }}>
                              <Checkbox
                                checked={selected}
                                onChange={() => toggleImportComponent(component.key)}
                              />
                            </TableCell>
                            <TableCell sx={{ py: 1.5 }}>
                              <Box display="flex" alignItems="center" gap={1}>
                                <Typography fontWeight={700} sx={{ fontSize: '13px' }}>{component.label}</Typography>
                                {component.sensitive && (
                                  <Chip size="small" color="warning" variant="outlined" label="敏感" sx={{ height: 18, fontSize: 10 }} />
                                )}
                              </Box>
                            </TableCell>
                            <TableCell sx={{ fontSize: '13px', color: 'text.secondary', py: 1.5 }}>
                              {detail.desc}
                            </TableCell>
                            <TableCell sx={{ py: 1.5 }}>
                              {detail.conflict}
                            </TableCell>
                            <TableCell sx={{ fontSize: '13px', color: 'text.secondary', py: 1.5 }}>
                              {detail.strategy}
                            </TableCell>
                          </TableRow>
                        )
                      })}
                    </TableBody>
                  </Table>
                </TableContainer>

                <Box>
                  <Typography fontSize={14} fontWeight={600} mb={1}>
                    🧩 冲突处理策略
                  </Typography>
                  <Box display="grid" gridTemplateColumns={{ xs: '1fr', md: '1fr 1fr' }} gap={1.25}>
                    {([
                      {
                        mode: 'merge' as BackupImportMode,
                        title: '增量去重合并',
                        description: '安全策略。系统自动比对重复记录并略过；SIM/eSIM卡缓存键行使用 Upsert 覆盖更新；配置项按被选域合并。',
                      },
                      {
                        mode: 'replace' as BackupImportMode,
                        title: '物理清空覆写',
                        description: '灾难重建策略。先截断清空本系统的对应组件存储，然后再将 JSON 数据强制覆盖。原数据在覆盖后丢失！',
                      },
                    ]).map((item) => {
                      const active = importMode === item.mode
                      return (
                        <ButtonBase
                          key={item.mode}
                          onClick={() => setImportMode(item.mode)}
                          disableRipple
                          sx={(theme) => ({
                            alignItems: 'flex-start',
                            border: '1px solid',
                            borderColor: active ? 'primary.main' : 'divider',
                            borderRadius: 1.5,
                            bgcolor: active
                              ? theme.palette.mode === 'light' ? 'rgba(25,118,210,0.03)' : 'rgba(144,202,249,0.08)'
                              : 'background.paper',
                            display: 'flex',
                            p: 2,
                            textAlign: 'left',
                            transition: 'none',
                          })}
                        >
                          <Box sx={{ pl: 0 }}>
                            <Typography
                              fontWeight={700}
                              sx={{
                                fontSize: 14,
                                color: active ? 'primary.main' : 'text.primary',
                                mb: 0.5
                              }}
                            >
                              {item.title} {item.mode === 'merge' ? '(推荐)' : '(危险)'}
                            </Typography>
                            <Typography
                              variant="body2"
                              sx={{
                                fontSize: 13,
                                fontWeight: 400,
                                color: 'text.secondary',
                                lineHeight: 1.5
                              }}
                            >
                              {item.description}
                            </Typography>
                          </Box>
                        </ButtonBase>
                      )
                    })}
                  </Box>
                </Box>

                {previewHasAuth && (
                  <Alert severity="warning" variant="outlined" sx={{ borderRadius: 1.5 }}>
                    <FormControlLabel
                      control={
                        <Switch
                          checked={authConfirmed}
                          onChange={(event) => setAuthConfirmed(event.target.checked)}
                        />
                      }
                      label="我确认允许恢复管理员登录凭据；恢复后当前会话会失效，需要使用备份包中的密码重新登录"
                    />
                  </Alert>
                )}

                <Stack direction={{ xs: 'column', sm: 'row' }} spacing={1.25} justifyContent="flex-end">
                  <Button
                    variant="outlined"
                    onClick={() => {
                      setPreview(null)
                      setImportFile(null)
                      setImportLocalFilename(null)
                      setImportComponents([])
                      setAuthConfirmed(false)
                    }}
                  >
                    清空预览
                  </Button>
                  <Button
                    variant="contained"
                    color={importMode === 'replace' ? 'warning' : 'primary'}
                    startIcon={<Restore />}
                    disabled={!canApplyImport || running}
                    onClick={() => setApplyDialogOpen(true)}
                  >
                    开始恢复
                  </Button>
                </Stack>
              </Stack>
            </CardContent>
          </Card>
        )}
      </Stack>
    )
  }

  const renderFileList = (
    sectionFiles: BackupLocalFile[],
    isPreRestore: boolean,
  ) => {
    if (sectionFiles.length === 0) {
      return (
        <Alert severity="info" variant="outlined" sx={{ borderRadius: 1.5 }}>
          暂无备份文件。
        </Alert>
      )
    }

    return (
      <Stack spacing={1.5}>
        {sectionFiles.map((file) => (
          <Box
            key={`${isPreRestore ? 'pre' : 'local'}-${file.name}`}
            sx={{
              display: 'flex',
              flexDirection: { xs: 'column', sm: 'row' },
              alignItems: { xs: 'flex-start', sm: 'center' },
              justifyContent: 'space-between',
              border: '1px solid',
              borderColor: 'divider',
              borderRadius: 1.5,
              p: 2,
              gap: 2,
              bgcolor: 'background.paper',
              transition: 'background-color 0.2s',
              '&:hover': {
                bgcolor: 'action.hover',
              },
            }}
          >
            {/* Left Info Column */}
            <Box display="flex" alignItems="center" gap={1.75} minWidth={0}>
              <Box
                sx={{
                  width: 38,
                  height: 38,
                  borderRadius: 1.5,
                  bgcolor: isPreRestore ? 'rgba(25, 118, 210, 0.08)' : 'rgba(46, 125, 50, 0.08)',
                  color: isPreRestore ? 'primary.main' : 'success.main',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                }}
              >
                {isPreRestore ? <History fontSize="medium" /> : <Archive fontSize="medium" />}
              </Box>
              <Box minWidth={0} textAlign="left">
                <Typography
                  variant="body2"
                  fontWeight={600}
                  fontFamily="monospace"
                  sx={{ wordBreak: 'break-all' }}
                >
                  {file.name}
                </Typography>
                <Stack direction="row" spacing={1.5} alignItems="center" mt={0.5} mb={0.75}>
                  {!isPreRestore && file.backup_kind && (
                    <Chip
                      size="small"
                      label={backupKindLabel(file.backup_kind)}
                      variant="outlined"
                      color={file.backup_kind === 'full' ? 'primary' : 'default'}
                      sx={{ height: 18, fontSize: 10 }}
                    />
                  )}
                  {!file.valid && (
                    <Chip
                      size="small"
                      label="包校验失败"
                      variant="outlined"
                      color="error"
                      sx={{ height: 18, fontSize: 10 }}
                    />
                  )}
                  <Typography variant="caption" color="text.secondary">
                    时间: {formatDateTime(file.modified_at)}
                  </Typography>
                  <Typography variant="caption" color="text.secondary">
                    大小: {formatBytes(file.size)}
                  </Typography>

                </Stack>

                {/* Chips showing backup contents */}
                <Box display="flex" gap={0.5} flexWrap="wrap">
                  {isPreRestore ? (
                    <>
                      <Chip size="small" label="SQLite 数据库克隆" sx={{ height: 20, fontSize: 10, bgcolor: 'rgba(25, 118, 210, 0.04)', color: 'primary.main', border: '1px solid rgba(25, 118, 210, 0.12)' }} />
                      <Chip size="small" label="完整数据快照" sx={{ height: 20, fontSize: 10, bgcolor: 'rgba(25, 118, 210, 0.04)', color: 'primary.main', border: '1px solid rgba(25, 118, 210, 0.12)' }} />
                    </>
                  ) : (
                    file.valid
                      ? file.components.map((comp) => (
                        <Chip
                          key={comp}
                          size="small"
                          label={componentLabel(comp)}
                          sx={{ height: 20, fontSize: 10.5 }}
                        />
                      ))
                      : (
                        <Chip
                          size="small"
                          label={file.error || '不是有效的 SimAdmin 备份包'}
                          color="error"
                          variant="outlined"
                          sx={{ height: 20, fontSize: 10.5, maxWidth: 360 }}
                        />
                      )
                  )}
                </Box>
              </Box>
            </Box>

            {/* Actions Row */}
            <Stack direction="row" spacing={1} sx={{ alignSelf: { xs: 'flex-end', sm: 'center' } }}>
              <Button
                variant="outlined"
                size="small"
                startIcon={<Restore />}
                disabled={running || !file.valid}
                onClick={() => {
                  if (isPreRestore) {
                    void triggerSnapshotRollback(file)
                  } else {
                    void previewLocalFile(file)
                  }
                }}
              >
                {isPreRestore ? '回滚' : '恢复'}
              </Button>
              <IconButton
                size="small"
                disabled={running}
                onClick={() => void downloadLocalFile(file)}
                title="下载"
              >
                <Download fontSize="small" />
              </IconButton>
              <IconButton
                size="small"
                color="error"
                disabled={running}
                onClick={() => void deleteLocalFile(file)}
                title="删除"
              >
                <Delete fontSize="small" />
              </IconButton>
            </Stack>
          </Box>
        ))}
      </Stack>
    )
  }

  const renderFilesTab = () => (
    <Grid container spacing={2.5} alignItems="stretch">
      {/* Left Column: Local Backups List & Snapshots */}
      <Grid size={{ xs: 12, lg: 8 }} sx={{ display: 'flex', flexDirection: 'column' }}>
        <Stack spacing={2.5} sx={{ flexGrow: 1, height: '100%' }}>
          {/* Local Backups List Card */}
          <Card sx={{ flexGrow: 1, display: 'flex', flexDirection: 'column' }}>
            <CardHeader
              title={
                <Box display="flex" alignItems="center" gap={1}>
                  <Storage color="primary" fontSize="small" />
                  <Typography fontWeight={700}>本地备份文件库</Typography>
                </Box>
              }
              action={
                <Typography sx={{ fontSize: '12px', fontWeight: 400, color: '#94a3b8', mt: 0.75 }}>
                  路径：{config.storage.local_dir || '/opt/simadmin/backups'}
                </Typography>
              }
            />
            <CardContent sx={{ pt: 1.5, flexGrow: 1, display: 'flex', flexDirection: 'column' }}>
              {renderFileList(files.backups, false)}
            </CardContent>
          </Card>

          {/* Auto disaster Snapshots Card */}
          <Card sx={{ flexGrow: 1, display: 'flex', flexDirection: 'column' }}>
            <CardHeader
              title={
                <Box display="flex" alignItems="center" gap={1}>
                  <History color="primary" fontSize="small" />
                  <Typography fontWeight={700}>恢复前自动快照</Typography>
                </Box>
              }
              action={
                <Typography sx={{ fontSize: '12px', fontWeight: 400, color: '#94a3b8', mt: 0.75 }}>
                  路径：{config.storage.local_dir ? `${config.storage.local_dir}/pre-restore` : '/opt/simadmin/backups/pre-restore'}
                </Typography>
              }
            />
            <CardContent sx={{ pt: 1.5, flexGrow: 1, display: 'flex', flexDirection: 'column' }}>
              {renderFileList(files.pre_restore, true)}
            </CardContent>
          </Card>
        </Stack>
      </Grid>

      {/* Right Column: Auto Cleanup settings */}
      <Grid size={{ xs: 12, lg: 4 }} sx={{ display: 'flex', flexDirection: 'column' }}>
        <Card sx={{ flexGrow: 1, display: 'flex', flexDirection: 'column', height: '100%' }}>
          <CardHeader
            title="自动清理设置"
            titleTypographyProps={{ fontSize: 16, fontWeight: 600 }}
            action={
              <Button
                variant="contained"
                startIcon={<Save />}
                onClick={() => void saveConfig()}
                disabled={saving}
                sx={{ height: '36.5px' }}
              >
                保存设置
              </Button>
            }
          />
          <CardContent sx={{ pt: 0.5, flexGrow: 1, display: 'flex', flexDirection: 'column' }}>
            <Stack spacing={2} sx={{ flexGrow: 1 }}>
              {renderCleanupRow(
                '按保留时长清理',
                '超过设定天数的本地备份包与快照会被删除',
                config.cleanup.retention_days_enabled,
                config.cleanup.retention_days,
                '天',
                (checked) => patchCleanup({ retention_days_enabled: checked }),
                (value) => patchCleanup({ retention_days: value }),
              )}
              <Divider />
              {renderCleanupRow(
                '按最大备份数清理',
                '总数超限时自动删除最旧的备份包或快照',
                config.cleanup.max_files_enabled,
                config.cleanup.max_files,
                '份',
                (checked) => patchCleanup({ max_files_enabled: checked }),
                (value) => patchCleanup({ max_files: value }),
              )}
            </Stack>
          </CardContent>
        </Card>
      </Grid>
    </Grid>
  )

  if (loading) {
    return (
      <Box display="flex" justifyContent="center" alignItems="center" minHeight="50vh">
        <CircularProgress size={32} />
      </Box>
    )
  }

  return (
    <Box>
      <Box display="flex" alignItems="center" mb={2} flexWrap="wrap" gap={2}>
        <Box display="flex" alignItems="center" gap={1.5}>
          <Typography variant="h5" fontWeight={700}>
            备份与恢复
          </Typography>
          {/* 内联统计指标 */}
          <Box display={{ xs: 'none', md: 'flex' }} gap={1} ml={2}>
            <Chip
              variant="outlined"
              size="small"
              icon={<Schedule sx={{ fontSize: '14px !important' }} />}
              label={periodicBackupLabel}
              color={isPeriodicBackupEnabled ? 'primary' : 'default'}
              sx={{ bgcolor: 'rgba(148, 163, 184, 0.06)', fontWeight: 500 }}
            />
          </Box>
        </Box>
      </Box>

      {/* Tabs */}
      <Box sx={{ borderBottom: 1, borderColor: 'divider', mb: 2.5, height: 48, display: 'flex', alignItems: 'center' }}>
        <Tabs
          value={tab}
          onChange={(_, value: number) => setTab(value)}
          variant="scrollable"
          scrollButtons="auto"
          sx={{
            minHeight: 48,
            height: 48,
            '& .MuiTabs-indicator': {
              bottom: 0,
            },
          }}
        >
          <Tab
            icon={<Archive />}
            iconPosition="start"
            label="数据备份"
            sx={{
              minHeight: 48,
              height: 48,
              fontSize: 14,
              py: 0,
            }}
          />
          <Tab
            icon={<UploadFile />}
            iconPosition="start"
            label="数据恢复"
            sx={{
              minHeight: 48,
              height: 48,
              fontSize: 14,
              py: 0,
            }}
          />
          <Tab
            icon={<Folder />}
            iconPosition="start"
            label="本地库与快照"
            sx={{
              minHeight: 48,
              height: 48,
              fontSize: 14,
              py: 0,
            }}
          />
        </Tabs>
      </Box>

      {tab === 0 && renderExportTab()}
      {tab === 1 && renderImportTab()}
      {tab === 2 && renderFilesTab()}

      <Dialog
        open={applyDialogOpen}
        onClose={() => setApplyDialogOpen(false)}
        maxWidth="sm"
        fullWidth
        slotProps={{ paper: { sx: { borderRadius: 2.5 } } }}
      >
        <DialogTitle sx={{ display: 'flex', alignItems: 'center', gap: 1, fontWeight: 700 }}>
          <WarningAmber color={importMode === 'replace' ? 'warning' : 'primary'} />
          确认恢复数据
        </DialogTitle>
        <DialogContent>
          <DialogContentText component="div">
            <Stack spacing={1.5}>
              <Typography variant="body2">
                将以“{importMode === 'merge' ? '增量去重合并' : '清空后覆写'}”策略恢复
                {importComponents.length} 个组件。
              </Typography>
              <Stack direction="row" spacing={0.75} flexWrap="wrap" useFlexGap>
                {importComponents.map((component) => (
                  <Chip key={component} size="small" label={componentLabel(component)} />
                ))}
              </Stack>
              <Alert severity="info" variant="outlined" sx={{ borderRadius: 1.5 }}>
                执行恢复前会先把当前数据组件克隆一份写入恢复前快照目录。
              </Alert>
              {authSelectedForImport && (
                <Alert severity="warning" variant="outlined" sx={{ borderRadius: 1.5 }}>
                  已选择登录凭据组件，恢复完毕后当前管理会话会被注销，页面自动重新装载。
                </Alert>
              )}
            </Stack>
          </DialogContentText>
        </DialogContent>
        <DialogActions sx={{ px: 3, py: 2 }}>
          <Button variant="outlined" onClick={() => setApplyDialogOpen(false)} disabled={running}>
            取消
          </Button>
          <Button
            variant="contained"
            color={importMode === 'replace' ? 'warning' : 'primary'}
            onClick={() => void applyImport()}
            disabled={running}
          >
            确认恢复
          </Button>
        </DialogActions>
      </Dialog>

      {/* Progress simulation Dialog */}
      <Dialog
        open={progressOpen}
        maxWidth="sm"
        fullWidth
        slotProps={{ paper: { sx: { bgcolor: 'background.paper', borderRadius: 2.5 } } }}
      >
        <DialogTitle sx={{ display: 'flex', alignItems: 'center', gap: 1, fontWeight: 700, pb: 1 }}>
          {progressType === 'export' ? <SettingsBackupRestore color="primary" /> : <Restore color="secondary" />}
          <Typography variant="h6" fontWeight={700}>
            {progressComplete
              ? (progressType === 'export' ? '数据归档已完成' : progressType === 'rollback' ? '数据快照回滚已完成' : '数据恢复应用已完成')
              : (progressType === 'export' ? '正在打包生成归档...' : progressType === 'rollback' ? '系统数据快照回滚中...' : '正在写入并恢复数据库数据...')}
          </Typography>
        </DialogTitle>
        <DialogContent>
          <Stack spacing={2.5} sx={{ mt: 1 }}>
            <Box sx={{ width: '100%' }}>
              <Stack direction="row" justifyContent="space-between" alignItems="center" sx={{ position: 'relative', mb: 2 }}>
                {/* Steps line */}
                <Box sx={{ position: 'absolute', top: 12, left: '10%', right: '10%', height: 2, bgcolor: 'divider', zIndex: 0 }} />
                <Box sx={{ position: 'absolute', top: 12, left: '10%', right: '10%', height: 2, width: `${(progressStep / 3) * 80}%`, bgcolor: 'primary.main', zIndex: 1, transition: 'width 0.3s ease' }} />

                {/* Step 1 */}
                <Stack alignItems="center" sx={{ zIndex: 2, flex: 1 }}>
                  <Box sx={{
                    width: 26, height: 26, borderRadius: '50%',
                    bgcolor: progressStep >= 0 ? (progressStep > 0 ? 'success.main' : 'primary.main') : 'background.default',
                    color: progressStep >= 0 ? 'white' : 'text.disabled',
                    border: '2px solid',
                    borderColor: progressStep >= 0 ? (progressStep > 0 ? 'success.main' : 'primary.main') : 'divider',
                    display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 12, fontWeight: 700
                  }}>
                    1
                  </Box>
                  <Typography variant="caption" sx={{ mt: 0.5, fontWeight: progressStep === 0 ? 700 : 500 }}>
                    {progressType === 'export' ? '初始化' : '读取校验'}
                  </Typography>
                </Stack>

                {/* Step 2 */}
                <Stack alignItems="center" sx={{ zIndex: 2, flex: 1 }}>
                  <Box sx={{
                    width: 26, height: 26, borderRadius: '50%',
                    bgcolor: progressStep >= 1 ? (progressStep > 1 ? 'success.main' : 'primary.main') : 'background.default',
                    color: progressStep >= 1 ? 'white' : 'text.disabled',
                    border: '2px solid',
                    borderColor: progressStep >= 1 ? (progressStep > 1 ? 'success.main' : 'primary.main') : 'divider',
                    display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 12, fontWeight: 700
                  }}>
                    2
                  </Box>
                  <Typography variant="caption" sx={{ mt: 0.5, fontWeight: progressStep === 1 ? 700 : 500 }}>
                    {progressType === 'export' ? '序列组件' : '物理快照'}
                  </Typography>
                </Stack>

                {/* Step 3 */}
                <Stack alignItems="center" sx={{ zIndex: 2, flex: 1 }}>
                  <Box sx={{
                    width: 26, height: 26, borderRadius: '50%',
                    bgcolor: progressStep >= 2 ? (progressStep > 2 ? 'success.main' : 'primary.main') : 'background.default',
                    color: progressStep >= 2 ? 'white' : 'text.disabled',
                    border: '2px solid',
                    borderColor: progressStep >= 2 ? (progressStep > 2 ? 'success.main' : 'primary.main') : 'divider',
                    display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 12, fontWeight: 700
                  }}>
                    3
                  </Box>
                  <Typography variant="caption" sx={{ mt: 0.5, fontWeight: progressStep === 2 ? 700 : 500 }}>
                    {progressType === 'export' ? '打包压缩' : '写入数据库'}
                  </Typography>
                </Stack>

                {/* Step 4 */}
                <Stack alignItems="center" sx={{ zIndex: 2, flex: 1 }}>
                  <Box sx={{
                    width: 26, height: 26, borderRadius: '50%',
                    bgcolor: progressStep >= 3 ? 'success.main' : 'background.default',
                    color: progressStep >= 3 ? 'white' : 'text.disabled',
                    border: '2px solid',
                    borderColor: progressStep >= 3 ? 'success.main' : 'divider',
                    display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 12, fontWeight: 700
                  }}>
                    4
                  </Box>
                  <Typography variant="caption" sx={{ mt: 0.5, fontWeight: progressStep === 3 ? 700 : 500 }}>
                    {progressType === 'export' ? '执行存储' : '重载完成'}
                  </Typography>
                </Stack>
              </Stack>
            </Box>

            {/* Progress Value Info */}
            <Box>
              <Box display="flex" justifyContent="space-between" mb={0.5}>
                <Typography variant="caption" fontWeight={700}>{progressStatus}</Typography>
                <Typography variant="caption" fontWeight={700}>{progressValue}%</Typography>
              </Box>
              <Box sx={{ width: '100%', height: 6, bgcolor: 'action.focus', borderRadius: 3, overflow: 'hidden' }}>
                <Box sx={{ width: `${progressValue}%`, height: '100%', bgcolor: 'primary.main', transition: 'width 0.2s ease', borderRadius: 3 }} />
              </Box>
            </Box>

            {/* Blue Monospaced Terminal Logs */}
            <Box sx={{
              bgcolor: '#090f1a',
              color: '#7cd0ff',
              borderRadius: 1.5,
              p: 2,
              fontFamily: 'Fira Code, monospace',
              fontSize: 12,
              lineHeight: 1.6,
              height: 180,
              overflowY: 'auto',
              border: '1px solid',
              borderColor: 'divider',
              boxShadow: 'inset 0 0 8px rgba(0,0,0,0.5)',
              textAlign: 'left'
            }} id="terminal-box-logs">
              {progressLogs.map((log, idx) => (
                <div key={idx} style={{ marginTop: idx === 0 ? 0 : 3 }}>{log}</div>
              ))}
            </Box>
          </Stack>
        </DialogContent>
        <DialogActions sx={{ px: 3, py: 2 }}>
          {progressComplete && (
            <Button
              variant="contained"
              fullWidth
              onClick={handleCloseProgressDialog}
            >
              {progressType === 'import' && importAuthAppliedGlobal ? '关闭并注销重载' : '关闭并返回工作台'}
            </Button>
          )}
        </DialogActions>
      </Dialog>

      <ErrorSnackbar error={error} onClose={() => setError(null)} />
      <Snackbar
        open={Boolean(success)}
        autoHideDuration={4000}
        onClose={() => setSuccess(null)}
        anchorOrigin={{ vertical: 'top', horizontal: 'center' }}
      >
        <Alert severity="success" variant="filled" onClose={() => setSuccess(null)}>
          {success}
        </Alert>
      </Snackbar>
    </Box>
  )
}
