# GitGuard 第一周开发规范与执行计划

> **文档版本：** 1.0  
> **开发阶段：** Week 1 — 工程基础与 Git Hook 原型  
> **技术栈：** Vue 3 + TypeScript + Vite + Tauri 2 + Rust  
> **开发模式：** 本地优先、跨平台、测试驱动  
> **本周核心目标：** 建立清晰的工程边界，并证明 `git pre-push` Hook 能够可靠阻止一次推送。

---

## 1. 本周目标

第一周不以界面完成度为目标，而是建立后续开发可以长期依赖的工程基础。

本周必须完成以下工作：

- [ ] 建立 Vue 3 + TypeScript + Vite + Tauri 2 工程。
- [ ] 建立 Rust Cargo Workspace。
- [ ] 明确核心模块职责和依赖方向。
- [ ] 创建 `gitguard-core`、`gitguard-policy`、`gitguard-cli`。
- [ ] 定义统一的策略决策类型、错误类型和 CLI 退出码。
- [ ] 实现最小 `pre-push` Hook 原型。
- [ ] 使用真实 Git 临时仓库验证允许和拒绝两条路径。
- [ ] 建立基础测试、代码检查、CI 和 README。
- [ ] 记录本周未完成事项及安全限制。

### 本周完成的定义

第一周结束时，开发者应能从零启动项目、运行自动化测试，并使用临时 Git 仓库证明：

1. 策略明确允许时，测试 push 可以成功。
2. 策略拒绝时，测试 push 失败，远端引用不发生变化。
3. 需要确认但确认机制尚未实现时，push 被拒绝。
4. CLI 异常或输入无法解析时，不会静默放行。
5. 关键行为由自动化测试验证，而不是依赖手动演示。

---

## 2. 项目技术规范

| 层级     | 技术选择                           | 职责                     |
| -------- | ---------------------------------- | ------------------------ |
| 前端     | Vue 3 + TypeScript                 | 用户界面和交互           |
| 构建     | Vite                               | 前端开发与生产构建       |
| 桌面容器 | Tauri 2                            | 窗口、原生应用入口       |
| 后端     | Rust stable                        | Git 集成与核心逻辑       |
| 工作空间 | Cargo Workspace                    | 管理 Rust Crate          |
| CLI      | Rust 二进制程序                    | 接收 Hook 请求、调用策略 |
| 测试     | Rust tests + Git 集成测试 + Vitest | 验证模块及真实行为       |
| 版本控制 | Git                                | 管理源代码、文档和变更   |

### 2.1 固定规则

- 前端使用 Vue 3 Composition API 和 `<script setup lang="ts">`。
- Rust 使用稳定版工具链。
- 固定 Node 包管理器并提交对应 lockfile。
- 所有业务决策必须由 Rust 后端执行。
- 核心策略模块不能依赖 Vue、Tauri UI 或数据库实现。
- 第一周不引入云端业务服务、遥测或自动同步。
- 新增依赖必须有明确用途，避免过早引入复杂框架。

---

## 3. 工程目录结构

第一周建议采用以下最小结构：

```text
gitguard/
├── apps/
│   └── desktop/
│       ├── src/
│       │   ├── components/
│       │   ├── views/
│       │   ├── router/
│       │   ├── stores/
│       │   ├── services/
│       │   └── types/
│       ├── package.json
│       └── src-tauri/
│           └── src/
│               ├── main.rs
│               ├── lib.rs
│               └── commands/
├── crates/
│   ├── gitguard-core/
│   ├── gitguard-policy/
│   └── gitguard-cli/
├── tests/
│   ├── fixtures/
│   └── integration/
├── docs/
│   ├── PRD.md
│   ├── ARCHITECTURE.md
│   ├── SECURITY.md
│   └── WEEK-1.md
├── Cargo.toml
├── rust-toolchain.toml
├── package.json
├── pnpm-lock.yaml
├── .editorconfig
├── .gitignore
└── README.md
```

注意：

- `pnpm-lock.yaml` 只有在选择 pnpm 时才使用。
- 不要创建大量没有实现内容的空模块。
- 后续再按实际需要添加 `gitguard-git`、`gitguard-storage` 和 `gitguard-identity`。
- 不要把所有 Rust 代码塞进 Tauri 的 `main.rs`。

---

## 4. 模块职责和依赖边界

### 4.1 `gitguard-core`

负责：

- 通用基础类型。
- 策略决策数据结构。
- 可复用错误类型。
- 模块之间共享的稳定接口。

禁止：

- 依赖 Vue 或 Tauri UI。
- 依赖具体数据库实现。
- 自行弹出桌面窗口。
- 直接执行 Git 命令。

### 4.2 `gitguard-policy`

负责：

- 纯策略判断。
- 根据输入上下文返回决策。
- 提供可测试、可解释的决策原因。

禁止：

- 访问 UI 状态。
- 弹窗或等待用户交互。
- 执行任意 shell 命令。
- 因为规则无法判断就默认允许。

### 4.3 `gitguard-cli`

负责：

