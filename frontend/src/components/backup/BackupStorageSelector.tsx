import {
  Box,
  TextField,
  ToggleButton,
  ToggleButtonGroup,
  Typography,
  Theme,
} from '@mui/material'
import {
  CloudOff,
  Download,
  Folder,
} from '@mui/icons-material'

export type BackupDestination = 'local' | 'download' | 'webdav'

type BackupStorageSelectorProps = {
  destination: BackupDestination
  localDir: string
  onDestinationChange: (destination: BackupDestination) => void
  onLocalDirChange?: (localDir: string) => void
  disableDownload?: boolean
  hideDownload?: boolean
  localDirDisabled?: boolean
  localDirReadOnly?: boolean
  label?: string
  textFieldSx?: object
}

const toggleButtonSx = (theme: Theme) => ({
  py: 0.75,
  textTransform: 'none',
  fontSize: '14px',
  fontWeight: 500,
  color: 'text.secondary',
  transition: 'all 0.2s ease',
  '&.Mui-selected': {
    bgcolor: theme.palette.mode === 'light' ? 'background.paper' : 'rgba(255, 255, 255, 0.05)',
    color: 'primary.main',
    fontWeight: 600,
    boxShadow: theme.palette.mode === 'light' ? '0 2px 6px -2px rgba(15, 23, 42, 0.15)' : 'none',
    '&:hover': {
      bgcolor: theme.palette.mode === 'light' ? 'background.paper' : 'rgba(255, 255, 255, 0.08)',
    }
  },
  '&:hover': {
    bgcolor: 'transparent',
    color: 'text.primary',
  },
})

export default function BackupStorageSelector({
  destination,
  localDir,
  onDestinationChange,
  onLocalDirChange,
  disableDownload = false,
  hideDownload = false,
  localDirDisabled = false,
  localDirReadOnly = false,
  label = '存储位置',
  textFieldSx,
}: BackupStorageSelectorProps) {
  return (
    <Box>
      <Typography variant="body2" fontWeight={600} color="text.secondary" mb={1}>
        {label}
      </Typography>
      <ToggleButtonGroup
        exclusive
        fullWidth
        value={destination}
        onChange={(_, value: BackupDestination | null) => {
          if (value) onDestinationChange(value)
        }}
        sx={{
          bgcolor: 'action.hover',
          p: 0.5,
          borderRadius: 1.5,
          border: '1px solid',
          borderColor: 'divider',
          '& .MuiToggleButtonGroup-grouped': {
            border: 0,
            '&.Mui-disabled': {
              border: 0,
            },
            '&:not(:first-of-type)': {
              borderRadius: 1.5,
            },
            '&:first-of-type': {
              borderRadius: 1.5,
            },
          },
        }}
      >
        <ToggleButton value="local" sx={toggleButtonSx}>
          <Folder fontSize="small" sx={{ mr: 0.75 }} />
          本地存储
        </ToggleButton>
        {!hideDownload && (
          <ToggleButton value="download" disabled={disableDownload} sx={toggleButtonSx}>
            <Download fontSize="small" sx={{ mr: 0.75 }} />
            我的电脑
          </ToggleButton>
        )}
        <ToggleButton value="webdav" disabled sx={toggleButtonSx}>
          <CloudOff fontSize="small" sx={{ mr: 0.75 }} />
          WebDAV(预留)
        </ToggleButton>
      </ToggleButtonGroup>

      {destination === 'local' && (
        <TextField
          label="本地备份目录"
          value={localDir}
          onChange={(event) => onLocalDirChange?.(event.target.value)}
          disabled={localDirDisabled}
          fullWidth
          slotProps={{
            input: {
              readOnly: localDirReadOnly,
            },
          }}
          sx={{ mt: 2.25, ...textFieldSx }}
        />
      )}
    </Box>
  )
}
