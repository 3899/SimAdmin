<a href="https://github.com/3899/SimAdmin">
  <img src="https://socialify.git.ci/3899/SimAdmin/image?description=1&descriptionEditable=%E9%9D%A2%E5%90%91%20Debian%20%E5%B9%B3%E5%8F%B0%E6%94%AF%E6%8C%81%20SIM%2FeSIM%20%E8%9C%82%E7%AA%9D%E8%AE%BE%E5%A4%87%E7%9A%84%E5%BC%80%E6%BA%90%20Web%20%E7%AE%A1%E7%90%86%E7%B3%BB%E7%BB%9F&font=Source%20Code%20Pro&logo=https%3A%2F%2Fgithub.com%2F3899%2FSimAdmin%2Fblob%2Fmain%2Ffrontend%2Fpublic%2Fsimadmin-logo.svg%3Fraw%3Dtrue&name=1&owner=1&pattern=Floating%20Cogs&theme=Auto" alt="SimAdmin" />
</a>

<div align="center">
  <br/>

  <div>
    <a href="https://github.com/3899/">
      <img 
        alt="Debian"
        src="https://img.shields.io/badge/Debian-%23D70A53?logo=debian&logoColor=white&style=flat-square" 
      />
    </a >
    <a href="./LICENSE">
      <img
        src="https://img.shields.io/github/license/3899/SimAdmin?style=flat-square"
      />
    </a >
    <a href="https://github.com/3899/SimAdmin/releases">
      <img
        src="https://img.shields.io/github/v/release/3899/SimAdmin?style=flat-square"
      />
    </a >
    <a href="https://github.com/3899/SimAdmin/releases">
      <img
        src="https://img.shields.io/github/downloads/3899/SimAdmin/total?style=flat-square"
      />  
    </a >
  </div>

  <br/>

  <picture>
    <img src="./static/Dashboard.png" width="100%" alt="Dashboard" />
	<br/><br/>
	<img src="./static/SIM.png" width="100%" alt="SIM" />
	<br/><br/>
	<img src="./static/eSIM.png" width="100%" alt="eSIM" />
	<br/><br/>
	<img src="./static/Cellular_Network.png" width="100%" alt="Cellular_Network" />
	<br/><br/>
	<img src="./static/WLAN.png" width="100%" alt="WLAN" />
	<br/><br/>
	<img src="./static/SMS.png" width="100%" alt="SMS" />
	<br/><br/>
	<img src="./static/NotificationLogs.png" width="100%" alt="NotificationLogs" />
	<br/><br/>
	<img src="./static/NotificationRules.png" width="100%" alt="NotificationRules" />
	<br/><br/>
	<img src="./static/DeviceStatusRule.png" width="100%" alt="DeviceStatusRule" />
	<br/><br/>
	<img src="./static/NotificationChannels.png" width="100%" alt="NotificationChannels" />
	<br/><br/>
	<img src="./static/Automation.png" width="100%" alt="Automation" />
	<br/><br/>
	<img src="./static/Basic_Configuration.png" width="100%" alt="Basic_Configuration" />
	<br/><br/>
	<img src="./static/Security_Settings.png" width="100%" alt="Security_Settings" />
	<br/><br/>
	<img src="./static/DataBackup.png" width="100%" alt="DataBackup" />
	<br/><br/>
	<img src="./static/DataRecovery.png" width="100%" alt="DataRecovery" />
	<br/><br/>
	<img src="./static/Library&Snapshot.png" width="100%" alt="Library&Snapshot" />
	<br/><br/>
	<img src="./static/OTA.png" width="100%" alt="OTA" />
	<br/><br/>
	<img src="./static/Dashboard_Dark.png" width="100%" alt="Dashboard_Dark" />
	<br/><br/>
  </picture>
  
</div>

# SimAdmin - SIM/eSIM 中枢

SimAdmin 是一套面向 Debian 蜂窝 CPE、随身 WiFi、软路由类设备的 SIM/eSIM、蜂窝网络、短信、DDNS 和系统状态管理系统。

当前项目由 Rust 后端和 React 前端组成：

