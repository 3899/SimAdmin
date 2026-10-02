import { useState, useEffect, type ReactNode } from 'react'
import {
  Dialog,
  DialogTitle,
  DialogContent,
  DialogActions,
  IconButton,
  Typography,
  Box,
  Stack,
  Chip,
  Button,
  Tabs,
  Tab,
  Avatar,
  Card,
  Divider,
  Link,
  Tooltip,
} from '@mui/material'
import {
  Close as CloseIcon,
  GitHub as GitHubIcon,
  Favorite as FavoriteIcon,
  InfoOutlined as InfoIcon,
  PeopleOutline as PeopleIcon,
  Coffee as CoffeeIcon,
  OpenInNew as OpenInNewIcon,
} from '@mui/icons-material'
import {
  CONTRIBUTORS,
  CONTRIBUTORS_URL,
  SPONSORS_LIST,
  SPONSOR_QR_CODES,
  fetchLatestSponsors,
  type SponsorItem,
} from '../data/credits'

interface AboutDialogProps {
  open: boolean
  activeTab?: number
  onClose: () => void
  onTabChange?: (tab: number) => void
}

interface CustomTabPanelProps {
  children?: ReactNode
  index: number
  value: number
}

function TabPanel(props: CustomTabPanelProps) {
  const { children, value, index, ...other } = props
  return (
    <div
      role="tabpanel"
      hidden={value !== index}
      id={`about-tabpanel-${index}`}
      aria-labelledby={`about-tab-${index}`}
      {...other}
    >
      {value === index && <Box sx={{ pt: 1.5, pb: 1 }}>{children}</Box>}
    </div>
  )
}

