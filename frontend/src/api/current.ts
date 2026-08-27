import type {
  AirplaneModeRequest,
  AirplaneModeResponse,
  ApiResponse,
  ApnListResponse,
  AutomationConfig,
  AutomationLogsResponse,
  BackupBlobResponse,
  BackupComponentKey,
  BackupConfig,
  BackupExportLocalResponse,
  BackupImportApplyResponse,
  BackupImportMode,
  BackupImportPreview,
  BackupLocalFilesResponse,
  BackupOptionsResponse,
  AuthSettingsResponse,
  AuthStatusResponse,
  BandLockRequest,
  BandLockStatus,
  BasebandRestartResponse,
  CallHistoryResponse,
  CallListResponse,
  CallSettingsResponse,
  CellLocationResponse,
  CellLockRequest,
  CellLockResult,
  CellLockStatusResponse,
  ChangePasswordRequest,
  CellsResponse,
  ConnectionAddressesResponse,
  ConnectivityCheckResponse,
  DataConnectionRequest,
  DataConnectionStatus,
  DdnsConfig,
  DdnsLogsResponse,
  DdnsStatusResponse,
  DdnsSyncResponse,
  DeviceInfo,
  EsimCommandResponse,
  EsimDownloadRequest,
  EsimConfig,
  EsimEuiccInfo,
  EsimLpacRepairRequest,
  EsimLpacRepairResponse,
  EsimLpacStatusResponse,
  EsimProfilesResponse,
  LoginRequest,
  HubConfig,
  HubSettingsResponse,
  ManualRegisterRequest,
  NetworkInfo,
  NetworkInterfacesResponse,
  NotificationConfig,
  NotificationLogsResponse,
  NotificationQueueResponse,
  OperatorListResponse,
  OtaStatusResponse,
  OtaLatestReleaseResponse,
  OtaLatestReleaseRequest,
  OtaOnlinePrepareRequest,
  OtaUploadResponse,
  RadioMode,
  RadioModeResponse,
  RoamingRequest,
  RoamingResponse,
  SecurityConfig,
  SetApnRequest,
  SignalStrengthResponse,
  SimInfo,
  UpdateSimCacheRequest,
  SmsMessage,
  SmsConversationRequest,
  SmsListRequest,
  SmsStats,
  SystemStatsResponse,
  WebhookTestResponse,
  WorkMode,
  WorkModeRequest,
  WorkModeResponse,
  WlanConnectRequest,
  WlanForgetRequest,
  WlanProfileRequest,
  WlanProfilesResponse,
  WlanScanResponse,
  WlanStatusResponse,
} from './types'

type SmsListResponse = {
  messages: SmsMessage[]
}

const API_BASE = '/api'

type RequestOptions = RequestInit & {
  returnText?: boolean
  timeoutMs?: number
  skipAuthRedirect?: boolean
}

export interface SimAdminRequest {
  method: string
  path: string
  body: unknown
  timeoutMs?: number
}

export type SimAdminRequestTransport = <T>(request: SimAdminRequest) => Promise<T>

export interface SimAdminBinaryRequest {
  method: string
  path: string
  body: Blob
  filename?: string
  timeoutMs?: number
}

export type SimAdminBinaryTransport = <T>(request: SimAdminBinaryRequest) => Promise<T>

function redirectToLogin() {
  const currentPath = `${window.location.pathname}${window.location.search}`
  if (window.location.pathname === '/login') return
  window.location.assign(currentPath === '/' ? '/login' : `/login?next=${encodeURIComponent(currentPath)}`)
}

function httpStatusMessage(status: number) {
  if (status === 400) return '请求参数有误'
  if (status === 401) return '请先登录'
  if (status === 403) return '没有权限执行此操作'
  if (status === 404) return '请求的接口不存在'
  if (status === 408) return '请求超时'
  if (status === 413) return '上传内容过大'
  if (status >= 500) return '服务器处理失败'
  return `请求失败，状态码 ${status}`
}

function throwIfApiEnvelopeError(payload: unknown): void {
  if (typeof payload !== 'object' || payload === null) return
  if (!('status' in payload)) return
  const status = (payload as { status: unknown }).status
  const message = (payload as { message?: unknown }).message
  if (status === 'error' && typeof message === 'string') {
    throw new Error(message)
  }
}