- 后端：Rust + Axum + zbus，主要通过 ModemManager D-Bus 接口管理 modem，并在部分场景使用 `mmcli`、`qmicli` 或 AT 直连兜底。
- 前端：React + Vite + Material UI，提供仪表盘、SIM 卡管理、蜂窝网络、设备网络、短信管理、通知中心、自动化中心和 OTA 更新页面。
- 部署形态：后端二进制同进程托管前端 SPA，默认安装到 `/opt/simadmin`，通过 systemd 运行。

健康检查整体按支持 ModemManager 的 Linux 蜂窝设备组织，不同 modem 固件、内核、ModemManager 版本暴露的能力不同，具体功能以实际设备为准。

## 📖 文档导航

*   🚀 **[安装与部署指南](./docs/install.md)**：设备一键安装/卸载、后台默认访问地址及首次管理员密码设置。
*   📜 **[版本更新记录](./docs/changelog.md)**：历史版本详细的更新说明日志。
*   ⚙️ **[运行环境与系统管理](./docs/environment.md)**：目标设备硬件与依赖指令要求、默认安装路径、eSIM 管理机制、systemd 服务维护及数据持久化。
*   🛠️ **[开发者指南](./docs/developer.md)**：项目工程结构、前端与后端开发编译、OTA 构建、ADB 部署调试及 D-Bus 接口说明。
*   🔌 **[REST API 接口文档](./bruno-api/README.md)**：详细的 REST API 路由映射表、请求/响应报文规约与 Bruno API 调试集合。

---

## 免责声明

本项目会直接操作蜂窝 modem、SIM 注册、数据拨号、APN、频段、飞行模式、NetworkManager、systemd 服务、系统重启和 OTA 文件替换；iptables/ip6tables 仅用于只读网络诊断，不会自动清空宿主机防火墙规则。

请仅在你拥有控制权的设备上使用。错误配置可能导致断网、无法注册网络、SIM 漫游计费、设备需要手动恢复，甚至 OTA 后服务无法启动。任何使用本项目造成的后果由使用者自行承担。

部分接口受硬件和 ModemManager 能力限制：

- 频段锁定依赖 ModemManager 暴露的 `SupportedBands` / `CurrentBands` / `SetCurrentBands`。
- 小区锁定当前为后端内存态展示，不会下发真实硬件锁小区命令。

## 开源协议声明

本项目采用 GNU General Public License v3.0 (GPLv3) 开源协议。

你可以：

- 自由使用、研究、修改本软件。
- 分发本软件副本。
- 分发修改后的版本。

但你必须：

1. 保留版权声明和许可证声明。
2. 分发本软件或修改版本时，以 GPLv3 协议公开完整源代码。
3. 基于本项目的衍生作品继续使用 GPLv3 协议。
4. 明确标注修改内容和修改日期。
5. 分发时附带完整 GPLv3 许可证文本。

严禁将本项目或其衍生版本闭源后作为专有软件分发。

## 社区交流

⚠️ 温馨提示：群聊仅限日常讨论和经验分享，如需反馈问题或提交新需求。

<table>
  <thead>
    <tr>
      <th width="50%">QQ 群</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td>
        <picture>
          <source media="(prefers-color-scheme: dark)" srcset="./static/Community/Community_QQ_Dark.png" />
          <source media="(prefers-color-scheme: light)" srcset="./static/Community/Community_QQ_Light.png" />
          <img src="./static/Community/Community_QQ_Light.png" />
        </picture>
      </td>
    </tr>
  </tbody>
</table>

---

## 核心功能

### Web 管理页面

| 页面 | 路由 | 说明 |
|------|------|------|
| 登录认证 | `/login` | 首次设置管理员密码、登录后台 |
| 仪表盘 | `/` | 包含在线状态、运营商、信号、网络延迟、数据/漫游/飞行模式快捷开关、系统资源、温度、流量，以及设备信息 |
| SIM 卡管理 | `/sim` | 全面展示卡状态、标识信息、解锁次数及存储路径，支持号码与短信中心行内修改；在 eSIM 模式下集成管理与写卡功能 |
| 蜂窝网络 | `/network` | 网络注册、服务小区和邻区、运营商扫描、APN、射频模式、频段锁定、小区锁定状态 |
| 设备网络 | `/device-network` | WLAN 客户端联网、无线网络扫描和连接、DDNS 动态解析配置和同步日志 |
| 短信管理 | `/sms` | 接收短信、发送短信、短信列表、会话、统计、删除对话、删除短信 |
| 通知中心 | `/notifications` | 转发日志、转发规则、转发通道、多通道测试发送 |
| 自动化中心 | `/automation` | 管理自动化任务，以及检索、筛选和清理任务执行日志 |
| 系统配置 | `/config` | 基本系统配置，包含设备运行模式设置、数据连接、漫游、飞行模式等 |
| 安全性设置 | `/config/security` | 管理员密码修改、密码策略、登录保护及会话超时等安全配置 |
| 数据备份与恢复 | `/config/backup` | 系统配置、短信、通知等 10 大类组件的打包备份与分步导入恢复 |
| OTA 更新 | `/ota` | 上传 OTA 包、在线获取 Release、验证、应用或取消更新 |

