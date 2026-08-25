import { useState } from 'react'
import { Outlet } from 'react-router-dom'
import { Box, useMediaQuery, useTheme, type Theme } from '@mui/material'
import Sidebar from './Sidebar'
import TopBar from './TopBar'
import { RefreshContext } from '../../contexts/RefreshContext'
import { LAYOUT_BOTTOM_ACTION_BAR_HEIGHT, LAYOUT_BOTTOM_ACTION_BAR_ID } from './layoutConstants'

const DRAWER_WIDTH = 224
const DRAWER_MINI_WIDTH = 64
const LAYOUT_TRANSITION = '300ms cubic-bezier(0.4, 0, 0.2, 1)'

interface MainLayoutProps {
  showLogout?: boolean
}

export default function MainLayout({ showLogout = false }: MainLayoutProps) {
  const theme = useTheme<Theme>()
  const isMobile = useMediaQuery(theme.breakpoints.down('sm'))
  const [mobileOpen, setMobileOpen] = useState(false)
  const [desktopOpen, setDesktopOpen] = useState(true) // 桌面端侧边栏状态，默认展开
  const [refreshInterval, setRefreshInterval] = useState(3000) // 默认 3 秒（移动端友好）
  const [refreshKey, setRefreshKey] = useState(0)

  const handleDrawerToggle = () => {
    if (isMobile) {
      setMobileOpen(!mobileOpen)
    } else {
      setDesktopOpen(!desktopOpen)
    }
  }

  const triggerRefresh = () => {
    setRefreshKey((prev) => prev + 1)
  }

  return (
    <RefreshContext.Provider
      value={{ refreshInterval, setRefreshInterval, refreshKey, triggerRefresh }}
    >
      <Box
        sx={{
          display: 'flex',
          height: '100vh',
          position: 'relative',
          overflow: 'hidden',
          bgcolor: 'background.default',
          '&::before': {
            content: '""',
            position: 'fixed',
            inset: 0,
            pointerEvents: 'none',
            zIndex: 0,
            background: (currentTheme) => currentTheme.palette.mode === 'light'
              ? 'radial-gradient(circle at 6% 2%, rgba(147,197,253,0.48), transparent 32%), radial-gradient(circle at 96% 24%, rgba(196,181,253,0.36), transparent 34%), radial-gradient(circle at 42% 106%, rgba(110,231,183,0.28), transparent 34%)'
              : 'radial-gradient(circle at 8% 0%, rgba(18,150,219,0.26), transparent 34%), radial-gradient(circle at 98% 24%, rgba(124,58,237,0.24), transparent 34%), radial-gradient(circle at 42% 110%, rgba(16,185,129,0.16), transparent 36%)',
          },
        }}
      >
        {/* 侧边栏 */}
        <Sidebar
          drawerWidth={DRAWER_WIDTH}
          miniWidth={DRAWER_MINI_WIDTH}
          mobileOpen={mobileOpen}
          desktopOpen={desktopOpen}
          onClose={handleDrawerToggle}
          isMobile={isMobile}
        />

        <Box
          sx={{
            display: 'flex',
            flexDirection: 'column',
            flexGrow: 1,
            minWidth: 0,
            height: '100vh',
            position: 'relative',
            zIndex: 1,
            transition: `width ${LAYOUT_TRANSITION}`,
            willChange: 'width',
          }}
        >
          {/* 顶部导航栏 */}
          <TopBar
            drawerWidth={desktopOpen ? DRAWER_WIDTH : DRAWER_MINI_WIDTH}
            onMenuClick={handleDrawerToggle}
            refreshInterval={refreshInterval}
            onRefreshIntervalChange={setRefreshInterval}
            showLogout={showLogout}
          />

          <Box
            sx={{
              borderBottom: '1px solid',
              borderColor: (currentTheme) => currentTheme.palette.mode === 'light'
                ? 'rgba(226,232,240,0.6)'
                : 'rgba(148,163,184,0.16)',
              flexShrink: 0,
            }}
          />

          {/* 主内容区 */}
          <Box
            component="main"
            sx={{
              flexGrow: 1,
              minHeight: 0,
              overflow: 'auto',
              p: { xs: 2, sm: 3 },
            }}
          >
            <Outlet />
          </Box>

          <Box
            id={LAYOUT_BOTTOM_ACTION_BAR_ID}
            sx={(currentTheme) => ({
              flexShrink: 0,
              '&:empty': {
                display: 'none',
              },
              '&:not(:empty)': {
                alignItems: 'center',
                bgcolor: 'transparent',
                borderTop: '1px solid',
                borderColor: currentTheme.palette.mode === 'light'
                  ? 'rgba(226,232,240,0.72)'
                  : 'rgba(148,163,184,0.18)',
                display: 'flex',
                height: LAYOUT_BOTTOM_ACTION_BAR_HEIGHT,
                minHeight: LAYOUT_BOTTOM_ACTION_BAR_HEIGHT,
                px: { xs: 2, sm: 3 },
              },
            })}
          />
        </Box>
      </Box>
    </RefreshContext.Provider>
  )
}
