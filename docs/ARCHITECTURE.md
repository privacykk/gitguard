# GitGuard 架构说明（推送二次确认 MVP）

## 依赖方向

```text
Vue 3 UI（apps/desktop/src）
  ↓ invoke / event
Tauri commands（apps/desktop/src-tauri）
  ↓
gitguard-hook / gitguard-git / gitguard-config / gitguard-ipc → gitguard-core

Git pre-push Hook（shell 脚本）
  ↓
gitguard-cli ──┬→ gitguard-policy → gitguard-core        （--policy 调试模式）
               └→ gitguard-config / gitguard-git / gitguard-ipc

gitguard-cli ── Unix Domain Socket ── Tauri commands
```

硬约束：

- `gitguard-core` 只放共享数据、错误和退出码，不执行 Git，不依赖 Tauri、Vue、数据库或 shell。
- `gitguard-policy` 是纯策略函数：不访问 UI/数据库、不弹窗、不执行命令、不读时间或随机数；无法判断时不返回 `Allow`。
- `gitguard-git` 是唯一执行 Git 的地方，统一使用 `Command` 参数数组、`GIT_TERMINAL_PROMPT=0`，外部输入先校验再交给 Git。
- `gitguard-ipc` 只搬运请求与响应，不做策略判断；通信失败一律是 `IpcError`。
- `gitguard-hook` 只管理 Hook 文件，不判断是否允许推送，也不启动常驻进程。
- 桌面端与前端都不实现策略：界面状态、隐藏按钮都不构成安全控制，放行决定只来自 Rust。

## 模块职责

| 模块                     | 职责                                                                                                                                |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------- |
| `gitguard-core`          | `Decision`、`PolicyResult`、`RefUpdate`、`PushConfirmationRequest/Response`、`ConfirmationDecision`、`GitGuardError`、`CliExitCode` |
| `gitguard-policy`        | 原型策略 `evaluate`，供 `--policy` 调试模式使用                                                                                     |
| `gitguard-config`        | 本地 JSON 配置读写（全局默认值、仓库登记与开关、Hook 状态），原子写入；损坏时不覆盖                                                 |
| `gitguard-git`           | 仓库根路径、Hook 目录、提交列表、ref 预校验（`Command` 参数数组，不做策略）                                                         |
| `gitguard-ipc`           | Unix Domain Socket 服务端与客户端、JSON + 换行协议、超时、socket 权限 `0600`                                                        |
| `gitguard-hook`          | `pre-push` Hook 的安装、卸载、备份链式调用与健康检查                                                                                |
| `gitguard-cli`           | 参数校验、完整读取 stdin、逐行解析 ref、选择确认通道（终端 / 桌面端）、确认后重新校验 ref、退出码映射（bin：`gitguard`）            |
| `apps/desktop/src-tauri` | `AppState`（配置 + 待确认请求 + IPC 错误）、IPC 服务端、Tauri commands；确认时只回 `Confirmed`，不执行 Git                          |
| `apps/desktop/src`       | Vue 界面、i18n 语言包、状态码到文案的映射；只做展示与交互                                                                           |

## 确认通道选择

确认在哪里展示由运行环境自动决定，不提供配置项：

| 场景                                                    | 判据                                          | 确认方式                           |
| ------------------------------------------------------- | --------------------------------------------- | ---------------------------------- |
| 终端里执行（系统终端、VS Code / JetBrains 内置终端）    | stdout 或 stderr 是终端，且 `/dev/tty` 可打开 | CLI 在终端内直接确认，桌面端不参与 |
| 图形客户端一键推送（SourceTree、Fork、GitHub Desktop…） | 没有控制终端（输出被管道接管）                | CLI 经 IPC 请求桌面端确认          |
| CI / 未启动桌面端                                       | 没有控制终端且 IPC 不可用                     | 退出码 7，远端不变                 |

