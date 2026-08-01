export type DeviceId = string;
export type ProfileId = string;
export type KeyboardModelId =
  | "magic-keyboard-touch-id"
  | "magic-keyboard-compact"
  | "magic-keyboard-numeric";

export type ConnectionState = "connected" | "connecting" | "disconnected";

export interface BatteryState {
  percent: number;
  charging: boolean;
}

export interface DeviceStatus {
  id: DeviceId;
  name: string;
  modelId: KeyboardModelId;
  connection: ConnectionState;
  battery: BatteryState | null;
  fnLock: boolean;
  activeProfileId: ProfileId | null;
}

export type MediaCommand =
  | "play-pause"
  | "previous"
  | "next"
  | "volume-up"
  | "volume-down"
  | "mute";

export type KeyAction =
  | { type: "disabled" }
  | { type: "key"; key: string }
  | { type: "shortcut"; keys: string[] }
  | { type: "media"; command: MediaCommand }
  | { type: "launch"; target: string };

export interface KeyMapping {
  sourceKey: string;
  action: KeyAction;
}

export interface Profile {
  id: ProfileId;
  name: string;
  mappings: KeyMapping[];
}

export interface KeyboardConfig {
  version: 1;
  profiles: Profile[];
  deviceProfiles: Record<DeviceId, ProfileId>;
}

export interface RemapDiagnostics {
  hookInstalled: boolean;
  remappingEnabled: boolean;
  requireMagicKeyboard: boolean;
  magicKeyboardPresent: boolean;
  fnLock: boolean;
  fnHeld?: boolean;
  bindingCount: number;
  rawInputReady?: boolean;
  rawMagicDevices?: number;
  keysSeen?: number;
  lastVk: number | null;
  lastKeyName: string | null;
  lastAction: string | null;
  lastError: string | null;
  lastStatus?: string | null;
  recentKeys: string[];
}

export interface AppSnapshot {
  devices: DeviceStatus[];
  selectedDeviceId: DeviceId | null;
  profiles: Profile[];
  remapperActive?: boolean;
  statusMessage?: string;
  launchAtStartup?: boolean;
  diagnostics?: RemapDiagnostics;
}

export const MEDIA_COMMANDS: readonly MediaCommand[] = [
  "play-pause",
  "previous",
  "next",
  "volume-up",
  "volume-down",
  "mute",
];

/** Destination keys offered in the mapping UI (must match remapper names). */
export const DESTINATION_KEYS: readonly { value: string; label: string }[] = [
  { value: "Delete", label: "Delete" },
  { value: "Backspace", label: "Backspace" },
  { value: "Escape", label: "Escape" },
  { value: "Enter", label: "Enter" },
  { value: "Tab", label: "Tab" },
  { value: "Space", label: "Space" },
  { value: "Insert", label: "Insert" },
  { value: "Home", label: "Home" },
  { value: "End", label: "End" },
  { value: "PageUp", label: "Page Up" },
  { value: "PageDown", label: "Page Down" },
  { value: "ArrowLeft", label: "Left arrow" },
  { value: "ArrowRight", label: "Right arrow" },
  { value: "ArrowUp", label: "Up arrow" },
  { value: "ArrowDown", label: "Down arrow" },
  { value: "F1", label: "F1" },
  { value: "F2", label: "F2" },
  { value: "F3", label: "F3" },
  { value: "F4", label: "F4" },
  { value: "F5", label: "F5" },
  { value: "F6", label: "F6" },
  { value: "F7", label: "F7" },
  { value: "F8", label: "F8" },
  { value: "F9", label: "F9" },
  { value: "F10", label: "F10" },
  { value: "F11", label: "F11" },
  { value: "F12", label: "F12" },
  { value: "BrightnessDown", label: "Brightness down" },
  { value: "BrightnessUp", label: "Brightness up" },
  { value: "Sleep", label: "Sleep" },
];

export const LAUNCH_PRESETS: readonly { value: string; label: string }[] = [
  { value: "calculator", label: "Calculator" },
  { value: "notepad", label: "Notepad" },
  { value: "explorer", label: "File Explorer" },
];

export const SHORTCUT_PRESETS: readonly { label: string; keys: string[] }[] = [
  { label: "Copy", keys: ["Control", "C"] },
  { label: "Paste", keys: ["Control", "V"] },
  { label: "Cut", keys: ["Control", "X"] },
  { label: "Undo", keys: ["Control", "Z"] },
  { label: "Save", keys: ["Control", "S"] },
  { label: "Select all", keys: ["Control", "A"] },
  { label: "Task view", keys: ["Meta", "Tab"] },
  { label: "Windows search", keys: ["Meta", "S"] },
  { label: "Lock screen", keys: ["Meta", "L"] },
  { label: "Snipping tool", keys: ["Meta", "Shift", "S"] },
];

export interface FunctionRowDefinition {
  key: string;
  symbol: string;
  label: string;
  action: KeyAction;
}

export const FUNCTION_ROW_DEFAULTS: readonly FunctionRowDefinition[] = [
  { key: "F1", symbol: "☀−", label: "Brightness down", action: { type: "key", key: "BrightnessDown" } },
  { key: "F2", symbol: "☀+", label: "Brightness up", action: { type: "key", key: "BrightnessUp" } },
  { key: "F3", symbol: "▣", label: "Task view", action: { type: "shortcut", keys: ["Meta", "Tab"] } },
  { key: "F4", symbol: "⌕", label: "Windows search", action: { type: "shortcut", keys: ["Meta", "S"] } },
  { key: "F5", symbol: "🎙", label: "Dictation", action: { type: "shortcut", keys: ["Meta", "H"] } },
  { key: "F6", symbol: "◐", label: "Sleep", action: { type: "key", key: "Sleep" } },
  { key: "F7", symbol: "◀|", label: "Previous track", action: { type: "media", command: "previous" } },
  { key: "F8", symbol: "▶", label: "Play / pause", action: { type: "media", command: "play-pause" } },
  { key: "F9", symbol: "|▶", label: "Next track", action: { type: "media", command: "next" } },
  { key: "F10", symbol: "🔇", label: "Mute", action: { type: "media", command: "mute" } },
  { key: "F11", symbol: "🔉", label: "Volume down", action: { type: "media", command: "volume-down" } },
  { key: "F12", symbol: "🔊", label: "Volume up", action: { type: "media", command: "volume-up" } },
];

export function cloneAction(action: KeyAction): KeyAction {
  return structuredClone(action);
}

export function cloneProfile(profile: Profile): Profile {
  return structuredClone(profile);
}
