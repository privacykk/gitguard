# GitGuard 开发规范

> 适用于日常开发与代码评审。模块边界与依赖方向见 [ARCHITECTURE.md](ARCHITECTURE.md)，安全边界见 [SECURITY.md](SECURITY.md)，AI 代理的执行约定见 [../AGENTS.md](../AGENTS.md)。
> 阶段目标与里程碑见 [develop/GitGuard-MVP-路线图.md](develop/GitGuard-MVP-路线图.md)，桌面端页面结构见 [develop/GitGuard-桌面端-信息架构与交互设计.md](develop/GitGuard-桌面端-信息架构与交互设计.md)；本文件是长期有效的工程规范。

## 1. 阶段与范围

当前处于**推送二次确认 MVP 阶段**（仅 macOS / Linux）：`pre-push` Hook、`gitguard-cli` 的配置/确认流程、Unix Domain Socket IPC、桌面端确认界面、仓库登记与 Hook 安装、推送确认审计与诊断摘要均已实现。

- 规划中但尚未实现的模块不得在代码、UI、文档中声明为可用：SQLite 与数据模型、身份/remote 管理、正式策略配置与优先级计算、Hook 升级与完整性校验、系统凭据库、审计的防篡改与配置变更记录、Windows 实现、三平台真实环境验收。
- 新增能力前先确认它属于当前阶段目标；跨阶段功能应先更新路线图再实现。
- 所有放行决定必须来自 Rust；前端只负责展示与交互。

## 2. 环境准备

| 依赖    | 版本要求                                                     |
| ------- | ------------------------------------------------------------ |
| Node.js | 22 或当前 LTS                                                |
| pnpm    | 9.2.0（通过 Corepack 固定，`packageManager` 字段）           |
| Rust    | stable，组件 `clippy`、`rustfmt`（见 `rust-toolchain.toml`） |
| MSRV    | 1.88（`Cargo.toml` 中 `rust-version`）                       |
| Git     | 2.x                                                          |
| Tauri   | 桌面容器所需的平台系统依赖                                   |

```bash
corepack enable
pnpm install --frozen-lockfile
```

- 依赖版本以 `pnpm-lock.yaml`、`Cargo.lock` 为准；不要手工编辑 lockfile。
- 修改依赖必须提交更新后的 lockfile，并与代码改动放在同一提交中。

## 3. 本地开发

```bash
pnpm dev          # 仅前端，Vite 固定 1420 端口
pnpm tauri dev    # 前端 + Tauri 桌面容器
cargo build -p gitguard-cli
```

手动试验 Hook 时，优先在一次性临时仓库中进行：

```bash
git rev-parse --git-path hooks/pre-push   # 先确认目标路径
cargo build -p gitguard-cli
```

- 真实模式（`gitguard hook pre-push --repository-id <uuid> <remote-name> <remote-url>`）读配置后自动选择确认通道：有控制终端时在终端确认，否则走 IPC 请求桌面端；`--policy` 调试模式不读配置也不发 IPC。`--repository-id` 由桌面端安装 Hook 时写入，必须是 UUID；带 id 但配置里找不到同 id 记录时按「记录丢失」处理，仍要求确认。
- 桌面端「添加仓库」会安装 Hook；已存在的用户 Hook 会先备份为 `pre-push.gitguard-backup` 并链式调用，不得改成直接覆盖。
- 不要在个人日常仓库反复试验；调试确认流程时用临时仓库，避免误触真实远端。

## 4. 分支、提交与评审

- `main` 始终保持可构建、可验证；改动通过分支与 Pull Request 合入。
- CI（`.github/workflows/ci.yml`）仅在 Actions 页面手动触发（`workflow_dispatch`），合入前应有手动运行通过的结果；不得通过删除步骤、放宽参数或跳过测试来绕过。
- 提交信息采用 Conventional Commits：

| 前缀           | 用途             |
| -------------- | ---------------- |
| `feat(scope):` | 新功能           |
| `fix(scope):`  | 修复缺陷         |
| `test(scope):` | 测试相关         |
| `docs:`        | 文档             |
| `refactor:`    | 不改变行为的重构 |
| `build:`       | 构建或工程配置   |
| `chore:`       | 杂项维护         |
| `perf:`        | 性能优化         |

- 一个提交只包含一个逻辑变更；格式化、重命名与功能改动分开提交。
- 提交前自查 `git diff`，确认没有临时文件、调试输出、密钥或无关文件。
- 评审关注：模块边界、失败路径、测试覆盖、文档同步、安全边界是否被削弱。

## 5. 代码规范

### 5.1 Rust

