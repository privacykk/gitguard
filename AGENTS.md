# AGENTS.md

本文件是 AI 编码代理在本仓库工作时的执行约定；人类开发流程与更完整的规范见 [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md)。

## 1. 项目定位

GitGuard 是本地优先的 Git 推送保护工具。前端为 Vue 3 + TypeScript + Vite，桌面容器为 Tauri 2，核心逻辑位于 Rust Cargo Workspace。

当前仓库处于 **推送二次确认 MVP 阶段**（仅 macOS / Linux），实际存在的能力是：

- `gitguard hook pre-push --repository-id <uuid> <remote-name> <remote-url>`：读取 Hook stdin、逐行解析 ref、按本地配置选择确认通道（终端 / 桌面端），确认后重新校验 ref 并映射稳定退出码。`--repository-id` 是桌面端写进 Hook 脚本的登记记录 id，用于识别记录丢失；不带该参数时按仓库规范路径查记录。
- `gitguard hook pre-push --policy <allow|deny|confirm|unavailable>`：不读配置、不发 IPC 的原型调试模式。
- 终端确认：`crates/gitguard-cli/src/terminal.rs` 通过 `/dev/tty` 在当前终端展示 remote、ref 与待推送 commit，分页浏览，输入 `y`/`yes` 确认。
- 桌面端：Unix Domain Socket IPC 服务、待确认请求列表与提交预览、用户输入 `yes` 后重新校验 ref 并回 `Confirmed`（不执行 Git 命令）、仓库登记与 Hook 安装、超时设置。
- `gitguard-hook`：Hook 安装 / 卸载 / 健康检查，外来 Hook 备份为 `pre-push.gitguard-backup` 并链式调用。
- 审计与诊断：`gitguard-audit` 定义推送确认事件与统一脱敏，CLI 在推送进程内写入 `<配置目录>/audit.jsonl`（JSONL，保留 30 天 / 2000 条，写入失败只告警）；桌面端只读，提供审计页、错误码排查建议以及「复制 / 导出诊断摘要」。
- Rust 单元测试，以及 Unix 上真实执行 `git push` 的集成测试。

**尚不存在**：SQLite 与数据模型、身份/remote 管理、正式策略配置与优先级计算、一次性批准之外的授权模型、Hook 升级与完整性校验、系统凭据库、审计的防篡改与配置变更记录、跨平台（Windows）实现、三平台真实环境验收。

因此：

- 不得在 README、UI、文档或提交信息中把未实现的能力描述为已经具备。
- 不得用“原型能跑通”推断安全能力；安全边界以 [docs/SECURITY.md](docs/SECURITY.md) 为准。

## 2. 信息优先级

1. 用户当前指令。
2. [docs/SECURITY.md](docs/SECURITY.md)：任何改动不得削弱其声明的边界。
3. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)：依赖方向与模块职责。
4. 本文件与 [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md)：工程规范。
5. 现有代码与测试：实际行为。
6. [docs/develop/GitGuard-MVP-路线图.md](docs/develop/GitGuard-MVP-路线图.md)（路线图与未实现清单）与 [docs/develop/GitGuard-桌面端-信息架构与交互设计.md](docs/develop/GitGuard-桌面端-信息架构与交互设计.md)（页面结构）：与代码冲突时以代码为准，并顺手修正文档。

## 3. 仓库地图

| 路径                       | 职责                                                                                                                                                                                   |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/gitguard-core`     | `Decision`、`PolicyResult`、`RefUpdate`、推送确认请求/响应、`GitGuardError`、`CliExitCode`                                                                                             |
| `crates/gitguard-policy`   | 原型策略 `evaluate` 与 `PrototypePolicy`，纯逻辑，仅供 `--policy` 调试模式                                                                                                             |
| `crates/gitguard-config`   | 本地 JSON 配置读写（全局默认值、仓库开关、Hook 状态），原子写入，不做策略                                                                                                              |
| `crates/gitguard-ipc`      | Unix Domain Socket 的 JSON 传输层、超时与 socket 权限（`0600`），不做策略                                                                                                              |
| `crates/gitguard-hook`     | `pre-push` Hook 安装/卸载/健康检查与备份链式调用；把桌面端分配的仓库 id 写入脚本（只接受 UUID 字符集），不判断是否推送                                                                 |
| `crates/gitguard-cli`      | 参数校验（`--repository-id` / `--policy`）、完整读取 stdin、逐行解析 ref、按 id / 路径 / 全局开关判定、选择通道、写审计事件（失败只告警）、退出码映射（`gitguard`）                    |
| `crates/gitguard-audit`    | 审计事件模型与原因码、统一脱敏（URL / Token / 私钥 / 控制字符）、JSONL 追加与保留清理、诊断摘要渲染与导出；不做策略、不执行 Git                                                        |
| `crates/gitguard-git`      | 仓库根路径、Hook 目录、提交列表、ref 预校验、祖先关系判断（`Command` 参数数组，不执行 push，不做策略）                                                                                 |
| `apps/desktop/src`         | Vue 界面、侧边栏页面导航（`navigation.ts`）、i18n 语言包、状态码映射与返回值形状校验、`components/ui` 公共展示组件，只展示状态，不做策略判断                                           |
| `apps/desktop/src-tauri`   | `AppState`（含审计路径）、IPC 服务端、Tauri commands（含 `get_audit_log` / `build_diagnostics` / `export_diagnostics`）、原生菜单构建（`menu.rs`）；确认时只回 `Confirmed`，不执行 Git |
| `docs/`                    | 架构、安全、开发规范、MVP 路线图与桌面端界面规范                                                                                                                                       |
| `.vscode/settings.json`    | 工作区级编辑器配置：关闭 Vetur 校验，Vue 语言服务只走 Volar（详见 §6）                                                                                                                 |
| `.github/workflows/ci.yml` | 唯一 CI 流水线                                                                                                                                                                         |

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
Vue UI → Tauri commands → gitguard-hook / gitguard-git / gitguard-config / gitguard-ipc → gitguard-core
Git pre-push Hook → gitguard-cli → gitguard-policy → gitguard-core
Git pre-push Hook → gitguard-cli → gitguard-config / gitguard-git / gitguard-ipc / gitguard-audit
Tauri commands（只读）→ gitguard-audit → gitguard-core
gitguard-cli → /dev/tty（终端确认通道）
gitguard-cli ⇄ Unix Domain Socket ⇄ Tauri commands
```

