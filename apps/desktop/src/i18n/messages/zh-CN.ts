const zhCN = {
  app: {
    eyebrow: "本地优先的 Git 推送保护",
    title: "GitGuard",
    intro: "第一周工程原型：核心策略与 Hook 退出行为由 Rust 后端负责。",
  },
  language: {
    label: "界面语言",
  },
  development: {
    title: "开发状态",
    status: {
      week1_confirmation_unavailable:
        "工程基础已就绪。确认界面尚未实现，需要确认的推送会被拒绝。",
      unknown:
        "无法从 Rust 后端读取开发状态；按 fail-closed 约定，这不代表推送已被放行。",
    },
  },
};

export type MessageSchema = typeof zhCN;

export default zhCN;
