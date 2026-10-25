# GitGuard 桌面端信息架构与交互设计

本文描述桌面端（`apps/desktop`）的页面结构、交互规则与实现方案。范围只覆盖**当前仓库真正具备的能力**；未实现的页面按「规划中」呈现，不把计划写成现状。安全边界以 [docs/SECURITY.md](../SECURITY.md) 为准，依赖方向以 [docs/ARCHITECTURE.md](../ARCHITECTURE.md) 为准。

## 1. 目标与约束

目标：

1. 把原先「一页同时放推送确认与仓库设置」的界面拆成可预期的导航结构，覆盖总览、身份、仓库、策略、确认、日志六块规划，以及应用设置。
2. 导航、页面标题、菜单与快捷键全部走 i18n（`zh-CN` / `en-US`），不出现硬编码文案。
3. 设置页可通过 macOS 通用的 `⌘,`、其它系统的 `Ctrl+,` 快速打开；原生菜单同步切换语言。

约束：

- 前端只展示状态与收集输入，**不实现策略**；放行决定只来自 Rust 与 CLI。
- 未实现的能力不得在界面、菜单或文档里被描述为已具备。
- 不新增网络请求、云同步、遥测、后台常驻进程或数据库。
- UI 技术栈固定为 Element Plus 2 + Tailwind CSS 4（Vue 3 + Composition API），不再引入其它 UI 框架、状态库或路由库。

## 2. 信息架构

```text
GitGuard
├── 总览 Dashboard
├── 身份管理 Identities          （规划中）
├── 仓库管理 Repositories
├── 安全策略 Policies
├── 推送确认 Approval
├── 操作日志 Audit               （规划中）
└── 设置 Settings                （侧边栏底部，⌘, / Ctrl+,）
```

导航是单窗口内的视图切换（`AppView` 联合类型 + `ref`），不引入 vue-router：

- 侧边栏常驻六个业务页面；设置放在底部，与业务页面区分。
- 当前页使用 `aria-current="page"` 标记；待确认请求存在时，「推送确认」显示提示点，并提供可读文本。
- 设置页记录来源页，`Esc` 或「返回」回到来源页。

数据来源全部是现有 Tauri command：

| 页面     | 数据/命令                                                                                    |
| -------- | -------------------------------------------------------------------------------------------- |
| 总览     | `get_settings`、`get_pending_push_request`、`push-request-received` 事件                     |
| 身份管理 | 无（只展示规划与安全边界）                                                                   |
| 仓库管理 | `get_settings`、`add_repository`、`toggle_repo_protection`、`@tauri-apps/plugin-dialog`      |
| 安全策略 | `get_settings`、`set_unregistered_protection`                                                |
| 推送确认 | `get_pending_push_request`、`confirm_push`、`cancel_push`、`push-request-received`           |
| 操作日志 | 无（当前不持久化审计）                                                                       |
| 设置     | `get_settings`、`set_confirmation_timeout`、`set_app_menu`（新增）、`open-settings` 菜单事件 |

## 3. 布局与视觉基线

- 左侧侧边栏宽 15rem，可收起为 4.75rem 的图标窄条；右侧内容区上限 80rem 并在剩余空间居中（窗口宽时不把内容挤在左侧）。
- 卡片统一进 `.grid`（`auto-fit` + 20rem 最小宽度）：宽屏最多三列，中宽两列，窄窗单列；窄窗下侧边栏保持固定宽度，不会降级为顶部横向导航，需要更多横向空间时手动收起为图标窄条。
- 间距使用 0.25rem 基准；卡片圆角 0.75rem；信息层级靠字重与颜色区分，不靠大色块。
- 状态色语义固定：绿色=正常/已启用，黄色=待确认/需注意，红色=失败/冲突，蓝色=可操作。
- 所有交互元素保留键盘可达的 `focus-visible` 轮廓；开关使用 `role="switch"` + `aria-checked`。

## 4. 页面规格

### 4.1 总览

目的：打开应用 5 秒内回答「保护是否在生效、有没有待处理问题」。

