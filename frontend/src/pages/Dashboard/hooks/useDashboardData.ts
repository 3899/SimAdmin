import { useState, useCallback, useEffect, useRef } from 'react'
import { useSimAdminApi } from '@/contexts/ApiContext'
import type {
  DeviceInfo,
  NetworkInfo,
  CellsResponse,
  QosInfo,
  SimInfo,
  SystemStatsResponse,
  AirplaneModeResponse,
  RoamingResponse,
  ConnectionAddressesResponse,
} from '@/api/types'
import { isTransientModemError, createThrottledWarner } from '@/utils/modemErrors'

export const SPEED_HISTORY_MAX_POINTS = 30
const SLOW_DATA_REFRESH_INTERVAL = 30_000

/** ModemManager 通常不暴露 QCI；在数据连接开启时从 WWAN 网卡字节速率估算上下行（kbps，与旧 QosInfo 字段一致）。 */
function qosFromWwanInterface(stats: SystemStatsResponse, dataActive: boolean): QosInfo | null {
  if (!dataActive || !stats.network_speed?.interfaces?.length) return null
  const wwan = stats.network_speed.interfaces.find(
    (i) =>
      i.interface.startsWith('wwan') ||
      i.interface.startsWith('wwp') ||
      i.interface.toLowerCase().includes('mbim'),
  )
  if (!wwan) return null
  return {
    qci: 0,
    dl_speed: (wwan.rx_bytes_per_sec * 8) / 1000,
    ul_speed: (wwan.tx_bytes_per_sec * 8) / 1000,
    source: 'interface',
  }
}

export interface InterfaceSpeedHistory {
  rx: number[]
  tx: number[]
  totalRx: number
  totalTx: number
}

export interface ConnectivityResult {
  ipv4: { success: boolean; latency_ms?: number }
  ipv6: { success: boolean; latency_ms?: number }
}

export type ConnectionAddresses = ConnectionAddressesResponse

export interface DashboardData {
  deviceInfo: DeviceInfo | null
  simInfo: SimInfo | null
  systemStats: SystemStatsResponse | null
  networkInfo: NetworkInfo | null
  dataStatus: boolean
  cellsInfo: CellsResponse | null
  qosInfo: QosInfo | null
  airplaneMode: AirplaneModeResponse | null
  connectivity: ConnectivityResult | null
  connectionAddresses: ConnectionAddresses
  speedHistory: Record<string, InterfaceSpeedHistory>
  roaming: RoamingResponse | null
}

export interface DashboardActions {
  toggleData: () => Promise<void>
  toggleAirplaneMode: () => Promise<void>
  toggleRoaming: () => Promise<void>
  loadData: () => Promise<void>
}

const throttledWarn = createThrottledWarner(10_000)

