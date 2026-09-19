import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vitest/config";

/**
 * 测试配置。与 vite.config.ts 保持同一个 `@` 别名。
 *
 * - 单元测试跑在 jsdom 里，DOM 相关的 hook（useDebouncedValue）可以直接测；
 * - 纯函数测试（mergeItems / sourceIcons）不依赖 DOM，jsdom 只是默认环境，不影响它们；
 * - `@tauri-apps/api` 只在事件与 invoke 里用到，相关测试一律通过 vi.mock 隔离，
 *   测试进程里永远不会有真正的 Tauri 运行时。
 */
export default defineConfig({
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}"],
    // 只统计 src 下被测试覆盖的代码，排除测试文件自身与入口。
    coverage: {
      include: ["src/lib/**", "src/hooks/**", "src/types/**"],
    },
  },
});