- 页面标题 + 状态徽标：保护运行中 / 有待处理的问题 / 尚未启用任何保护 / 只读模式。
- 四张指标卡：已登记仓库数、已启用保护数、待确认请求（0/1）、未登记仓库拦截（开/关）。
- 「运行状态」：配置文件是否可读、桌面端 IPC 是否运行；错误直接展示稳定错误码对应文案。
- 「Hook 检查」：汇总所有仓库的 `HookStatus`；`Healthy` 之外都列为需处理，`Unknown` 按 fail-closed 文案呈现。
- 「已登记仓库」：最多展示 3 条（路径 + Hook 状态 + 保护开关状态），其余前往仓库管理。
- 快捷动作：添加仓库、调整安全策略、查看确认流程。

空态与异常：

- 无仓库：提示「添加后才会安装 Hook」，指标归零，不伪造健康状态。
- 配置读取失败：总览顶部展示 fail-closed 错误，状态徽标为「有待处理的问题」。
- 浏览器直接打开 Vite：显示只读提示，所有指标不可操作。

### 4.2 身份管理（规划中）

目的：说明身份能力尚未实现，同时讲清安全边界，避免用户误以为 GitGuard 管理凭据。

- 顶部「规划中」徽标 + 明确声明：不创建/保存/切换 Git 身份，不读取系统凭据。
- 安全边界卡片：不使用 `user.name` / `user.email` 作为远端认证身份；不保存 Token、私钥或含密码的 URL。
- 三张规划卡片（创建与编辑身份、按仓库绑定身份、身份优先级），统一标记「规划中」，不可点击。

### 4.3 仓库管理

目的：登记仓库、安装 Hook、按仓库开关保护。

- 已保护仓库列表：路径、Hook 状态（`Healthy` 绿、`Modified`/`Conflict` 红、`Missing` 中性）、推送保护开关。
- 添加表单：绝对路径输入 + 系统文件夹选择器 + 「添加后启用推送保护」复选框。
  - 复选框默认不勾选，且**不跟随**全局「未登记仓库」设置，避免两个开关互相影响。
  - 文件夹选择取消时保留输入框内容。
- 说明：已有外来 Hook 会备份为 `pre-push.gitguard-backup` 后再写链式脚本。

禁止：在前端推断 Hook 是否正常；未知状态一律按 fail-closed 文案显示。

### 4.4 安全策略

目的：调整「哪些仓库需要确认」，并解释判定顺序。

- 全局默认：一个开关（未登记仓库也要确认推送），调用 `set_unregistered_protection`，只影响未登记仓库。
- 判定顺序（只读说明）：已登记仓库看自己的开关 → 未登记仓库看全局默认 → 无法判断一律拒绝。
- 按仓库汇总（只读）：路径 + 该仓库开关 + Hook 状态，修改入口指向仓库管理，避免两处开关不同步。
- 确认通道说明：终端优先、无控制终端时回落桌面端，通道不可配置。
- 超时入口：指向设置页。
- 规划中的策略能力（优先级、身份/remote 绑定）单独列卡片，不可操作。

### 4.5 推送确认

目的：在倒计时内让用户核对目标并显式确认。

- 有待确认请求：直接渲染现有 `PushConfirmation` 面板（仓库、remote、URL、ref、Commit、倒计时、输入 `yes`）。
- 无请求：空态 + 三步流程说明 + 「终端内确认不会出现在这里」的提示。
- 全局打断：任何页面收到新请求时自动切到本页，并把窗口带到前台；离开本页时保留顶部横幅「有推送请求等待确认」，点击可返回。
- 确认成功后展示「原 git push 将继续」提示并自动收起；确认失败展示稳定错误码对应的 fail-closed 文案。
- 安全规则不变：必须手工输入 `yes`；倒计时结束、取消、未知输入都不等于确认。

### 4.6 操作日志（规划中）

目的：说明当前没有审计能力，避免把界面当审计凭证。

- 「规划中」徽标 + 明确声明：只在 stderr 输出错误，不持久化、不上传。
- 规划卡片：推送事件记录、配置变更记录、脱敏与导出。
- 边界提示：当前任何页面内容都不构成审计凭证。

### 4.7 设置

目的：应用级偏好的唯一入口。

