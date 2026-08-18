import { useState } from 'react'
import {
  Box,
  Card,
  CardContent,
  Typography,
  Stack,
  Chip,
  IconButton,
  Tooltip,
  TextField,
  CircularProgress,
  Snackbar,
  Alert,
} from '@mui/material'
import {
  SimCard,
  Visibility,
  VisibilityOff,
  Edit,
  Check,
  Close,
} from '@mui/icons-material'
import { getSensitiveStyle } from '../utils'
import type { SimInfo } from '@/api/types'
import { useSimAdminApi } from '@/contexts/ApiContext'

interface SimCardInfoProps {
  simInfo: SimInfo | null
  onRefresh?: () => void
}

export function SimCardInfo({ simInfo, onRefresh }: SimCardInfoProps) {
  const api = useSimAdminApi()
  const [showInfo, setShowInfo] = useState(false)
  const [editingPhone, setEditingPhone] = useState(false)
  const [editingSmsc, setEditingSmsc] = useState(false)
  const [phoneInput, setPhoneInput] = useState('')
  const [smscInput, setSmscInput] = useState('')
  const [savingPhone, setSavingPhone] = useState(false)
  const [savingSmsc, setSavingSmsc] = useState(false)

  const [snackbar, setSnackbar] = useState<{ open: boolean; message: string; severity: 'success' | 'error' }>({
    open: false,
    message: '',
    severity: 'success',
  })

  const showMsg = (message: string, severity: 'success' | 'error') => {
    setSnackbar({ open: true, message, severity })
  }

  const valueTextSx = { fontSize: '0.75rem', textAlign: 'right' } as const

  const validatePhoneStr = (val: string) => /^\+?\d+$/.test(val.trim())

  const handleSavePhone = async () => {
    if (!phoneInput.trim()) {
      setEditingPhone(false)
      return
    }
    if (!validatePhoneStr(phoneInput)) {
      showMsg('号码格式错误，只能包含数字和开头的+', 'error')
      return
    }

    setSavingPhone(true)
    try {
      await api.updateSimCache({ phone_number: phoneInput.trim() })
      showMsg('号码缓存已更新', 'success')
      setEditingPhone(false)
      if (onRefresh) onRefresh()
    } catch (err) {
      showMsg(err instanceof Error ? err.message : String(err), 'error')
    } finally {
      setSavingPhone(false)
    }
  }

  const handleSaveSmsc = async () => {
    if (!smscInput.trim()) {
      setEditingSmsc(false)
      return
    }
    if (!validatePhoneStr(smscInput)) {
      showMsg('号码格式错误，只能包含数字和开头的+', 'error')
      return
    }

    setSavingSmsc(true)
    try {
      await api.updateSimCache({ sms_center: smscInput.trim() })
      showMsg('短信中心缓存已更新', 'success')
      setEditingSmsc(false)
      if (onRefresh) onRefresh()
    } catch (err) {
      showMsg(err instanceof Error ? err.message : String(err), 'error')
    } finally {
      setSavingSmsc(false)
    }
  }

  const isPhoneEmpty = !simInfo?.phone_numbers?.length
  const isSmscEmpty = !simInfo?.sms_center

  return (
    <>
      <Card sx={{ height: '100%' }}>
        <CardContent>
          <Box display="flex" alignItems="center" gap={1} mb={2}>
            <SimCard color="primary" />
            <Typography variant="subtitle1" fontWeight={700}>SIM 卡信息</Typography>
            <Chip
              label={simInfo?.present ? '已插入' : '未插入'}
              color={simInfo?.present ? 'success' : 'error'}
              size="small"
              variant="outlined"
              sx={{ ml: 'auto' }}
            />
            <Tooltip title={showInfo ? '隐藏敏感信息' : '显示完整信息'}>
              <IconButton size="small" onClick={() => setShowInfo(!showInfo)}>
                {showInfo ? <VisibilityOff fontSize="small" /> : <Visibility fontSize="small" />}
              </IconButton>
            </Tooltip>
          </Box>

          <Stack spacing={1.5}>
            <Box display="flex" justifyContent="space-between" alignItems="center" gap={2}>
              <Typography variant="caption" color="text.secondary">ICCID</Typography>
              <Typography data-sensitive="true" variant="body2" sx={{ ...valueTextSx, ...getSensitiveStyle(showInfo) }}>
                {simInfo?.iccid || 'N/A'}
              </Typography>
            </Box>

            <Box display="flex" justifyContent="space-between" alignItems="center" gap={2}>
              <Typography variant="caption" color="text.secondary" sx={{ flexShrink: 0 }}>手机号</Typography>
              {editingPhone ? (
                <Box display="flex" alignItems="center" gap={0.5}>
                  <TextField
                    size="small"
                    variant="standard"
                    placeholder="+86..."
                    value={phoneInput}
                    onChange={(e) => setPhoneInput(e.target.value)}
                    disabled={savingPhone}
                    inputProps={{ style: { fontSize: '0.75rem', textAlign: 'right' } }}
                  />
                  <IconButton size="small" color="success" onClick={() => void handleSavePhone()} disabled={savingPhone}>
                    {savingPhone ? <CircularProgress size={16} /> : <Check fontSize="small" />}
                  </IconButton>
                  <IconButton size="small" color="error" onClick={() => setEditingPhone(false)} disabled={savingPhone}>
                    <Close fontSize="small" />
                  </IconButton>
                </Box>
              ) : (
                <Box display="flex" alignItems="center" gap={0.5}>
                  <Typography data-sensitive="true" variant="body2" sx={{ ...valueTextSx, ...getSensitiveStyle(showInfo) }}>
                    {!isPhoneEmpty ? simInfo.phone_numbers[0] : 'N/A'}
                  </Typography>
                  {showInfo && (isPhoneEmpty || simInfo?.phone_number_is_manual) && simInfo?.present && (
                    <IconButton size="small" onClick={() => { setPhoneInput(simInfo?.phone_numbers?.[0] || ''); setEditingPhone(true); }}>
                      <Edit sx={{ fontSize: '1rem' }} />
                    </IconButton>
                  )}
                </Box>
              )}
            </Box>

            <Box display="flex" justifyContent="space-between" alignItems="center" gap={2}>
              <Typography variant="caption" color="text.secondary" sx={{ flexShrink: 0 }}>短信中心</Typography>
              {editingSmsc ? (
                <Box display="flex" alignItems="center" gap={0.5}>
                  <TextField
                    size="small"
                    variant="standard"
                    placeholder="+86..."
                    value={smscInput}
                    onChange={(e) => setSmscInput(e.target.value)}
                    disabled={savingSmsc}
                    inputProps={{ style: { fontSize: '0.75rem', textAlign: 'right' } }}
                  />
                  <IconButton size="small" color="success" onClick={() => void handleSaveSmsc()} disabled={savingSmsc}>
                    {savingSmsc ? <CircularProgress size={16} /> : <Check fontSize="small" />}
                  </IconButton>
                  <IconButton size="small" color="error" onClick={() => setEditingSmsc(false)} disabled={savingSmsc}>
                    <Close fontSize="small" />
                  </IconButton>
                </Box>
              ) : (
                <Box display="flex" alignItems="center" gap={0.5}>
                  <Typography data-sensitive="true" variant="body2" sx={{ ...valueTextSx, ...getSensitiveStyle(showInfo) }}>
                    {!isSmscEmpty ? simInfo.sms_center : '未读取到'}
                  </Typography>
                  {showInfo && (isSmscEmpty || simInfo?.sms_center_is_manual) && simInfo?.present && (
                    <IconButton size="small" onClick={() => { setSmscInput(simInfo?.sms_center || ''); setEditingSmsc(true); }}>
                      <Edit sx={{ fontSize: '1rem' }} />
                    </IconButton>
                  )}
                </Box>
              )}
            </Box>

            <Box display="flex" justifyContent="space-between" alignItems="center" gap={2}>
              <Typography variant="caption" color="text.secondary">MCC/MNC</Typography>
              <Typography variant="body2" sx={valueTextSx}>
                {simInfo?.mcc || '?'}/{simInfo?.mnc || '?'}
              </Typography>
            </Box>
          </Stack>
        </CardContent>
      </Card>

      <Snackbar
        open={snackbar.open}
        autoHideDuration={4000}
        onClose={() => setSnackbar((prev) => ({ ...prev, open: false }))}
        anchorOrigin={{ vertical: 'bottom', horizontal: 'center' }}
      >
        <Alert severity={snackbar.severity} sx={{ width: '100%' }}>
          {snackbar.message}
        </Alert>
      </Snackbar>
    </>
  )
}
