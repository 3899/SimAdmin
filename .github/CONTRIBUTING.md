# 📝 贡献指南

非常感谢您对 `SimAdmin` 项目的关注和贡献！

为了确保项目的健壮性与高质量演进，在您提交贡献之前，请花几分钟阅读以下指南。

---

## 🏗️ 透明的开发

- `SimAdmin` 的所有工作均在 GitHub 上公开进行（[https://github.com/3899/SimAdmin](https://github.com/3899/SimAdmin)）。
- 无论是核心团队成员还是外部贡献者的 Pull Request，都将经历相同的 Review 和自动化 CI 流程。

---

## 🐛 提交 Issue

我们使用 [GitHub Issues](https://github.com/3899/SimAdmin/issues) 进行 Bug 反馈和新特性建议。

在提交 Issue 前，请遵循以下步骤：
1. **搜索已有问题**：检查是否已经有类似或已被解答的 Issue。
2. **提供复现细节**：对于 Bug 报告，请提供完整的运行环境信息（硬件架构 `aarch64` / `x86_64`、Debian/Ubuntu 版本、ModemManager/NetworkManager 版本等）以及日志与复现步骤。
3. **清晰描述期望**：对于新功能建议，请清晰指出您想要的变更以及期望的最终行为。

---

## 🚀 提交 Pull Request

### 共建流程

1. **认领或创建 Issue**：在 GitHub 上创建 Issue 并认领，或在已有的 Issue 中留言表明您正在着手处理，以避免多人重复劳动。
2. **本地分支开发**：从 `dev` 分支拉取新分支进行开发（命名推荐：`feat/xxx` 或 `fix/xxx`）。
3. **完成代码与测试**：编写代码，并完成前后端的本地编译与 Lint 校验。
4. **提交 PR**：将分支推送到您的 Fork 仓库，并向官方仓库的 `dev` 分支提交 Pull Request。

### 开发准备工作

要进行本地开发和调试，您需要准备以下环境：
- **Rust 编译环境**：后端需要 [Rust](https://www.rust-lang.org/) (2021 Edition)。
- **Node.js 与 pnpm**：前端需要 [Node.js](https://nodejs.org/) (v20+) 与 [pnpm](https://pnpm.io/)。
- **系统依赖**（Linux 开发/运行测试）：需要 `pkg-config`、`libdbus-1-dev` 等开发依赖。

### 本地编译与运行

- **前端开发调试**：
  ```bash
  cd frontend
  pnpm install
  npm run dev
  ```
- **前端打包与 Lint 校验**：
  ```bash
  cd frontend
  npm run build
  ```
- **后端检查与编译**：
  ```bash
  cd backend
  cargo check
  cargo build
  ```

---

## 🎨 代码开发规范

为了保持代码库的整洁和高可读性，请遵守以下编码规范：

1. **格式化与 Linter**：
   - 前端代码提交前需通过 ESLint 校验 (`npm run lint`)。
   - 后端 Rust 代码需使用 `rustfmt` 进行格式化 (`cargo fmt`)，并通过 `cargo clippy` 警告检查。

2. **注释与 API 契约规范**：
   - 涉及核心业务（如 D-Bus 指令交互、APN / DDNS / 短信 / eSIM / 备份恢复等）的结构体与公共 API 必须附带清晰的中文注释。
   - 前后端通信接口需同步维护 `frontend/src/api/contracts.ts` 与 `frontend/src/api/current.ts` 类型契约。

3. **系统服务与 D-Bus / SQLite 安全原则**：
   - 涉及底层系统（ModemManager / NetworkManager / systemd）交互时，需具备智能超时保护与异常容错降级，避免死锁或长时间阻塞事件循环。
   - 数据库事务（SQLite）需保持短小精悍，严格遵守“随用随开、即用即关”原则。

4. **提交信息规范**：
   - 我们推崇使用 [Conventional Commits](https://www.conventionalcommits.org/zh-hans/v1.0.0/) 规范来书写提交信息。
   - 常用前缀示例：
     - `feat:` 新增功能
     - `fix:` 修复 Bug
     - `refactor:` 代码重构（无功能、Bug 变更）
     - `perf:` 性能或体验优化
     - `docs:` 仅文档更新
     - `bump:` 版本号升级或依赖库更新
