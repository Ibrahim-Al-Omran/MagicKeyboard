import "./styles.css";
import { createSettingsApp } from "./app";
import {
  MockIpcAdapter,
  TauriIpcAdapter,
  type IpcAdapter,
  type TauriInvoke,
} from "./ipc";

declare global {
  interface Window {
    __MAGIC_KEYBOARD_IPC_INVOKE__?: TauriInvoke;
    __TAURI_INTERNALS__?: unknown;
    __TAURI__?: unknown;
  }
}

function isTauriRuntime(): boolean {
  return (
    typeof window !== "undefined" &&
    ("__TAURI_INTERNALS__" in window ||
      "__TAURI__" in window ||
      import.meta.env.VITE_IPC_MODE === "tauri")
  );
}

async function createAdapter(): Promise<IpcAdapter> {
  if (window.__MAGIC_KEYBOARD_IPC_INVOKE__) {
    return new TauriIpcAdapter(window.__MAGIC_KEYBOARD_IPC_INVOKE__);
  }

  if (isTauriRuntime()) {
    const { invoke } = await import("@tauri-apps/api/core");
    const bridge: TauriInvoke = (command, args) => invoke(command, args);
    window.__MAGIC_KEYBOARD_IPC_INVOKE__ = bridge;
    return new TauriIpcAdapter(bridge);
  }

  return new MockIpcAdapter({ latencyMs: 80 });
}

async function boot(): Promise<void> {
  const root = document.querySelector<HTMLElement>("#app");
  if (!root) {
    throw new Error("The settings application root was not found.");
  }

  try {
    const app = createSettingsApp(root, await createAdapter());
    await app.start();
  } catch (error) {
    const alert = document.createElement("p");
    alert.className = "startup-error";
    alert.setAttribute("role", "alert");
    alert.textContent =
      error instanceof Error ? error.message : "Settings could not start.";
    root.replaceChildren(alert);
  }
}

void boot();
