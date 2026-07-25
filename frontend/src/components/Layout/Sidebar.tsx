import { useMemo, type ElementType } from 'react'
import { useNavigate, useLocation } from 'react-router-dom'
import {
  Box,
  Drawer,
  List,
  ListItem,
  ListItemButton,
  ListItemIcon,
  ListItemText,
  Toolbar,
  Tooltip,
  Typography,
  Divider,
} from '@mui/material'
import {
  Dashboard as DashboardIcon,
  SignalCellularAlt as SignalIcon,
  Settings as SettingsIcon,
  Sms as SmsIcon,
  NotificationsActive as NotificationsIcon,
  GitHub as GitHubIcon,
  SystemUpdateAlt as OtaIcon,
  Router as RouterIcon,
  SimCard as SimIcon,
  AutoMode as AutomationIcon,
  Shield as SecurityIcon,
  SettingsBackupRestore as BackupRestoreIcon,
} from '@mui/icons-material'

const SIDEBAR_TRANSITION = '300ms cubic-bezier(0.4, 0, 0.2, 1)'

interface SidebarProps {
  drawerWidth: number
  miniWidth: number
  mobileOpen: boolean
  desktopOpen: boolean
  onClose: () => void
  isMobile: boolean
}

interface DirectMenuItem {
  type: 'direct'
  path: string
  label: string
  icon: ElementType
}

interface GroupMenuItem {
  type: 'group'
  label: string
  items: Array<{
    path: string
    label: string
    icon: ElementType
  }>
}

type MenuConfigItem = DirectMenuItem | GroupMenuItem

const menuGroups: MenuConfigItem[] = [
  { type: 'direct', path: '/', label: '仪表盘', icon: DashboardIcon },
  { type: 'direct', path: '/sim', label: 'SIM 卡', icon: SimIcon },
  { type: 'direct', path: '/sms', label: '短信管理', icon: SmsIcon },
  // { path: '/phone', label: '电话管理', icon: PhoneIcon },
  {
    type: 'group',
    label: '网络',
    items: [
      { path: '/network', label: '蜂窝网络', icon: SignalIcon },
      { path: '/device-network', label: '设备网络', icon: RouterIcon },
    ],
  },
  {
    type: 'group',
    label: '自动化与通知',
    items: [
      { path: '/automation', label: '自动化中心', icon: AutomationIcon },
      { path: '/notifications', label: '通知中心', icon: NotificationsIcon },
    ],
  },
  {
    type: 'group',
    label: '系统',
    items: [
      { path: '/config/security', label: '安全性', icon: SecurityIcon },
      { path: '/config', label: '基本配置', icon: SettingsIcon },
      { path: '/config/backup', label: '备份与恢复', icon: BackupRestoreIcon },
      { path: '/ota', label: 'OTA 更新', icon: OtaIcon },
    ],
  },
]

