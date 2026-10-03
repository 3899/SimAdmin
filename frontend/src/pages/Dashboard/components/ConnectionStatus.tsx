import { Box, Card, CardContent, Typography, Stack } from '@mui/material'
import type { QosInfo } from '@/api/types'
import type { ConnectivityResult, ConnectionAddresses } from '../hooks/useDashboardData'

interface ConnectionStatusProps {
  qosInfo: QosInfo | null
  connectivity: ConnectivityResult | null
  connectionAddresses: ConnectionAddresses
}

export function ConnectionStatus({ qosInfo, connectivity, connectionAddresses }: ConnectionStatusProps) {
  return (
    <Card sx={{ height: '100%' }}>
      <CardContent>
        <Typography variant="subtitle2" color="text.secondary" gutterBottom>
          连接状态
        </Typography>

        <Stack spacing={1}>
          <Box display="flex" justifyContent="space-between" alignItems="center">
            <Typography variant="caption" color="text.secondary">QoS</Typography>
            <Typography
              variant="body2"
              fontWeight="medium"
              title="3GPP QoS 等级通常由核心网下发；本机无法直接从 ModemManager 读取时，使用 WWAN 网卡流量统计估算速率。"
            >
              {!qosInfo
                ? '当前未接入'
                : qosInfo.source === 'interface'
                  ? '已连接'
                  : qosInfo.qci > 0
                    ? `QCI ${qosInfo.qci}`
                    : '已连接'}
            </Typography>
          </Box>

          <Box display="flex" justifyContent="space-between" alignItems="center">
            <Typography variant="caption" color="text.secondary">下行</Typography>
            <Typography variant="body2" fontWeight="medium">
              {qosInfo?.dl_speed ? `${(qosInfo.dl_speed / 1000).toFixed(0)} Mbps` : '-'}
            </Typography>
          </Box>

          <Box display="flex" justifyContent="space-between" alignItems="center">
            <Typography variant="caption" color="text.secondary">上行</Typography>
            <Typography variant="body2" fontWeight="medium">
              {qosInfo?.ul_speed ? `${(qosInfo.ul_speed / 1000).toFixed(0)} Mbps` : '-'}
            </Typography>
          </Box>

          <Box display="flex" justifyContent="space-between" alignItems="flex-start">
            <Typography variant="caption" color="text.secondary">IPv4</Typography>
            <Typography
              variant="body2"
              fontWeight="medium"
              fontFamily="monospace"
              textAlign="right"
              data-sensitive="true"
              sx={{ maxWidth: '65%', wordBreak: 'break-all' }}
            >
              {connectionAddresses.ipv4[0] || '-'}
            </Typography>
          </Box>

          <Box display="flex" justifyContent="space-between" alignItems="flex-start">
            <Typography variant="caption" color="text.secondary">IPv6</Typography>
            <Typography
              variant="body2"
              fontWeight="medium"
              fontFamily="monospace"
              textAlign="right"
              data-sensitive="true"
              sx={{ maxWidth: '65%', wordBreak: 'break-all' }}
            >
              {connectionAddresses.ipv6[0] || '-'}
            </Typography>
          </Box>

          <Box display="flex" justifyContent="space-between" alignItems="center" pt={0.5} borderTop={1} borderColor="divider">
            <Box display="flex" alignItems="center" gap={0.5}>
              <Box sx={{ width: 6, height: 6, borderRadius: '50%', bgcolor: connectivity?.ipv4?.success ? 'success.main' : 'error.main' }} />
              <Typography variant="caption" color="text.secondary">IPv4</Typography>
              <Typography variant="caption" fontWeight="medium" color={connectivity?.ipv4?.success ? 'success.main' : 'error.main'}>
                {connectivity?.ipv4?.success ? `${connectivity.ipv4.latency_ms?.toFixed(0)}ms` : 'x'}
              </Typography>
            </Box>

            <Box display="flex" alignItems="center" gap={0.5}>
              <Box sx={{ width: 6, height: 6, borderRadius: '50%', bgcolor: connectivity?.ipv6?.success ? 'success.main' : 'error.main' }} />
              <Typography variant="caption" color="text.secondary">IPv6</Typography>
              <Typography variant="caption" fontWeight="medium" color={connectivity?.ipv6?.success ? 'success.main' : 'error.main'}>
                {connectivity?.ipv6?.success ? `${connectivity.ipv6.latency_ms?.toFixed(0)}ms` : 'x'}
              </Typography>
            </Box>
          </Box>
        </Stack>
      </CardContent>
    </Card>
  )
}
