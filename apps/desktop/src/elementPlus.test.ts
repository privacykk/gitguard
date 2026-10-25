import { describe, expect, it } from "vitest";
import { createApp } from "vue";
import { elementPlus } from "./elementPlus";

/** 以原始文本读入所有单文件组件，用来收集模板里出现的 `el-*` 标签。 */
const templates = import.meta.glob(["./**/*.vue"], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

function toPascalCase(tag: string): string {
  return tag
    .split("-")
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join("");
}

describe("Element Plus 按需注册", () => {
  it("模板里用到的 el-* 组件都已在 elementPlus.ts 中注册", () => {
    const app = createApp({});
    app.use(elementPlus);

    const missing: string[] = [];
    let used = 0;
    for (const source of Object.values(templates)) {
      for (const match of source.matchAll(/<el-([a-z0-9-]+)/g)) {
        used += 1;
        const name = `El${toPascalCase(match[1])}`;
        if (app.component(name) === undefined) {
          missing.push(`${name}（${match[0]}）`);
        }
      }
    }

    // 防止 glob 意外读不到文件时这条断言变成空转。
    expect(used).toBeGreaterThan(0);
    expect(missing).toEqual([]);
  });
});