- `gitguard-core`：只放共享数据、错误与退出码；不执行 Git，不依赖 Tauri、Vue、数据库或 shell。
- `gitguard-policy`：纯策略判断；不得访问 UI/数据库、弹窗、执行命令、读取时间或随机数；无法判断时不得返回 `Allow`。仅在 `--policy` 调试模式下使用。
- `gitguard-cli`：只做协议层工作（参数、stdin、ref 解析、选择确认通道、退出码映射）；不得吞掉错误返回成功，不得把未校验输入拼进 shell 命令；确认通过后必须重新校验 ref 并以 0 放行，让原 `git push` 完成唯一一次推送，不得自己再执行 push。
- `gitguard-cli` 的终端界面：交互只读写 `/dev/tty`（Hook stdin 已被 Git 占用），渲染前必须过滤控制字符、ANSI 与 bidi 控制符并脱敏 URL 凭据；分页、超时与未知输入都不得被视为确认。
- `gitguard-git`：只封装 Git 调用（仓库根路径、Hook 目录、提交列表、ref 预校验、祖先关系判断）；不判断策略，不拼 shell，不执行 push，统一设置 `GIT_TERMINAL_PROMPT=0`，OID 与 ref 先校验再传给 Git。
- `gitguard-ipc`：只搬运请求/响应；通信失败一律返回 `IpcError`，不得把「没有响应」当成确认。
- `gitguard-audit`：只定义审计事件、脱敏规则、JSONL 读写与诊断摘要；不判断策略、不执行 Git、不读环境变量。审计只由 CLI 写入，桌面端只读；写入或保留清理失败只由 CLI 往 `stderr` 记一行，不得改变退出码或推送结果；任何字段在写盘与展示前都必须脱敏，审计缺失、损坏或被改写都不得被当成「已确认」。
- `gitguard-hook`：只管理 Hook 文件；不得覆盖未备份的用户 Hook，不得启动常驻进程；写进脚本的仓库 id 必须校验字符集与长度，不得把未过滤的外部字符串拼进脚本。
- `apps/desktop/src-tauri`：不实现策略，也不执行 Git 命令；确认时先重新校验 ref 再回 `Confirmed`，推送由原 `git push` 完成。原生菜单（`menu.rs`）只做国际化与「设置…」导航，不得新增任何绕开 Hook 的入口。
- `apps/desktop`：不得实现策略；前端状态、快捷键与隐藏按钮不构成安全控制，放行决定只能来自 Rust。
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
- UI 技术栈固定为 Element Plus 2（`src/elementPlus.ts` 按需注册）与 Tailwind CSS 4（`src/style.css` 的 `@theme` 令牌）；不要在未更新本节前引入第二套 UI 库、状态库或路由库。
- 在模板里新增 `el-*` 组件时，必须同时在 `src/elementPlus.ts` 注册、在 `src/style.css` 的 `layer(element-plus)` 下补对应 `theme-chalk/el-*.css`（`el-icon` 在 `base.css`）；`src/elementPlus.test.ts` 会挂住未注册的标签。
- 展示结构优先复用 `src/components/ui`（`PageHeader`、`SurfaceCard`、`CardGrid`、`StatusTag`、`SettingRow` 等）；页面只组合这些组件，不重复写卡片、页头与指标布局。
- 颜色、圆角与阴影用 Tailwind 令牌（`text-ink-muted`、`border-line`、`rounded-card`、`shadow-card` 等）；Element Plus 变量已在 `:root` 对齐，不要在组件里写死十六进制颜色。
- 字号同样只用令牌：`text-meta`（13px 正文/说明）、`text-caption`（12px 辅助信息）、`text-micro`（11px 角标）、`text-title`（15px 卡片标题），其余用 Tailwind 自带的 `text-xs` / `text-sm` / `text-2xl`。不要在模板里写 `text-[0.8125rem]` 这类魔法值；需要新档位时先加到 `@theme`。
- 组件样式默认用 Tailwind 原子类；只有覆盖 Element Plus 内部结构时才写 `<style scoped>` + `:deep()`。避免使用 `el-select` / `el-checkbox` 等 JS 交互复杂的控件承载安全相关输入，确认仍必须手输 `yes`。

