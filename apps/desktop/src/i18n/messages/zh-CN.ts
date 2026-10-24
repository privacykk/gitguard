const zhCN = {
  app: {
    title: "GitGuard",
    subtitle: "本地优先的 Git 推送保护",
    loading: "正在加载…",
    desktopUnavailable:
      "未检测到桌面端后端：本页面只读，推送仍会被拒绝（fail-closed）。",
  },
  language: {
    label: "界面语言",
  },
  push: {
    title: "推送确认",
    intro: "以下内容即将推送到远端，请逐项确认。",
    repository: "仓库",
    remote: "Remote",
    remoteUrl: "Remote 地址",
    targetRef: "目标分支",
    sourceRef: "本地分支",
    commits: "待推送的 Commit",
    noCommits: "无新增 Commit",
    kind: {
      create: "将创建新的远端分支",
      update: "将更新远端分支",
      delete: "将删除远端分支",
    },
    yesLabel: "要执行本次推送，请输入 yes",
    inputPlaceholder: "yes",
    confirmButton: "确认本次推送",
    cancelButton: "取消",
    executing: "正在确认…",
    remaining: "剩余确认时间：{seconds} 秒",
    expired: "确认已超时，本次推送已被拒绝。",
    confirmed:
      "已确认。原始 git push 将继续执行，推送结果请在终端或 Git 客户端查看。",
  },
  commit: {
    sha: "SHA",
    author: "作者",
    time: "时间",
    expand: "展开全部 {count} 条",
    collapse: "收起",
    empty: "没有需要展示的 Commit。",
  },
  settings: {
    title: "仓库设置",
    intro:
      "添加仓库后 GitGuard 会安装 pre-push Hook：推送前先展示目标分支与 Commit，确认后原始 git push 才会继续。",
    repositories: "已保护的仓库",
    empty: "还没有添加任何仓库。",
    addLabel: "仓库路径",
    addPlaceholder: "/absolute/path/to/repository",
    addButton: "添加仓库",
    adding: "正在添加…",
    browseButton: "选择文件夹…",
    browsing: "正在打开…",
    addWithProtection: "添加后启用推送保护",
    toggleLabel: "推送保护：{path}",
    protection: "推送保护",
    enabled: "已启用",
    disabled: "已关闭",
    hookStatusLabel: "Hook 状态",
    hookStatus: {
      healthy: "正常",
      missing: "未安装",
      modified: "内容被修改",
      conflict: "与其它 Hook 冲突",
      unknown: "无法判断（按 fail-closed 处理）",
    },
    timeoutLabel: "确认超时时间（秒）",
    timeoutSave: "保存",
    timeoutSaved: "已保存",
    defaultProtection: "新仓库默认启用推送保护",
  },
  errors: {
    unknown: "操作失败，原因未识别；推送仍保持被拒绝状态。",
    configuration_unreadable:
      "配置文件无法读取，已停止写入以避免覆盖原始内容。",
    configuration_write_failed: "无法写入配置文件。",
    repository_not_found: "找不到该仓库的配置记录。",
    hook_install_failed: "Hook 安装失败。",
    cli_binary_not_found: "找不到 gitguard CLI，无法写入 Hook。",
    not_a_repository: "所选路径不是 Git 仓库。",
    path_not_absolute:
      "请填写仓库的绝对路径（可用「选择文件夹」按钮），相对路径无法确定位置。",
    no_pending_request: "该确认请求已超时或被取消。",
    request_invalid: "仓库状态在确认期间发生变化，已放弃推送。",
    invalid_timeout: "超时时间必须是 1 到 3600 之间的整数秒。",
    ipc_unavailable: "桌面端 IPC 服务未启动，Hook 无法连接。",
    internal_error: "桌面端内部错误。",
  },
};

export type MessageSchema = typeof zhCN;

export default zhCN;
