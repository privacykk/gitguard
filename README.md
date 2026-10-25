# GitGuard

本地优先的 Git 推送二次确认工具。当前为 **推送二次确认 MVP**：`git push` 在真正推送前会被 Hook 拦截，展示目标 ref 与待推送 commit。在终端里执行时确认信息直接打印在终端；在 SourceTree 等图形客户端里执行时由桌面端展示确认界面。确认通过后由原本的 `git push` 继续完成推送，GitGuard 不再自己执行第二次 push。

当前仅在 macOS / Linux 上可用（IPC 基于 Unix Domain Socket，Hook 脚本依赖 POSIX shell），Windows 尚未实现。

## 环境要求

- Node.js 22 或兼容的当前 LTS
- pnpm 9.2.0（通过 Corepack 固定）
- Rust stable，以及 `rustfmt`、`clippy`
- Git 2.x
- 运行 Tauri 桌面容器还需要对应平台的[系统依赖](https://v2.tauri.app/start/prerequisites/)

## 从零启动

```bash
# 启用 Corepack，让 pnpm 按 package.json 固定为 9.2.0
corepack enable
# 严格按 pnpm-lock.yaml 安装前端依赖，锁文件与 package.json 不一致时直接报错（与 CI 一致）
pnpm install --frozen-lockfile
# 构建 CLI：桌面端安装 Hook 时需要知道 `gitguard` 的绝对路径
cargo build -p gitguard-cli
# 启动 Tauri 桌面容器（beforeDevCommand 会自动拉起 1420 端口的前端）
pnpm tauri dev
```

桌面端按以下顺序寻找 CLI，找不到就拒绝安装 Hook（不会写入猜测的路径）：

1. 环境变量 `GITGUARD_CLI_PATH` 指向的绝对路径文件；
2. 与桌面端可执行文件同目录的 `gitguard`（开发时为 `target/debug/gitguard`）。

只调试前端时可以用 `pnpm dev`（http://localhost:1420）；此时没有 Tauri 后端，页面只读并提示推送仍会被拒绝。

## 使用流程

1. 在桌面端「仓库管理」点击「选择文件夹…」（或直接填写绝对路径），再用「添加后启用推送保护」复选框决定初始状态，然后添加。GitGuard 会安装 `pre-push` Hook，并把本次登记的仓库 id 写进脚本；若已存在其它 `pre-push`，先备份为 `pre-push.gitguard-backup`，再写入链式调用脚本。对已登记的路径，添加只刷新 Hook 脚本，开关与仓库 id 沿用原记录。
2. 在该仓库执行 `git push`。确认通道按运行环境自动选择：
   - **在终端里执行**：Hook 直接在当前终端列出 remote、目标分支与待推送 commit（含作者与提交时间）；分支名使用 `git branch` 里的简称（如 `branch: test`，改名推送才显示 `local -> remote`），输入 `y`/`yes` 才放行；内容超过一屏时分页显示。每个 ref 会标出 `(create)`、`(update)`、`(rewrite)` 或 `(delete)`。
   - **在 SourceTree、Fork、VS Code 等图形客户端里执行**：Hook 进程没有控制终端，改为通过本地 Unix Domain Socket 请求桌面端确认。
3. 桌面端展示 remote 名称与 URL、目标 ref、待推送 commit 列表和倒计时，非快进更新标为改写风险；必须手工输入 `yes` 才能确认。
4. 确认后 CLI 重新校验 ref 未变化，再以退出码 0 放行，由原本的 `git push` 继续推送（终端或客户端里能看到真实的推送进度与凭据交互）；取消或超时则本次 push 被拒绝，远端不变。

终端确认界面的操作方式：

| 输入        | 作用                       |
| ----------- | -------------------------- |
| `y` / `yes` | 确认本次推送               |
| `n` / `no`  | 取消本次推送（等价于拒绝） |
| `m`         | 下一页（每页 30 行）       |
| `b`         | 上一页                     |

等待确认的上限与桌面端相同，取配置里的 `confirmation_timeout_seconds`；超时、输入流结束（EOF）或无法识别的输入都不会被当作确认。终端界面用颜色区分关键信息：分支名、提交标题与键位加粗，新建绿、更新黄、改写与删除红，标签与时间用暗色；设置 `NO_COLOR` 或 `TERM=dumb` 时自动退化为纯文本。非快进更新（`--force`、`--force-with-lease` 或 refspec 里的 `+`）标为 `(rewrite)`，提示若远端接受本次推送，远端已有但本地没有的提交会被丢弃；tag 更新一律按改写展示。祖先关系无法判断时（对象存在但不是 commit、Git 比对失败）也按改写展示；远端对象在本地缺失时提交列表本身就列不出，按既有规则拒绝本次推送，不会展示标记。该标记是客户端根据祖先关系的推断，不能区分 `--force` 与普通分叉推送，只影响提示，不改变放行判断。终端里展示的分支名、commit 主题与 remote URL 会先过滤控制字符与 ANSI 转义序列，URL 中的用户名/密码替换为 `***`，避免伪造界面或泄漏凭据。

图形客户端不会显示 Hook 的输出，因此点击「推送」后客户端可能看起来停在「推送中」：此时应切到 GitGuard 桌面端确认或取消。有新请求时 GitGuard 会把窗口切到前台并请求系统注意力提示，超时后客户端会收到钩子失败的结果。

保护是「按仓库开启」的：只有你在桌面端添加过的仓库才会被拦截，未添加的仓库默认放行，安装 GitGuard 不会把其它仓库的 push 一起卡住。界面因此分成两层：

- 「仓库管理」里每条记录各有一个推送保护开关，只控制该仓库；
- 「安全策略」里的「未添加的仓库也要确认推送」是末位默认值，只对没有添加过的仓库生效，不会覆盖已添加仓库的开关；
- 「设置」里的确认超时对终端与桌面端两个通道都生效。

关闭保护的仓库由 CLI 直接放行（不弹确认）。

Hook 脚本里带着登记时的仓库 id，用来确认「这条 Hook 属于哪次登记」。如果配置文件被删除或回滚，导致脚本里的 id 找不到对应记录，CLI 仍会要求确认并提示重新登记，不会因为「查不到配置」而直接放行。

### 从旧版本升级

旧版本写进仓库的 Hook 脚本里含一个跳过判断：设置 `GITGUARD_CONTROLLED_PUSH=1` 时脚本不调用 CLI、直接放行。本版本写出的脚本与 CLI 都不再识别该变量，但已安装的脚本不会自动更新——请在桌面端对每个受保护仓库重新添加一次：同一路径只重写 Hook 脚本，开关沿用列表里的状态，同时脚本会带上该仓库的 id，旧脚本里的跳过判断才会消失。旧脚本不带 id，GitGuard 无法检测它的配置记录是否还在，只能在记录缺失时回落到全局开关（默认关闭），所以升级后重新添加是必须的。Hook 版本与完整性校验仍不在当前范围内，GitGuard 不会主动发现旧脚本。

配置里的 `default_push_confirmation` 已改名为 `require_confirmation_for_unregistered_repositories`（只对未添加的仓库生效）；旧键仍能读入，下次保存时改写为新键。旧值会被保留：如果它原本是 `true`，升级后「未添加的仓库」仍会被拦截，想改成只保护已添加的仓库，在设置页把这个全局开关关掉即可。

## 桌面端界面

左侧边栏按产品信息架构组织，当前页由前端状态维护（`apps/desktop/src/navigation.ts`），不引入路由库。界面使用 Element Plus 2 与 Tailwind CSS 4，重复的页头、卡片、栅格与状态标签集中在 `apps/desktop/src/components/ui` 复用：

| 页面        | 内容                                                            | 状态                     |
| ----------- | --------------------------------------------------------------- | ------------------------ |
| 01 总览     | 已登记/已启用仓库数、待确认请求、配置与 IPC 健康、Hook 状态汇总 | 已实现                   |
| 02 身份管理 | Git 身份与凭据管理的规划边界说明                                | 规划中，无对应后端能力   |
| 03 仓库管理 | 选择目录登记仓库、逐仓库推送保护开关、Hook 健康状态             | 已实现                   |
| 04 安全策略 | 未添加仓库的全局默认开关、判定顺序、只读的按仓库汇总            | 部分实现（无优先级计算） |
| 05 推送确认 | 推送目标 ref、remote、待推送 commit、倒计时与手工输入 `yes`     | 已实现                   |
| 06 操作日志 | 推送确认事件、保留策略、脱敏诊断摘要与导出                      | 已实现                   |
| 设置        | 界面语言、确认超时、快捷键、配置文件路径、关于                  | 已实现                   |

- 新的待确认请求会直接把界面切到「推送确认」；如果当时停在其它页面，顶部会留一条横幅提示，错过请求也会看到。
- 侧边栏可以收起成只留图标的窄条，为内容区让出宽度；选择会记住（`localStorage` 的 `gitguard.sidebarCollapsed`）。收起只影响观感与导航展开，不影响任何 command 与放行判断。
- 内容区随窗口宽度自适应：内容在 `80rem` 宽度内居中，卡片栅格按可用宽度从单列增加到多列；窄窗下侧边栏保持固定宽度，可手动收起为图标窄条给内容让位。
- 「设置」可以从原生菜单「设置…」或快捷键打开：macOS 是 `⌘,`，其它平台是 `Ctrl+,`；`Esc` 关闭设置并回到上一页。快捷键只负责导航，不构成任何安全控制。
- 原生菜单（关于 / 服务 / 隐藏 / 编辑 / 视图 / 窗口）跟随界面语言重建：前端调用 `set_app_menu(locale)`，菜单触发时通过 `open-settings` 事件通知前端。
- 「身份管理」仍是占位页：后端尚无对应能力，页面明确标注「规划中」，不展示伪造数据，也不作为任何安全保证。
- 「操作日志」读取 CLI 写入本机的审计事件（`audit.jsonl`），展示事件类型、结果、原因码、确认通道、仓库与 remote，并可复制或导出诊断摘要；页面只读，审计由 CLI 在推送时写入。

## CLI 用法

真实 Hook 模式（Git 调用 Hook 时会自动追加 remote 参数）：

```bash
# Hook 脚本里的写法：--repository-id 由桌面端安装 Hook 时写入；
# 两个位置参数由 Git 提供，CLI 只校验和展示，不会执行它们
gitguard hook pre-push --repository-id <uuid> <remote-name> <remote-url>
```

`--repository-id` 必须是 UUID，否则按参数错误退出；它是「这条 Hook 属于哪次登记」的标记：带 id 但配置里找不到同 id 记录时按「记录丢失」处理，仍要求确认并提示重新登记；不带 id 视作旧版脚本，按仓库规范路径查记录，查不到才回落全局开关。

stdin 每行四字段 `<local ref> <local oid> <remote ref> <remote oid>`；oid 须为 40 位（SHA-1）或 64 位（SHA-256）十六进制，删除分支时 local ref 为 `(delete)`、local oid 全 0。

原型调试模式不读配置、不发 IPC，显式指定 `--policy` 的四种取值之一（`allow`、`deny`、`confirm`、`unavailable`），可用于排查 Hook 与解析问题：

```bash
./target/debug/gitguard hook pre-push --policy allow <<'EOF'
refs/heads/main aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa refs/heads/main bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
EOF
```

## 退出码

| 退出码 | 含义                                                 |
| ------ | ---------------------------------------------------- |
| 0      | 明确允许，或确认通过且 ref 重新校验后放行            |
| 1      | 明确拒绝（含用户取消、确认超时、确认响应为 Error）   |
| 2      | 原型调试模式下的「需要确认」（真实模式不会返回该码） |
| 3      | 参数或 Hook stdin 非法                               |
| 4      | 策略不可用                                           |
| 5      | stdin/stderr I/O 错误                                |
| 6      | 内部错误                                             |
| 7      | IPC 连接、超时、响应非法或 `request_id` 不匹配       |

只有「显式允许」或「用户确认且 ref 重新校验通过」返回 0，其余全部非零。确认后 GitGuard 不再执行 push，因此不需要用非零退出码阻止 Git 重复推送：数据由原本的那次 `git push` 发送。错误写入 `stderr`，程序行为不依赖日志文本。

## 配置文件与运行文件

| 内容       | 位置                                                                                                                    |
| ---------- | ----------------------------------------------------------------------------------------------------------------------- |
| 配置       | macOS：`~/Library/Application Support/dev.gitguard.desktop/config.json`；Linux：`$XDG_CONFIG_HOME/gitguard/config.json` |
| IPC socket | 平台配置目录下 `dev.gitguard.desktop/gitguard.sock`（权限 `0600`）                                                      |
| Hook 备份  | `<hooks 目录>/pre-push.gitguard-backup`                                                                                 |
| 审计日志   | 与配置文件同目录的 `audit.jsonl`（JSONL，权限 `0600`）                                                                  |
| 诊断摘要   | 导出时写入配置文件同目录的 `diagnostics-<毫秒时间戳>.txt`（权限 `0600`）                                                |

配置文件采用「同目录临时文件 + rename」原子写入；JSON 损坏或 `schema_version` 不认识时，桌面端停止写盘并提示，不会用默认值覆盖原文件。

## 审计与诊断

- 审计只由 CLI 写入：进入确认流程时写一条 `request_created`，随后写本次结果（`confirmed` / `cancelled` / `timeout` / `failure`），字段包括时间（Unix 秒）、事件、结果、稳定原因码、确认通道、仓库路径、仓库 id、remote 名称与 URL、`request_id` 与 ref 数量。桌面端只读，不补记，避免同一次推送出现两条互相矛盾的记录。
- 写入前统一脱敏：URL 中的用户名与密码、`access_token` 等敏感查询参数、`KEY=VALUE` 形式的 Token / 密码 / 私钥、常见 Token 前缀（`ghp_`、`xoxb-` 等）以及 PEM 私钥整段都会被替换；控制字符、ANSI 转义与 bidi 控制符替换为空格，单字段截断到 512 字符。
- 保留策略：最多 30 天 / 2000 条，每次写入后按「同目录临时文件 + rename」清理一次；没有需要删除的内容时不重写文件。
- 审计写入或清理失败只在 `stderr` 输出一行提示，不改变退出码，也不会把「没写成审计」当成放行或拒绝的理由。
- 「操作日志」页读取尾部事件（最多 200 条，最新在上）；坏行只计入「跳过的行」，不对内容做猜测。
- 「复制诊断摘要」/「导出诊断摘要」由 Rust 侧渲染并脱敏，内容包含应用版本、平台、配置与审计路径、配置 / IPC 错误码、仓库计数、最近 10 条事件与前端最近一次错误码；不包含提交内容、环境变量或凭据。

审计文件是本机普通文件，同一用户下的其它进程可以改写，因此它不是防篡改凭证，界面内容也不能代替 Git 的推送记录。

## 检查与测试

```bash
# 前端：ESLint，--max-warnings 0（warning 也判定为失败）
pnpm lint
# 前端：vue-tsc --noEmit 类型检查
pnpm type-check
# 前端：Vitest + jsdom 单元测试
pnpm test:frontend
# 前端：类型检查 + vite 生产构建
pnpm build

# Rust：格式检查（只检查，不修改文件）
cargo fmt --all -- --check
# Rust：lint，告警视为错误
cargo clippy --workspace --all-targets --all-features -- -D warnings
# Rust：单元测试 + Unix 上的 Hook 集成测试
cargo test --workspace
```

`gitguard-cli` 的 Unix 集成测试会创建临时工作仓库和本地 bare remote，安装仅属于该临时仓库的 Hook，并真实执行 `git push`。测试覆盖：

- 调试模式的允许、拒绝、待确认、策略不可用；
- 确认流程不可用（桌面端未运行且没有控制终端）时的阻断与退出码 7；
- 用户取消、超时、Hook 执行失败后的推送阻断，以及远端 ref 不变；
- 仓库级关闭保护直接放行（退出码 0），以及 Hook 记录丢失、Hook 记录的 id 与配置不一致时的阻断；
- 多 refs、创建分支、删除分支；
- 空输入、非法输入、非法 UTF-8 和 stdin 读取错误；
- 终端确认界面的确认/取消、分页、EOF、控制字符转义与 URL 脱敏（`gitguard-cli` 单元测试）。

测试不访问真实远端，不读取用户全局或系统 Git 配置（`GIT_CONFIG_GLOBAL` / `GIT_CONFIG_SYSTEM` 指向 `/dev/null`），也不修改日常仓库。Windows 上 IPC 与 shell Hook 尚未实现，相关集成测试带 `#![cfg(unix)]`，会被跳过；Rust 单元测试仍在各平台执行。

## 界面语言

桌面界面支持简体中文（`zh-CN`）与英文（`en-US`），使用 `vue-i18n` 实现：

- 初始语言优先级：本地记忆（`localStorage` 键 `gitguard.locale`）→ 系统语言 → 简体中文；在设置页（侧边栏底部「设置」或 `⌘,` / `Ctrl+,`）可手动切换。
- 原生菜单栏（关于 / 设置 / 编辑 / 视图 / 窗口）也跟随该语言重建，英文界面不会残留中文菜单，反之亦然。
- 文案集中在 `apps/desktop/src/i18n/messages`，`en-US` 用 `satisfies MessageSchema` 保证与 `zh-CN` 结构一致；新增语言需同时补齐对应语言包。
- Tauri command 只返回稳定错误码（如 `not_a_repository`）与 `HookStatus` 取值，展示文案由前端按当前语言渲染；未知状态码会显示 fail-closed 文案，不代表推送已被放行。
- 语言切换只影响展示，不改变 CLI 退出码或任何放行决定。

## 参与开发

- [AGENTS.md](AGENTS.md)：AI 编码代理的执行约定、仓库地图与自检清单。
- [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md)：开发规范，包含环境、提交信息、代码规范、测试要求与安全清单。
- [docs/develop/GitGuard-MVP-路线图.md](docs/develop/GitGuard-MVP-路线图.md)：阶段目标、里程碑、测试矩阵与未实现清单；桌面端页面结构见 [界面规范](docs/develop/GitGuard-桌面端-信息架构与交互设计.md)。
- [docs/develop/GitGuard-待办清单.md](docs/develop/GitGuard-待办清单.md)：路线图的派生快照，只记当前实现状态与待办顺序。

## 工程边界与已知限制

模块职责与链路见 [ARCHITECTURE.md](docs/ARCHITECTURE.md)，安全边界见 [SECURITY.md](docs/SECURITY.md)，开发规范见 [CONTRIBUTING.md](docs/CONTRIBUTING.md)。

当前明确未完成：SQLite 与数据模型、身份/remote 管理、正式策略配置与优先级计算、Hook 升级与完整性校验、系统凭据库、审计的防篡改与远地汇聚（当前只记推送确认事件、不记配置变更）、多 push URL 与 URL rewrite 验证、Windows 支持、三平台真实环境验收。以上能力不得由当前 MVP 推断为已经提供。
