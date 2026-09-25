import { useMemo, useState } from 'react'
import {
  Box,
  Card,
  CardContent,
  Tab,
  Tabs,
  Typography,
  useTheme,
  type Theme,
} from '@mui/material'
import { alpha } from '@/utils/theme'
import { Speed, ArrowDownward, ArrowUpward } from '@mui/icons-material'
import { LineChart } from '@mui/x-charts/LineChart'
import { formatBytes, formatSpeed } from '../utils'
import { type InterfaceSpeedHistory } from '../hooks/useDashboardData'
import type { SystemStatsResponse } from '@/api/types'

interface NetworkSpeedProps {
  systemStats: SystemStatsResponse | null
  speedHistory: Record<string, InterfaceSpeedHistory>
}

function interfacePriority(name: string): number {
  if (name.startsWith('wwan')) return 0
  if (name.startsWith('usb')) return 1
  if (name.startsWith('wlan')) return 2
  if (name.startsWith('eth') || name.startsWith('en')) return 3
  if (name.startsWith('docker') || name.startsWith('veth') || name.startsWith('br-')) return 10
  return 5
}

function preferredInterfaces(systemStats: SystemStatsResponse | null) {
  const interfaces = systemStats?.network_speed?.interfaces ?? []
  return [...interfaces].sort((a, b) => {
    const prioA = interfacePriority(a.interface)
    const prioB = interfacePriority(b.interface)
    if (prioA !== prioB) return prioA - prioB
    return a.interface.localeCompare(b.interface)
  })
}

export function NetworkSpeed({ systemStats, speedHistory }: NetworkSpeedProps) {
  const theme = useTheme<Theme>()
  const interfaces = useMemo(() => preferredInterfaces(systemStats), [systemStats])
  const [selectedInterface, setSelectedInterface] = useState('')

  const effectiveSelectedInterface = interfaces.some((iface) => iface.interface === selectedInterface)
    ? selectedInterface
    : interfaces[0]?.interface
  const selected = interfaces.find((iface) => iface.interface === effectiveSelectedInterface) ?? interfaces[0]
  const history = selected ? speedHistory[selected.interface] : undefined
  const rxData = history?.rx ?? []
  const txData = history?.tx ?? []
  const chartLength = Math.max(rxData.length, txData.length)
  const xAxis = Array.from({ length: chartLength }, (_, index) => index + 1)
  const hasChartData = chartLength > 1
  const rxColor = alpha(theme.palette.success.main, 0.72)
  const txColor = alpha(theme.palette.primary.main, 0.72)
  const rxTextColor = alpha(theme.palette.success.main, 0.82)
  const txTextColor = alpha(theme.palette.primary.main, 0.82)

  return (
    <Card sx={{ height: '100%', overflow: 'hidden' }}>
      <CardContent>
        <Box
          sx={{
            display: 'flex',
            alignItems: { xs: 'flex-start', sm: 'center' },
            justifyContent: 'space-between',
            gap: 1.5,
            mb: 2,
            pb: 1.5,
            borderBottom: '1px solid',
            borderColor: 'divider',
            flexDirection: { xs: 'column', sm: 'row' },
          }}
        >
          <Box display="flex" alignItems="center" flexWrap="wrap" gap={1}>
            <Speed color="primary" />
            <Typography variant="subtitle1" fontWeight={700}>实时网速</Typography>
            {selected && (
              <Box display="flex" alignItems="center" flexWrap="wrap" gap={1}>
                <Box display="flex" alignItems="center" gap={0.35}>
                  <ArrowDownward fontSize="small" sx={{ color: rxTextColor }} />
                  <Typography variant="body2" sx={{ color: rxTextColor }}>
                    {formatSpeed(selected.rx_bytes_per_sec)}
                  </Typography>
                </Box>
                <Box display="flex" alignItems="center" gap={0.35}>
                  <ArrowUpward fontSize="small" sx={{ color: txTextColor }} />
                  <Typography variant="body2" sx={{ color: txTextColor }}>
                    {formatSpeed(selected.tx_bytes_per_sec)}
                  </Typography>
                </Box>
              </Box>
            )}
          </Box>

          {interfaces.length > 0 && (
            <Box
              sx={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'flex-end',
                gap: 1,
                maxWidth: '100%',
                flexWrap: { xs: 'wrap', md: 'nowrap' },
              }}
            >
              {selected && (
                <Typography
                  variant="caption"
                  color="text.secondary"
                  sx={{ flexShrink: 0, whiteSpace: 'nowrap' }}
                >
                  总流量 {formatBytes(selected.total_rx_bytes)} / {formatBytes(selected.total_tx_bytes)}
                </Typography>
              )}
              <Tabs
                value={effectiveSelectedInterface ?? false}
                onChange={(_, value: string) => setSelectedInterface(value)}
                variant="scrollable"
                scrollButtons="auto"
                sx={{
                  minHeight: 32,
                  maxWidth: '100%',
                  bgcolor: (currentTheme) => currentTheme.palette.mode === 'light' ? 'rgba(241,245,249,0.72)' : 'rgba(15,23,42,0.36)',
                  borderRadius: 1.5,
                  p: 0.35,
                  '& .MuiTabs-indicator': { display: 'none' },
                  '& .MuiTab-root': {
                    minHeight: 28,
                    px: 1.35,
                    py: 0.45,
                    borderRadius: 1.15,
                    fontSize: '0.75rem',
                    textTransform: 'none',
                  },
                  '& .Mui-selected': {
                    bgcolor: 'background.paper',
                    boxShadow: '0 8px 18px -14px rgba(15,23,42,0.5)',
                  },
                }}
              >
                {interfaces.map((iface) => (
                  <Tab key={iface.interface} value={iface.interface} label={iface.interface} />
                ))}
              </Tabs>
            </Box>
          )}
        </Box>

        {selected ? (
          <>
            <Box sx={{ height: 230, width: '100%' }}>
              {hasChartData ? (
                <LineChart
                  xAxis={[{ data: xAxis, disableLine: true, disableTicks: true }]}
                  yAxis={[{
                    min: 0,
                    valueFormatter: (value: number) => formatSpeed(value),
                    disableLine: true,
                    disableTicks: true,
                  }]}
                  series={[
                    {
                      data: rxData,
                      label: '下载',
                      color: rxColor,
                      area: true,
                      showMark: false,
                      curve: 'natural',
                    },
                    {
                      data: txData,
                      label: '上传',
                      color: txColor,
                      area: true,
                      showMark: false,
                      curve: 'natural',
                    },
                  ]}
                  height={230}
                  hideLegend
                  margin={{ top: 0, right: 16, bottom: 12, left: 52 }}
                  grid={{ horizontal: true }}
                />
              ) : (
                <Box display="flex" alignItems="center" justifyContent="center" height="100%">
                  <Typography variant="body2" color="text.secondary">等待趋势数据</Typography>
                </Box>
              )}
            </Box>
          </>
        ) : (
          <Typography variant="body2" color="text.secondary">暂无数据</Typography>
        )}
      </CardContent>
    </Card>
  )
}
