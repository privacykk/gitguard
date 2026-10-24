# GitGuard 架构说明（Week 1）

## 依赖方向

```text
Vue 3 UI
  ↓
Tauri commands
  ↓
gitguard-core

Git pre-push Hook
  ↓
gitguard-cli
  ↓
gitguard-policy → gitguard-core
```

`gitguard-policy` 不依赖 UI、Tauri、数据库或 shell。`gitguard-core` 只包含共享数据、错误和退出码，不执行 Git。最终 push 放行决定由 CLI 根据 Rust 策略结果产生。

## 模块职责

- `gitguard-core`：`Decision`、`PolicyResult`、`RefUpdate`、统一错误、稳定退出码。
- `gitguard-policy`：Week 1 的显式原型策略；输入为空时返回错误，无法判断时不允许。
- `gitguard-cli`：参数验证、完整读取 stdin、逐行解析所有 ref、调用策略、映射退出码。
- `gitguard-desktop`：最小 Tauri 入口和开发状态命令，不实现策略。
- `apps/desktop/src`：Vue Composition API 界面与 i18n 语言包，仅展示当前原型状态；界面文案不参与策略判断。

## 界面文案与状态码

- Tauri command 返回稳定标识（如 `week1_confirmation_unavailable`），不返回面向用户的句子。
- 前端在 `apps/desktop/src/i18n/developmentStatus.ts` 对标识做白名单收窄；未知或缺失的标识统一渲染 fail-closed 文案，不推断推送结果。
- 支持 `zh-CN` 与 `en-US`。初始语言优先级：本地记忆（`localStorage` 键 `gitguard.locale`）→ 系统语言 → `zh-CN`；切换后写回本地记忆并同步 `document.lang`。
- 文案与语言切换只影响展示，不改变退出码、`reason_code` 或任何放行决定。

## Hook 请求处理

1. 校验子命令和必需的 `--policy`，可选参数只能是 Git 提供的 remote 名称与 URL 一对参数。
2. 将 stdin 完整读取为 UTF-8；读取失败即返回 I/O 错误。
3. 每行必须包含 local ref、local OID、remote ref、remote OID 四项；OID 支持 SHA-1 和 SHA-256 长度。
4. 全零 remote OID 表示创建，全零 local OID 表示删除；不把输入假定为普通分支更新。
5. 任意一行非法都会拒绝整个请求，不截断多 refs。
6. 仅 `Decision::Allow` 映射为 0；`Deny` 和 `RequireConfirmation` 都非零。

## 错误边界

| 错误                           | 退出码 |
| ------------------------------ | ------ |
| 参数、空输入、非法行、非法 OID | 3      |
| 策略不可用                     | 4      |
| stdin/stderr I/O               | 5      |
| 未分类内部错误                 | 6      |

可预期错误通过 `Result` 传递到 CLI 边界。业务逻辑不解析日志文本；错误信息只写 `stderr`。

## 测试边界

策略与解析由单元测试覆盖。Unix 下，Rust 集成测试通过参数数组启动 Git，在临时工作仓库和 bare remote 间进行真实 push，并比较远端 refs。CI（Actions 页面手动触发）在构建前端后执行完整 workspace 检查，确保 Tauri `frontendDist` 可用。