export default function Sidebar({
  drawerWidth,
  miniWidth,
  mobileOpen,
  desktopOpen,
  onClose,
  isMobile,
}: SidebarProps) {
  const navigate = useNavigate()
  const location = useLocation()

  const directItems = useMemo(() => {
    return menuGroups.filter((item): item is DirectMenuItem => item.type === 'direct')
  }, [])

  const groupItems = useMemo(() => {
    return menuGroups.filter((item): item is GroupMenuItem => item.type === 'group')
  }, [])

  const groupSubItems = useMemo(() => {
    const list: Array<{ path: string; label: string; icon: ElementType; parentLabel: string }> = []
    for (const group of menuGroups) {
      if (group.type === 'group') {
        for (const subItem of group.items) {
          list.push({
            path: subItem.path,
            label: subItem.label,
            icon: subItem.icon,
            parentLabel: group.label,
          })
        }
      }
    }
    return list
  }, [])

  const handleNavigation = (path: string): void => {
    void navigate(path)
    if (isMobile) onClose()
  }

  const renderDrawer = (compact = false) => (
    <Box sx={{ display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}>
      <Toolbar
        sx={{
          minHeight: 64,
          px: 0,
        }}
      >
        <Box
          sx={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: compact ? 'center' : 'flex-start',
            gap: 1.25,
            width: '100%',
            minWidth: 0,
          }}
        >
          <Box
            component="img"
            src="/simadmin-logo.svg"
            alt="SimAdmin"
            sx={{
              width: 38,
              height: 38,
              mx: compact ? 0 : '13px',
              flexShrink: 0,
              display: 'block',
              transition: `margin ${SIDEBAR_TRANSITION}, width ${SIDEBAR_TRANSITION}, height ${SIDEBAR_TRANSITION}`,
            }}
          />
          <Typography
            variant="h6"
            noWrap
            component="div"
            fontWeight={700}
            sx={{
              opacity: compact ? 0 : 1,
              maxWidth: compact ? 0 : 132,
              transform: compact ? 'translateX(-6px)' : 'translateX(0)',
              overflow: 'hidden',
              transition: `opacity 180ms ease, max-width ${SIDEBAR_TRANSITION}, transform ${SIDEBAR_TRANSITION}`,
            }}
          >
            SimAdmin
          </Typography>
        </Box>
      </Toolbar>
      <List sx={{ flexGrow: 1, py: 1.5, px: compact ? 0.75 : 1, overflowY: 'auto', overflowX: 'hidden' }}>
        {compact ? (
          <>
            {/* Top Direct Items (Compact) */}
            {directItems.map((item) => {
              const selected = location.pathname === item.path
              const IconComponent = item.icon
              return (
                <ListItem key={item.path} disablePadding>
                  <Tooltip title={item.label} placement="right">
                    <ListItemButton
                      selected={selected}
                      onClick={() => handleNavigation(item.path)}
                      sx={{
                        minHeight: 44,
                        borderRadius: 1.5,
                        justifyContent: 'center',
                        px: 0,
                        mb: 0.5,
                        color: selected ? 'primary.main' : 'text.secondary',
                        '&.Mui-selected': {
                          bgcolor: (theme) => theme.palette.mode === 'light' ? 'rgba(255,255,255,0.68)' : 'rgba(30,41,59,0.72)',
                          boxShadow: '0 8px 22px -18px rgba(18,150,219,0.6)',
                          borderRight: '2px solid',
                          borderColor: 'primary.main',
                        },
                        '&.Mui-selected:hover, &:hover': {
                          bgcolor: (theme) => theme.palette.mode === 'light' ? 'rgba(255,255,255,0.58)' : 'rgba(30,41,59,0.64)',
                        },
                      }}
                    >
                      <ListItemIcon
                        sx={{
                          minWidth: 0,
                          color: 'inherit',
                          justifyContent: 'center',
                        }}
                      >
                        <IconComponent sx={{ fontSize: 20 }} />
                      </ListItemIcon>
                    </ListItemButton>
                  </Tooltip>
                </ListItem>
              )
            })}

            {/* Non-full-width Divider (Compact) */}
            <ListItem disablePadding sx={{ my: 1, px: 1 }}>
              <Divider
                variant="middle"
                sx={{
                  width: '100%',
                  borderColor: (theme) => theme.palette.mode === 'light' ? 'rgba(0,0,0,0.06)' : 'rgba(255,255,255,0.06)',
                }}
              />
            </ListItem>

            {/* Bottom Grouped Items (Compact) */}
            {groupSubItems.map((item) => {
              const selected = location.pathname === item.path
              const IconComponent = item.icon
              return (
                <ListItem key={item.path} disablePadding>
                  <Tooltip title={`${item.parentLabel} / ${item.label}`} placement="right">
                    <ListItemButton
                      selected={selected}
                      onClick={() => handleNavigation(item.path)}
                      sx={{
                        minHeight: 44,
                        borderRadius: 1.5,
                        justifyContent: 'center',
                        px: 0,
                        mb: 0.5,
                        color: selected ? 'primary.main' : 'text.secondary',
                        '&.Mui-selected': {
                          bgcolor: (theme) => theme.palette.mode === 'light' ? 'rgba(255,255,255,0.68)' : 'rgba(30,41,59,0.72)',
                          boxShadow: '0 8px 22px -18px rgba(18,150,219,0.6)',
                          borderRight: '2px solid',
                          borderColor: 'primary.main',
                        },
                        '&.Mui-selected:hover, &:hover': {
                          bgcolor: (theme) => theme.palette.mode === 'light' ? 'rgba(255,255,255,0.58)' : 'rgba(30,41,59,0.64)',
                        },
                      }}
                    >
                      <ListItemIcon
                        sx={{
                          minWidth: 0,
                          color: 'inherit',
                          justifyContent: 'center',
                        }}
                      >
                        <IconComponent sx={{ fontSize: 20 }} />
                      </ListItemIcon>
                    </ListItemButton>
                  </Tooltip>
                </ListItem>
              )
            })}
          </>
        ) : (
          <>
            {/* Top Direct Items (Expanded) */}
            {directItems.map((group) => {
              const selected = location.pathname === group.path
              const IconComponent = group.icon
              return (
                <ListItem key={group.path} disablePadding>
                  <ListItemButton
                    selected={selected}
                    onClick={() => handleNavigation(group.path)}
                    sx={{
                      minHeight: 44,
                      borderRadius: 1.5,
                      justifyContent: 'flex-start',
                      px: 0,
                      mb: 0.5,
                      color: selected ? 'primary.main' : 'text.secondary',
                      '&.Mui-selected': {
                        bgcolor: (theme) => theme.palette.mode === 'light' ? 'rgba(255,255,255,0.68)' : 'rgba(30,41,59,0.72)',
                        boxShadow: '0 8px 22px -18px rgba(18,150,219,0.6)',
                        borderRight: '2px solid',
                        borderColor: 'primary.main',
                      },
                      '&.Mui-selected:hover, &:hover': {
                        bgcolor: (theme) => theme.palette.mode === 'light' ? 'rgba(255,255,255,0.58)' : 'rgba(30,41,59,0.64)',
                      },
                    }}
                  >
                    <ListItemIcon
                      sx={{
                        minWidth: 38,
                        width: 48,
                        color: 'inherit',
                        justifyContent: 'center',
                        flexShrink: 0,
                      }}
                    >
                      <IconComponent sx={{ fontSize: 20 }} />
                    </ListItemIcon>
                    <ListItemText
                      primary={group.label}
                      primaryTypographyProps={{ noWrap: true, fontSize: 14, fontWeight: selected ? 700 : 500 }}
                    />
                  </ListItemButton>
                </ListItem>
              )
            })}

            {/* Non-full-width Divider (Expanded) */}
            <ListItem disablePadding sx={{ my: 1 }}>
              <Divider
                variant="middle"
                sx={{
                  width: '100%',
                  borderColor: (theme) => theme.palette.mode === 'light' ? 'rgba(0,0,0,0.06)' : 'rgba(255,255,255,0.06)',
                }}
              />
            </ListItem>

            {/* Bottom Grouped Items (Expanded) */}
            {groupItems.map((group, index) => (
              <Box key={group.label} sx={{ mt: index === 0 ? 3 : 2.5, mb: 0.5 }}>
                <Typography
                  variant="caption"
                  color="text.disabled"
                  sx={{
                    px: 2,
                    display: 'block',
                    fontWeight: 700,
                    textTransform: 'uppercase',
                    letterSpacing: '0.08em',
                    fontSize: '0.7rem',
                    mb: 0.75,
                  }}
                >
                  {group.label}
                </Typography>
                <List disablePadding sx={{ pl: 0.5 }}>
                  {group.items.map((subItem) => {
                    const selected = location.pathname === subItem.path
                    const SubIconComponent = subItem.icon
                    return (
                      <ListItem key={subItem.path} disablePadding>
                        <ListItemButton
                          selected={selected}
                          onClick={() => handleNavigation(subItem.path)}
                          sx={{
                            minHeight: 44,
                            borderRadius: 1.5,
                            justifyContent: 'flex-start',
                            px: 0,
                            mb: 0.5,
                            color: selected ? 'primary.main' : 'text.secondary',
                            '&.Mui-selected': {
                              bgcolor: (theme) => theme.palette.mode === 'light' ? 'rgba(255,255,255,0.68)' : 'rgba(30,41,59,0.72)',
                              boxShadow: '0 8px 22px -18px rgba(18,150,219,0.6)',
                              borderRight: '2px solid',
                              borderColor: 'primary.main',
                            },
                            '&.Mui-selected:hover, &:hover': {
                              bgcolor: (theme) => theme.palette.mode === 'light' ? 'rgba(255,255,255,0.58)' : 'rgba(30,41,59,0.64)',
                            },
                          }}
                        >
                          <ListItemIcon
                            sx={{
                              minWidth: 38,
                              width: 48,
                              color: 'inherit',
                              justifyContent: 'center',
                              flexShrink: 0,
                            }}
                          >
                            <SubIconComponent sx={{ fontSize: 20 }} />
                          </ListItemIcon>
                          <ListItemText
                            primary={subItem.label}
                            primaryTypographyProps={{ noWrap: true, fontSize: 14, fontWeight: selected ? 700 : 500 }}
                          />
                        </ListItemButton>
                      </ListItem>
                    )
                  })}
                </List>
              </Box>
            ))}
          </>
        )}
      </List>

      <Box
        sx={{
          p: compact ? 1 : 2,
          borderTop: 1,
          borderColor: 'divider',
          display: 'flex',
          flexDirection: 'column',
          alignItems: compact ? 'center' : 'flex-start',
        }}
      >
        <Box
          component="a"
          href="https://github.com/3899/SimAdmin"
          target="_blank"
          rel="noopener noreferrer"
          sx={{
            display: 'flex',
            alignItems: 'center',
            gap: compact ? 0 : 0.5,
            color: 'text.secondary',
            fontSize: '0.75rem',
            textDecoration: 'none',
            width: 'fit-content',
            '&:hover': { color: 'primary.main' },
          }}
        >
          <GitHubIcon sx={{ fontSize: compact ? 22 : 16 }} />
          <Typography
            variant="caption"
            color="inherit"
            sx={{
              opacity: compact ? 0 : 1,
              maxWidth: compact ? 0 : 110,
              overflow: 'hidden',
              whiteSpace: 'nowrap',
              transition: `opacity ${SIDEBAR_TRANSITION}, max-width ${SIDEBAR_TRANSITION}`,
            }}
          >
            3899/SimAdmin
          </Typography>
        </Box>
        <Box
          sx={{
            opacity: compact ? 0 : 1,
            maxHeight: compact ? 0 : 48,
            overflow: 'hidden',
            transition: `opacity ${SIDEBAR_TRANSITION}, max-height ${SIDEBAR_TRANSITION}`,
          }}
        >
          <Typography variant="caption" color="text.disabled" sx={{ display: 'block', mt: 0.5 }}>
            v{__APP_VERSION__} ({__GIT_BRANCH__}/{__GIT_COMMIT__})
          </Typography>
          <Typography variant="caption" color="text.disabled" sx={{ display: 'block', mt: 0.5 }}>
            Copyright © 2026 @3899
          </Typography>
        </Box>
      </Box>
    </Box>
  )

  const paperSx = {
    boxSizing: 'border-box',
    borderRadius: 0,
    borderRight: '1px solid',
    borderColor: 'divider',
    bgcolor: (theme: import('@mui/material').Theme) => theme.palette.mode === 'light' ? 'rgba(255,255,255,0.42)' : 'rgba(15,23,42,0.54)',
    boxShadow: '4px 0 24px -16px rgba(15,23,42,0.28)',
    backdropFilter: 'blur(28px)',
    WebkitBackdropFilter: 'blur(28px)',
  } as const

  return (
    <Box
      component="nav"
      sx={{
        width: { xs: 0, sm: desktopOpen ? drawerWidth : miniWidth },
        flexShrink: { sm: 0 },
        transition: `width ${SIDEBAR_TRANSITION}`,
        willChange: 'width',
      }}
    >
      <Drawer
        variant="temporary"
        open={mobileOpen}
        onClose={onClose}
        ModalProps={{ keepMounted: true }}
        sx={{
          display: { xs: 'block', sm: 'none' },
          '& .MuiDrawer-paper': { ...paperSx, width: drawerWidth },
        }}
      >
        {renderDrawer(false)}
      </Drawer>

      <Drawer
        variant="persistent"
        open
        sx={{
          display: { xs: 'none', sm: 'block' },
          '& .MuiDrawer-paper': {
            ...paperSx,
            width: desktopOpen ? drawerWidth : miniWidth,
            overflowX: 'hidden',
            transition: `width ${SIDEBAR_TRANSITION}`,
            willChange: 'width',
          },
        }}
      >
        {renderDrawer(!desktopOpen)}
      </Drawer>
    </Box>
  )
}
