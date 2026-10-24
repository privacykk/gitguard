# GitGuard

本地优先的 Git 推送策略工具。当前为 **Week 1 工程与 `pre-push` Hook 原型**，不是完整安全产品。

## 环境要求

- Node.js 22 或兼容的当前 LTS
- pnpm 9.2.0（通过 Corepack 固定）
- Rust stable，以及 `rustfmt`、`clippy`
- Git 2.x
- 运行 Tauri 桌面容器还需要对应平台的[系统依赖](https://v2.tauri.app/start/prerequisites/)

## 从零启动

```bash
corepack enable
pnpm install --frozen-lockfile
pnpm dev
```

在另一个终端启动 Tauri 桌面容器：

```bash
pnpm tauri dev
```

仅启动 Rust CLI：

```bash
cargo build -p gitguard-cli
./target/debug/gitguard hook pre-push --policy allow <<'EOF'
refs/heads/main aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa refs/heads/main bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
EOF
```

原型必须显式指定 `--policy`：`allow`、`deny`、`confirm` 或 `unavailable`。除 `allow` 外均不会放行。`confirm` 在确认服务尚未实现时固定拒绝，`unavailable` 用于验证策略故障时的 fail-closed 行为。

## 检查与测试

```bash
pnpm lint
pnpm type-check
pnpm test:frontend
pnpm build
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

`gitguard-cli` 的 Unix 集成测试会创建临时工作仓库和本地 bare remote，安装仅属于该临时仓库的 Hook，并真实执行 `git push`。测试覆盖：

- 明确允许后的普通推送；
- 拒绝、待确认、策略不可用后的推送阻断和远端 ref 不变；
- 多 refs、创建分支、删除分支；
- CLI 不存在及 Hook 执行失败时的阻断；
- 空输入、非法输入、非法 UTF-8 和 stdin 读取错误。

测试不访问真实远端，不读取用户全局或系统 Git 配置，也不修改日常仓库。Windows 的 shell Hook 集成测试当前不会运行，已登记为跨平台后续工作；Rust 单元测试仍在各平台执行。

## 参与开发

- [AGENTS.md](AGENTS.md)：AI 编码代理的执行约定、仓库地图与自检清单。
- [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md)：开发规范，包含环境、提交信息、代码规范、测试要求与安全清单。

## CLI 退出码

| 退出码 | 含义                       |
| ------ | -------------------------- |
| 0      | 明确允许                   |
| 1      | 明确拒绝                   |
| 2      | 需要确认，但确认服务不可用 |
| 3      | 参数或 Hook stdin 非法     |
| 4      | 策略不可用                 |
| 5      | stdin/stderr I/O 错误      |
| 6      | 内部错误                   |

错误写入 `stderr`，程序行为不依赖日志文本。

## 原型 Hook

```sh
#!/bin/sh
exec /absolute/path/to/gitguard hook pre-push --policy allow "$@"
```

**只应在临时测试仓库手动安装。** 本周没有 Hook 安装器，不会备份、覆盖、升级或恢复已有 Hook。请先用 `git rev-parse --git-path hooks/pre-push` 检查目标路径，避免覆盖已有文件。CLI 不存在或执行失败时，Git 会因 Hook 非零退出而终止 push。

## 工程边界与已知限制

模块职责见 [ARCHITECTURE.md](docs/ARCHITECTURE.md)，安全边界见 [SECURITY.md](docs/SECURITY.md)，开发规范见 [CONTRIBUTING.md](docs/CONTRIBUTING.md)，完整计划见 [WEEK-1.md](docs/WEEK-1.md)。

本周明确未完成：SQLite、身份与仓库管理、正式策略配置、IPC、确认 UI、一次性批准、Hook 安装/恢复、多 push URL 与 URL rewrite 验证、Windows/macOS/Linux 全平台真实环境验收、系统凭据库。以上功能不得由当前原型推断为已经提供。

## 里程碑提交

源码和文档已准备为 Week 1 里程碑，但是否提交及提交信息由开发者决定；自动化执行不会擅自创建 Git commit。
