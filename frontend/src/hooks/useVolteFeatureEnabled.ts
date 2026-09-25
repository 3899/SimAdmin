import { useEffect, useState } from 'react'
import { useSimAdminApi } from '../contexts/ApiContext'

export const VOLTE_MODULE_STORAGE_KEY = 'simadmin_volte_module_enabled'

/** @deprecated 已废弃：状态现由后端配置统一驱动，保留仅用于向下兼容 */
export function getStoredVolteModuleEnabled(): boolean | null {
  return null
}

/** @deprecated 已废弃：状态现由后端配置统一驱动，保留仅用于向下兼容 */
export function setStoredVolteModuleEnabled(_enabled: boolean) {
  // no-op
}

export function useVolteFeatureEnabled() {
  const api = useSimAdminApi()
  const [volteEnabled, setVolteEnabled] = useState(false)
  const [configLoading, setConfigLoading] = useState(true)

  useEffect(() => {
    let active = true
    api.getVolteControl()
      .then((response) => {
        if (active && response.data) {
          setVolteEnabled(Boolean(response.data.config?.feature_enabled))
        }
      })
      .catch((error) => {
        console.error('Failed to load VoLTE control config:', error)
      })
      .finally(() => {
        if (active) setConfigLoading(false)
      })
    return () => {
      active = false
    }
  }, [api])

  useEffect(() => {
    const handleToggle = (event: Event) => {
      const detail = (event as CustomEvent).detail
      if (detail && typeof detail.feature_enabled === 'boolean') {
        setVolteEnabled(detail.feature_enabled)
      } else if (detail && typeof detail.module_enabled === 'boolean') {
        setVolteEnabled(detail.module_enabled)
      }
    }
    window.addEventListener('volte-feature-toggled', handleToggle)
    return () => window.removeEventListener('volte-feature-toggled', handleToggle)
  }, [])

  return { volteEnabled, configLoading }
}
