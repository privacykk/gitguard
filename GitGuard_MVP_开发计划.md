# GitGuard — MVP 开发计划

> 版本 1.0 · Vue 3 + TypeScript + Vite + Tauri 2 + Rust · 本地优先 · 约 6 周

## 目标与原则
- Windows、macOS、Linux 桌面应用；业务数据完全本地保存，不使用云端业务后端、注册、同步或自动遥测。
- 先验证 `pre-push` Hook 能可靠阻止 push，再做完整 UI。
- 策略判断和最终放行决定由 Rust 后端完成；Vue 仅展示和编辑。
- 服务不可用、IPC 失败、超时、策略不明时默认拒绝（fail-closed）。
- 本机管理员可绕过 Hook；不要承诺抵御拥有完整本机控制权的用户。

## 技术栈
- 前端：Vue 3 Composition API、TypeScript、Vite、Vue Router、Pinia、Vitest
- UI：Naive UI（候选）
- 桌面：Tauri 2
- 后端：Rust workspace
- 数据：SQLite + migrations；HTTPS Token 交给 OS Credential Manager / Keychain / Secret Service
- Git 拦截：`pre-push` Hook + `gitguard` CLI + 本地 IPC + 确认 UI

## 六周路线图

### 第 1 周 — 工程骨架与 Hook 原型
- [ ] 初始化 Rust workspace、Vue/Tauri 工程、CI、格式化和测试
- [ ] 创建 `gitguard-policy`、`gitguard-cli`
- [ ] 用临时 Git 仓库和 bare remote 验证 Hook stdin 与非零退出码
- [ ] 记录 Windows/macOS/Linux 构建与 Hook 差异

**验收：** Hook 返回非零时 push 确实被阻止；有从零启动说明。

### 第 2 周 — 数据、身份与仓库
- [ ] SQLite migrations：identities、repositories、remotes、policies、policy_branches、audit_events、settings
- [ ] 仓库扫描、remote 读取、Git user.name/user.email 读取与编辑
- [ ] 记录 SSH/HTTPS 类型与非敏感凭据引用
- [ ] 实现迁移和数据目录定位

**验收：** 仓库与身份可持久化；数据库和日志不出现 Token/私钥。

### 第 3 周 — 策略引擎
- [ ] 输入包含仓库、身份、remote、目标 refs、force/delete 标志
- [ ] 输出 `Allow` / `RequireConfirmation` / `Deny`，附原因与规则 ID
- [ ] 优先级：紧急禁用 → 显式禁止 → 高风险强制确认 → 仓库/身份/分支规则 → 明确允许 → 全局默认
- [ ] 冲突按 `Deny > RequireConfirmation > Allow`
- [ ] 测试多个 refs、通配分支、删除分支、force push、未知输入

**验收：** 策略模块独立可测；不明输入不会静默放行。

### 第 4 周 — Hook、CLI、IPC、确认流程
- [ ] 解析 Hook stdin、remote 和 URL
- [ ] 实现 hook、status、install、uninstall 等 CLI 命令
- [ ] 使用平台权限可控的本地 IPC，不用无认证固定端口 HTTP
- [ ] 确认绑定单次 push 的仓库、remote、refs、风险标志、随机 request ID 和过期时间
- [ ] 超时、取消、服务未运行、IPC 失败均拒绝
- [ ] 已有 `pre-push` Hook 必须备份并兼容/恢复，不可静默覆盖

**验收：** 端到端证明批准才继续；其他故障路径均阻止 push。

### 第 5 周 — UI、审计、导入导出
- [ ] Dashboard、Identities、Repositories、Policies、Approval、Audit
- [ ] 表单校验、加载/失败/空状态
- [ ] 审计记录决策、原因和用户结果并脱敏
- [ ] 非敏感配置导入导出；排除 Token/私钥
- [ ] 卸载默认保留数据，提供明确删除选项

**验收：** 主要页面可用；审计无秘密；导入导出往返测试通过。

### 第 6 周 — 跨平台验收与发布候选
- [ ] Windows、macOS、Linux 真实环境安装、升级、卸载和推送测试
- [ ] 覆盖 SSH/HTTPS、pushurl、URL rewrite、remote.pushDefault、push.default、直接 URL
- [ ] 覆盖 force、force-with-lease、删除分支、多 refs、多 push URL
- [ ] 测试 Hook 被删除/修改、服务崩溃、IPC 超时
- [ ] 完成 SECURITY.md、故障排查、备份恢复和发布清单

**验收：** 核心测试通过或将未通过项明确列为发布阻断/已知限制。

## 最低验收清单
- [ ] Allow 规则允许普通 push
- [ ] 受保护分支批准成功、取消后失败
- [ ] 服务/IPC/超时/过期请求均不能放行
- [ ] 多 refs、force、force-with-lease、删除分支处理正确
- [ ] 多 push URL、pushurl、URL rewrite、直接 URL 不可静默绕过
- [ ] SSH 与 HTTPS 实际推送成功，凭据不进入日志
- [ ] 不同仓库/身份策略隔离
- [ ] 已有 Hook 可备份、兼容或恢复
- [ ] Hook 被删除/修改时提示失效
- [ ] 配置导入导出、数据库迁移、备份恢复通过
- [ ] 三个平台完成真实 Git 集成测试

## 数据与安全边界
- SQLite：身份标签、仓库、remote、策略、审计和非敏感设置。
- 系统凭据库：HTTPS Token；配置仅保存 credential reference。
- SSH 私钥：留在 SSH Agent/用户现有密钥工具中。
- 默认不上传数据；日志脱敏；导出不包含秘密。
- 本地 IPC 的批准必须短期、一次性，并绑定具体 push 请求。
- 数据目录通过平台 API 获取；不要硬编码用户目录。

## 第一个工作日
- [ ] 建立仓库并把本计划提交为 `docs/MVP-PLAN.md`
- [ ] 初始化 Rust workspace 与 Vue/Tauri 工程
- [ ] 创建 `gitguard-policy`、`gitguard-cli`
- [ ] 建立临时仓库 + bare remote 的 Hook 集成测试
- [ ] 优先完成“Hook 非零退出码能阻止 push”的证明
