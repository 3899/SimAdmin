# 开发者指南

## 项目结构

```text
.
├── backend/          # Rust + Axum 后端，ModemManager、SQLite、OTA、通知、系统接口
├── frontend/         # React + Vite + MUI 前端
├── bruno-api/        # Bruno API 调试集合
├── scripts/          # 构建、部署、systemd、modem 恢复脚本
├── install_latest.sh # 设备侧一键安装 / 升级脚本
├── uninstall.sh      # 设备侧一键卸载脚本
├── VERSION           # 项目版本号
└── LICENSE           # GPLv3 许可证
```

---

## 前端开发

```bash
cd frontend
pnpm install
pnpm dev
```

### 构建前端

```bash
cd frontend
pnpm run build
```

前端构建产物输出到 `frontend/dist/`，部署后会复制为 `/opt/simadmin/www/`。

---

## 后端开发

```bash
cd backend
cargo check
cargo run -- --host :: --port 3000
```

### 参数和环境变量

| 参数 | 环境变量 | 默认值 | 说明 |
|------|----------|--------|------|
| `--host` / `-H` | `HOST` | `::` | 监听地址，默认双栈 IPv4/IPv6 |
| `--port` / `-p` | `PORT` | `3000` | HTTP 监听端口 |

在普通开发机上运行后端时，如果没有 system D-Bus、ModemManager 或 modem，硬件相关接口会返回错误，这是预期行为。

---

## 构建与部署

### 构建完整 OTA 包

```bash
./scripts/build.sh
```

#### 常用选项

```bash
./scripts/build.sh --backend-only
./scripts/build.sh --frontend-only
./scripts/build.sh --no-upx
./scripts/build.sh --no-ota
./scripts/build.sh --target=x86_64
./scripts/build.sh --target=armv7 --no-upx
```

*Windows 下建议在 WSL2 Ubuntu 中执行完整 OTA 构建。原生 PowerShell 不能直接运行 Bash 脚本；Git Bash 容易受 Node/npm/pnpm PATH 影响，完整 OTA 仍需要目标架构对应的 Linux musl 工具链：*

```bash
./scripts/build.sh --no-upx
```

ARMv7 本地后端交叉编译需要 `armv7-unknown-linux-musleabihf` Rust target 和
`arm-linux-musleabihf-gcc` linker。没有本机 ARMv7 musl 工具链时，可在安装 Docker
后使用 cross-rs（CI 使用同一目标镜像）：

```bash
rustup target add armv7-unknown-linux-musleabihf
cargo install cross --locked
cross build --locked --release --target armv7-unknown-linux-musleabihf -p simadmin
```

在没有 ARMv7 工具链的主机上，可以先运行与工具链无关的边界检查：

```bash
bash ./scripts/test-armv7.sh
```

该检查覆盖架构别名、ARMv7 OTA 产物命名、Shell 语法，以及 ARMv7 强制跳过
`lpac`（包括显式 ARM64 覆盖变量）。它不能替代 CI 交叉编译、QEMU 启动或 UFI210
真机验收。

生成 OTA 包前仍需先构建前端，然后执行
`./scripts/pack-ota.sh --target=armv7`；打包脚本会拒绝非 ARM ELF32 后端。

#### 构建脚本动作说明

- 同步 `VERSION` 到 `backend/Cargo.toml` 和 `frontend/package.json`。
- 使用 `pnpm-lock.yaml` 时通过 `pnpm install --frozen-lockfile`、`pnpm run lint` 和 `pnpm exec vite build` 构建前端到 `frontend/dist/`。
- 默认交叉编译后端到 `target/aarch64-unknown-linux-musl/release/simadmin`；传入 `--target=armv7` 时生成 `target/armv7-unknown-linux-musleabihf/release/simadmin`，传入 `--target=x86_64` 时生成 `target/x86_64-unknown-linux-musl/release/simadmin`。
- 可选使用 UPX 压缩后端二进制；未安装 UPX 时会自动跳过压缩。ARMv7 默认跳过 UPX，待 UFI210 真机验收后可通过 `SIMADMIN_ALLOW_UPX_ARMV7=1` 显式启用。
- 生成 `release/simadmin_<version>_<target>.tar.gz` OTA 包，并在 `meta.json` 中写入相同 target triple。

GitHub Actions 会构建 ARM64、ARMv7 hard-float 和 x86_64 三种产物，发布 `simadmin-aarch64.tar.gz`、`simadmin-armv7.tar.gz` 与 `simadmin-x86_64.tar.gz`。ARMv7 使用 cross-rs 交叉工具链；`simadmin.tar.gz` 继续作为 ARM64 兼容别名。

#### Windows PowerShell 构建