- Hook 的 stdin 已被 Git 占用（传 ref 列表），因此终端交互必须另开 `/dev/tty`；stdout/stderr 是否终端只用于判断「用户能否看到输出」，两个条件同时成立才走终端确认。
- 桌面端通道不依赖任何客户端能力：收到请求后保存为待确认请求，并把主窗口切到前台、请求系统注意力提示，避免用户看着图形客户端干等。
- 终端界面每页 30 行，`m` / `b` 翻页，`y` / `yes` 确认，`n` / `no` 取消；EOF、超时与无法识别的输入一律按「未确认」处理。
- 终端界面只展示必要信息：分支/标签名取 `refs/heads/`、`refs/tags/` 之后的简称（`branch: test (update)`），只有改名推送才显示 `local -> remote`；每条提交同时列出作者与分钟级时间；新建/更新/删除分别用绿/黄/红区分，`NO_COLOR` 或 `TERM=dumb` 时不着色。单页时不显示页码。
- 渲染前过滤控制字符、ANSI 转义与 bidi 控制符，remote URL 中的用户名/密码替换为 `***`，避免伪造界面或泄漏凭据。
- 读终端使用独立线程 + `mpsc::recv_timeout`，超时后主线程不会被阻塞，Hook 仍能按超时拒绝。

## 推送确认链路

1. `git push` 触发 `pre-push`，Git 通过参数传入 `<remote-name> <remote-url>`，通过 stdin 传入四字段 ref 行。
2. Hook 脚本执行 `gitguard hook pre-push <remote-name> <remote-url>`；脚本里没有任何「跳过确认」的开关。存在被备份的用户 Hook 时，只有 CLI 以 0 放行后才会 `exec` 它；CLI 拒绝则 `|| exit $?` 立即结束，不会绕到原 Hook。
3. CLI 完整读取 stdin，逐行解析 `local ref / local oid / remote ref / remote oid`；任意一行非法都拒绝整个请求。
4. CLI 通过 `git rev-parse --show-toplevel` 得到仓库规范路径，读取配置；该仓库显式关闭保护时直接放行（退出码 0）。
5. 否则按上一节选择确认通道：终端通道在本地渲染并等待输入；桌面端通道构造 `PushConfirmationRequest`（`uuid` v4 作为 `request_id`），经 IPC 发送并等待响应，等待上限取配置值（最少 1 秒）。
6. 桌面端收到请求后列出每个 ref 的待推送提交（列不出即回 `Error`），把它存为当前唯一待确认请求，向前端发 `push-request-received` 事件，并在最长「配置超时」内等待用户决定。
7. 前端展示仓库、remote、目标 ref、提交列表与倒计时；用户必须手工输入 `yes` 才允许确认。
8. 用户确认时，桌面端先 `verify_push_preconditions`（ref 仍指向请求中的 OID）；失败则回 `Error`，让 Hook 拒绝本次 push；通过则只回 `Confirmed`，不执行任何 Git 命令。
9. CLI 收到 `Confirmed` 后再次 `verify_push_preconditions`，通过则输出提示并以退出码 0 结束，由原本的 `git push` 继续发送数据——推送只发生一次，且发生在用户自己的终端或客户端里。

| Hook 收到的响应                 | CLI 行为                     |
| ------------------------------- | ---------------------------- |
| `Confirmed` 且 ref 重新校验通过 | 退出码 0，原 `git push` 继续 |
| `Confirmed` 但 ref 已变化       | 退出码 1，远端不变           |
| `Cancelled` / `Timeout`         | 退出码 1，远端不变           |
| `Error`                         | 退出码 1，远端不变           |
| 无响应 / 连接失败               | 退出码 7，远端不变           |
| `request_id` 不匹配             | 退出码 7，远端不变           |

## 一次性批准与并发

- 同一时刻只保存一个待确认请求；新请求到来时，旧请求按 `Cancelled` 结束。
- `request_id` 是取出凭据：取出后立即消费，重复确认或 ID 不匹配返回 `no_pending_request`。
- 若 Hook 已超时离开（响应通道关闭），桌面端的确认回执无处可达；由于桌面端不再执行 Git 命令，此时不会出现「界面显示超时但远端已被更新」。
- 超时只清理仍匹配当前 `request_id` 的请求，不会误删更新的请求。
- 确认只对本次请求的 ref 集合有效，不产生任何可复用的授权。

## 配置文件与运行文件

| 内容          | 位置                                                                                   |
| ------------- | -------------------------------------------------------------------------------------- |
| 配置（macOS） | `~/Library/Application Support/dev.gitguard.desktop/config.json`                       |
| 配置（Linux） | `$XDG_CONFIG_HOME/gitguard/config.json`（未设置时为 `~/.config/gitguard/config.json`） |
| IPC socket    | 平台配置目录下 `dev.gitguard.desktop/gitguard.sock`（权限 `0600`）                     |
| Hook 备份     | `<hooks 目录>/pre-push.gitguard-backup`                                                |

