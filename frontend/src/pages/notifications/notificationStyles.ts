import { alpha, type Theme } from '@mui/material/styles'

export const notificationFilterTextFieldSx = {
  '& .MuiInputLabel-root': {
    fontSize: 14,
  },
  '& .MuiInputBase-input': {
    fontSize: 14,
  },
  '& .MuiMenuItem-root': {
    fontSize: 14,
  },
  '& .MuiOutlinedInput-root': {
    bgcolor: 'transparent',
    borderRadius: 1.5,
    '& .MuiOutlinedInput-notchedOutline': {
      borderColor: 'divider',
    },
    '&:hover .MuiOutlinedInput-notchedOutline': {
      borderColor: 'text.disabled',
    },
    '&.Mui-focused .MuiOutlinedInput-notchedOutline': {
      borderColor: '#1296DB',
    },
  },
} as const

export const notificationToggleButtonSx = {
  minHeight: 40,
  fontWeight: 500,
  fontSize: '13px',
  '&.Mui-selected': {
    color: 'primary.main',
    fontWeight: 700,
    backgroundColor: 'transparent !important',
    borderColor: (theme: Theme) => `${theme.palette.primary.main} !important`,
    '&:hover': {
      backgroundColor: (theme: Theme) => alpha(theme.palette.primary.main, 0.04),
    },
  },
} as const