async function request<T>(
  url: string,
  options: RequestOptions = {},
  transport: SimAdminRequestTransport | null = null,
): Promise<T> {
  const { returnText, timeoutMs, skipAuthRedirect, ...fetchOptions } = options
  if (transport) {
    if (returnText) throw new Error('远程设备面板不支持文本响应')
    let body: unknown = null
    if (typeof fetchOptions.body === 'string' && fetchOptions.body.length > 0) {
      body = JSON.parse(fetchOptions.body)
    } else if (fetchOptions.body !== null && fetchOptions.body !== undefined) {
      throw new Error('远程设备面板仅支持 JSON 请求')
    }
    const response = await transport<T>({
      method: fetchOptions.method ?? 'GET',
      path: url,
      body,
      timeoutMs,
    })
    throwIfApiEnvelopeError(response)
    return response
  }
  const controller = timeoutMs ? new AbortController() : undefined
  const timeoutId = controller
    ? window.setTimeout(() => controller.abort(), timeoutMs)
    : undefined

  let response: Response
  try {
    response = await fetch(`${API_BASE}${url}`, {
      headers: {
        'Content-Type': 'application/json',
        ...fetchOptions.headers,
      },
      credentials: 'same-origin',
      ...fetchOptions,
      signal: controller?.signal ?? fetchOptions.signal,
    })
  } catch (err) {
    if (controller?.signal.aborted) {
      throw new Error(`Request timed out after ${timeoutMs}ms`)
    }
    throw err
  } finally {
    if (timeoutId !== undefined) window.clearTimeout(timeoutId)
  }

  if (!response.ok) {
    if (response.status === 401 && !skipAuthRedirect) {
      redirectToLogin()
    }
    let apiMessage: string | undefined
    try {
      const payload = await response.json()
      if (typeof payload === 'object' && payload !== null && 'message' in payload) {
        const message = (payload as { message?: unknown }).message
        if (typeof message === 'string') apiMessage = message
      }
    } catch {
      // Fall back to the HTTP status below.
    }
    if (apiMessage) throw new Error(apiMessage)
    throw new Error(httpStatusMessage(response.status))
  }

  if (returnText) {
    return (await response.text()) as T
  }

  const json = (await response.json()) as T
  throwIfApiEnvelopeError(json)
  return json
}