- `schema_version` 当前为 1；版本不认识或 JSON 损坏时返回错误，调用方停止写盘，绝不用默认值覆盖用户文件。
- 写入采用「同目录临时文件 + `rename`」原子替换。
- 全局默认：`default_push_confirmation = true`，`confirmation_timeout_seconds = 180`（命令层限制为 1–3600）。

## Hook 生命周期

- Hook 目录由 `git rev-parse --git-path hooks` 解析，因此尊重用户的 `core.hooksPath`。
- 安装：没有 Hook 时直接写入；已是 GitGuard 的 Hook 时按当前 CLI 路径重写；存在外来 Hook 时备份为 `pre-push.gitguard-backup` 并写入链式脚本；外来 Hook 且备份已存在时拒绝安装。
- 卸载：有备份则还原为 `pre-push`；没有备份则删除；Hook 已被其它工具替换时拒绝删除。
- 健康检查只读文件：含 GitGuard 标记为 `Healthy`，缺失为 `Missing`，无标记且有备份为 `Modified`，无标记且无备份为 `Conflict`，读不到为 `Unknown`。
- 安装与卸载只在用户通过桌面端「添加仓库」时发生，不会扫描或修改未登记的仓库。

## 仓库登记与保护开关

- 目录选择走 `@tauri-apps/plugin-dialog` 的 `open({ directory: true })`；capability 只授予 `dialog:allow-open`，对话框返回值不被信任，一律在 Rust 侧重新校验。
- `add_repository` 先校验：非绝对路径返回 `path_not_absolute`，不是目录返回 `not_a_repository`；通过后用 `git rev-parse --show-toplevel` 归一化为仓库根路径，配置里保存的是绝对路径，不保存相对路径。
- 添加时通过 `enabled` 传入该仓库的推送保护初始值，因此不必「先添加再点一次开关」。
- `toggle_repo_protection` 只改单个仓库的 `push_confirmation_enabled`；`set_default_protection` 只改全局 `default_push_confirmation`，不影响已登记仓库。
- 开关只影响 CLI 是否发起确认：关闭保护的仓库在确认前直接放行（退出码 0），其余路径仍 fail-closed。

## 界面文案与状态码

- Tauri command 只返回稳定错误码（如 `not_a_repository`、`hook_install_failed`）与 `HookStatus` 取值，不返回面向用户的句子。
- 前端在 `apps/desktop/src/statusCodes.ts` 做白名单收窄；未知错误码统一渲染 fail-closed 文案，不推断推送结果。
- command 返回值在 `apps/desktop/src/api.ts` 做形状校验；形状不完整时按「没有待确认请求」处理，不会凭空生成可确认的界面。
- 支持 `zh-CN` 与 `en-US`。初始语言优先级：本地记忆（`localStorage` 键 `gitguard.locale`）→ 系统语言 → `zh-CN`；切换后写回本地记忆并同步 `document.lang`。
- 文案与语言切换只影响展示，不改变退出码或任何放行决定。

## 错误边界

| 错误                                          | 退出码 |
| --------------------------------------------- | ------ |
| 明确允许                                      | 0      |
| 明确拒绝 / 取消 / 超时 / 确认响应为 `Error`   | 1      |
| 原型调试模式要求确认（不推送）                | 2      |
| 参数、空输入、非法行、非法 OID                | 3      |
| 策略不可用                                    | 4      |
| stdin/stderr I/O                              | 5      |
| 未分类内部错误                                | 6      |
| IPC 连接、超时、响应非法、`request_id` 不匹配 | 7      |

只有显式 `Allow` 映射为 0。可预期错误通过 `Result` 传到 CLI 边界；业务逻辑不解析日志文本，错误信息只写 `stderr`。

## 测试边界

- 策略、Hook stdin 解析、退出码映射、配置读写、Hook 生命周期、IPC 往返、桌面端命令均有单元测试。
- `gitguard-git` 与 `gitguard-hook` 的测试使用 `tempfile` 临时仓库；桌面端命令测试通过注入配置路径与临时仓库隔离真实用户配置。
- Unix 集成测试在临时工作仓库与本地 bare remote 之间真实执行 `git push`，覆盖放行、拒绝、IPC 不可用与仓库级关闭保护，并断言被拒绝时远端 ref 未变化。
- IPC 仅实现 Unix（macOS/Linux）；Windows 需要命名管道实现，相关集成测试带 `#![cfg(unix)]` 并在 Windows 跳过。
- CI（Actions 页面手动触发）在构建前端后执行完整 workspace 检查，确保 Tauri `frontendDist` 可用。
