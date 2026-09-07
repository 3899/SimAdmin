# 安装与部署指南


## 设备侧一键安装 / 升级

在目标设备上以 root 执行：

```bash
curl -fsSL https://raw.githubusercontent.com/3899/SimAdmin/main/install_latest.sh | sh
```

### 国内网络环境

```bash
curl -fsSL https://gh-proxy.com/https://raw.githubusercontent.com/3899/SimAdmin/main/install_latest.sh | sh
```

### 指定版本与产物包

默认下载并安装标准包 `simadmin-aarch64.tar.gz` / `simadmin-armv7.tar.gz` / `simadmin-x86_64.tar.gz`，支持的 WFC 产物包 `simadmin-wfc-aarch64.tar.gz` / `simadmin-wfc-armv7.tar.gz` / `simadmin-wfc-x86_64.tar.gz`：

```bash
# 安装最新版本的 WFC 产物包
curl -fsSL https://raw.githubusercontent.com/3899/SimAdmin/main/install_latest.sh | sh -s -- --wfc

# 同时指定版本与 WFC 产物包
curl -fsSL https://raw.githubusercontent.com/3899/SimAdmin/main/install_latest.sh | sh -s -- -v1.1.8 --wfc

# 通过环境变量指定版本与 WFC 产物包
curl -fsSL https://raw.githubusercontent.com/3899/SimAdmin/main/install_latest.sh | VERSION=v1.1.8 WFC=1 sh
```

### 安装脚本参数说明

| 参数 | 说明 |
|------|------|
| `-v, --version VERSION` | 指定安装目标版本（例如 `v1.1.8` 或 `1.1.8`），默认 `latest` |
| `--wfc` | 指定下载安装 WFC 产物包 |
| `-a, --asset NAME` | 自定义 Release 产物文件名 |
| `--install-dir PATH` | 指定安装目录，默认 `/opt/simadmin` |
| `--service-name NAME` | 指定 systemd 主服务名，默认 `simadmin` |
| `--deps-mode MODE` | 依赖策略：`auto`、`minimal`、`full`、`skip`，默认 `auto` |
| `--modem-protocol MODE` | modem 工具策略：`auto`、`qmi`、`mbim`、`at`、`all`，默认 `auto` |
| `--refresh-modem` | 强制刷新 udev 设备并重启 ModemManager |
| `--no-refresh-modem` | 不刷新 udev 设备，也不为刷新动作重启 ModemManager |
| `--no-lpac` | 跳过 lpac (eSIM CLI) 的自动下载与安装 |
| `--lpac-only` | 仅安装或更新共享 lpac 运行时，不安装 SimAdmin 服务 |
| `-h, --help` | 显示帮助信息 |

### 可选环境变量

```bash
curl -fsSL https://raw.githubusercontent.com/3899/SimAdmin/main/install_latest.sh \
  | REPO=3899/SimAdmin INSTALL_DIR=/opt/simadmin SERVICE_NAME=simadmin VERSION=latest WFC=1 sh
```

常用安装策略环境变量：

| 环境变量 | 默认值 | 说明 |
|------|------|------|
| `SIMADMIN_DEPS_MODE` | `auto` | 依赖策略，含义见下文 |
| `SIMADMIN_APT_UPDATE` | `auto` | `auto` 仅缺包时更新索引；`always` 每次运行更新一次；`never` 不更新索引 |
| `SIMADMIN_MODEM_PROTOCOL` | `auto` | 自动检测 QMI/MBIM；也可显式指定 `qmi`、`mbim`、`at` 或 `all` |
| `SIMADMIN_INSTALL_SYSTEM_DEPS` | `1` | 设为 `0` 等价于依赖策略 `skip`，用于兼容旧调用方式 |
| `SIMADMIN_ENABLE_NETWORKMANAGER` | `1` | 是否安装（`auto` 模式）并启用 NetworkManager |
| `SIMADMIN_REFRESH_MODEM_DEVICES` | `auto` | 首装、依赖变化或未枚举到 modem 时刷新；也可设为 `0`/`1` |
| `SIMADMIN_VERIFY_ASSET` | `auto` | 官方 Release 自动校验 GitHub asset SHA-256；可设为 `0` 禁用或 `1` 强制 |
| `SIMADMIN_ASSET_SHA256` | 空 | 自定义 `ASSET_URL` 时提供可信 SHA-256 |
| `SIMADMIN_TMPDIR` | 空 | 优先使用的临时目录；不可用时自动回退 |
| `SIMADMIN_MIN_MODEMMANAGER_VERSION` | 空 | 可选的 ModemManager 最低 Debian 版本要求 |
| `SIMADMIN_MIN_NETWORKMANAGER_VERSION` | 空 | 可选的 NetworkManager 最低 Debian 版本要求 |
| `GH_PROXY` | `https://gh-proxy.com/` | 首个 GitHub 代理前缀 |
| `GH_PROXY_FALLBACKS` | `https://ghproxy.net/ https://githubproxy.cc/` | 以空格分隔的备用 GitHub 代理前缀，按书写顺序尝试 |

