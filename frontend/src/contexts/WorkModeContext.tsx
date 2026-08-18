/* eslint-disable react-refresh/only-export-components */
import { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react'
import type { ReactNode } from 'react'
import { useSimAdminApi } from './ApiContext'
import type { WorkMode } from '../api/types'

interface WorkModeContextValue {
  mode: WorkMode
  workerRunning: boolean
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
  const [loading, setLoading] = useState(true)

  const refreshWorkMode = useCallback(async () => {
    try {
      const response = await api.getWorkMode()
      setMode(response.data?.mode ?? 'sim')
      setWorkerRunning(response.data?.worker_running ?? false)
    } catch {
      setMode('sim')
      setWorkerRunning(false)
    } finally {
      setLoading(false)
    }
  }, [api])

  useEffect(() => {
    void refreshWorkMode()
  }, [refreshWorkMode])

  const value = useMemo(
    () => ({ mode, workerRunning, loading, refreshWorkMode }),
    [mode, workerRunning, loading, refreshWorkMode],
  )

  return (
    <WorkModeContext.Provider value={value}>
      {children}
    </WorkModeContext.Provider>
  )
}
