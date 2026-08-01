import type {
  AppSnapshot,
  DeviceId,
  DeviceStatus,
  KeyboardConfig,
  Profile,
  ProfileId,
} from "./model";
import { FUNCTION_ROW_DEFAULTS, cloneAction } from "./model";
import { assertValidConfig } from "./validation";

interface CommandSpec<Request, Response> {
  request: Request;
  response: Response;
}

export interface IpcCommandMap {
  get_snapshot: CommandSpec<Record<string, never>, AppSnapshot>;
  refresh_devices: CommandSpec<Record<string, never>, AppSnapshot>;
  select_device: CommandSpec<{ deviceId: DeviceId }, AppSnapshot>;
  set_fn_lock: CommandSpec<
    { deviceId: DeviceId; enabled: boolean },
    DeviceStatus
  >;
  set_launch_at_startup: CommandSpec<{ enabled: boolean }, AppSnapshot>;
  set_active_profile: CommandSpec<
    { deviceId: DeviceId; profileId: ProfileId },
    DeviceStatus
  >;
  save_profile: CommandSpec<{ profile: Profile }, AppSnapshot>;
  get_diagnostics: CommandSpec<
    Record<string, never>,
    import("./model").RemapDiagnostics
  >;
  test_volume_up: CommandSpec<Record<string, never>, void>;
  test_delete: CommandSpec<Record<string, never>, void>;
}

export type IpcCommand = keyof IpcCommandMap;
type RequestOf<K extends IpcCommand> = IpcCommandMap[K]["request"];
type ResponseOf<K extends IpcCommand> = IpcCommandMap[K]["response"];

export interface IpcAdapter {
  invoke<K extends IpcCommand>(
    command: K,
    request: RequestOf<K>,
  ): Promise<ResponseOf<K>>;
}

export class SettingsClient {
  constructor(private readonly adapter: IpcAdapter) {}

  getSnapshot(): Promise<AppSnapshot> {
    return this.adapter.invoke("get_snapshot", {});
  }

  refreshDevices(): Promise<AppSnapshot> {
    return this.adapter.invoke("refresh_devices", {});
  }

  selectDevice(deviceId: DeviceId): Promise<AppSnapshot> {
    return this.adapter.invoke("select_device", { deviceId });
  }

  setFnLock(deviceId: DeviceId, enabled: boolean): Promise<DeviceStatus> {
    return this.adapter.invoke("set_fn_lock", { deviceId, enabled });
  }

  setLaunchAtStartup(enabled: boolean): Promise<AppSnapshot> {
    return this.adapter.invoke("set_launch_at_startup", { enabled });
  }

  setActiveProfile(
    deviceId: DeviceId,
    profileId: ProfileId,
  ): Promise<DeviceStatus> {
    return this.adapter.invoke("set_active_profile", { deviceId, profileId });
  }

  saveProfile(profile: Profile): Promise<AppSnapshot> {
    return this.adapter.invoke("save_profile", { profile });
  }

  getDiagnostics() {
    return this.adapter.invoke("get_diagnostics", {});
  }

  testVolumeUp(): Promise<void> {
    return this.adapter.invoke("test_volume_up", {});
  }

  testDelete(): Promise<void> {
    return this.adapter.invoke("test_delete", {});
  }
}

export class IpcError extends Error {
  constructor(
    message: string,
    public readonly code = "ipc_error",
  ) {
    super(message);
    this.name = "IpcError";
  }
}

export type TauriInvoke = <T>(
  command: string,
  args?: Record<string, unknown>,
) => Promise<T>;

export class TauriIpcAdapter implements IpcAdapter {
  constructor(private readonly invokeTauri: TauriInvoke) {}

  async invoke<K extends IpcCommand>(
    command: K,
    request: RequestOf<K>,
  ): Promise<ResponseOf<K>> {
    try {
      return await this.invokeTauri<ResponseOf<K>>(
        command,
        request as Record<string, unknown>,
      );
    } catch (error) {
      const message =
        error instanceof Error
          ? error.message
          : typeof error === "string"
            ? error
            : "The native service failed.";
      throw new IpcError(message, "tauri_command_failed");
    }
  }
}

const DEFAULT_SNAPSHOT: AppSnapshot = {
  devices: [
    {
      id: "magic-keyboard-1",
      name: "Magic Keyboard",
      modelId: "magic-keyboard-compact",
      connection: "connected",
      battery: { percent: 84, charging: false },
      fnLock: false,
      activeProfileId: "standard",
    },
    {
      id: "travel-keyboard",
      name: "Travel Keyboard",
      modelId: "magic-keyboard-compact",
      connection: "disconnected",
      battery: null,
      fnLock: true,
      activeProfileId: "travel",
    },
  ],
  selectedDeviceId: "magic-keyboard-1",
  remapperActive: false,
  launchAtStartup: true,
  statusMessage:
    "Browser preview uses mock devices. Open the native .exe for real remapping.",
  diagnostics: {
    hookInstalled: false,
    remappingEnabled: false,
    requireMagicKeyboard: false,
    magicKeyboardPresent: false,
    fnLock: false,
    bindingCount: 0,
    rawInputReady: false,
    rawMagicDevices: 0,
    keysSeen: 0,
    lastVk: null,
    lastKeyName: null,
    lastAction: null,
    lastError: null,
    lastStatus: null,
    recentKeys: [],
  },
  profiles: [
    {
      id: "standard",
      name: "Standard",
      mappings: [
        ...FUNCTION_ROW_DEFAULTS.map(({ key, action }) => ({
          sourceKey: key,
          action: cloneAction(action),
        })),
        {
          sourceKey: "CapsLock",
          action: { type: "shortcut", keys: ["Control", "Shift", "P"] },
        },
      ],
    },
    {
      id: "travel",
      name: "Travel",
      mappings: [
        {
          sourceKey: "F12",
          action: { type: "launch", target: "calculator" },
        },
      ],
    },
  ],
};

