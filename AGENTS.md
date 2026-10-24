# AGENTS.md

本文件是 AI 编码代理在本仓库工作时的执行约定；人类开发流程与更完整的规范见 [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md)。

## 1. 项目定位

GitGuard 是本地优先的 Git 推送保护工具。前端为 Vue 3 + TypeScript + Vite，桌面容器为 Tauri 2，核心逻辑位于 Rust Cargo Workspace。

当前仓库处于 **Week 1 原型阶段**，实际存在的能力只有：

- `gitguard hook pre-push --policy <allow|deny|confirm|unavailable>`：读取 Hook stdin、逐行解析 ref、调用原型策略、映射稳定退出码。
- Tauri 最小入口与一个只读的开发状态命令。
- Rust 单元测试，以及 Unix 上真实执行 `git push` 的集成测试。

**尚不存在**：SQLite 与数据模型、身份/仓库/remote 管理、正式策略配置、IPC、确认 UI、一次性批准、Hook 安装/升级/卸载、系统凭据库、日志脱敏、跨平台真实环境验收。

因此：

- 不得在 README、UI、文档或提交信息中把未实现的能力描述为已经具备。
- 不得用“原型能跑通”推断安全能力；安全边界以 [docs/SECURITY.md](docs/SECURITY.md) 为准。

## 2. 信息优先级

1. 用户当前指令。
2. [docs/SECURITY.md](docs/SECURITY.md)：任何改动不得削弱其声明的边界。
3. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)：依赖方向与模块职责。
4. 本文件与 [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md)：工程规范。
5. 现有代码与测试：实际行为。
6. [docs/WEEK-1.md](docs/WEEK-1.md)（历史周计划）与 [docs/GitGuard*MVP*开发计划.md](docs/GitGuard_MVP_开发计划.md)（路线图）：与代码冲突时以代码为准，并顺手修正文档。

## 3. 仓库地图

| 路径                       | 职责                                                                            |
| -------------------------- | ------------------------------------------------------------------------------- |
| `crates/gitguard-core`     | `Decision`、`PolicyResult`、`RefUpdate`、`GitGuardError`、`CliExitCode`         |
| `crates/gitguard-policy`   | 原型策略 `evaluate` 与 `PrototypePolicy`，纯逻辑                                |
| `crates/gitguard-cli`      | 参数校验、完整读取 stdin、逐行解析 ref、调用策略、退出码映射（bin：`gitguard`） |
| `apps/desktop/src`         | Vue 界面、i18n 语言包与状态码映射，只展示状态，不做策略判断                     |
| `apps/desktop/src-tauri`   | Tauri 入口与 commands，不实现策略                                               |
| `docs/`                    | 架构、安全、Week 1 计划、MVP 路线图                                             |
| `.github/workflows/ci.yml` | 唯一 CI 流水线                                                                  |

## 4. 环境与命令

| 目的          | 命令                                                                   |
| ------------- | ---------------------------------------------------------------------- |
| 安装依赖      | `corepack enable && pnpm install --frozen-lockfile`                    |
| 前端开发      | `pnpm dev`（Vite，固定端口 1420）                                      |
| 桌面容器      | `pnpm tauri dev`                                                       |
| 构建 CLI      | `cargo build -p gitguard-cli`                                          |
| 前端 lint     | `pnpm lint`（`--max-warnings 0`）                                      |
| 前端类型检查  | `pnpm type-check`（`vue-tsc --noEmit`）                                |
| 前端测试      | `pnpm test:frontend`（Vitest + jsdom）                                 |
| 前端生产构建  | `pnpm build`（类型检查 + `vite build`）                                |
| Rust 格式检查 | `cargo fmt --all -- --check`                                           |
| Rust lint     | `cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| Rust 测试     | `cargo test --workspace`                                               |
| 全部测试      | `pnpm test`（前端 + Rust）                                             |

`gitguard-cli` 的 Hook 集成测试带 `#![cfg(unix)]`，Windows 上会被跳过；不要因此宣称 Windows 已验证。

## 5. 架构硬约束

```text
Vue UI → Tauri commands → gitguard-core
Git pre-push Hook → gitguard-cli → gitguard-policy → gitguard-core
```

- `gitguard-core`：只放共享数据、错误与退出码；不执行 Git，不依赖 Tauri、Vue、数据库或 shell。
- `gitguard-policy`：纯策略判断；不得访问 UI/数据库、弹窗、执行命令、读取时间或随机数；无法判断时不得返回 `Allow`。
- `gitguard-cli`：只做协议层工作（参数、stdin、ref 解析、退出码映射）；不得吞掉错误返回成功，不得把未校验输入拼进 shell 命令。
- `apps/desktop`：不得实现策略；前端状态与隐藏按钮不构成安全控制，放行决定只能来自 Rust。
- 新增 crate 或调整依赖方向前，先更新 `docs/ARCHITECTURE.md` 并在改动说明中讲清理由。

## 6. 代码规范

### Rust