对于 `github.com`、`raw.githubusercontent.com`、`objects.githubusercontent.com` 和 `api.github.com` 地址，安装脚本依次尝试 `GH_PROXY`、`GH_PROXY_FALLBACKS` 中的每个代理，全部失败后才使用官方 GitHub 地址直连兜底。该顺序同时用于 Release 应用包、源码仓库中的 unit/recovery 文件、Release API 和 lpac；非 GitHub 自定义 URL 不拼接代理前缀，始终直接访问。

### 依赖安装策略

脚本先通过 `dpkg-query` 检查每个 Debian 软件包是否已安装，并在配置了最低版本时通过 `dpkg --compare-versions` 校验版本。所有要求都已满足时，默认不会执行 `apt-get update`，也不会重复执行 `apt-get install`；缺少依赖时只安装缺少或版本不足的包。

| 模式 | 自动补齐内容 | 适用场景 |
|------|------|------|
| `auto` | 显式安装 `ca-certificates`、`curl`、`tar`、D-Bus、udev、ModemManager、基础网络/诊断工具；按配置加入 NetworkManager；按设备能力加入 QMI 或 MBIM 工具；仅在支持 lpac 的架构加入 PC/SC 运行库 | 默认，保证对应硬件路径的完整功能 |
| `minimal` | 保证下载安装和 ModemManager 主路径所需的 `ca-certificates curl tar dbus udev modemmanager`；启用 NetworkManager 时也保证 `network-manager` | 已自行维护其他网络工具的裁剪系统；部分诊断能力可能不可用 |
| `full` | `auto` 的能力检测结果，加上 NetworkManager 和可选 iptables 只读诊断工具 | 希望同时准备完整诊断能力的通用镜像 |
| `skip` | 不执行任何 apt 操作，已有的 `curl`、`tar` 等命令仍必须可用 | 只读系统、离线部署或由镜像预装全部依赖 |

默认 `auto` 中各包不是按 CPU 架构机械安装：QMI/MBIM 根据设备节点和 ModemManager 信息选择，与 ARM64、AMD64、ARMv7 无关；未连接 modem 时同时准备 QMI/MBIM 工具，避免之后插入设备还要重跑安装。ARMv7 MVP 始终排除 lpac 和 `libpcsclite1`。`unzip` 用于 zip OTA，`psmisc` 提供重启准备所需的 `killall`，`iproute2` 和 `iputils-ping` 用于网络状态与连通性路径；`iptables` 只在 `full` 中补齐，程序缺少它时只会失去对应的只读诊断。

`SIMADMIN_APT_UPDATE=never` 只禁止刷新软件包索引，不会忽略缺包；如果本地索引无法解析所需包，安装仍会明确失败。卸载脚本不会自动移除这些系统依赖，因为它们可能正被其他服务使用。

### Release 与源码文件边界

常规 SimAdmin 版本的 GitHub Release **只发布最终 SimAdmin 应用包**。`simadmin.service`、`simadmin-modem-recovery.sh` 和 `simadmin-modem-recovery.service` 不进入 Release，也不塞入应用 tar 包：

