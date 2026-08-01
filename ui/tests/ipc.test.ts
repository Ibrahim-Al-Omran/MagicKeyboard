import { describe, expect, it, vi } from "vitest";
import {
  IpcError,
  MockIpcAdapter,
  SettingsClient,
  TauriIpcAdapter,
  type TauriInvoke,
} from "../src/ipc";

describe("mock IPC adapter", () => {
  it("returns isolated snapshots", async () => {
    const client = new SettingsClient(new MockIpcAdapter());
    const first = await client.getSnapshot();
    first.devices[0]!.name = "Mutated in UI";

    const second = await client.getSnapshot();
    expect(second.devices[0]!.name).toBe("Magic Keyboard");
  });

  it("updates function lock and active profile", async () => {
    const client = new SettingsClient(new MockIpcAdapter());

    const fnResult = await client.setFnLock("magic-keyboard-1", true);
    const profileResult = await client.setActiveProfile(
      "magic-keyboard-1",
      "travel",
    );

    expect(fnResult.fnLock).toBe(true);
    expect(profileResult.activeProfileId).toBe("travel");
    expect((await client.getSnapshot()).devices[0]!.fnLock).toBe(true);
  });

  it("models native offline failures", async () => {
    const client = new SettingsClient(new MockIpcAdapter());

    await expect(
      client.setFnLock("travel-keyboard", false),
    ).rejects.toMatchObject({
      code: "offline",
    });
  });

  it("validates the complete configuration before saving", async () => {
    const client = new SettingsClient(new MockIpcAdapter());
    const snapshot = await client.getSnapshot();
    const standard = snapshot.profiles.find(
      (profile) => profile.id === "standard",
    )!;
    standard.name = "Travel";

    await expect(client.saveProfile(standard)).rejects.toThrow(
      "Profile names must be unique.",
    );
  });
});

describe("Tauri IPC adapter", () => {
  it("forwards typed command payloads", async () => {
    const invokeMock = vi.fn().mockResolvedValue({
      id: "magic-keyboard-1",
      name: "Magic Keyboard",
      modelId: "magic-keyboard-touch-id",
      connection: "connected",
      battery: null,
      fnLock: true,
      activeProfileId: "standard",
    });
    const invoke: TauriInvoke = <T>(command: string, args?: Record<string, unknown>) =>
      invokeMock(command, args) as Promise<T>;
    const client = new SettingsClient(new TauriIpcAdapter(invoke));

    await client.setFnLock("magic-keyboard-1", true);

    expect(invokeMock).toHaveBeenCalledWith("set_fn_lock", {
      deviceId: "magic-keyboard-1",
      enabled: true,
    });
  });

  it("normalizes native command failures", async () => {
    const invoke: TauriInvoke = () => Promise.reject(new Error("denied"));
    const client = new SettingsClient(new TauriIpcAdapter(invoke));

    await expect(client.getSnapshot()).rejects.toEqual(
      new IpcError("denied", "tauri_command_failed"),
    );
  });

  it("preserves safe string errors returned by Rust commands", async () => {
    const invoke: TauriInvoke = () => Promise.reject("device unavailable");
    const client = new SettingsClient(new TauriIpcAdapter(invoke));

    await expect(client.getSnapshot()).rejects.toEqual(
      new IpcError("device unavailable", "tauri_command_failed"),
    );
  });
});