export default function AboutDialog({
  open,
  activeTab = 0,
  onClose,
  onTabChange,
}: AboutDialogProps) {
  const [uncontrolledTab, setUncontrolledTab] = useState(0)
  const tab = onTabChange ? activeTab : uncontrolledTab
  const [sponsors, setSponsors] = useState<SponsorItem[]>(SPONSORS_LIST)

  useEffect(() => {
    if (open) {
      void fetchLatestSponsors().then((latest) => {
        if (latest && Array.isArray(latest)) {
          setSponsors(latest)
        }
      })
    }
  }, [open])

  const handleSelectTab = (newTab: number) => {
    if (onTabChange) {
      onTabChange(newTab)
    } else {
      setUncontrolledTab(newTab)
    }
  }

  const handleTabChange = (_: React.SyntheticEvent, newValue: number) => {
    handleSelectTab(newValue)
  }

  const smallTagSx = {
    height: 22,
    borderRadius: '4px',
    fontSize: '0.75rem',
  }

  return (
    <Dialog
      open={open}
      onClose={onClose}
      maxWidth="md"
      fullWidth
      slotProps={{
        paper: {
          sx: {
            borderRadius: 2.5,
            width: '100%',
            maxWidth: { xs: 'calc(100% - 24px)', sm: 660, md: 700 },
            maxHeight: { xs: 'calc(100% - 32px)', sm: '88vh' },
            m: { xs: 1.5, sm: 'auto' },
            bgcolor: (theme) =>
              theme.palette.mode === 'light'
                ? 'rgba(255, 255, 255, 0.98)'
                : 'rgba(23, 27, 34, 0.98)',
            backdropFilter: 'blur(20px)',
            border: '1px solid',
            borderColor: 'divider',
            boxShadow: '0 24px 48px -12px rgba(15, 23, 42, 0.35)',
            overflow: 'hidden',
          },
        },
      }}
    >
      <DialogTitle
        sx={{
          m: 0,
          p: 2,
          pb: 1,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
        }}
      >
        <Typography variant="subtitle1" fontWeight={700} color="text.secondary">
          关于与致谢
        </Typography>
        <IconButton
          aria-label="关闭"
          onClick={onClose}
          size="small"
          sx={{ color: 'text.secondary', '&:hover': { color: 'text.primary' } }}
        >
          <CloseIcon fontSize="small" />
        </IconButton>
      </DialogTitle>

      <DialogContent sx={{ p: { xs: 2, sm: 3 }, pt: 0.5 }}>
        <Stack
          alignItems="center"
          textAlign="center"
          spacing={1.1}
          sx={{ pt: 1, pb: 2.5 }}
        >
          <Box
            component="img"
            src="/simadmin-logo.svg"
            alt="SimAdmin"
            sx={{ width: 56, height: 56, display: 'block' }}
          />
          <Typography variant="h5" fontWeight={600}>
            SimAdmin
          </Typography>
          <Typography variant="body2" color="text.secondary">
            面向 Linux 蜂窝设备的开源 SIM / eSIM 设备管理系统
          </Typography>

          <Stack
            direction="row"
            spacing={0.8}
            pt={0.4}
            flexWrap="wrap"
            justifyContent="center"
            useFlexGap
          >
            <Chip
              size="small"
              color="primary"
              variant="outlined"
              label={`v${__APP_VERSION__}`}
              sx={smallTagSx}
            />
            <Chip
              size="small"
              color="success"
              variant="outlined"
              label={`${__GIT_BRANCH__}/${__GIT_COMMIT__}`}
              sx={{ ...smallTagSx, fontFamily: 'monospace' }}
            />
            <Chip size="small" variant="outlined" label="Rust + React" sx={smallTagSx} />
            <Chip size="small" variant="outlined" label="GPLv3" sx={smallTagSx} />
          </Stack>

          <Stack
            direction="row"
            spacing={1}
            pt={0.4}
            flexWrap="wrap"
            justifyContent="center"
            useFlexGap
            sx={{ '& .MuiButton-root': { fontWeight: 400, borderRadius: '4px' } }}
          >
            <Button
              size="small"
              variant="outlined"
              startIcon={<GitHubIcon />}
              href="https://github.com/3899/SimAdmin"
              target="_blank"
              rel="noreferrer"
            >
              GitHub 仓库
            </Button>
            <Button
              size="small"
              color="primary"
              variant="outlined"
              startIcon={<FavoriteIcon />}
              onClick={() => handleSelectTab(0)}
            >
              支持作者
            </Button>
          </Stack>
        </Stack>

        {/* Tab 导航 */}
        <Box sx={{ borderBottom: 1, borderColor: 'divider' }}>
          <Tabs
            value={tab}
            onChange={handleTabChange}
            variant="fullWidth"
            textColor="primary"
            indicatorColor="primary"
            sx={{ minHeight: 42 }}
          >
            <Tab
              icon={<FavoriteIcon fontSize="small" />}
              iconPosition="start"
              label="致谢与赞助"
              sx={{ minHeight: 42, fontSize: '0.875rem', fontWeight: 600, textTransform: 'none' }}
            />
            <Tab
              icon={<PeopleIcon fontSize="small" />}
              iconPosition="start"
              label="贡献者"
              sx={{ minHeight: 42, fontSize: '0.875rem', fontWeight: 600, textTransform: 'none' }}
            />
            <Tab
              icon={<InfoIcon fontSize="small" />}
              iconPosition="start"
              label="系统说明"
              sx={{ minHeight: 42, fontSize: '0.875rem', fontWeight: 600, textTransform: 'none' }}
            />
          </Tabs>
        </Box>

        {/* Tab 0: 致谢与赞助 */}
        <TabPanel value={tab} index={0}>
          <Stack spacing={1.75}>
            <Box>
              <Stack
                direction={{ xs: 'column', sm: 'row' }}
                spacing={2}
                justifyContent="center"
                alignItems="center"
              >
                <Card
                  variant="outlined"
                  sx={{
                    p: 1.5,
                    pb: 1,
                    textAlign: 'center',
                    flex: '1 1 0',
                    width: { xs: '100%', sm: 'auto' },
                    maxWidth: 220,
                    borderRadius: 1.5,
                  }}
                >
                  <Box
                    component="img"
                    src={SPONSOR_QR_CODES.wechat}
                    alt="微信赞助"
                    sx={{
                      width: '100%',
                      height: 'auto',
                      maxHeight: 168,
                      maxWidth: 168,
                      objectFit: 'contain',
                      borderRadius: 1,
                      bgcolor: '#fff',
                      display: 'inline-block',
                    }}
                  />
                  <Typography
                    variant="caption"
                    fontWeight={600}
                    sx={{ color: '#07c160', display: 'block', mt: 0.5 }}
                  >
                    微信赞助
                  </Typography>
                </Card>

                <Card
                  variant="outlined"
                  sx={{
                    p: 1.5,
                    pb: 1,
                    textAlign: 'center',
                    flex: '1 1 0',
                    width: { xs: '100%', sm: 'auto' },
                    maxWidth: 220,
                    borderRadius: 1.5,
                  }}
                >
                  <Box
                    component="img"
                    src={SPONSOR_QR_CODES.alipay}
                    alt="支付宝赞助"
                    sx={{
                      width: '100%',
                      height: 'auto',
                      maxHeight: 168,
                      maxWidth: 168,
                      objectFit: 'contain',
                      borderRadius: 1,
                      bgcolor: '#fff',
                      display: 'inline-block',
                    }}
                  />
                  <Typography
                    variant="caption"
                    fontWeight={600}
                    sx={{ color: '#1677ff', display: 'block', mt: 0.5 }}
                  >
                    支付宝赞助
                  </Typography>
                </Card>
              </Stack>
            </Box>
            <Box
              sx={{
                p: 2,
                borderRadius: 1.5,
                bgcolor: 'action.hover',
                border: '1px solid',
                borderColor: 'divider',
              }}
            >
              <Stack direction="row" spacing={2} alignItems="center">
                <CoffeeIcon
                  color="primary"
                  sx={{
                    fontSize: 32,
                    flexShrink: 0,
                  }}
                />
                <Typography variant="body2" color="text.secondary" sx={{ lineHeight: 1.6 }}>
                  SimAdmin 由开发者业余维护，开发不易，若它对您有所帮助，欢迎请作者喝杯咖啡，您的支持将帮助我们更好地维护和发展项目！
                </Typography>
              </Stack>
            </Box>

            <Divider />

            {/* 赞助榜*/}
            <Box>
              <Stack direction="row" justifyContent="space-between" alignItems="center" sx={{ mb: 1 }}>
                <Typography variant="subtitle2" fontWeight={600}>
                  赞助榜
                </Typography>
                <Tooltip title="赞助时请务必填写留言，以便我们收录到赞助名单中，感谢您的支持与鼓励！">
                  <Typography variant="caption" color="text.secondary" sx={{ cursor: 'pointer' }}>
                    如何上榜？
                  </Typography>
                </Tooltip>
              </Stack>

              {sponsors.length > 0 ? (
                <Box
                  sx={{
                    maxHeight: 220,
                    overflowY: 'auto',
                    pr: 1,
                    '&::-webkit-scrollbar': { width: 5 },
                    '&::-webkit-scrollbar-thumb': {
                      bgcolor: 'action.hover',
                      borderRadius: 2.5,
                    },
                  }}
                >
                  {sponsors.map((sponsor, idx) => (
                    <Box
                      key={idx}
                      sx={{
                        py: 0.9,
                        px: 0.5,
                        display: 'flex',
                        justifyContent: 'space-between',
                        alignItems: 'center',
                        borderBottom: idx < sponsors.length - 1 ? '1px dashed' : 'none',
                        borderColor: 'divider',
                      }}
                    >
                      <Box display="flex" alignItems="center" gap={1}>
                        <Typography variant="body2" fontWeight={600}>
                          {sponsor.name}
                        </Typography>
                        {sponsor.date && (
                          <Typography variant="caption" color="text.secondary" sx={{ fontWeight: 400 }}>
                            {sponsor.date}
                          </Typography>
                        )}
                      </Box>
                      <Typography
                        variant="caption"
                        color="text.secondary"
                        sx={{ fontWeight: 400, maxWidth: '65%', textAlign: 'right', wordBreak: 'break-word' }}
                      >
                        {sponsor.message || '—'}
                      </Typography>
                    </Box>
                  ))}
                </Box>
              ) : (
                <Typography
                  variant="caption"
                  color="text.secondary"
                  align="center"
                  display="block"
                  sx={{ py: 2 }}
                >
                  暂无赞助记录，欢迎成为首位支持者！
                </Typography>
              )}
            </Box>
          </Stack>
        </TabPanel>

        {/* Tab 1: 贡献者 */}
        <TabPanel value={tab} index={1}>
          <Stack spacing={2}>
            <Box display="flex" justifyContent="space-between" alignItems="center" flexWrap="wrap" gap={1}>
              <Typography variant="subtitle2" fontWeight={600}>
                贡献者
              </Typography>
              <Link
                href={CONTRIBUTORS_URL}
                target="_blank"
                rel="noopener noreferrer"
                variant="caption"
                color="primary"
                underline="hover"
                sx={{ display: 'inline-flex', alignItems: 'center', gap: 0.5 }}
              >
                全部贡献者详见 GitHub
                <OpenInNewIcon sx={{ fontSize: 13 }} />
              </Link>
            </Box>

            <Box
              sx={{
                display: 'grid',
                gridTemplateColumns: { xs: '1fr', sm: '1fr 1fr', md: 'repeat(3, 1fr)' },
                gap: 1.25,
              }}
            >
              {CONTRIBUTORS.map((c) => (
                <Card
                  key={c.name}
                  variant="outlined"
                  sx={{
                    p: 1.25,
                    display: 'flex',
                    alignItems: 'center',
                    gap: 1.25,
                    textDecoration: 'none',
                    color: 'inherit',
                    borderRadius: 1.5,
                    transition: 'all 0.15s ease',
                    '&:hover': {
                      borderColor: 'primary.main',
                      bgcolor: 'action.hover',
                    },
                  }}
                  component={Link}
                  href={c.github}
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  <Avatar
                    src={c.avatar}
                    alt={c.name}
                    sx={{ width: 36, height: 36, border: '1px solid', borderColor: 'divider' }}
                  >
                    {c.name.slice(0, 1)}
                  </Avatar>
                  <Box minWidth={0}>
                    <Typography variant="body2" fontWeight={600} noWrap sx={{ fontSize: '0.85rem' }}>
                      {c.name}
                    </Typography>
                    <Typography variant="caption" color="text.secondary" noWrap display="block" sx={{ fontSize: '0.72rem' }}>
                      {c.role}
                    </Typography>
                  </Box>
                </Card>
              ))}
            </Box>
          </Stack>
        </TabPanel>

        {/* Tab 2: 系统说明 */}
        <TabPanel value={tab} index={2}>
          <Stack spacing={2}>
            <Box>
              <Typography variant="subtitle2" fontWeight={600} gutterBottom>
                架构与技术栈
              </Typography>
              <Typography variant="body2" color="text.secondary" paragraph sx={{ mb: 1 }}>
                • 后端采用 <b>Rust + Axum</b> 构建，基于 ModemManager D-Bus 接口、AT 指令与 QMI 协议管理蜂窝模组。
              </Typography>
              <Typography variant="body2" color="text.secondary" paragraph sx={{ mb: 1 }}>
                • 前端采用 <b>React 19 + TypeScript + Material UI</b> 单页应用，由后端二进制同进程高速托管。
              </Typography>
              <Typography variant="body2" color="text.secondary">
                • 针对嵌入式 ARMv7、AArch64 及 x86_64 设备深度优化，轻量、安全、低资源占用。
              </Typography>
            </Box>

            <Divider />

            <Box>
              <Typography variant="subtitle2" fontWeight={600} gutterBottom>
                开源协议
              </Typography>
              <Typography variant="body2" color="text.secondary">
                本项目采用 <b>GNU General Public License v3.0 (GPLv3)</b> 协议开源。严禁闭源修改后作为专有商业固件分发，衍生作品须保持同等开源授权并保留原作者署名。
              </Typography>
            </Box>
          </Stack>
        </TabPanel>
      </DialogContent>

      <DialogActions sx={{ px: 2.5, py: 1.5, bgcolor: 'action.hover' }}>
        <Typography variant="caption" color="text.secondary" sx={{ mr: 'auto' }}>
          Copyright © 2026 @3899
        </Typography>
        <Button onClick={onClose} size="small" variant="text">
          关闭
        </Button>
      </DialogActions>
    </Dialog>
  )
}