- 界面语言：复用 `LanguageSwitch`，切换后立即生效并写入 `localStorage`。
- 确认行为：确认超时（1–3600 秒）表单，保存成功才显示「已保存」。
- 快捷键：打开设置 `⌘,` / `Ctrl+,`；关闭设置 `Esc`；提示确认必须手输 `yes`（不提供快捷键）。
- 配置文件：只读展示 `configPath`（来自 `get_settings`），说明由界面写入、损坏时不覆盖。
- 关于：MVP 定位与平台限制。
- 浏览器环境：只读展示，不显示可提交控件。

## 5. 全局交互

| 触发                    | 行为                                                                     |
| ----------------------- | ------------------------------------------------------------------------ |
| 新推送请求（事件/轮询） | 自动切到推送确认页；窗口由 Rust 侧切到前台                               |
| 离开确认页且请求仍在    | 顶部黄色横幅 + 侧边栏提示点，点击回到确认页                              |
| 侧边栏折叠按钮          | 在 15rem（图标+文字）与 4.75rem（仅图标）间切换，选择写入 `localStorage` |
| `⌘,` / `Ctrl+,`         | 打开设置页并记录来源页                                                   |
| 原生菜单「设置…」       | Rust 发送 `open-settings` 事件，前端打开设置页                           |
| `Esc`（设置页）         | 返回来源页                                                               |
| 语言切换                | 文案、`<html lang>`、原生菜单三处同步                                    |

轮询（1s）与事件双通道并存：事件只用于尽快刷新，轮询兜底超时后的自动关闭。

## 6. 原生菜单国际化

macOS / Linux 的菜单栏由 Tauri 创建，默认英文。方案：

- Rust `menu` 模块按语言构建菜单：应用菜单（关于、设置…、服务、隐藏、退出）、编辑、视图、窗口。
- 「设置…」菜单项带 `CmdOrCtrl+,` 加速键，触发时只向前端发送 `open-settings` 事件。
- 新增命令 `set_app_menu(locale)`；前端在挂载时与语言切换后调用，用当前 i18n 语言重建菜单。
- 未知 locale 回落到默认语言；菜单重建失败只写 stderr，不影响任何安全判断。

## 7. 实现方案

```text
apps/desktop/src
├── App.vue                     # 视图切换、轮询/事件、快捷键、全局提示、侧边栏折叠状态
├── main.ts                     # 注册 i18n 与按需的 Element Plus 组件
├── elementPlus.ts              # 按需注册清单（与 style.css 的 el-*.css 一一对应）
├── style.css                   # Tailwind v4 入口：@theme 设计令牌 + 分层引入的 EP 样式
├── navigation.ts               # AppView 联合类型与导航项
├── platform.ts                 # 平台判断与快捷键展示文案
├── api.ts                      # Tauri command 封装与返回值形状校验
├── statusCodes.ts              # 状态码/错误码白名单收窄与展示色调
├── components/
│   ├── AppSidebar.vue          # el-menu 导航 + 折叠按钮 + 设置入口 + 待确认 badge
│   ├── PushConfirmation.vue    # 确认面板（倒计时 + 手输 yes）
│   ├── CommitList.vue / LanguageSwitch.vue
│   └── ui/                     # 公共展示组件，页面只组合这些
│       ├── PageHeader.vue      # 标题 + 说明 + 右侧动作
│       ├── SurfaceCard.vue     # 卡片容器（tone=brand/planned）
│       ├── CardGrid.vue        # 自适应列栅格
│       ├── StatCard.vue        # 指标卡
│       ├── StatusTag.vue       # 状态胶囊（el-tag）
│       ├── SettingRow.vue      # 左文右控件
│       ├── RepoRow.vue         # 仓库行（路径 + Hook 状态 + 开关）
│       ├── PathText.vue        # 路径截断 + tooltip
│       ├── NoticeBanner.vue    # el-alert 封装
│       ├── EmptyPanel.vue      # 空态
│       ├── StepList.vue        # 有序步骤
│       └── PlannedList.vue     # 规划中能力列表
└── views/
    ├── DashboardView.vue
    ├── IdentitiesView.vue
    ├── RepositoriesView.vue    # 原 RepoSettings 的仓库部分
    ├── PoliciesView.vue
    ├── ApprovalView.vue
    ├── AuditView.vue
    └── SettingsView.vue
```

Rust：

