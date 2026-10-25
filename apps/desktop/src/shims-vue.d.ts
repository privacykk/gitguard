/**
 * `*.vue` 的兜底类型声明。
 *
 * 类型检查以 `pnpm type-check`（vue-tsc）为准：Volar 会把每个 `.vue` 文件解析成真实模块，
 * 通配声明不会被用到。它只服务于没有 Volar 接管 `.vue` 的编辑器语言服务，
 * 用来消掉 `Module '...vue' has no default export` 这类误报，不影响 `vue-tsc` 的推导。
 *
 * 注意：这里不要写 `any`，也不要删除；新增文件类型声明请放进本目录的 `*.d.ts`。
 */
declare module "*.vue" {
  import type { DefineComponent } from "vue";

  const component: DefineComponent<
    Record<string, unknown>,
    Record<string, unknown>,
    unknown
  >;
  export default component;
}