export function useDashboardData(refreshInterval: number, refreshKey: number) {
  const api = useSimAdminApi()
  const [initialLoading, setInitialLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [deviceInfo, setDeviceInfo] = useState<DeviceInfo | null>(null)
  const [simInfo, setSimInfo] = useState<SimInfo | null>(null)
  const [systemStats, setSystemStats] = useState<SystemStatsResponse | null>(null)
  const [networkInfo, setNetworkInfo] = useState<NetworkInfo | null>(null)
  const [dataStatus, setDataStatus] = useState(false)
  const [cellsInfo, setCellsInfo] = useState<CellsResponse | null>(null)
  const [qosInfo, setQosInfo] = useState<QosInfo | null>(null)
  const [airplaneMode, setAirplaneMode] = useState<AirplaneModeResponse | null>(null)
  const [connectivity, setConnectivity] = useState<ConnectivityResult | null>(null)
  const [connectionAddresses, setConnectionAddresses] = useState<ConnectionAddresses>({ ipv4: [], ipv6: [] })
  const [roaming, setRoaming] = useState<RoamingResponse | null>(null)
  const [speedHistory, setSpeedHistory] = useState<Record<string, InterfaceSpeedHistory>>({})
  const speedHistoryRef = useRef<Record<string, InterfaceSpeedHistory>>({})
  const loadingRef = useRef(false)
  const lastSlowRefreshRef = useRef(0)
  const latestStatsRef = useRef<SystemStatsResponse | null>(null)
  const latestDataActiveRef = useRef<boolean>(false)

  const updateSpeedHistory = useCallback((stats: SystemStatsResponse | null) => {
    if (!stats?.network_speed?.interfaces) return

    const nextHistory = { ...speedHistoryRef.current }

    for (const iface of stats.network_speed.interfaces) {
      const existing = nextHistory[iface.interface] || { rx: [], tx: [], totalRx: 0, totalTx: 0 }
      const rx = [...existing.rx, iface.rx_bytes_per_sec]
      const tx = [...existing.tx, iface.tx_bytes_per_sec]

      if (rx.length > SPEED_HISTORY_MAX_POINTS) {
        rx.shift()
        tx.shift()
      }

      nextHistory[iface.interface] = {
        rx,
        tx,
        totalRx: iface.total_rx_bytes,
        totalTx: iface.total_tx_bytes,
      }
    }

    speedHistoryRef.current = nextHistory
    setSpeedHistory(nextHistory)
  }, [])

  const loadData = useCallback(async (background = false) => {
    if (loadingRef.current) return
    loadingRef.current = true

    // 首屏快速解除 loading：最多等待 150ms 或首批数据到达即解除，绝不阻断界面呈现
    const earlyTimer = !background
      ? window.setTimeout(() => setInitialLoading(false), 150)
      : undefined

    const refreshSlowData = !background
      || Date.now() - lastSlowRefreshRef.current >= SLOW_DATA_REFRESH_INTERVAL
    if (refreshSlowData) lastSlowRefreshRef.current = Date.now()
    if (!background) setError(null)
    const failures: string[] = []

    const executeTask = async <T,>(
      promise: Promise<T>,
      label: string,
      onSuccess: (data: T) => void,
    ) => {
      try {
        const res = await promise
        if (res) onSuccess(res)
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err)
        failures.push(`${label}: ${message}`)
      }
    }

    try {
      // 1. 系统指标（极快，通常 10-30ms）
      const statsTask = executeTask(api.getSystemStats(), 'stats', (res) => {
        if (res?.data) {
          latestStatsRef.current = res.data
          setSystemStats(res.data)
          updateSpeedHistory(res.data)
          setQosInfo(qosFromWwanInterface(res.data, latestDataActiveRef.current))
        }
      })

      // 2. 基础控制状态（极快）
      const dataTask = executeTask(api.getDataStatus(), 'data', (res) => {
        if (res?.data) {
          latestDataActiveRef.current = res.data.active
          setDataStatus(res.data.active)
          if (latestStatsRef.current) {
            setQosInfo(qosFromWwanInterface(latestStatsRef.current, res.data.active))
          }
        }
      })

      const airplaneTask = executeTask(api.getAirplaneMode(), 'airplane-mode', (res) => {
        if (res?.data) setAirplaneMode(res.data)
      })

      const roamingTask = executeTask(api.getRoamingStatus(), 'roaming', (res) => {
        if (res?.data) setRoaming(res.data)
      })

      // 3. 基带与 SIM 相关（依赖 ModemManager）
      const deviceTask = refreshSlowData
        ? executeTask(api.getDeviceInfo(), 'device', (res) => {
            if (res?.data) setDeviceInfo(res.data)
          })
        : Promise.resolve()

      const simTask = refreshSlowData
        ? executeTask(api.getSimInfo(), 'sim', (res) => {
            if (res?.data) setSimInfo(res.data)
          })
        : Promise.resolve()

      const networkTask = refreshSlowData
        ? executeTask(api.getNetworkInfo(), 'network', (res) => {
            if (res?.data) setNetworkInfo(res.data)
          })
        : Promise.resolve()

      const addressesTask = refreshSlowData
        ? executeTask(api.getNetworkConnectionAddresses(), 'connection-addresses', (res) => {
            if (res?.data) setConnectionAddresses(res.data)
          })
        : Promise.resolve()

      const cellsTask = executeTask(api.getCellsInfo(), 'cells', (res) => {
        if (res?.data) setCellsInfo(res.data)
      })

      // 4. 外网连通性检测（依赖公网 ping，较慢）
      const connectivityTask = refreshSlowData
        ? executeTask(api.getConnectivity(), 'connectivity', (res) => {
            if (res?.data) setConnectivity(res.data)
          })
        : Promise.resolve()

      // 快速通道：基础状态 (stats, switches) 完成后立即解除 initialLoading
      void Promise.race([
        Promise.allSettled([statsTask, dataTask, airplaneTask, roamingTask]),
        new Promise((resolve) => window.setTimeout(resolve, 150)),
      ]).then(() => {
        setInitialLoading(false)
      })

      // 等待本轮所有任务收敛
      await Promise.allSettled([
        statsTask,
        dataTask,
        airplaneTask,
        roamingTask,
        deviceTask,
        simTask,
        networkTask,
        addressesTask,
        cellsTask,
        connectivityTask,
      ])

      setInitialLoading(false)

      // 错误处理：过滤所有开机/搜网/暂态错误，仅真实故障向用户弹窗
      if (failures.length > 0) {
        const nonTransient = failures.filter((f) => !isTransientModemError(f))
        if (nonTransient.length > 0) {
          setError(nonTransient[0])
        } else {
          throttledWarn('Dashboard', failures.join('; '))
        }
      }
    } catch (err) {
      const nonTransient = !isTransientModemError(err)
      if (nonTransient) {
        setError(err instanceof Error ? err.message : String(err))
      } else {
        throttledWarn('Dashboard', String(err))
      }
      setInitialLoading(false)
    } finally {
      if (earlyTimer !== undefined) window.clearTimeout(earlyTimer)
      loadingRef.current = false
    }
  }, [api, updateSpeedHistory])

  const toggleData = useCallback(async () => {
    try {
      const nextStatus = !dataStatus
      await api.setDataStatus(nextStatus)
      latestDataActiveRef.current = nextStatus
      setDataStatus(nextStatus)
      if (latestStatsRef.current) {
        setQosInfo(qosFromWwanInterface(latestStatsRef.current, nextStatus))
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }, [api, dataStatus])

  const toggleAirplaneMode = useCallback(async () => {
    const snapshot = airplaneMode
    const nextEnabled = !snapshot?.enabled
    if (snapshot) {
      setAirplaneMode({ ...snapshot, enabled: nextEnabled })
    }
    try {
      const response = await api.setAirplaneMode(nextEnabled)
      if (response.data) setAirplaneMode(response.data)
    } catch (err) {
      if (snapshot) setAirplaneMode(snapshot)
      setError(err instanceof Error ? err.message : String(err))
    }
  }, [api, airplaneMode])

  const toggleRoaming = useCallback(async () => {
    try {
      const nextAllowed = !roaming?.roaming_allowed
      const response = await api.setRoamingAllowed(nextAllowed)
      if (response.data) setRoaming(response.data)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }, [api, roaming])

  useEffect(() => {
    // 首次加载：background = false，错误会展示给用户
    const timeout = window.setTimeout(() => {
      void loadData(false)
    }, 0)

    let interval: number | undefined
    if (refreshInterval > 0) {
      // 后台轮询：background = true，仅非暂态错误展示
      interval = window.setInterval(() => void loadData(true), refreshInterval)
    }

    return () => {
      window.clearTimeout(timeout)
      if (interval !== undefined) {
        window.clearInterval(interval)
      }
    }
  }, [refreshInterval, refreshKey, loadData])

  return {
    initialLoading,
    error,
    setError,
    data: {
      deviceInfo,
      simInfo,
      systemStats,
      networkInfo,
      dataStatus,
      cellsInfo,
      qosInfo,
      airplaneMode,
      connectivity,
      connectionAddresses,
      speedHistory,
      roaming,
    } as DashboardData,
    actions: {
      toggleData,
      toggleAirplaneMode,
      toggleRoaming,
      loadData,
    } as DashboardActions,
  }
}