- 使用稳定工具链（`rust-toolchain.toml`），edition 2021，MSRV 1.88；新 crate 的 `version`、`edition`、`rust-version`、`license` 一律用 `.workspace = true` 继承。
- 提交必须通过 `rustfmt` 与 `clippy -D warnings`；不要用 `#[allow(...)]` 压制告警来“通过”检查。
- 可预期错误通过 `GitGuardError` 传递到 CLI 边界；新增错误变体必须同时更新 `exit_code_for_error` 与退出码测试。
- 外部输入禁止 `unwrap()`/`expect()`；`expect()` 只允许用于测试、启动期不可恢复错误，并在消息中说明不变量。
- 不使用 `io::Error::new(io::ErrorKind::Other, ..)`，改用 `io::Error::other(..)`。
- `reason_code`、退出码是稳定契约：改动需同步 README、`docs/ARCHITECTURE.md` 与测试，不能随提示文案漂移。
- 策略保持纯函数、输入输出显式；不要通过解析日志文本影响程序行为。

### Vue / TypeScript

- 使用 Composition API 与 `<script setup lang="ts">`；`tsconfig` 已开启 `strict`，禁止 `any`（必要时用 `unknown` 收窄）。
- 组件只负责展示与交互；业务类型与决策逻辑放在 Rust，经 Tauri command 暴露。
- 保留现有可访问性写法（语义标签、`aria-labelledby` 等）。
- 用户可见文案统一放在 `apps/desktop/src/i18n/messages`，组件通过 `useI18n()` 获取，不硬编码字符串。
- 新增或修改文案时同步补齐 `zh-CN` 与 `en-US`；`en-US` 使用 `satisfies MessageSchema` 保证结构一致。Tauri command 只返回稳定状态码，不返回面向用户的句子。
- 组件样式使用 `scoped`；不要引入未声明的 UI 框架或状态库。

### 格式与文件

- 遵循 `.editorconfig`：UTF-8、LF、行尾无空白、文件末尾换行；Rust/TOML 用 4 空格，其余默认 2 空格。
- 不要为统一缩进或排序重排无关文件，避免制造噪声 diff。
- 新增依赖需说明用途；内部 crate 通过 `[workspace.dependencies]` 引用，仅测试使用的依赖放 `dev-dependencies`。

## 7. 测试规范

- 行为变更必须带测试；退出码、fail-closed 路径、解析边界必须有回归测试。
- 单元测试放在实现文件内的 `#[cfg(test)] mod tests`，覆盖决策、`reason_code` 与错误分类。
- Hook 集成测试只允许使用 `tempfile` 临时目录中的工作仓库与本地 bare remote，并设置 `GIT_CONFIG_GLOBAL`、`GIT_CONFIG_SYSTEM` 为 `/dev/null`、`GIT_TERMINAL_PROMPT=0`。
- 调用 Git 一律使用 `Command` 的参数数组；shell 只出现在临时 Hook 脚本中，且路径必须转义。
- 断言“被拒绝”时必须同时断言远端 ref 未变化，而不是只看退出码。
- 禁止访问真实远端、网络、用户全局 Git 配置或日常仓库。
- 不得删除、跳过或放宽失败测试来让流程变绿；不得用 mock 替代真实 Git 却声称完成集成验证。

## 8. 安全红线

- 未知、非法或不可判断一律 fail-closed；只有显式 `Decision::Allow` 映射为 0。
- 不新增网络请求、云同步、遥测或后台常驻服务。
- 不记录 Token、私钥、完整环境变量或含密码的 URL；错误信息只写 `stderr`。
- 不把 Hook stdin、ref、remote、URL 或 Git 配置拼接成 shell 命令。
- 不自动安装、覆盖或删除用户仓库中的 Hook；任何 Hook 操作只在临时测试仓库中进行。
- 不把 `user.name`、`user.email` 当作远端认证身份。

## 9. 工作流程

1. 先读相关文档与相邻代码，再动手；不确定时优先搜索而不是猜测。
2. 保持最小改动，沿用周围代码的命名、注释密度与习惯写法。
3. 一个提交只处理一个逻辑变更；不要夹带格式化或无关重构。
4. 改动后先跑受影响的最小检查，结束前跑完整检查块。
5. 行为变化同步文档：命令/限制 → `README.md`；边界与退出码 → `docs/ARCHITECTURE.md`；安全保证 → `docs/SECURITY.md`；规范 → `AGENTS.md` / `docs/CONTRIBUTING.md`。
6. 除非用户明确要求，不执行 `git commit`、`git push`、`git reset --hard`、force push，也不修改 Git 配置。
7. 不修改 CI 来绕过失败；确需调整时先说明原因。

## 10. 提交信息

采用 Conventional Commits：`type(scope): 描述`。

- `type`：`feat`、`fix`、`test`、`docs`、`refactor`、`build`、`chore`、`perf`。
- `scope`：可选，取 `cli`、`policy`、`core`、`desktop`、`docs`、`ci` 等。
- 示例：`feat(cli): reject refs with malformed object ids`、`docs: record week 1 limits`。

## 11. 完成前自检

- [ ] `pnpm lint`、`pnpm type-check`、`pnpm test:frontend`、`pnpm build` 通过。
- [ ] `cargo fmt --all -- --check`、`cargo clippy ... -D warnings`、`cargo test --workspace` 通过。
- [ ] 新增或变更的公开行为有测试，拒绝路径断言了远端 ref 未变化。
- [ ] 代码放在正确的 crate，没有破坏依赖方向。
- [ ] 所有失败路径仍返回非零退出码，没有静默放行。
- [ ] README / ARCHITECTURE / SECURITY 等相关文档已同步，未声明未实现的功能。
- [ ] 没有引入未声明的依赖、秘密、网络调用或 CI 绕过。