- CLI 入口。
- `hook pre-push` 子命令。
- 读取和解析 Hook 的标准输入。
- 调用策略模块。
- 将决策转换为稳定的进程退出码。

禁止：

- 在 CLI 中实现 UI。
- 依赖前端决定是否放行。
- 吞掉错误并返回成功。
- 将未经校验的输入拼接成 shell 命令。

### 4.4 `apps/desktop`

负责：

- Vue 界面。
- Tauri 应用入口。
- 用户输入和状态展示。
- 后续的本地服务交互。

第一周只要求工程能启动，不要求完成完整业务界面。

### 4.5 依赖方向

```text
Vue UI
  ↓
Tauri commands / application services
  ↓
Rust core modules

Git pre-push Hook
  ↓
gitguard CLI
  ↓
gitguard-policy / gitguard-core
```

**核心规则：策略模块不依赖 UI；Hook 的最终退出码不能由前端决定。**

---

## 5. 策略决策类型

第一周先固定三种决策：

```rust
pub enum Decision {
    Allow,
    RequireConfirmation,
    Deny,
}

pub struct PolicyResult {
    pub decision: Decision,
    pub reason_code: String,
}
```

允许根据实际实现调整字段，但必须满足以下约束：

- 决策使用明确的枚举类型。
- 决策原因使用稳定的代码。
- 不允许通过解析日志文本决定程序行为。
- 策略模块能够脱离 UI 独立测试。

建议原因代码：

| 原因代码                | 含义             |
| ----------------------- | ---------------- |
| `explicit_allow`        | 规则明确允许     |
| `explicit_deny`         | 规则明确拒绝     |
| `confirmation_required` | 需要用户确认     |
| `invalid_input`         | 输入无效         |
| `policy_unavailable`    | 策略无法完成判断 |

### 本周的关键规则

由于第一周尚未实现正式的用户确认流程：

**`RequireConfirmation` 必须转为拒绝本次 push，而不是自动放行。**

---

## 6. Git Hook 原型规范

### 6.1 Hook 的作用

Git `pre-push` Hook 在推送前执行。

Git 会通过 Hook 的标准输入提供待更新引用信息。实现必须读取所有输入行，不能只检查第一行。

每行数据包含：

- 本地 ref。
- 本地对象 ID。
- 远端 ref。
- 远端对象 ID。

需要注意：

- 创建分支时，对象 ID 可能为全零值。
- 删除远端分支时，远端对象 ID 可能为空。
- 一次 push 可能更新多个 ref。
- 不能假设所有输入都属于普通分支更新。

### 6.2 最小 Hook 脚本

```sh
#!/bin/sh
exec gitguard hook pre-push "$@"
```

这是第一周的原型脚本。

本周只允许在临时测试仓库中安装，不得自动覆盖开发者日常仓库已有的 Hook。

### 6.3 退出码规范

| 结果                 | 退出行为   |
| -------------------- | ---------- |
| 明确允许             | 返回 `0`   |
| 明确拒绝             | 返回非 `0` |
| 需要确认但无确认服务 | 返回非 `0` |
| 输入无法解析         | 返回非 `0` |
| CLI 内部错误         | 返回非 `0` |
| 策略不可用           | 返回非 `0` |

退出码必须由代码中的常量统一管理，并通过测试固定其行为。

错误信息写入 `stderr`。不要把调试日志、错误文本和机器可读输出混在一起。

### 6.4 第一周必须覆盖的测试

- [ ] Hook 能读取全部 stdin。
- [ ] Allow 路径能够完成测试 push。
- [ ] Deny 路径能够阻止测试 push。
- [ ] Deny 后，bare remote 中的目标 ref 未发生变化。
- [ ] RequireConfirmation 在无确认服务时拒绝。
- [ ] 空输入、非法输入和读取错误不会放行。
- [ ] 多个 refs 不会被截断。
- [ ] 创建分支、删除分支和普通更新均有测试。
- [ ] CLI 不存在或执行失败时，Git push 不会静默成功。

---

## 7. 安全与工程边界

第一周只建立安全基础，不宣称已经完成完整安全闭环。

### 必须遵守

1. 未知情况不能默认允许。
2. 不得把用户输入、Git 配置或 Hook stdin 直接拼接成 shell 命令。
3. 调用外部程序时，优先使用参数数组。
4. 不记录 Token、私钥、完整环境变量或 URL 中的密码。
5. 不对真实远端执行集成测试。
6. 不在第一周自动覆盖已有 Hook。
7. 不将 `user.name`、`user.email` 当作远端认证账号。
8. 不把前端隐藏按钮当成安全控制。
9. 不添加非必要网络请求、云同步或遥测。
10. 所有无法安全判断的路径必须有明确失败行为。

### 必须登记为后续任务

- 正式的本地 IPC。
- 一次性批准请求及过期机制。
- 用户确认 UI。
- 后台服务异常时的完整 fail-closed 验证。
- Hook 安装、升级、卸载和恢复。
- 多 push URL、URL rewrite 等 Git 边界。
- Windows、macOS、Linux 的真实环境验证。

---

## 8. 代码规范

### Rust

提交前执行：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

规则：