### 诊断与静态检查（零红灯）

- 交付前，本次改动过的文件不得残留任何编辑器诊断：ESLint 错误/警告、`vue-tsc` 类型错误、Tailwind/CSS 建议项、Rust 的 `rust-analyzer` 告警都要清零。跑完 `pnpm lint` / `pnpm type-check` 不等于结束，还要检查编辑器里实际有没有红点或黄点。
- 不允许用 `any`、`@ts-ignore`、`@ts-expect-error`、`eslint-disable`、`#[allow(...)]` 把诊断压下去。确有必要（第三方类型缺陷）时必须收紧在最小范围，并在同一行写明原因。
- 编辑器报错与 `pnpm type-check` 结论相反时，先排查语言服务本身（扩展冲突、缓存、项目配置），再改代码。本仓库的已知案例：Vetur 与 Volar 同时启用会对 `import Foo from "./Foo.vue"` 报 `has no default export`，由 `.vscode/settings.json` 关闭 Vetur 校验解决，**不要因此改动组件导出方式**。
- `src/shims-vue.d.ts` 是给不识别 `.vue` 的语言服务兜底的声明，不得删除、不得在其中写 `any`；新增全局类型声明放进同目录的 `*.d.ts`，并确保 `tsconfig.json` 的 `include` 覆盖到。
- 交付前主动用编辑器的诊断工具（`GetProblems` 或 Problems 面板）扫一遍本次改动文件及相邻被引用的文件，确认没有新增诊断。

### 格式与文件

- 遵循 `.editorconfig`：UTF-8、LF、行尾无空白、文件末尾换行；Rust/TOML 用 4 空格，其余默认 2 空格。
- 不要为统一缩进或排序重排无关文件，避免制造噪声 diff。
- 新增依赖需说明用途；内部 crate 通过 `[workspace.dependencies]` 引用，仅测试使用的依赖放 `dev-dependencies`。

## 7. 测试规范

- 行为变更必须带测试；退出码、fail-closed 路径、解析边界必须有回归测试。
- 单元测试放在实现文件内的 `#[cfg(test)] mod tests`，覆盖决策、`reason_code` 与错误分类。
- 前端测试通过 `data-*` 钩子定位元素（如 `[data-confirm]`、`[data-repo-switch]`、`[data-add-form]`），不依赖 Element Plus 内部类名；`mount` 时需同时注册 `i18n` 与 `elementPlus`（`src/elementPlus.ts`），并为 jsdom 补 `ResizeObserver` 空实现；模板里新增 `el-*` 组件时 `src/elementPlus.test.ts` 会失败提示未注册。
- Hook 集成测试只允许使用 `tempfile` 临时目录中的工作仓库与本地 bare remote，并设置 `GIT_CONFIG_GLOBAL`、`GIT_CONFIG_SYSTEM` 为 `/dev/null`、`GIT_TERMINAL_PROMPT=0`。
- 调用 Git 一律使用 `Command` 的参数数组；shell 只出现在临时 Hook 脚本中，且路径必须转义。
- 断言“被拒绝”时必须同时断言远端 ref 未变化，而不是只看退出码。
- 禁止访问真实远端、网络、用户全局 Git 配置或日常仓库。
- 不得删除、跳过或放宽失败测试来让流程变绿；不得用 mock 替代真实 Git 却声称完成集成验证。

## 8. 安全红线

- 未知、非法或不可判断一律 fail-closed；只有显式 `Decision::Allow` 或「用户确认 + ref 重新校验通过」映射为 0。
- 终端与日志输出前必须过滤控制字符、ANSI/bidi 序列，并脱敏含凭据的 URL。
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
- [ ] 改动过的文件在编辑器里没有报错或警告（含 IDE 语言服务的诊断），没有用 `any` / `@ts-ignore` / `eslint-disable` 压制问题。
- [ ] `cargo fmt --all -- --check`、`cargo clippy ... -D warnings`、`cargo test --workspace` 通过。
- [ ] 新增或变更的公开行为有测试，拒绝路径断言了远端 ref 未变化。
- [ ] 代码放在正确的 crate，没有破坏依赖方向。
- [ ] 所有失败路径仍返回非零退出码，没有静默放行；确认路径只返回 0 且不重复推送。
- [ ] README / ARCHITECTURE / SECURITY 等相关文档已同步，未声明未实现的功能。
- [ ] 没有引入未声明的依赖、秘密、网络调用或 CI 绕过。
