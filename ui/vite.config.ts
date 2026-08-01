import { defineConfig } from "vitest/config";

export default defineConfig(({ mode }) => {
  const isTauri = mode === "tauri";

  return {
    // Relative asset URLs are required inside the Tauri webview.
    base: isTauri ? "./" : "/",
    define: isTauri
      ? {
          "import.meta.env.VITE_IPC_MODE": JSON.stringify("tauri"),
        }
      : undefined,
    build: {
      target: "es2022",
    },
    test: {
      environment: "happy-dom",
      include: ["tests/**/*.test.ts"],
      restoreMocks: true,
    },
  };
});
