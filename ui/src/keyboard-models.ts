import type { KeyboardModelId } from "./model";

export interface KeyboardKey {
  code: string;
  label: string;
  secondary?: string;
  width?: number;
  unavailable?: boolean;
}

export interface KeyboardModel {
  id: KeyboardModelId;
  name: string;
  rows: KeyboardKey[][];
}

const key = (
  code: string,
  label = code,
  secondary?: string,
  width?: number,
): KeyboardKey => ({ code, label, secondary, width });

const functionRow = (ending: KeyboardKey): KeyboardKey[] => [
  key("Escape", "esc"),
  ...Array.from({ length: 12 }, (_, index) =>
    key(`F${index + 1}`, `F${index + 1}`),
  ),
  ending,
];

const compactBody: KeyboardKey[][] = [
  [
    key("Backquote", "`", "~"),
    ...Array.from({ length: 10 }, (_, index) =>
      key(`Digit${(index + 1) % 10}`, String((index + 1) % 10)),
    ),
    key("Minus", "-", "_"),
    key("Equal", "=", "+"),
    key("Backspace", "delete", undefined, 2),
  ],
  [
    key("Tab", "tab", undefined, 1.5),
    ..."QWERTYUIOP".split("").map((letter) => key(`Key${letter}`, letter)),
    key("BracketLeft", "[", "{"),
    key("BracketRight", "]", "}"),
    key("Backslash", "\\", "|", 1.5),
  ],
  [
    key("CapsLock", "caps lock", undefined, 1.8),
    ..."ASDFGHJKL".split("").map((letter) => key(`Key${letter}`, letter)),
    key("Semicolon", ";", ":"),
    key("Quote", "'", '"'),
    key("Enter", "return", undefined, 2.2),
  ],
  [
    key("ShiftLeft", "shift", undefined, 2.3),
    ..."ZXCVBNM".split("").map((letter) => key(`Key${letter}`, letter)),
    key("Comma", ",", "<"),
    key("Period", ".", ">"),
    key("Slash", "/", "?"),
    key("ShiftRight", "shift", undefined, 2.3),
  ],
  [
    key("Fn", "fn"),
    key("ControlLeft", "control", undefined, 1.2),
    key("AltLeft", "option", "⌥", 1.2),
    key("MetaLeft", "command", "⌘", 1.4),
    key("Space", "", undefined, 5.6),
    key("MetaRight", "command", "⌘", 1.4),
    key("AltRight", "option", "⌥", 1.2),
    key("ArrowLeft", "←"),
    key("ArrowDown", "↓"),
    key("ArrowUp", "↑"),
    key("ArrowRight", "→"),
  ],
];

const compactRows = (ending: KeyboardKey): KeyboardKey[][] => [
  functionRow(ending),
  ...compactBody,
];

const numericRows: KeyboardKey[][] = [
  [
    key("Escape", "esc"),
    ...Array.from({ length: 19 }, (_, index) =>
      key(`F${index + 1}`, `F${index + 1}`),
    ),
    { code: "TouchID", label: "Touch ID", unavailable: true },
  ],
  [
    ...compactBody[0]!,
    key("Insert", "help"),
    key("Home", "home"),
    key("PageUp", "page up"),
    key("NumLock", "clear"),
    key("NumpadDivide", "/"),
    key("NumpadMultiply", "*"),
    key("NumpadSubtract", "-"),
  ],
  [
    ...compactBody[1]!,
    key("Delete", "delete"),
    key("End", "end"),
    key("PageDown", "page down"),
    key("Numpad7", "7"),
    key("Numpad8", "8"),
    key("Numpad9", "9"),
    key("NumpadAdd", "+"),
  ],
  [
    ...compactBody[2]!,
    key("Numpad4", "4"),
    key("Numpad5", "5"),
    key("Numpad6", "6"),
  ],
  [
    ...compactBody[3]!,
    key("ArrowUp", "↑"),
    key("Numpad1", "1"),
    key("Numpad2", "2"),
    key("Numpad3", "3"),
    key("NumpadEnter", "enter"),
  ],
  [
    ...compactBody[4]!.slice(0, 7),
    key("ArrowLeft", "←"),
    key("ArrowDown", "↓"),
    key("ArrowRight", "→"),
    key("Numpad0", "0", undefined, 2),
    key("NumpadDecimal", "."),
  ],
];

export const KEYBOARD_MODELS: readonly KeyboardModel[] = [
  {
    id: "magic-keyboard-touch-id",
    name: "Magic Keyboard with Touch ID",
    rows: compactRows({ code: "TouchID", label: "Touch ID", unavailable: true }),
  },
  {
    id: "magic-keyboard-compact",
    name: "Magic Keyboard A1644 (compact)",
    rows: compactRows({
      code: "Eject",
      label: "eject",
      secondary: "⏏",
      // Windows never delivers Consumer Eject (0xB8) to apps — needs a HID filter.
      unavailable: true,
    }),
  },
  {
    id: "magic-keyboard-numeric",
    name: "Magic Keyboard with Numeric Keypad",
    rows: numericRows,
  },
];

export function keyboardModel(id: KeyboardModelId): KeyboardModel {
  return KEYBOARD_MODELS.find((model) => model.id === id) ?? KEYBOARD_MODELS[0]!;
}