function contentDispositionFilename(headerValue: string | null, fallback: string) {
  if (!headerValue) return fallback
  const utf8Match = headerValue.match(/filename\*=UTF-8''([^;]+)/i)
  if (utf8Match?.[1]) {
    try {
      return decodeURIComponent(utf8Match[1].replace(/"/g, ''))
    } catch {
      return utf8Match[1].replace(/"/g, '')
    }
  }
  const plainMatch = headerValue.match(/filename="?([^"]+)"?/i)
  return plainMatch?.[1] ?? fallback
}

async function binaryJsonRequest<T>(
  url: string,
  body: Blob,
  timeoutMs = 60000,
): Promise<T> {
  const controller = new AbortController()
  const timeoutId = window.setTimeout(() => controller.abort(), timeoutMs)

  let response: Response
  try {
    response = await fetch(`${API_BASE}${url}`, {
      method: 'POST',
      body,
      credentials: 'same-origin',
      headers: {
        'Content-Type': 'application/octet-stream',
      },
      signal: controller.signal,
    })
  } catch (err) {
    if (controller.signal.aborted) {
      throw new Error(`Request timed out after ${timeoutMs}ms`)
    }
    throw err
  } finally {
    window.clearTimeout(timeoutId)
  }

  if (!response.ok) {
    if (response.status === 401) redirectToLogin()
    throw new Error(httpStatusMessage(response.status))
  }

  const json = (await response.json()) as T
  throwIfApiEnvelopeError(json)
  return json
}

async function blobDownloadRequest(
  url: string,
  options: RequestInit = {},
  fallbackFilename = 'simadmin-backup.zip',
  timeoutMs = 60000,
): Promise<BackupBlobResponse> {
  const controller = new AbortController()
  const timeoutId = window.setTimeout(() => controller.abort(), timeoutMs)

  let response: Response
  try {
    response = await fetch(`${API_BASE}${url}`, {
      credentials: 'same-origin',
      headers: {
        'Content-Type': 'application/json',
        ...options.headers,
      },
      ...options,
      signal: controller.signal,
    })
  } catch (err) {
    if (controller.signal.aborted) {
      throw new Error(`Request timed out after ${timeoutMs}ms`)
    }
    throw err
  } finally {
    window.clearTimeout(timeoutId)
  }

  if (!response.ok) {
    if (response.status === 401) redirectToLogin()
    throw new Error(httpStatusMessage(response.status))
  }

  const contentType = response.headers.get('content-type') ?? ''
  if (contentType.includes('application/json')) {
    const payload = await response.json()
    throwIfApiEnvelopeError(payload)
    let message = '备份导出失败'
    if (typeof payload === 'object' && payload !== null && 'message' in payload) {
      const payloadMessage = (payload as { message?: unknown }).message
      if (typeof payloadMessage === 'string') message = payloadMessage
    }
    throw new Error(message)
  }

  const blob = await response.blob()
  const header = new Uint8Array(await blob.slice(0, 4).arrayBuffer())
  const zipLike = header[0] === 0x50 && header[1] === 0x4b
  if (!zipLike) {
    throw new Error('下载的备份文件不是有效 ZIP，请检查登录状态或本地备份文件是否损坏')
  }

  return {
    blob,
    filename: contentDispositionFilename(response.headers.get('content-disposition'), fallbackFilename),
  }
}

export class SimAdminCurrentAPI {
  private readonly transport: SimAdminRequestTransport | null
  private readonly binaryTransport: SimAdminBinaryTransport | null

  constructor(
    transport: SimAdminRequestTransport | null = null,
    binaryTransport: SimAdminBinaryTransport | null = null,
  ) {
    this.transport = transport
    this.binaryTransport = binaryTransport
  }

  get supportsBinaryTransfer() {
    return this.transport === null
  }

  get supportsOtaUpload() {
    return this.transport === null || this.binaryTransport !== null
  }

  private request<T>(url: string, options: RequestOptions = {}) {
    return request<T>(url, options, this.transport)
  }

  async getAuthStatus() {
    return this.request<ApiResponse<AuthStatusResponse>>('/auth/status', {
      skipAuthRedirect: true,
    })
  }

  async setupAdminPassword(password: string) {
    const body: LoginRequest = { password }
    return this.request<ApiResponse<null>>('/auth/setup', {
      method: 'POST',
      body: JSON.stringify(body),
      skipAuthRedirect: true,
    })
  }

  async login(password: string) {
    const body: LoginRequest = { password }
    return this.request<ApiResponse<null>>('/auth/login', {
      method: 'POST',
      body: JSON.stringify(body),
      skipAuthRedirect: true,
    })
  }

  async changeAdminPassword(newPassword: string) {
    const body: ChangePasswordRequest = {
      new_password: newPassword,
    }
    return this.request<ApiResponse<null>>('/auth/password', {
      method: 'POST',
      body: JSON.stringify(body),
    })
  }

  async getAuthSettings() {
    return this.request<ApiResponse<AuthSettingsResponse>>('/auth/settings')
  }

  async setAuthSettings(settings: SecurityConfig) {
    return this.request<ApiResponse<SecurityConfig>>('/auth/settings', {
      method: 'POST',
      body: JSON.stringify(settings),
    })
  }

  async logout() {
    return this.request<ApiResponse<null>>('/auth/logout', {
      method: 'POST',
      body: JSON.stringify({}),
      skipAuthRedirect: true,
    })
  }

  async health() {
    return this.request<{ status: string; message: string; version: string }>('/health')
  }

  async getHubSettings() {
    return this.request<ApiResponse<HubSettingsResponse>>('/hub')
  }

  async setHubSettings(config: HubConfig) {
    return this.request<ApiResponse<HubSettingsResponse>>('/hub', {
      method: 'POST',
      body: JSON.stringify(config),
    })
  }

  async unbindHub() {
    return this.request<ApiResponse<HubSettingsResponse>>('/hub/unbind', { method: 'POST' })
  }

  async getWorkMode() {
    return this.request<ApiResponse<WorkModeResponse>>('/work-mode')
  }

  async setWorkMode(mode: WorkMode) {
    const body: WorkModeRequest = { mode, confirm: true }
    return this.request<ApiResponse<WorkModeResponse>>('/work-mode', {
      method: 'POST',
      body: JSON.stringify(body),
      timeoutMs: 10000,
    })
  }

  async getEsimConfig() {
    return this.request<ApiResponse<EsimConfig>>('/esim/config')
  }

  async setEsimConfig(config: EsimConfig) {
    return this.request<ApiResponse<void>>('/esim/config', {
      method: 'POST',
      body: JSON.stringify(config),
    })
  }

  async getEsimEuicc(live = false) {
    return this.request<ApiResponse<EsimEuiccInfo>>(live ? '/esim/euicc?live=1' : '/esim/euicc', {
      timeoutMs: 30000,
    })
  }

  async getEsimProfiles() {
    return this.request<ApiResponse<EsimProfilesResponse>>('/esim/profiles', {
      timeoutMs: 30000,
    })
  }

  async getCachedEsimProfiles() {
    return this.request<ApiResponse<EsimProfilesResponse>>('/esim/profiles?cached=1', {
      timeoutMs: 5000,
    })
  }

  async getEsimLpacStatus() {
    return this.request<ApiResponse<EsimLpacStatusResponse>>('/esim/lpac/status', {
      timeoutMs: 15000,
    })
  }

  async repairEsimLpac(config: EsimLpacRepairRequest) {
    return this.request<ApiResponse<EsimLpacRepairResponse>>('/esim/lpac/repair', {
      method: 'POST',
      body: JSON.stringify(config),
      timeoutMs: 120000,
    })
  }

  async enableEsimProfile(iccid: string) {
    return this.request<ApiResponse<EsimCommandResponse>>(`/esim/profiles/${encodeURIComponent(iccid)}/enable`, {
      method: 'POST',
      body: JSON.stringify({}),
      timeoutMs: 10000,
    })
  }

  async renameEsimProfile(iccid: string, name: string) {
    return this.request<ApiResponse<EsimCommandResponse>>(`/esim/profiles/${encodeURIComponent(iccid)}/rename`, {
      method: 'POST',
      body: JSON.stringify({ name }),
      timeoutMs: 60000,
    })
  }

  async deleteEsimProfile(iccid: string) {
    return this.request<ApiResponse<EsimCommandResponse>>(`/esim/profiles/${encodeURIComponent(iccid)}`, {
      method: 'DELETE',
      timeoutMs: 60000,
    })
  }

  async downloadEsimProfile(requestData: EsimDownloadRequest) {
    return this.request<ApiResponse<EsimCommandResponse>>('/esim/profiles', {
      method: 'POST',
      body: JSON.stringify(requestData),
      timeoutMs: 180000, // 3 minutes timeout
    })
  }

  async getDeviceInfo() {
    return this.request<ApiResponse<DeviceInfo>>('/device')
  }

  async getSimInfo() {
    return this.request<ApiResponse<SimInfo>>('/sim', {
      timeoutMs: 2500,
    })
  }

  async refreshSimDetails() {
    return this.request<ApiResponse<Record<string, never>>>('/sim/details/refresh', {
      method: 'POST',
      body: JSON.stringify({}),
      timeoutMs: 2500,
    })
  }

  async updateSimCache(data: UpdateSimCacheRequest) {
    return this.request<ApiResponse<void>>('/sim/cache', {
      method: 'POST',
      body: JSON.stringify(data),
    })
  }

  async getNetworkInfo() {
    return this.request<ApiResponse<NetworkInfo>>('/network')
  }

  async getCellsInfo() {
    return this.request<ApiResponse<CellsResponse>>('/cells')
  }

  async startCellMonitor() {
    return this.request<ApiResponse<Record<string, never>>>('/cell-monitor/start', {
      method: 'POST',
      body: JSON.stringify({}),
    })
  }

  async stopCellMonitor() {
    return this.request<ApiResponse<Record<string, never>>>('/cell-monitor/stop', {
      method: 'POST',
      body: JSON.stringify({}),
    })
  }

  async getDataStatus() {
    return this.request<ApiResponse<DataConnectionStatus>>('/data')
  }

  async setDataStatus(active: boolean) {
    const body: DataConnectionRequest = { active }
    return this.request<ApiResponse<DataConnectionStatus>>('/data', {
      method: 'POST',
      body: JSON.stringify(body),
    })
  }

  async restartBaseband() {
    return this.request<ApiResponse<BasebandRestartResponse>>('/baseband/restart', {
      method: 'POST',
      body: JSON.stringify({}),
    })
  }

  async getBasebandRestartStatus() {
    return this.request<ApiResponse<BasebandRestartResponse>>('/baseband/restart/status')
  }

  async restartService() {
    return this.request<ApiResponse<Record<string, never>>>('/service/restart', {
      method: 'POST',
      body: JSON.stringify({}),
    })
  }

  async rebootSystem(delaySeconds = 1) {
    return this.request<ApiResponse<{ delay_seconds: number }>>('/system/reboot', {
      method: 'POST',
      body: JSON.stringify({ delay_seconds: delaySeconds }),
    })
  }

  async getRoamingStatus() {
    return this.request<ApiResponse<RoamingResponse>>('/roaming')
  }

  async setRoamingAllowed(allowed: boolean) {
    const body: RoamingRequest = { allowed }
    return this.request<ApiResponse<RoamingResponse>>('/roaming', {
      method: 'POST',
      body: JSON.stringify(body),
    })
  }

  async getAirplaneMode() {
    return this.request<ApiResponse<AirplaneModeResponse>>('/airplane-mode')
  }

  async setAirplaneMode(enabled: boolean) {
    const body: AirplaneModeRequest = { enabled }
    return this.request<ApiResponse<AirplaneModeResponse>>('/airplane-mode', {
      method: 'POST',
      body: JSON.stringify(body),
    })
  }

  async getSystemStats() {
    return this.request<ApiResponse<SystemStatsResponse>>('/stats', {
      timeoutMs: 2500,
    })
  }

  async getNetworkInterfaces() {
    return this.request<ApiResponse<NetworkInterfacesResponse>>('/network/interfaces')
  }

  async getNetworkConnectionAddresses() {
    return this.request<ApiResponse<ConnectionAddressesResponse>>('/network/connection-addresses')
  }

  async getSignalStrength() {
    return this.request<ApiResponse<SignalStrengthResponse>>('/network/signal-strength')
  }

  async getCellLocationInfo() {
    return this.request<ApiResponse<CellLocationResponse>>('/location/cell-info')
  }

  async getOperators() {
    return this.request<ApiResponse<OperatorListResponse>>('/network/operators')
  }

  async scanOperators() {
    return this.request<ApiResponse<OperatorListResponse>>('/network/operators/scan')
  }

  async registerOperatorManual(mccmnc: string) {
    const body: ManualRegisterRequest = { mccmnc }
    return this.request<ApiResponse<Record<string, never>>>('/network/register-manual', {
      method: 'POST',
      body: JSON.stringify(body),
    })
  }

  async registerOperatorAuto() {
    return this.request<ApiResponse<Record<string, never>>>('/network/register-auto', {
      method: 'POST',
      body: JSON.stringify({}),
    })
  }

  async getApnList() {
    return this.request<ApiResponse<ApnListResponse>>('/apn')
  }

  async setApn(config: SetApnRequest) {
    return this.request<ApiResponse<Record<string, unknown>>>('/apn', {
      method: 'POST',
      body: JSON.stringify(config),
    })
  }

  async getRadioMode() {
    return this.request<ApiResponse<RadioModeResponse>>('/radio-mode')
  }

  async setRadioMode(mode: RadioMode) {
    return this.request<ApiResponse<Record<string, never>>>('/radio-mode', {
      method: 'POST',
      body: JSON.stringify({ mode }),
    })
  }

  async getBandLockStatus() {
    return this.request<ApiResponse<BandLockStatus>>('/band-lock')
  }

  async setBandLock(config: BandLockRequest) {
    return this.request<ApiResponse<Record<string, never>>>('/band-lock', {
      method: 'POST',
      body: JSON.stringify(config),
    })
  }

  async getCellLockStatus() {
    return this.request<ApiResponse<CellLockStatusResponse>>('/cell-lock')
  }

  async setCellLock(config: CellLockRequest) {
    return this.request<ApiResponse<CellLockResult>>('/cell-lock', {
      method: 'POST',
      body: JSON.stringify(config),
    })
  }

  async unlockAllCells() {
    return this.request<ApiResponse<CellLockResult>>('/cell-lock/unlock-all', {
      method: 'POST',
      body: JSON.stringify({}),
    })
  }

  async getConnectivity() {
    return this.request<ApiResponse<ConnectivityCheckResponse>>('/connectivity')
  }

  async getDdnsConfig() {
    return this.request<ApiResponse<DdnsConfig>>('/device-network/ddns/config')
  }

  async setDdnsConfig(config: DdnsConfig) {
    return this.request<ApiResponse<DdnsConfig>>('/device-network/ddns/config', {
      method: 'POST',
      body: JSON.stringify(config),
    })
  }

  async getDdnsStatus() {
    return this.request<ApiResponse<DdnsStatusResponse>>('/device-network/ddns/status')
  }

  async syncDdnsNow() {
    return this.request<ApiResponse<DdnsSyncResponse>>('/device-network/ddns/sync', {
      method: 'POST',
      body: JSON.stringify({}),
    })
  }

  async getDdnsLogs() {
    return this.request<ApiResponse<DdnsLogsResponse>>('/device-network/ddns/logs')
  }

  async clearDdnsLogs() {
    return this.request<ApiResponse<Record<string, never>>>('/device-network/ddns/logs/clear', {
      method: 'POST',
      body: JSON.stringify({}),
    })
  }

  async getWlanStatus() {
    return this.request<ApiResponse<WlanStatusResponse>>('/device-network/wlan/status')
  }

  async setWlanEnabled(enabled: boolean) {
    return this.request<ApiResponse<WlanStatusResponse>>('/device-network/wlan/enabled', {
      method: 'POST',
      body: JSON.stringify({ enabled }),
    })
  }

  async scanWlan() {
    return this.request<ApiResponse<WlanScanResponse>>('/device-network/wlan/scan', {
      method: 'POST',
      body: JSON.stringify({}),
    })
  }

  async getWlanProfiles() {
    return this.request<ApiResponse<WlanProfilesResponse>>('/device-network/wlan/profiles')
  }

  async forgetWlan(config: WlanForgetRequest) {
    return this.request<ApiResponse<WlanProfilesResponse>>('/device-network/wlan/forget', {
      method: 'POST',
      body: JSON.stringify(config),
    })
  }

  async connectWlan(config: WlanConnectRequest) {
    return this.request<ApiResponse<WlanStatusResponse>>('/device-network/wlan/connect', {
      method: 'POST',
      body: JSON.stringify(config),
    })
  }

  async disconnectWlan() {
    return this.request<ApiResponse<WlanStatusResponse>>('/device-network/wlan/disconnect', {
      method: 'POST',
      body: JSON.stringify({}),
    })
  }

  async saveWlanProfile(config: WlanProfileRequest) {
    return this.request<ApiResponse<WlanStatusResponse>>('/device-network/wlan/profile', {
      method: 'POST',
      body: JSON.stringify(config),
    })
  }

  async sendSms(phoneNumber: string, content: string) {
    return this.request<ApiResponse<{ path: string }>>('/sms/send', {
      method: 'POST',
      body: JSON.stringify({ phone_number: phoneNumber, content }),
    })
  }

  async getSmsList(params?: SmsListRequest) {
    const query = new URLSearchParams()
    if (params?.limit) query.append('limit', params.limit.toString())
    if (params?.offset) query.append('offset', params.offset.toString())
    if (params?.direction) query.append('direction', params.direction)
    const queryStr = query.toString() ? `?${query.toString()}` : ''
    return this.request<ApiResponse<SmsListResponse>>(`/sms/list${queryStr}`)
  }

  async getSmsConversation(params: SmsConversationRequest) {
    const query = new URLSearchParams()
    query.append('phone_number', params.phone_number)
    if (params.limit) query.append('limit', params.limit.toString())
    return this.request<ApiResponse<SmsListResponse>>(`/sms/conversation?${query.toString()}`)
  }

  async getSmsStats() {
    return this.request<ApiResponse<SmsStats>>('/sms/stats')
  }

  async clearAllSms() {
    return this.request<ApiResponse<Record<string, never>>>('/sms/clear', {
      method: 'POST',
    })
  }

  async deleteSmsMessage(id: number) {
    return this.request<ApiResponse<{ deleted: number }>>(`/sms/message/${id}`, {
      method: 'DELETE',
    })
  }

  async deleteSmsConversation(phoneNumber: string) {
    return this.request<ApiResponse<{ deleted: number }>>(
      `/sms/conversation/${encodeURIComponent(phoneNumber)}`,
      {
        method: 'DELETE',
      },
    )
  }

  async deleteSmsBatch(payload: { ids?: number[]; phone_numbers?: string[] }) {
    return this.request<ApiResponse<{ deleted: number }>>('/sms/batch-delete', {
      method: 'POST',
      body: JSON.stringify(payload),
    })
  }

  async getCalls() {
    return this.request<ApiResponse<CallListResponse>>('/calls')
  }

  async dialCall(phoneNumber: string) {
    return this.request<ApiResponse<{ path: string }>>('/call/dial', {
      method: 'POST',
      body: JSON.stringify({ phone_number: phoneNumber }),
    })
  }

  async hangupCall(path: string) {
    return this.request<ApiResponse<Record<string, never>>>('/call/hangup', {
      method: 'POST',
      body: JSON.stringify({ path }),
    })
  }

  async hangupAllCalls() {
    return this.request<ApiResponse<Record<string, never>>>('/call/hangup-all', {
      method: 'POST',
      body: JSON.stringify({}),
    })
  }

  async answerCall(path: string) {
    return this.request<ApiResponse<Record<string, never>>>('/call/answer', {
      method: 'POST',
      body: JSON.stringify({ path }),
    })
  }

  async getCallHistory(params?: { limit?: number; offset?: number }) {
    const query = new URLSearchParams()
    if (params?.limit) query.append('limit', params.limit.toString())
    if (params?.offset) query.append('offset', params.offset.toString())
    const queryStr = query.toString() ? `?${query.toString()}` : ''
    return this.request<ApiResponse<CallHistoryResponse>>(`/call/history${queryStr}`)
  }

  async deleteCallRecord(id: number) {
    return this.request<ApiResponse<Record<string, never>>>(`/call/history/${id}`, {
      method: 'DELETE',
    })
  }

  async clearCallHistory() {
    return this.request<ApiResponse<Record<string, never>>>('/call/history/clear', {
      method: 'POST',
    })
  }

  async getCallSettings() {
    return this.request<ApiResponse<CallSettingsResponse>>('/call/settings')
  }

  async setCallWaiting(enabled: boolean) {
    return this.request<ApiResponse<Record<string, never>>>('/call/settings', {
      method: 'POST',
      body: JSON.stringify({ property: 'VoiceCallWaiting', value: enabled ? 'enabled' : 'disabled' }),
    })
  }

  async getNotificationConfig() {
    return this.request<ApiResponse<NotificationConfig>>('/notifications/config')
  }

  async setNotificationConfig(config: NotificationConfig) {
    return this.request<ApiResponse<Record<string, unknown>>>('/notifications/config', {
      method: 'POST',
      body: JSON.stringify(config),
    })
  }

  async testNotificationChannel(channel: string) {
    return this.request<ApiResponse<WebhookTestResponse>>(`/notifications/test/${channel}`, {
      method: 'POST',
    })
  }

  async getNotificationLogs(params?: { type?: string; status?: string; q?: string; start_date?: string; end_date?: string; limit?: number; offset?: number }) {
    const query = new URLSearchParams()
    if (params?.type) query.append('type', params.type)
    if (params?.status) query.append('status', params.status)
    if (params?.q) query.append('q', params.q)
    if (params?.start_date) query.append('start_date', params.start_date)
    if (params?.end_date) query.append('end_date', params.end_date)
    if (params?.limit) query.append('limit', params.limit.toString())
    if (params?.offset) query.append('offset', params.offset.toString())
    const queryStr = query.toString() ? `?${query.toString()}` : ''
    return this.request<ApiResponse<NotificationLogsResponse>>(`/notifications/logs${queryStr}`)
  }

  async clearNotificationLogs(filters?: { type?: string; status?: string; start_date?: string; end_date?: string }) {
    return this.request<ApiResponse<{ deleted: number }>>('/notifications/logs/clear', {
      method: 'POST',
      body: JSON.stringify(filters ?? {}),
    })
  }

  async getNotificationQueue(params?: { limit?: number }) {
    const query = new URLSearchParams()
    if (params?.limit) query.append('limit', params.limit.toString())
    const queryStr = query.toString() ? `?${query.toString()}` : ''
    return this.request<ApiResponse<NotificationQueueResponse>>(`/notifications/queue${queryStr}`)
  }

  async retryNotificationQueueItem(id: number | string) {
    return this.request<ApiResponse<{ updated: number }>>(`/notifications/queue/${id}/retry`, {
      method: 'POST',
    })
  }

  async deleteNotificationQueueItem(id: number | string) {
    return this.request<ApiResponse<{ updated: number }>>(`/notifications/queue/${id}`, {
      method: 'DELETE',
    })
  }

  async retryAllNotificationQueue() {
    return this.request<ApiResponse<{ updated: number }>>('/notifications/queue/retry-all', {
      method: 'POST',
    })
  }

  async clearNotificationQueue() {
    return this.request<ApiResponse<{ updated: number }>>('/notifications/queue/clear', {
      method: 'POST',
    })
  }

  async getOtaStatus() {
    return this.request<ApiResponse<OtaStatusResponse>>('/ota/status')
  }

  async uploadOta(file: File) {
    if (this.binaryTransport) {
      const response = await this.binaryTransport<ApiResponse<OtaUploadResponse>>({
        method: 'POST',
        path: '/ota/upload',
        body: file,
        filename: file.name,
        timeoutMs: 5 * 60 * 1000,
      })
      throwIfApiEnvelopeError(response)
      return response
    }
    const response = await fetch(`${API_BASE}/ota/upload`, {
      method: 'POST',
      body: file,
      credentials: 'same-origin',
      headers: {
        'Content-Type': 'application/octet-stream',
      },
    })

    if (!response.ok) {
      if (response.status === 401) {
        redirectToLogin()
      }
      throw new Error(httpStatusMessage(response.status))
    }

    return response.json() as Promise<ApiResponse<OtaUploadResponse>>
  }

  async prepareOnlineOta(config: OtaOnlinePrepareRequest) {
    return this.request<ApiResponse<OtaUploadResponse>>('/ota/online-prepare', {
      method: 'POST',
      body: JSON.stringify(config),
    })
  }

  async getLatestOtaRelease(config: OtaLatestReleaseRequest) {
    return this.request<ApiResponse<OtaLatestReleaseResponse>>('/ota/latest-release', {
      method: 'POST',
      body: JSON.stringify(config),
    })
  }

  async applyOta(restartNow = false) {
    return this.request<ApiResponse<{ applied: boolean }>>('/ota/apply', {
      method: 'POST',
      body: JSON.stringify({ restart_now: restartNow }),
    })
  }

  async cancelOta() {
    return this.request<ApiResponse<Record<string, unknown>>>('/ota/cancel', {
      method: 'POST',
    })
  }

  async getBackupOptions() {
    return this.request<ApiResponse<BackupOptionsResponse>>('/backup/options')
  }

  async getBackupConfig() {
    return this.request<ApiResponse<BackupConfig>>('/backup/config')
  }

  async setBackupConfig(config: BackupConfig) {
    return this.request<ApiResponse<BackupConfig>>('/backup/config', {
      method: 'POST',
      body: JSON.stringify({ config }),
    })
  }

  async exportBackup(components: BackupComponentKey[]) {
    return blobDownloadRequest('/backup/export', {
      method: 'POST',
      body: JSON.stringify({ components }),
    }, 'simadmin-backup.zip', 120000)
  }

  async exportBackupLocal(components: BackupComponentKey[]) {
    return this.request<ApiResponse<BackupExportLocalResponse>>('/backup/export-local', {
      method: 'POST',
      body: JSON.stringify({ components }),
      timeoutMs: 120000,
    })
  }

  async clearBackupData(components: BackupComponentKey[]) {
    return this.request<ApiResponse<{ cleared_components: BackupComponentKey[] }>>('/backup/data/clear', {
      method: 'POST',
      body: JSON.stringify({ components }),
      timeoutMs: 120000,
    })
  }

  async previewBackupImport(file: Blob) {
    return binaryJsonRequest<ApiResponse<BackupImportPreview>>('/backup/import/preview', file, 120000)
  }

  async applyBackupImport(file: Blob, mode: BackupImportMode, components: BackupComponentKey[]) {
    const query = new URLSearchParams()
    query.append('mode', mode)
    query.append('components', components.join(','))
    return binaryJsonRequest<ApiResponse<BackupImportApplyResponse>>(
      `/backup/import/apply?${query.toString()}`,
      file,
      120000,
    )
  }

  async previewBackupLocalFile(filename: string) {
    return this.request<ApiResponse<BackupImportPreview>>(
      `/backup/files/${encodeURIComponent(filename)}/preview`
    )
  }

  async applyBackupLocalFile(filename: string, mode: BackupImportMode, components: BackupComponentKey[]) {
    const query = new URLSearchParams()
    query.append('mode', mode)
    query.append('components', components.join(','))
    return this.request<ApiResponse<BackupImportApplyResponse>>(
      `/backup/files/${encodeURIComponent(filename)}/apply?${query.toString()}`,
      {
        method: 'POST',
        timeoutMs: 120000,
      },
    )
  }

  async getBackupFiles() {
    return this.request<ApiResponse<BackupLocalFilesResponse>>('/backup/files')
  }

  async downloadBackupFile(filename: string) {
    return blobDownloadRequest(
      `/backup/files/${encodeURIComponent(filename)}`,
      { method: 'GET' },
      filename,
      120000,
    )
  }

  async deleteBackupFile(filename: string) {
    return this.request<ApiResponse<{ deleted: boolean }>>(`/backup/files/${encodeURIComponent(filename)}`, {
      method: 'DELETE',
    })
  }

  async getAutomationConfig() {
    return this.request<ApiResponse<AutomationConfig>>('/automation/config')
  }

  async setAutomationConfig(config: AutomationConfig) {
    return this.request<ApiResponse<Record<string, unknown>>>('/automation/config', {
      method: 'POST',
      body: JSON.stringify(config),
    })
  }

  async testAutomationTask(taskId: string) {
    return this.request<ApiResponse<Record<string, unknown>>>(`/automation/test/${encodeURIComponent(taskId)}`, {
      method: 'POST',
    })
  }

  async getAutomationLogs(params?: { type?: string; status?: string; start_date?: string; end_date?: string; q?: string; limit?: number; offset?: number }) {
    const query = new URLSearchParams()
    if (params?.type) query.append('type', params.type)
    if (params?.status) query.append('status', params.status)
    if (params?.q) query.append('q', params.q)
    if (params?.start_date) query.append('start_date', params.start_date)
    if (params?.end_date) query.append('end_date', params.end_date)
    if (params?.limit) query.append('limit', params.limit.toString())
    if (params?.offset) query.append('offset', params.offset.toString())
    const queryStr = query.toString() ? `?${query.toString()}` : ''
    return this.request<ApiResponse<AutomationLogsResponse>>(`/automation/logs${queryStr}`)
  }

  async clearAutomationLogs(filters?: { type?: string; status?: string; start_date?: string; end_date?: string }) {
    return this.request<ApiResponse<{ deleted: number }>>('/automation/logs/clear', {
      method: 'POST',
      body: JSON.stringify(filters ?? {}),
    })
  }
}

export const api = new SimAdminCurrentAPI()

export * from './types'