- 统一继承 workspace 配置：`version`、`edition`、`rust-version`、`license` 使用 `.workspace = true`；内部 crate 依赖通过 `[workspace.dependencies]` 声明。
- 必须通过 `rustfmt` 默认格式与 `clippy -D warnings`；不要用 `#[allow(...)]` 掩盖问题。
- 错误处理：
  - 可预期错误使用 `GitGuardError`（`InvalidInput` / `PolicyUnavailable` / `Io` / `Internal` / `IpcError`）。
  - 新错误变体必须同步 `exit_code_for_error` 与退出码测试，保持 README 中的退出码表有效。
  - IPC 失败必须映射为 `IpcError`（退出码 7），不得降级成拒绝以外的成功路径。
  - 外部输入不使用 `unwrap()`；`expect()` 仅用于测试或启动期不可恢复错误，消息须说明不变量。
  - 不使用 `io::Error::new(io::ErrorKind::Other, ..)`，改用 `io::Error::other(..)`。
- 不使用日志文本驱动业务行为；错误信息只写 `stderr`。
- 审计只由 CLI 在推送进程内写入；写入与保留清理失败只往 `stderr` 加一行，不得改变退出码、`Outcome` 或推送结果。写盘前与展示前必须经 `gitguard-audit` 的脱敏，不得把未脱敏的 remote URL、提交内容或环境变量写进审计与诊断。
- `Decision`、`PolicyResult.reason_code`、`CliExitCode` 是稳定契约，改动需同步文档与测试。
- 策略层保持纯逻辑：显式输入、显式输出，不依赖 UI、IO、时间或随机数。
- 新增第三方依赖需说明用途与替代方案；仅测试使用放 `dev-dependencies`。

### 5.2 Vue / TypeScript

- 使用 Composition API 与 `<script setup lang="ts">`。
- `strict` 已开启：禁止 `any`，外部数据先按 `unknown` 建模再收窄；为业务数据定义类型。
- 组件只承担展示与交互，不实现 Git 策略，也不决定是否放行。
- 保留语义化标签与可访问性属性（如 `aria-labelledby`）。
- 用户可见文案统一放在 `src/i18n/messages/{zh-CN,en-US}.ts`，组件通过 `useI18n()` 获取，不硬编码字符串。
- 新增语言或文案时同步补齐所有语言包；`en-US` 使用 `satisfies MessageSchema` 校验结构，缺少 key 会在 `pnpm type-check` 阶段失败。
- Tauri command 返回稳定错误码或枚举值而非面向用户的句子；白名单校验、未知值降级与状态码到文案的映射放在 `src/statusCodes.ts`，command 返回值的形状校验放在 `src/api.ts`。
- 形状不完整或无法识别的返回值必须降级为「没有待确认请求」或 fail-closed 文案，不得凭空构造可确认的界面。
- 前端语言切换只影响展示，不参与策略判断，也不构成安全控制。
- UI 使用 Element Plus 2 + Tailwind CSS 4：展示结构放进 `src/components/ui`（页头、卡片、栅格、状态标签等）复用，组件样式默认用 Tailwind 原子类，只在覆盖 Element Plus 内部结构时用 `scoped` + `:deep()`。新增 UI 库、状态库或路由前先更新技术栈说明。
- Element Plus 按需注册：模板新增 `el-*` 组件时同步改 `src/elementPlus.ts` 与 `src/style.css` 里的 `theme-chalk/el-*.css`，`src/elementPlus.test.ts` 会拦住漏注册。
- 颜色、字号、圆角、阴影只用令牌：文字用 `text-ink` / `text-ink-muted`，字号用 `text-meta`（13px）/ `text-caption`（12px）/ `text-micro`（11px）/ `text-title`（15px）或 Tailwind 自带档位，不要写 `text-[0.8125rem]` 这类魔法值；新档位先加到 `src/style.css` 的 `@theme`。
- `pnpm lint`、`pnpm type-check`、`pnpm build` 必须无告警通过。

### 5.3 编辑器诊断与格式

- 提交前，你改动过的文件在编辑器里必须没有报错或警告：ESLint、`vue-tsc`、Tailwind 建议项、`rust-analyzer` 都要清零。命令跑绿不等于完成，还要看 Problems 面板。
- 不允许用 `any`、`@ts-ignore`、`@ts-expect-error`、`eslint-disable`、`#[allow(...)]` 压掉诊断；确有第三方类型缺陷时，把范围收到最小并写明原因。
- 编辑器报错与 `pnpm type-check` 结论相反时，先排查语言服务再改代码。Vetur 与 Volar 同时启用会对 `import Foo from "./Foo.vue"` 报 `has no default export`，本仓库已在 `.vscode/settings.json` 关闭 Vetur 校验；不要为此改组件导出方式。
- `src/shims-vue.d.ts` 是不识别 `.vue` 的语言服务的兜底声明，不要删除，也不要在其中写 `any`；新的全局声明放进同目录 `*.d.ts` 并确保被 `tsconfig.json` 的 `include` 覆盖。

### 5.4 格式与文件

- 遵循 `.editorconfig`：UTF-8、LF、行尾无空白、文件末尾换行；`*.rs`、`*.toml` 使用 4 空格，其余默认 2 空格。
- 不整体重排无关文件；不改动与任务无关的缩进、引号或分号风格。
- 不提交构建产物与本地状态：`target/`、`node_modules/`、`dist/`、`src-tauri/gen/schemas/` 已在 `.gitignore` 中。

### 5.5 文档