export interface MockIpcOptions {
  initialSnapshot?: AppSnapshot;
  latencyMs?: number;
}

export class MockIpcAdapter implements IpcAdapter {
  private snapshot: AppSnapshot;
  private readonly latencyMs: number;

  constructor(options: MockIpcOptions = {}) {
    this.snapshot = structuredClone(
      options.initialSnapshot ?? DEFAULT_SNAPSHOT,
    );
    this.latencyMs = options.latencyMs ?? 0;
    this.assertSnapshotConfig();
  }

  async invoke<K extends IpcCommand>(
    command: K,
    request: RequestOf<K>,
  ): Promise<ResponseOf<K>> {
    if (this.latencyMs > 0) {
      await new Promise((resolve) => setTimeout(resolve, this.latencyMs));
    }

    const response = this.dispatch(command, request);
    return structuredClone(response) as ResponseOf<K>;
  }

  private dispatch(command: IpcCommand, request: unknown): unknown {
    switch (command) {
      case "get_snapshot":
      case "refresh_devices":
        return this.snapshot;
      case "select_device": {
        const { deviceId } = request as RequestOf<"select_device">;
        this.requireDevice(deviceId);
        this.snapshot.selectedDeviceId = deviceId;
        return this.snapshot;
      }
      case "set_fn_lock": {
        const { deviceId, enabled } = request as RequestOf<"set_fn_lock">;
        const device = this.requireConnectedDevice(deviceId);
        device.fnLock = enabled;
        return device;
      }
      case "set_launch_at_startup": {
        const { enabled } = request as RequestOf<"set_launch_at_startup">;
        this.snapshot.launchAtStartup = enabled;
        return this.snapshot;
      }
      case "set_active_profile": {
        const { deviceId, profileId } =
          request as RequestOf<"set_active_profile">;
        if (!this.snapshot.profiles.some((profile) => profile.id === profileId)) {
          throw new IpcError("The selected profile does not exist.", "not_found");
        }
        const device = this.requireConnectedDevice(deviceId);
        device.activeProfileId = profileId;
        return device;
      }
      case "save_profile": {
        const { profile } = request as RequestOf<"save_profile">;
        const profileIndex = this.snapshot.profiles.findIndex(
          (candidate) => candidate.id === profile.id,
        );
        if (profileIndex < 0) {
          throw new IpcError("The profile does not exist.", "not_found");
        }

        const nextProfiles = structuredClone(this.snapshot.profiles);
        nextProfiles[profileIndex] = structuredClone(profile);
        assertValidConfig(this.toConfig(nextProfiles));
        this.snapshot.profiles = nextProfiles;
        return this.snapshot;
      }
      case "get_diagnostics":
        return (
          this.snapshot.diagnostics ?? {
            hookInstalled: false,
            remappingEnabled: false,
            requireMagicKeyboard: false,
            magicKeyboardPresent: false,
            fnLock: false,
            bindingCount: 0,
            lastVk: null,
            lastKeyName: null,
            lastAction: null,
            lastError: null,
            recentKeys: [],
          }
        );
      case "test_volume_up":
        return undefined;
      case "test_delete":
        return undefined;
      default: {
        const exhaustive: never = command;
        throw new IpcError(
          `Unsupported command: ${String(exhaustive)}`,
          "unsupported_command",
        );
      }
    }
  }

  private requireDevice(deviceId: DeviceId): DeviceStatus {
    const device = this.snapshot.devices.find(
      (candidate) => candidate.id === deviceId,
    );
    if (!device) {
      throw new IpcError("The selected device does not exist.", "not_found");
    }
    return device;
  }

  private requireConnectedDevice(deviceId: DeviceId): DeviceStatus {
    const device = this.requireDevice(deviceId);
    if (device.connection !== "connected") {
      throw new IpcError("The selected device is not connected.", "offline");
    }
    return device;
  }

  private toConfig(profiles = this.snapshot.profiles): KeyboardConfig {
    return {
      version: 1,
      profiles,
      deviceProfiles: Object.fromEntries(
        this.snapshot.devices.flatMap((device) =>
          device.activeProfileId
            ? [[device.id, device.activeProfileId] as const]
            : [],
        ),
      ),
    };
  }

  private assertSnapshotConfig(): void {
    assertValidConfig(this.toConfig());
  }
}
