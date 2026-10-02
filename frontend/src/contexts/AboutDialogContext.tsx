/* eslint-disable react-refresh/only-export-components */
import { createContext, useContext, useState, useCallback, type ReactNode } from 'react'

interface AboutDialogContextValue {
  isOpen: boolean
  activeTab: number
  openAbout: (tab?: number) => void
  closeAbout: () => void
  setActiveTab: (tab: number) => void
}

const AboutDialogContext = createContext<AboutDialogContextValue | null>(null)

export function AboutDialogProvider({ children }: { children: ReactNode }) {
  const [isOpen, setIsOpen] = useState(false)
  const [activeTab, setActiveTab] = useState(0)

  const openAbout = useCallback((tab = 0) => {
    setActiveTab(tab)
    setIsOpen(true)
  }, [])

  const closeAbout = useCallback(() => {
    setIsOpen(false)
  }, [])

  return (
    <AboutDialogContext.Provider
      value={{
        isOpen,
        activeTab,
        openAbout,
        closeAbout,
        setActiveTab,
      }}
    >
      {children}
    </AboutDialogContext.Provider>
  )
}

export function useAboutDialog() {
  const context = useContext(AboutDialogContext)
  if (!context) {
    return {
      isOpen: false,
      activeTab: 0,
      openAbout: () => {},
      closeAbout: () => {},
      setActiveTab: () => {},
    }
  }
  return context
}