- 文档使用中文，命令、路径、标识符保持英文原名，不创造新的中英混写术语。
- 每份文档职责单一：`ARCHITECTURE.md` 讲边界与流程，`SECURITY.md` 讲保证与限制，本文件讲工程规范。
- 计划类文档的复选框只在真实验证后勾选；不得把计划改写为“已完成”。
- 行为变更时同步更新：命令与限制 → `README.md`；依赖方向与退出码 → `ARCHITECTURE.md`；安全保证 → `SECURITY.md`。

## 6. 测试规范

### 6.1 基本要求

- 新增或修改的行为必须有测试；缺陷修复必须附带能复现问题的测试。
- 退出码映射、fail-closed 路径、输入解析边界属于必须回归覆盖的范围。
- 测试应断言可观察行为（退出码、`stderr` 语义、远端 ref 变化），而不是实现细节。
- 不得删除、跳过（`#[ignore]`）或放宽失败测试来让流水线变绿。

### 6.2 Rust 单元测试

- 放在实现文件内的 `#[cfg(test)] mod tests`，使用 `super::*`。
- 策略测试覆盖三种决策、`reason_code` 与无法判断时的错误返回。
- 解析测试覆盖空输入、非法行、非法 OID（长度与十六进制）、多 refs、创建与删除。
- 审计测试覆盖 URL 凭据、`KEY=VALUE` 形式的 Token / 密码、PEM 私钥与控制字符的脱敏，保留期与条数上限的清理，以及坏行计数；断言输出中不再出现原始凭据。

### 6.3 Git Hook 集成测试

- 文件为 `crates/gitguard-cli/tests/git_hook.rs`，整体 `#![cfg(unix)]`；Windows 真实环境验证仍为待办。
- 只使用 `tempfile` 创建的临时目录：一个工作仓库 + 一个本地 bare remote。
- 子进程环境固定为 `GIT_CONFIG_GLOBAL=/dev/null`、`GIT_CONFIG_SYSTEM=/dev/null`、`GIT_TERMINAL_PROMPT=0`，并通过隔离的 `HOME` / `XDG_CONFIG_HOME` 让配置落到临时目录。
- 集成测试的子进程没有控制终端，因此走的是桌面端/IPC 通道；终端确认通道（`crates/gitguard-cli/src/terminal.rs`）由内存读写模拟终端输入的单元测试覆盖，不得为了跑通测试而放宽 Hook 里的检查。
- 调用 Git 使用 `Command` 的参数数组；禁止字符串拼接 shell 命令。shell 仅出现在临时 Hook 脚本内，路径需转义。
- 断言推送被阻止时，必须同时断言远端 ref 未变化。
- 不访问真实远端、网络或用户日常仓库。

### 6.4 前端测试

- 使用 Vitest（`jsdom` 环境）与 `@vue/test-utils`。
- 优先断言用户可见结果（标题、提示文案、状态），避免依赖内部状态。

## 7. 安全检查清单

每次改动前确认：

- [ ] 未知或不可判断路径仍然 fail-closed，没有新的默认放行分支。
- [ ] 没有新增网络请求、遥测、云同步或后台常驻服务。
- [ ] 没有将 Token、私钥、环境变量或含密码 URL 写入代码、日志、错误信息或测试夹具。
- [ ] 审计与诊断输出经过统一脱敏；审计写入失败只告警，不改变退出码；审计缺失或损坏不被当成「已确认」。
- [ ] 没有把外部输入拼接为 shell 命令；外部程序调用使用参数数组。
- [ ] 没有静默安装、覆盖或删除用户仓库中的 Hook；外来 Hook 必须先备份并链式调用。
- [ ] 没有把前端隐藏、按钮状态或本地缓存当作安全控制。
- [ ] 没有把 `user.name`、`user.email` 当作远端认证身份。

## 8. 引入依赖

1. 先确认标准库或现有依赖无法满足。
2. 说明用途、维护状态、许可证与体积影响。
3. 内部 crate 使用 workspace 依赖；仅测试使用的放 `dev-dependencies`。
4. 更新并提交 lockfile，确保 `--frozen-lockfile` 与 CI 可复现。
5. 前端需要新的 Tauri 插件或系统能力时，在 `apps/desktop/src-tauri/capabilities/default.json` 只声明用到的最小权限（目录选择只需要 `dialog:allow-open`），不要整包放开。

## 9. 提交前检查

```bash
pnpm lint
pnpm type-check
pnpm test:frontend
pnpm build
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

全部通过后再提交。CI 不会在 push 或 Pull Request 时自动运行，需在 Actions 页面手动触发 `workflow_dispatch`；它在 `ubuntu-latest` 上执行同一组检查，并额外安装 Linux Tauri 依赖。

## 10. 优先级

出现取舍时按以下顺序决策：

1. 正确性
2. 安全边界
3. 可测试性
4. 模块隔离
5. 跨平台可验证性
6. UI 美观度

不要用“看起来能运行”替代真实 Git 集成测试，也不要为了进度把未实现的确认机制默认放行。
