import { Box, Typography, Button, CircularProgress } from '@mui/material'
import { Delete } from '@mui/icons-material'

export interface SmsBatchBarProps {
  selectedText: string
  onDelete: () => void
  disabled?: boolean
  deleting?: boolean
}

export function SmsBatchBar({
  selectedText,
  onDelete,
  disabled = false,
  deleting = false,
}: SmsBatchBarProps) {
  return (
    <Box
      sx={{
        mx: 1.5,
        my: 1,
        p: 1,
        borderRadius: 1.5,
        bgcolor: 'action.hover',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        gap: 1,
      }}
    >
      <Typography variant="caption" fontWeight={600}>
        {selectedText}
      </Typography>
      <Button
        size="small"
        color="error"
        variant="contained"
        startIcon={deleting ? <CircularProgress size={14} color="inherit" /> : <Delete fontSize="small" />}
        onClick={onDelete}
        disabled={disabled || deleting}
      >
        删除
      </Button>
    </Box>
  )
}