`build/build-simadmin.ps1` 使用 Windows 上的 Zig C 工具链和 Rust target 构建完整 OTA 包，默认目标仍为 AArch64。构建 ARMv7 hard-float 包：

```powershell
pwsh -File .\build\build-simadmin.ps1 -Target armv7
```

脚本也接受 `armv7l`、`armhf` 和完整 target triple 作为 `-Target` 值，会自动安装缺失的 Rust target，并在打包前校验 ARM ELF32 hard-float 头。ARMv7 默认输出 `release/simadmin_<version>_armv7.tar.gz`，同时生成供安装脚本使用的 `release/simadmin-armv7.tar.gz`；使用 `-NoLatestAlias` 可关闭别名。若依赖已安装，可加 `-NoInstall` 跳过 `pnpm install`。

### 通过 ADB 部署

```bash
./scripts/deploy.sh
```

#### 常用选项

```bash
./scripts/deploy.sh --backend-only
./scripts/deploy.sh --frontend-only
./scripts/deploy.sh --no-restart
./scripts/deploy.sh --target=/opt/simadmin
./scripts/deploy.sh --build-target=x86_64
./scripts/deploy.sh --build-target=armv7
```

---

## 架构与契约说明

### 登录与接口保护

- 管理后台页面和 `/api/*` 业务接口默认需要登录；`/api/health`、`/api/auth/status`、`/api/auth/setup`、`/api/auth/login` 为公开接口。
- 未登录访问受保护页面会跳转到 `/login`；前端 API 请求遇到 `401` 会自动进入登录页，直接调用 API 时返回标准 JSON 错误。
- 会话使用 `simadmin_session` HttpOnly Cookie，默认有效期 7 天。重置或清除管理员密码会清空所有 Web 会话。
- 当前不提供手动登出入口，适合单管理员设备后台场景。

### 前后端契约

- 后端模型位于 `backend/src/models.rs`。
- 前端类型位于 `frontend/src/api/contracts.ts`。
- 前端 API 封装位于 `frontend/src/api/current.ts`。
- 路由集中在 `backend/src/main.rs` 和 `frontend/src/App.tsx`。

新增接口时建议同步修改：

1. `backend/src/models.rs`
2. `backend/src/handlers.rs`
3. `backend/src/main.rs`
4. `frontend/src/api/contracts.ts`
5. `frontend/src/api/current.ts`
6. 对应页面或 hook
7. `bruno-api/` 调试请求 (详情请参阅 [Bruno 接口文档](../bruno-api/README.md))

### D-Bus 操作序列化

会改变 modem 状态的操作应通过 `with_serial` 串行执行，避免 ModemManager 或底层设备出现并发冲突：

```rust
use crate::serial::with_serial;

pub async fn set_some_modem_state(conn: &Connection) -> zbus::Result<()> {
    with_serial(async {
        // D-Bus / modem operation
        Ok(())
    }).await
}
```

### 版本注入

`backend/build.rs` 会在编译期注入：

- `APP_VERSION`
- `GIT_BRANCH`
- `GIT_COMMIT`
- `APP_TARGET_TRIPLE`

其中版本号来自根目录 `VERSION`。

---

## ModemManager D-Bus 调试

当前底层实现以 ModemManager 接口交互为主。

### 核心接口

| 接口 | 说明 |
|------|------|
| `org.freedesktop.ModemManager1` | ModemManager 根服务 |
| `org.freedesktop.ModemManager1.Modem` | Modem 状态、开关、模式、频段 |
| `org.freedesktop.ModemManager1.Modem.Modem3gpp` | 运营商、注册、扫描 |
| `org.freedesktop.ModemManager1.Modem.Simple` | 简化连接和断开 |
| `org.freedesktop.ModemManager1.Modem.Messaging` | 短信发送和接收 |
| `org.freedesktop.ModemManager1.Sim` | SIM 属性 |
| `org.freedesktop.ModemManager1.Bearer` | 数据连接 bearer |

### 常用调试命令

```bash
# 查看 modem 列表
mmcli -L

# 查看 modem 详情
mmcli -m any

# 查看注册和连接简要状态
mmcli -m any --simple-status

# 查看 3GPP 定位信息
mmcli -m any --location-get

# 查看信号指标
mmcli -m any --signal-get

# 发送 AT 指令
mmcli -m any --command='AT+CGSN'
```

### D-Bus 信号与接口监控

```bash
# 监听 ModemManager 信号
dbus-monitor --system "sender='org.freedesktop.ModemManager1'"

# 查看 modem 0 暴露的接口
busctl introspect org.freedesktop.ModemManager1 /org/freedesktop/ModemManager1/Modem/0
```
