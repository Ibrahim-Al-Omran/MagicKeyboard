import { invoke } from "@tauri-apps/api/core";
import type { TauriInvoke } from "../src/ipc";

declare global {
  interface Window {
    __MAGIC_KEYBOARD_IPC_INVOKE__?: TauriInvoke;
  }
}

window.__MAGIC_KEYBOARD_IPC_INVOKE__ = (command, args) =>
  invoke(command, args);

await import("../src/main");
