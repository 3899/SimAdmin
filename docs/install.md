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

默认下载并安装标准包 `simadmin-aarch64.tar.gz` / `simadmin-x86_64.tar.gz`，支持的 WFC 产物包 `simadmin-wfc-aarch64.tar.gz` / `simadmin-wfc-x86_64.tar.gz`：

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
| `--no-lpac` | 跳过 lpac (eSIM CLI) 的自动下载与安装 |
| `-h, --help` | 显示帮助信息 |

### 可选环境变量

```bash
curl -fsSL https://raw.githubusercontent.com/3899/SimAdmin/main/install_latest.sh \
  | REPO=3899/SimAdmin INSTALL_DIR=/opt/simadmin SERVICE_NAME=simadmin VERSION=latest WFC=1 sh
```

### 安装脚本动作说明

- 根据 `uname -m` 自动选择 GitHub Release 中的 ARM64 或 x86_64 musl 包。
- ARM64 新包不可用时会回退到历史兼容文件名 `simadmin.tar.gz`；x86_64 不会使用该 ARM64 旧包。
- 解压后校验 `meta.json` 中的架构，再替换当前程序。
- 在 Debian / Ubuntu 上自动安装 ModemManager、NetworkManager、QMI/MBIM、PC/SC、udev 和解压工具等运行依赖。
- 启用 ModemManager 与 NetworkManager，重新加载 udev 规则并触发已有 modem 设备重新识别。
- 安装后端二进制到 `/opt/simadmin/simadmin`。
- 安装前端到 `/opt/simadmin/www`。
- 下载带 QMI APDU 后端的架构匹配 `lpac`，在替换旧版本前校验 `qmi` / `curl` 驱动和动态库完整性。
- 安装并启用 `simadmin.service`。
- 安装并启用 `simadmin-modem-recovery.service`。
- 删除旧版本遗留的 NetworkManager `wwan*` unmanaged 配置，使 `nmcli` 能管理蜂窝连接。

当前官方构建目标为 `aarch64-unknown-linux-musl` 和 `x86_64-unknown-linux-musl`。如需覆盖自动检测，可设置 `SIMADMIN_TARGET_ARCH=arm64` 或 `SIMADMIN_TARGET_ARCH=amd64`；也可以通过 `ASSET_NAME` / `ASSET_URL` 指定自定义产物。

如需保留宿主机现有网络管理方式，可设置 `SIMADMIN_ENABLE_NETWORKMANAGER=0`，脚本仍会安装 `nmcli`，但不会启用 NetworkManager；此时 SimAdmin 的 WLAN 和蜂窝数据连接功能不可用。还可以用 `SIMADMIN_INSTALL_SYSTEM_DEPS=0` 跳过 apt 依赖安装，或用 `SIMADMIN_REFRESH_MODEM_DEVICES=0` 跳过 udev 设备刷新。

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
| `--keep-user-data` | 保留 `/opt/simadmin/data.db`、SQLite sidecar 文件和配置文件 |
| `--install-dir PATH` | 指定安装目录，默认 `/opt/simadmin` |
| `--service-name NAME` | 指定主服务名，默认 `simadmin` |

### 卸载脚本动作说明

- 停止并禁用 `simadmin.service`。
- 停止并禁用 `simadmin-modem-recovery.service`。
- 删除 systemd 单元文件并执行 `daemon-reload` / `reset-failed`。
- 删除 `/usr/local/bin/simadmin-modem-recovery.sh`。
- 删除 `/etc/NetworkManager/conf.d/99-simadmin-unmanaged-modem.conf`，并在 NetworkManager 运行时重启它。
- 删除 `/tmp/ota_staging`。
- 默认删除 `/opt/simadmin` 和 `/data/config.json`；使用 `--keep-user-data` 时保留用户数据。
