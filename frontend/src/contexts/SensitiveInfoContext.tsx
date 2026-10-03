/* eslint-disable react-refresh/only-export-components */
import { createContext, useContext, useEffect, useMemo, useState, useCallback, type ReactNode } from 'react'
import { Box, GlobalStyles, type SxProps, type Theme } from '@mui/material'

export interface SensitiveInfoState {
  showSensitive: boolean
  toggleSensitive: () => void
  setShowSensitive: (show: boolean | ((prev: boolean) => boolean)) => void
}

const SensitiveInfoContext = createContext<SensitiveInfoState>({
  showSensitive: false,
  toggleSensitive: () => undefined,
  setShowSensitive: () => undefined,
})

export function SensitiveInfoProvider({ children }: { children: ReactNode }) {
  const [showSensitive, setShowSensitive] = useState(false)

  useEffect(() => {
    document.body.dataset.sensitiveVisible = String(showSensitive)
    return () => {
      delete document.body.dataset.sensitiveVisible
    }
  }, [showSensitive])

  const toggleSensitive = useCallback(() => {
    setShowSensitive((prev) => !prev)
  }, [])

  const value = useMemo(
    () => ({
      showSensitive,
      toggleSensitive,
      setShowSensitive,
    }),
    [showSensitive, toggleSensitive]
  )

  return (
    <SensitiveInfoContext.Provider value={value}>
      <GlobalStyles
        styles={{
          '[data-sensitive="true"]': {
            transition: 'filter 0.3s ease',
          },
          'body:not([data-sensitive-visible="true"]) [data-sensitive="true"]': {
            filter: 'blur(5px) !important',
            userSelect: 'none !important',
          },
          'body[data-sensitive-visible="true"] [data-sensitive="true"]': {
            filter: 'none !important',
            userSelect: 'auto !important',
          },
        }}
      />
      {children}
    </SensitiveInfoContext.Provider>
  )
}

export function useSensitiveInfo() {
  return useContext(SensitiveInfoContext)
}

export function useLocalSensitiveToggle() {
  const { showSensitive } = useSensitiveInfo()
  const [prevGlobal, setPrevGlobal] = useState(showSensitive)
  const [localShow, setLocalShow] = useState<boolean | null>(null)

  if (prevGlobal !== showSensitive) {
    setPrevGlobal(showSensitive)
    setLocalShow(null)
  }

  const show = localShow ?? showSensitive
  const toggle = useCallback(() => {
    setLocalShow((prev) => !(prev ?? showSensitive))
  }, [showSensitive])

  return [show, toggle] as const
}

export function SensitiveValue({ children, sx }: { children: ReactNode; sx?: SxProps<Theme> }) {
  return (
    <Box component="span" data-sensitive="true" sx={{ display: 'inline-block', ...sx }}>
      {children}
    </Box>
  )
}
