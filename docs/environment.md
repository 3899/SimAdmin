# 运行环境与系统管理

## 目标设备运行要求

- **操作系统**：Linux / Debian 系统。
- **系统管理器**：systemd。
- **权限**：需要 root 运行权限。
- **IPC 机制**：system D-Bus。
- **核心依赖包与指令**：
  - `ModemManager` 和 `mmcli`
  - `NetworkManager` 和 `nmcli`
  - `qmicli`（用于基站定位/网络小区信息兜底读取）
  - `iptables` / `ip6tables`（仅用于网络通路只读诊断；本程序不会自动修改或清空防火墙规则）
  - `ip` / `ifconfig` / `route`（用于网络状态诊断，其中 `ifconfig` 和 `route` 需确保系统已安装 `net-tools`）
  - `tar`（OTA 包解压必需）
  - `unzip` / `busybox unzip` / `python3`（用于自动解压及下载的 `lpac`；`unzip` 也用于手动上传 zip 格式 OTA 包）
- **eSIM 芯片管理**：eSIM 模式下的芯片/配置管理依赖开源的 `lpac` 辅助程序。

---

## 默认安装路径与文件说明

| 路径 | 说明 |
|------|------|
| `/opt/simadmin/simadmin` | 后端二进制程序 |
| `/opt/simadmin/www/` | 前端 Web 静态 SPA 资源文件 |
| `/opt/simadmin/lpac/` | 自动下载匹配当前架构的私有 `lpac` 程序目录，后端优先调用此路径 |
| `/opt/simadmin/data.db` | SQLite 数据库文件（保存短信记录、登录认证密码散列值、会话 Token、自动化日志等） |
| `/opt/simadmin/meta.json` | 记录当前安装包的 OTA 元数据（版本、构建时间、二进制 MD5 等） |
| `/data/config.json` | 优先加载的系统运行持久化配置文件 |
| `/opt/simadmin/config.json` | `/data` 目录不存在时，回退加载的系统配置文件 |
| `/tmp/ota_staging` | 上传 OTA 包后的临时校验与备份解压目录 |
| `/etc/systemd/system/simadmin.service` | SimAdmin 后端主服务守护单元 |
| `/etc/systemd/system/simadmin-modem-recovery.service` | 开机 modem 搜网异常自愈恢复服务单元 |
| `/usr/local/bin/simadmin-modem-recovery.sh` | 开机自愈监控与搜网状态恢复的执行脚本 |
| `/etc/NetworkManager/conf.d/99-simadmin-unmanaged-modem.conf` | NetworkManager 忽略托管 `wwan*` 接口配置，避免与主服务抢占调制解调器控制权 |

---

## eSIM 芯片管理机制

本项目中的 eSIM 指写入了 Profiles 数据的实体 eUICC 芯片 SIM 卡，插入设备物理卡槽后仍按普通卡使用。SimAdmin 的「eSIM 模式」开关仅控制 eSIM 管理面板在 Web 页面是否开放，并不切换设备板载硬件通路。

* **普通 SIM 模式**：「SIM 卡管理」页面中将隐藏「eSIM 管理」标签页，`/api/esim/*` 所有相关接口返回 `403`，前端不加载 eSIM 关联页面 chunk 资源，后端不会主动拉起或调用 `lpac`。
* **eSIM 模式**：切换到 eSIM 模式后，只有切入「eSIM 管理」Tab 页或用户执行 Profile 写卡、切换、重命名等操作时，后端才会按需调用 `lpac chip info`、`lpac profile list`、`lpac profile enable`、`lpac profile nickname` 和 `lpac profile delete` 进行瞬时通信。
* **`lpac` 下载与维护**：
  - 一键安装脚本 `install_latest.sh` 会根据 `uname -m` 和 glibc 版本，优先匹配架构并拉取 ESTK 的 `lpac` 静态编译程序至 `/opt/simadmin/lpac/lpac`。如果系统已存在可用版本则跳过下载。如果需要阻止脚本下载，请在安装时设置环境变量 `SIMADMIN_INSTALL_LPAC=0`。
  - 单独手动应用 OTA 包**不会**自动安装或升级 `lpac`。
  - 若系统检测到有 eSIM 支持却缺失 `lpac`，管理页面会提供「安装/修复 lpac」的便捷入口。其内部修复逻辑由后端内置 zip 解压引擎在内存中运行完成，不依赖外部环境命令。

## systemd 服务配置说明

### 主服务守护单元 (`simadmin.service`)

默认配置位于 `scripts/simadmin.service`：

- `WorkingDirectory=/opt/simadmin`
- `ExecStart=/opt/simadmin/simadmin`
- `Restart=always`
- `Environment=DBUS_SYSTEM_BUS_ADDRESS=unix:path=/var/run/dbus/system_bus_socket`

### 常用管理命令

```bash
# 查看主服务状态与调试日志
systemctl status simadmin --no-pager
journalctl -u simadmin -f

# 查看开机 modem 恢复服务的日志
systemctl status simadmin-modem-recovery --no-pager
journalctl -u simadmin-modem-recovery -f
```

---

## 数据持久化与存储设计

### 1. SQLite 数据库数据

保存在 `/opt/simadmin/data.db`，存储以下结构数据：

- `sms_messages`：短信收发记录明细。
- `auth_config`：管理员登录密码哈希、强密码策略等安全配置。
- `auth_sessions`：已建立的 Web Session Cookie 哈希与超时策略。
- `automation_logs`：定时与周期性自动化任务运行日志（包括名称、执行动作、触发状态及执行回执）。

*注：管理员密码和会话 token 不以明文存储。修改密码或清除管理员配置会同步置空所有旧会话令牌。*

### 2. 本地持久化配置文件

保存在 `/data/config.json`（或回退路径 `/opt/simadmin/config.json`），存储以下运行设置：

- 通知中心配置（各通知通道实例及其路由转发规则、通知模板）。
- 蜂窝漫游策略开关。
- 数据连接启用/禁用状态。
- 设备网络 DDNS 同步规则与配置。
- 自动化中心配置（任务触发器与对应执行动作指令等参数）。
