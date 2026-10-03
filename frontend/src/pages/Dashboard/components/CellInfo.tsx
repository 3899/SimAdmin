import { useState, type MouseEvent } from 'react'
import {
  Box,
  Card,
  CardContent,
  Typography,
  Chip,
  IconButton,
  Tooltip,
  Collapse,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  type Theme,
  TableRow,
  Paper,
} from '@mui/material'
import { alpha } from '@/utils/theme'
import { CellTower, Visibility, VisibilityOff, ExpandMore, ExpandLess, Info } from '@mui/icons-material'
import { useLocalSensitiveToggle } from '@/contexts/SensitiveInfoContext'
import { getSensitiveStyle, formatSignalValue, getSignalChipColor } from '../utils'
import type { CellsResponse } from '@/api/types'

interface CellInfoProps {
  cellsInfo: CellsResponse | null
}

export function CellInfo({ cellsInfo }: CellInfoProps) {
  const [expanded, setExpanded] = useState(true)
  const [showInfo, toggleShowInfo] = useLocalSensitiveToggle()

  return (
    <Card>
      <Box
        sx={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          p: 1.5,
          cursor: 'pointer',
          '&:hover': { bgcolor: 'action.hover' },
        }}
        onClick={() => setExpanded(!expanded)}
      >
        <Box display="flex" alignItems="center" gap={1} flexWrap="wrap">
          <CellTower fontSize="small" color="primary" />
          <Typography variant="subtitle2" fontWeight="medium">小区信息</Typography>
          <Tooltip title={showInfo ? '隐藏敏感信息' : '显示完整信息'}>
            <IconButton
              size="small"
              onClick={(e: MouseEvent) => {
                e.stopPropagation()
                toggleShowInfo()
              }}
            >
              {showInfo ? <VisibilityOff fontSize="small" /> : <Visibility fontSize="small" />}
            </IconButton>
          </Tooltip>
        </Box>
        <Box display="flex" alignItems="center" gap={0.5}>
          {cellsInfo?.cells && (
            <Chip label={`${cellsInfo.cells.length}`} size="small" color="primary" variant="outlined" />
          )}
          <IconButton size="small">
            {expanded ? <ExpandLess /> : <ExpandMore />}
          </IconButton>
        </Box>
      </Box>
      <Collapse in={expanded}>
        <CardContent sx={{ pt: 0, px: { xs: 1, sm: 2 } }}>
          {/* Serving Cell 摘要 */}
          {cellsInfo?.serving_cell && (
            <Box 
              data-sensitive={showInfo ? 'false' : 'true'}
              sx={{ 
                display: 'flex', 
                flexWrap: 'wrap', 
                gap: 1, 
                mb: 1.5, 
                p: 1, 
                bgcolor: 'action.hover', 
                borderRadius: 1,
                ...getSensitiveStyle(showInfo)
              }}
            >
              <Chip label={cellsInfo.serving_cell.tech?.toUpperCase() || '-'} size="small" color="primary" />
              <Typography variant="caption" color="text.secondary" sx={{ display: 'flex', alignItems: 'center', gap: 0.5 }}>
                CID: <strong>{cellsInfo.serving_cell.cell_id}</strong>
              </Typography>
              <Typography variant="caption" color="text.secondary" sx={{ display: 'flex', alignItems: 'center', gap: 0.5 }}>
                TAC: <strong>{cellsInfo.serving_cell.tac}</strong>
              </Typography>
            </Box>
          )}
          <TableContainer component={Paper} variant="outlined" sx={{ maxHeight: 300 }}>
            <Table size="small" stickyHeader>
              <TableHead>
                <TableRow>
                  <TableCell sx={{ minWidth: 50, py: 0.5, px: 1, fontSize: '0.7rem' }}>频段</TableCell>
                  <TableCell align="right" sx={{ minWidth: 55, py: 0.5, px: 0.5, fontSize: '0.7rem' }}>ARFCN</TableCell>
                  <TableCell align="right" sx={{ minWidth: 40, py: 0.5, px: 0.5, fontSize: '0.7rem' }}>PCI</TableCell>
                  <TableCell align="right" sx={{ minWidth: 50, py: 0.5, px: 0.5, fontSize: '0.7rem' }}>RSRP</TableCell>
                  <TableCell align="right" sx={{ minWidth: 45, py: 0.5, px: 0.5, fontSize: '0.7rem' }}>RSRQ</TableCell>
                  <TableCell align="right" sx={{ minWidth: 45, py: 0.5, px: 0.5, fontSize: '0.7rem' }}>SINR</TableCell>
                </TableRow>
              </TableHead>
              <TableBody>
                {cellsInfo?.cells && cellsInfo.cells.length > 0 ? (
                  cellsInfo.cells.map((cell, idx) => (
                    <TableRow 
                      key={idx} 
                      sx={{ 
                        bgcolor: cell.is_serving 
                          ? (theme: Theme) => {
                              const successMain = (theme.palette.success as { main: string }).main
                              return alpha(successMain, theme.palette.mode === 'dark' ? 0.15 : 0.08)
                            }
                          : 'inherit',
                      }}
                    >
                      <TableCell sx={{ py: 0.5, px: 1 }}>
                        <Box display="flex" alignItems="center" gap={0.5}>
                          {cell.is_serving && (
                            <Box 
                              sx={{ 
                                width: 6, 
                                height: 6, 
                                borderRadius: '50%', 
                                bgcolor: 'success.main',
                                flexShrink: 0,
                              }} 
                            />
                          )}
                          <Typography variant="caption" sx={{ fontSize: '0.75rem', fontWeight: cell.is_serving ? 600 : 400 }}>
                            {cell.band && cell.band !== '0' ? cell.band : '-'}
                          </Typography>
                        </Box>
                      </TableCell>
                      <TableCell align="right" sx={{ py: 0.5, px: 0.5, fontSize: '0.75rem', fontFamily: 'monospace' }}>{cell.arfcn || '-'}</TableCell>
                      <TableCell align="right" sx={{ py: 0.5, px: 0.5, fontSize: '0.75rem', fontFamily: 'monospace' }}>{cell.pci || '-'}</TableCell>
                      <TableCell align="right" sx={{ py: 0.5, px: 0.5 }}>
                        {cell.rsrp !== undefined ? (
                          <Chip
                            label={formatSignalValue(cell.rsrp)}
                            size="small"
                            color={getSignalChipColor(cell.rsrp)}
                            sx={{ height: 18, fontSize: '0.65rem', '& .MuiChip-label': { px: 0.5 } }}
                          />
                        ) : '-'}
                      </TableCell>
                      <TableCell align="right" sx={{ py: 0.5, px: 0.5, fontSize: '0.7rem', fontFamily: 'monospace' }}>{formatSignalValue(cell.rsrq)}</TableCell>
                      <TableCell align="right" sx={{ py: 0.5, px: 0.5, fontSize: '0.7rem', fontFamily: 'monospace' }}>{formatSignalValue(cell.sinr)}</TableCell>
                    </TableRow>
                  ))
                ) : (
                  <TableRow>
                    <TableCell colSpan={6} align="center">
                      <Box display="flex" alignItems="center" justifyContent="center" gap={1} py={1}>
                        <Info fontSize="small" color="disabled" />
                        <Typography variant="caption" color="text.secondary">暂无小区数据</Typography>
                      </Box>
                    </TableCell>
                  </TableRow>
                )}
              </TableBody>
            </Table>
          </TableContainer>
        </CardContent>
      </Collapse>
    </Card>
  )
}