- 安装 `latest` 时，应用包来自 latest Release，三个受管脚本从源码仓库 `main` 分支下载；
- 指定 `VERSION=1.2.3` 或 `-v1.2.3` 时，应用包来自 `v1.2.3` Release，三个受管脚本从源码仓库 `v1.2.3` tag 下载；
- 脚本先下载并校验全部远程输入，再停止或替换现有服务；自定义安装目录时会同步重写 unit 的 `WorkingDirectory` 和 `ExecStart`。

官方 Release 包使用 GitHub Release API 返回的 asset digest 做 SHA-256 校验，不需要额外发布 checksum、unit 或 recovery 附件。自定义 `ASSET_URL` 若未同时设置 `SIMADMIN_ASSET_SHA256`，`auto` 模式会告警；强制校验模式会直接拒绝安装。

### 安装脚本动作说明

- 根据 `uname -m` 自动选择 GitHub Release 中的 ARM64、ARMv7 hard-float 或 x86_64 musl 包，并拒绝与真机架构不一致的 `SIMADMIN_TARGET_ARCH` 覆盖。
- 校验 Release SHA-256、tar 路径/文件类型、`meta.json` 架构和 ELF class/machine；ARMv7 额外校验 hard-float 标志。
- 临时目录按 `SIMADMIN_TMPDIR`、`TMPDIR`、`/tmp`、`/var/tmp` 顺序回退；`/tmp` 缺失时自动创建为 `1777`，不会依赖 `/data/local/tmp` 必然存在。
- 在 Debian / Ubuntu 上检查并增量安装所选策略真正缺少的运行依赖；已满足时跳过耗时的 apt 索引刷新。
- 启用 ModemManager 与按配置启用 NetworkManager；仅在首装、系统依赖变化、未发现 modem 或显式要求时刷新 modem。
- 安装后端二进制到 `/opt/simadmin/simadmin`。
- 安装前端到 `/opt/simadmin/www`。
- 在 AArch64/x86_64 上下载带 QMI APDU 后端的架构匹配 `lpac`，在替换旧版本前校验 `qmi` / `curl` 驱动和动态库完整性；ARMv7 MVP 跳过 lpac 安装。
- 安装并启用 `simadmin.service`。
- 安装并启用 `simadmin-modem-recovery.service`。
- 删除旧版本遗留的 NetworkManager `wwan*` unmanaged 配置，使 `nmcli` 能管理蜂窝连接。
- 应用、前端、元数据、unit 和 recovery 文件使用 staging + 原子替换；服务未通过 systemd 与 `/api/health` 检查时恢复旧文件及原启用/运行状态。
- 重复安装完全相同的内容时不替换应用文件，也不重启 SimAdmin。

当前官方构建目标为 `aarch64-unknown-linux-musl`、`armv7-unknown-linux-musleabihf` 和 `x86_64-unknown-linux-musl`。如需覆盖自动检测，可设置 `SIMADMIN_TARGET_ARCH=arm64`、`SIMADMIN_TARGET_ARCH=armv7` 或 `SIMADMIN_TARGET_ARCH=amd64`；也可以通过 `ASSET_NAME` / `ASSET_URL` 指定自定义产物。

ARMv7 MVP 暂不提供 `lpac`/eSIM 支持。前端会隐藏“工作模式”设置和 eSIM 管理 Tab，安装脚本会跳过 ARMv7 的 lpac 下载，不会把 ARM64 lpac 安装到 32 位 ARMv7 设备上；后端仍会拒绝直接 eSIM/lpac API 调用。

如需保留宿主机现有网络管理方式，可设置 `SIMADMIN_ENABLE_NETWORKMANAGER=0`；默认 `auto` 依赖模式不会为此安装或启用 NetworkManager，也不会改变已在运行的 NetworkManager 状态，此时安装器不保证 SimAdmin 的 WLAN 和蜂窝数据连接功能可用。还可以用 `SIMADMIN_INSTALL_SYSTEM_DEPS=0` 跳过 apt 依赖安装，或用 `SIMADMIN_REFRESH_MODEM_DEVICES=0` 跳过 udev 设备刷新。

---

## 访问管理后台

安装成功并运行服务后，您可以通过浏览器访问管理后台：