- 使用稳定版 Rust。
- 不使用 `unwrap()` 处理可预期的外部输入错误。
- 不吞掉错误。
- 核心 API 保持小而明确。
- 错误必须能传递到 CLI 边界并映射为非零退出码。
- 策略模块应尽量保持纯逻辑，便于测试。

### Vue / TypeScript

规则：

- 使用 Composition API 和 `<script setup lang="ts">`。
- 为业务数据定义类型，避免随意使用 `any`。
- 组件只负责 UI，不实现 Git 策略。
- 固定包管理器并提交 lockfile。
- lint、类型检查和生产构建必须能够在 CI 执行。

### Git 提交

建议采用以下提交前缀：

- `feat:` 新功能。
- `fix:` 修复问题。
- `test:` 测试。
- `docs:` 文档。
- `refactor:` 重构。
- `build:` 构建或工程配置。

每个提交尽量只处理一个逻辑变更。

---

## 9. 每日执行计划

### Day 1：初始化工程

- [ ] 创建 Git 仓库。
- [ ] 添加 README、`.gitignore` 和 `.editorconfig`。
- [ ] 初始化 Rust Workspace。
- [ ] 创建 Vue 3 + TypeScript + Vite + Tauri 2 工程。
- [ ] 固定 Node 包管理器和 Rust 工具链。
- [ ] 验证前端启动与 Rust 测试。

**验收：** 新环境按 README 操作后，前端和 Rust 工程均能启动或构建。

### Day 2：模块边界与基础类型

- [ ] 创建 `gitguard-core`。
- [ ] 创建 `gitguard-policy`。
- [ ] 创建 `gitguard-cli`。
- [ ] 定义 `Decision` 和 `PolicyResult`。
- [ ] 定义统一错误类型与退出码。
- [ ] 为策略模块写单元测试。
- [ ] 更新 `docs/ARCHITECTURE.md`。

**验收：** 核心类型可编译，策略模块可独立测试。

### Day 3：CLI 与 Hook 输入

- [ ] 创建 `hook pre-push` 子命令。
- [ ] 实现 stdin 读取。
- [ ] 保留全部 ref 更新记录。
- [ ] 处理空输入和非法输入。
- [ ] 定义 stderr 输出规则。
- [ ] 测试各决策对应的退出码。

**验收：** 输入和错误行为可预测，错误不会转为成功退出码。

### Day 4：真实 Git 集成测试

- [ ] 创建临时工作仓库。
- [ ] 创建本地 bare remote。
- [ ] 只在临时仓库安装测试 Hook。
- [ ] 验证 Allow 路径。
- [ ] 验证 Deny 路径。
- [ ] 比较 push 前后的远端 refs。
- [ ] 覆盖多 refs、创建分支和删除分支输入。

**验收：** 拒绝 push 后，远端目标 ref 未改变。

### Day 5：CI 与里程碑

- [ ] 配置 Rust 格式检查。
- [ ] 配置 Clippy 和 Rust 测试。
- [ ] 配置前端 lint、类型检查和生产构建。
- [ ] 将 Git Hook 集成测试纳入自动化流程。
- [ ] 完善 README。
- [ ] 记录未完成事项和风险。
- [ ] 建立第一周里程碑提交。

**验收：** 所有必需检查可重复执行，并且结果有记录。

---

## 10. 第一周验收清单

只有全部满足，才算完成第一周。

- [ ] 新开发者能按 README 从零启动前端。
- [ ] Rust Workspace 可以构建。
- [ ] Rust 格式检查、Clippy 和测试通过。
- [ ] CLI 能被 Git Hook 调用。
- [ ] Hook 可以读取全部 ref 输入。
- [ ] Allow 测试 push 成功。
- [ ] Deny 测试 push 失败。
- [ ] Deny 后远端 ref 未改变。
- [ ] RequireConfirmation 在无确认机制时拒绝。
- [ ] CLI 或策略错误不会静默放行。
- [ ] 测试不会触碰真实远端或日常仓库。
- [ ] README、架构说明和已知限制已经提交。
- [ ] CI 可以重复运行必要检查。

---

## 11. 后续任务登记

以下任务不属于第一周交付，但必须保留在路线图中：

- [ ] SQLite 数据模型和迁移。
- [ ] 身份、仓库、remote 管理。
- [ ] 正式策略配置和优先级处理。
- [ ] 本地 IPC 的权限、认证、超时和请求绑定。
- [ ] 用户确认 UI。
- [ ] 一次性批准与过期机制。
- [ ] Hook 安装、升级、卸载和原有 Hook 恢复。
- [ ] 多 push URL、pushurl、URL rewrite、直接 URL。
- [ ] Windows、macOS、Linux 真实环境集成测试。
- [ ] 系统凭据库与日志脱敏。

## 12. 最终原则

第一周优先级排序：

1. 正确性。
2. 安全边界。
3. 可测试性。
4. 模块隔离。
5. 跨平台可验证性。
6. UI 美观度。

不要用“看起来能运行”替代真实 Git 集成测试；不要为了赶进度把未实现的确认机制默认放行；不要把第一周的原型误认为已经完成完整安全保护。