### 后端能力

- 单管理员密码登录，支持首次设置、会话 Cookie、受保护 API 拦截和 SSH 本机恢复。
- 设备信息、SIM 信息、网络注册信息读取。
- 数据连接开关和漫游策略持久化。
- 飞行模式控制。
- 数据备份与恢复：支持系统配置、短信、通知等 10 类组件打包归档、校验导入、定期清理、自动备份调度及导入前快照一键回滚。
- 基带重启流程和进度查询。
- 数据连接 watchdog，每 15 秒检查连接状态、iptables 规则数量和 modem 可用性；检测到宿主机防火墙规则时仅记录诊断日志，不自动清空规则。
- ModemManager 丢失时触发 `mmcli --scan-modems`，连续失败后重启 ModemManager。
- NetworkManager `wwan*` unmanaged 配置。
- 设备侧 WLAN 客户端连接管理，通过 NetworkManager/nmcli 扫描和连接无线局域网，WLAN 在线时优先作为设备默认出口。
- 原生 DDNS 同步，支持腾讯云 DNSPod、阿里云 AliDNS 和 Cloudflare，支持 IPv4/IPv6 独立配置、API/网卡取 IP 和变更/失败事件通知；默认通过网卡取 IP，可切换为内置多接口 API fallback。
- 短信发送、接收监听、SQLite 持久化和多渠道通知转发。
- 自动化中心双轨任务调度引擎：支持在后台并行执行自动化任务，提供`定点定时`（周几 + 具体时间，如每周二 03:00）和`间隔周期`（按分钟/小时/天，如每 180 天）调度模式。
- 多种自动化动作支持：支持`重启基带`、`安全重启系统`（支持延时）和`发送短信`（支持随机延迟、自动生成随机字符防止拦截、发送失败自动重试）。
- 自动化事件通知与运行日志管理：任务执行后写入本地 SQLite 日志表（支持关键词检索、日期筛选、手动/自动清理策略），并可将执行结果（成功/失败）作为事件实时推送至通知通道。
- APN 列表读取和 APN 修改。
- 运营商列表、扫描、手动注册、自动注册。
- eSIM 模式下按需调用 `lpac` 管理实体 eUICC SIM 卡 Profiles；普通 SIM 模式下不调用 eSIM 能力。
- 安装脚本按设备架构自动准备私有 `lpac`；OTA 包本身不绑定 `lpac` 架构或版本。
- OTA 上传、在线下载、校验、替换二进制和前端资源。

---

## 🎖️ 鸣谢

### 👥 贡献者

#### 💻 代码贡献者

<a href="https://github.com/3899/SimAdmin/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=3899/SimAdmin" alt="Contributors" />
</a>
<a href="https://github.com/nkguo" title="nkguo">
  <img src="https://wsrv.nl/?url=https://github.com/nkguo.png&w=64&h=64&fit=cover&mask=circle" alt="nkguo" />
</a>

#### 💖 特别贡献者

<a href="https://github.com/crossgg" title="crossgg">
  <img src="https://wsrv.nl/?url=https://github.com/crossgg.png&w=64&h=64&fit=cover&mask=circle" alt="crossgg" />
</a>
<a href="https://github.com/enjin1314" title="enjin1314">
  <img src="https://wsrv.nl/?url=https://github.com/enjin1314.png&w=64&h=64&fit=cover&mask=circle" alt="enjin1314" />
</a>


### 📦 参考项目

- [project-cpe](https://github.com/1orz/project-cpe)
- [SmsForwarder](https://github.com/pppscn/SmsForwarder)
- [ddns-go](https://github.com/jeessy2/ddns-go)
- [lpac](https://github.com/estkme-group/lpac)