- `src-tauri/src/menu.rs`：菜单标签、构建函数、`MenuLanguage` 与语言回落测试。
- `src-tauri/src/lib.rs`：setup 中设置菜单、注册 `on_menu_event`、注册 `set_app_menu` 命令；`SettingsView` 增加 `config_path`。

状态归属：

- `App.vue` 持有 `settings`、`pending`、`busy`、`errorKey`、`confirmed` 与当前视图。
- 页面组件只接收 props、发事件，不直接 `invoke`（仓库页的文件夹选择除外，与现状一致）。

## 8. 测试计划

前端（Vitest + jsdom）：

- 默认落在总览，导航切换到各页后标题与关键内容随语言变化。
- 设置页：`⌘,` / `Ctrl+,` 打开、`Esc` 返回、菜单事件打开、超时保存、语言切换调用 `set_app_menu`。
- 推送确认：待确认请求自动进入确认页；离开后出现横幅；手输 `yes` 才能确认；新请求清空输入；失败不显示成功提示；取消不触发 `confirm_push`。
- 仓库管理：未知 Hook 状态 fail-closed；文件夹选择取消保留输入；添加时保护开关不继承全局值；开关调用 `toggle_repo_protection`。
- 安全策略：全局开关调用 `set_unregistered_protection`；空仓库列表正常展示。
- 侧边栏：折叠按钮切换 `aria-expanded`、收起后靠 `aria-label` 保留可读名称、选择写入 `localStorage`、刷新后恢复收起态。
- 浏览器环境不调用任何 command。

测试实现注意：`mount` 时需同时注册 `i18n` 与 `elementPlus`（`src/elementPlus.ts`，与 `main.ts` 共用同一份按需清单），并为 jsdom 补 `ResizeObserver`；元素定位统一用组件上声明的 `data-*` 钩子（`[data-confirm]`、`[data-repo-switch]`、`[data-add-form]` 等），不依赖 Element Plus 内部类名。`el-switch` 的 `aria-checked` 在内层 `input` 上，`el-tooltip` 会覆盖触发元素的 `aria-expanded`，因此折叠按钮不再包 tooltip。`src/elementPlus.test.ts` 扫描模板里的 `el-*` 标签，新增组件忘记注册时会失败。

Rust：

- 菜单标签随语言切换，未知 locale 回落。
- `SettingsView` 序列化包含 `configPath`（camelCase 契约）。
- 既有 command、配置、Hook、IPC 测试保持通过。

## 9. 文档同步

- `README.md`：页面结构、设置快捷键、身份/日志「规划中」的说明。
- `docs/ARCHITECTURE.md`：桌面端界面结构、菜单 i18n、`open-settings` 事件与 `set_app_menu` 命令。
- `docs/SECURITY.md`：补充「界面导航、快捷键与原生菜单不构成安全控制」的说明。
- `AGENTS.md`：仓库地图与后端硬约束里补上菜单与页面导航的边界。

## 10. 实现状态

已按本文落地：七个视图、`AppSidebar`（含折叠）、`components/ui` 公共组件、`navigation.ts`、`platform.ts`、`menu.rs`、`set_app_menu` 命令、`open-settings` 事件、`configPath` 字段、`zh-CN` / `en-US` 全量文案。

布局：侧边栏宽度由 `AppSidebar` 的 Tailwind 类切换（`w-60` / `w-19`，即 15rem / 4.75rem），折叠状态保存在 `App.vue` 并写入 `localStorage`；内容区上限 `max-w-7xl`（80rem）并居中，卡片统一用 `CardGrid` 自适应列；Element Plus 样式按组件引入并放在 `layer(element-plus)`，Tailwind 原子类负责布局与覆盖。字号用 `text-meta` / `text-caption` / `text-micro` / `text-title` 令牌，不写任意值。

交付验证沿用仓库统一的检查块（`pnpm lint`、`pnpm type-check`、`pnpm test:frontend`、`pnpm build`、`cargo fmt --check`、`cargo clippy -D warnings`、`cargo test`），结果以 CI 与本地实际输出为准，不在本文固定用例数或产物体积。

未覆盖项：`pnpm tauri dev` 的真机窗口、Element Plus + Tailwind 的实际观感、Linux 桌面环境下的菜单行为。它们不影响退出码与放行逻辑，但尚未人工验收。
