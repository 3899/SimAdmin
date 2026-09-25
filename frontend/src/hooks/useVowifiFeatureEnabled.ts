import { useEffect, useState } from 'react'
import { useSimAdminApi } from '../contexts/ApiContext'

export function useVowifiFeatureEnabled() {
  const api = useSimAdminApi()
  const [vowifiEnabled, setVowifiEnabled] = useState(false)
  const [configLoading, setConfigLoading] = useState(true)

  useEffect(() => {
    let active = true
    api.getVowifiControl()
      .then((response) => {
        if (active && response.data) setVowifiEnabled(response.data.feature_enabled)
      })
      .catch((error) => {
        console.error('Failed to load VoWiFi control config:', error)
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
        setVowifiEnabled(detail.feature_enabled)
      }
    }
    window.addEventListener('vowifi-feature-toggled', handleToggle)
    return () => window.removeEventListener('vowifi-feature-toggled', handleToggle)
  }, [])

  return { vowifiEnabled, configLoading }
}
