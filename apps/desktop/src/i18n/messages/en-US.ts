import type { MessageSchema } from "./zh-CN";

const enUS = {
  app: {
    eyebrow: "Local-first Git push protection",
    title: "GitGuard",
    intro:
      "Week 1 engineering prototype: the core policy and Hook exit behavior are owned by the Rust backend.",
  },
  language: {
    label: "Interface language",
  },
  development: {
    title: "Development status",
    status: {
      week1_confirmation_unavailable:
        "The project foundation is ready. The confirmation UI is not implemented yet, so pushes that need confirmation are rejected.",
      unknown:
        "The development status could not be read from the Rust backend; under the fail-closed rule this does not mean the push was allowed.",
    },
  },
} satisfies MessageSchema;

export default enUS;