- **访问地址**：`http://<设备IP>:3000`
- **密码设定**：SimAdmin **未设默认初始密码**。首次访问时将自动跳转到 `/login` 的“设置管理员密码”页面，设定强密码后会自动登录并进入系统。

SimAdmin 采用单管理员登录模式，不包含多用户和权限细分系统。首次运行配置要求如下：

### 密码规则

- 8-64 个字符。
- 只能使用英文字母、数字和符号，不允许空格或中文。
- 至少包含两类字符，例如字母 + 数字、字母 + 符号或数字 + 符号。

### 关闭与调整密码保护

系统默认开启密码安全保护。如果您希望在局域网内免密直接使用，或需要修改密码的强度规则：

1. **关闭密码保护**：登录后台后，前往 「系统配置 - 安全性设置」 页面，将“密码保护”开关关闭并保存。在此之后，访问 Web 后台将不再要求输入密码。
2. **调整强度规则**：在 「系统配置 - 安全性设置」 页面中，可以自定义设置密码的最小长度（1-64 位）以及是否强制要求包含英文字母、数字和符号等强度校验规则。

### 忘记/清空密码

忘记密码时，可通过 SSH 登录目标设备后执行交互式重置：

```bash
/opt/simadmin/simadmin auth reset-password
```

如需清除管理员密码并让 Web UI 下次重新进入首次设置：

```bash
/opt/simadmin/simadmin auth clear
```

如果使用了自定义安装目录，请将 `/opt/simadmin/simadmin` 替换为实际后端二进制路径。

---

## 设备侧一键卸载

默认彻底卸载，删除服务、程序文件、前端文件、OTA 临时目录、NetworkManager 配置以及用户数据：

```bash
curl -fsSL https://raw.githubusercontent.com/3899/SimAdmin/main/uninstall.sh | sh
```

### 国内网络环境

```bash
curl -fsSL https://gh-proxy.com/https://raw.githubusercontent.com/3899/SimAdmin/main/uninstall.sh | sh
```

### 保留用户数据卸载

如需保留短信数据库和配置文件：

```bash
curl -fsSL https://raw.githubusercontent.com/3899/SimAdmin/main/uninstall.sh \
  | sh -s -- --keep-user-data
```

### 自定义环境卸载

自定义安装路径或服务名时，需要和安装时保持一致：

```bash
curl -fsSL https://raw.githubusercontent.com/3899/SimAdmin/main/uninstall.sh \
  | INSTALL_DIR=/opt/simadmin SERVICE_NAME=simadmin sh -s -- --keep-user-data
```

### 卸载脚本参数说明

| 参数 | 说明 |
|------|------|
| `--purge` | 删除全部 SimAdmin 文件和用户数据，默认行为 |
| `--keep-user-data` | 保留数据库、SQLite sidecar、`config.json`、备份和其他用户数据 |
| `--install-dir PATH` | 指定安装目录，默认 `/opt/simadmin` |
| `--service-name NAME` | 指定主服务名，默认 `simadmin` |

### 卸载脚本动作说明

- 停止并禁用 `simadmin.service`。
- 停止并禁用 `simadmin-modem-recovery.service`。
- 删除 systemd 单元文件；确有 unit/override 变化时才执行 `daemon-reload`，并清理失败状态。
- 删除 `/usr/local/bin/simadmin-modem-recovery.sh`。
- 删除 `/etc/NetworkManager/conf.d/99-simadmin-unmanaged-modem.conf`；仅当该文件实际存在且 NetworkManager 正在运行时重启它。
- 删除 SimAdmin 创建的 ModemManager debug override；仅当该文件实际存在且 ModemManager 正在运行时重启它。
- 删除 `/tmp/ota_staging`。
- 清理安装事务遗留的 `.new` / `.previous` 文件。
- 默认删除 `/opt/simadmin`、`/data/config.json`、`/data/hub-agent.db` 及其 SQLite sidecar；使用 `--keep-user-data` 时只删除受管应用文件，保留数据库、配置和备份。
- 与安装器共用 `/run/lock/simadmin-install.lock`，拒绝并发安装/卸载；重复卸载不会无意义重启 NM/MM。
- 不卸载 Debian 系统依赖，避免破坏其他程序。
