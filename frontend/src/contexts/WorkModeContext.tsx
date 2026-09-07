/* eslint-disable react-refresh/only-export-components */
import { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react'
import type { ReactNode } from 'react'
import { useSimAdminApi } from './ApiContext'
import type { WorkMode } from '../api/types'

interface WorkModeContextValue {
  mode: WorkMode
  workerRunning: boolean
  esimSupported: boolean
  loading: boolean
  refreshWorkMode: () => Promise<void>
}

const WorkModeContext = createContext<WorkModeContextValue | undefined>(undefined)

export function useWorkMode() {
  const context = useContext(WorkModeContext)
  if (!context) {
    throw new Error('useWorkMode must be used within WorkModeProvider')
  }
  return context
}

export function WorkModeProvider({ children }: { children: ReactNode }) {
  const api = useSimAdminApi()
  const [mode, setMode] = useState<WorkMode>('sim')
  const [workerRunning, setWorkerRunning] = useState(false)
  // Start hidden until the device capability is known, avoiding an ARMv7
  // eSIM/work-mode flash while the first request is in flight.
  const [esimSupported, setEsimSupported] = useState(false)
  const [loading, setLoading] = useState(true)

  const refreshWorkMode = useCallback(async () => {
    try {
      const response = await api.getWorkMode()
      setMode(response.data?.mode ?? 'sim')
      setWorkerRunning(response.data?.worker_running ?? false)
      // Treat the field as opt-in for compatibility with older backends.
      setEsimSupported(response.data?.esim_supported !== false)
    } catch {
      setMode('sim')
      setWorkerRunning(false)
      // Capability is opt-in. If the device endpoint is unavailable, keep
      // the eSIM/work-mode entry hidden instead of exposing a dead module.
      setEsimSupported(false)
    } finally {
      setLoading(false)
    }
  }, [api])

  useEffect(() => {
    void refreshWorkMode()
  }, [refreshWorkMode])

  const value = useMemo(
    () => ({ mode, workerRunning, esimSupported, loading, refreshWorkMode }),
    [mode, workerRunning, esimSupported, loading, refreshWorkMode],
  )

  return (
    <WorkModeContext.Provider value={value}>
      {children}
    </WorkModeContext.Provider>
  )
}
