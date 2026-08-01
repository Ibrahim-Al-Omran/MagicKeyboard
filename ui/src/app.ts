import { SettingsClient, type IpcAdapter } from "./ipc";
import { KEYBOARD_MODELS, keyboardModel } from "./keyboard-models";
import {
  FUNCTION_ROW_DEFAULTS,
  MEDIA_COMMANDS,
  cloneAction,
  cloneProfile,
  type AppSnapshot,
  type DeviceStatus,
  type KeyboardModelId,
  DESTINATION_KEYS,
  LAUNCH_PRESETS,
  SHORTCUT_PRESETS,
  type KeyAction,
  type KeyMapping,
  type Profile,
} from "./model";
import { validateProfile, type ValidationIssue } from "./validation";

interface ElementOptions {
  className?: string;
  text?: string;
  attributes?: Record<string, string>;
}

function element<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  options: ElementOptions = {},
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (options.className) node.className = options.className;
  if (options.text !== undefined) node.textContent = options.text;
  for (const [name, value] of Object.entries(options.attributes ?? {})) {
    node.setAttribute(name, value);
  }
  return node;
}

function option(value: string, label: string, selected = false): HTMLOptionElement {
  const node = element("option", { text: label });
  node.value = value;
  node.selected = selected;
  return node;
}

function actionLabel(type: KeyAction["type"]): string {
  switch (type) {
    case "disabled":
      return "Disabled";
    case "key":
      return "Single key";
    case "shortcut":
      return "Keyboard shortcut";
    case "media":
      return "Media control";
    case "launch":
      return "Launch application";
  }
}

function defaultAction(type: KeyAction["type"]): KeyAction {
  switch (type) {
    case "disabled":
      return { type };
    case "key":
      return { type, key: "Delete" };
    case "shortcut":
      return { type, keys: ["Control", "C"] };
    case "media":
      return { type, command: "play-pause" };
    case "launch":
      return { type, target: "" };
  }
}

function formatConnection(connection: DeviceStatus["connection"]): string {
  return connection[0]?.toUpperCase() + connection.slice(1);
}

function actionsEqual(left: KeyAction, right: KeyAction): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

function describeAction(action: KeyAction): string {
  switch (action.type) {
    case "disabled":
      return "Disabled";
    case "key":
      return action.key;
    case "shortcut":
      return action.keys.join(" + ");
    case "media":
      return action.command.replaceAll("-", " ");
    case "launch":
      return `Open ${action.target || "application"}`;
  }
}

export class SettingsApp {
  private readonly client: SettingsClient;
  private snapshot: AppSnapshot | null = null;
  private draftProfile: Profile | null = null;
  private issues: ValidationIssue[] = [];
  private errorMessage = "";
  private notice = "";
  private busy = false;
  private keyboardModelOverride: KeyboardModelId | null = null;
  private selectedKeyboardKey = "F1";

  constructor(
    private readonly root: HTMLElement,
    adapter: IpcAdapter,
  ) {
    this.client = new SettingsClient(adapter);
  }

  async start(): Promise<void> {
    this.busy = true;
    this.render();
    try {
      this.snapshot = await this.client.getSnapshot();
      this.syncDraftToSelectedDevice();
    } catch (error) {
      this.errorMessage = this.messageFrom(error);
    } finally {
      this.busy = false;
      this.render();
    }
  }

  private render(): void {
    this.root.replaceChildren();
    this.root.setAttribute("aria-busy", String(this.busy));

    const shell = element("div", { className: "app-shell" });
    shell.append(this.renderHeader());

    shell.append(
      element("div", {
        className: "sr-only",
        text: this.notice,
        attributes: { role: "status", "aria-live": "polite" },
      }),
    );

    if (this.snapshot?.statusMessage) {
      shell.append(
        element("div", {
          className: this.snapshot.remapperActive
            ? "banner banner-ok"
            : "banner banner-error",
          text: this.snapshot.statusMessage,
          attributes: { role: "status" },
        }),
      );
    }

    if (this.errorMessage) {
      shell.append(
        element("div", {
          className: "banner banner-error",
          text: this.errorMessage,
          attributes: { role: "alert" },
        }),
      );
    }

    if (!this.snapshot) {
      shell.append(
        element("p", {
          className: "loading",
          text: this.busy ? "Loading keyboard settings…" : "Settings unavailable.",
        }),
      );
    } else {
      const main = element("main");
      main.append(
        this.renderDevicePanel(),
        this.renderDiagnosticsPanel(),
        this.renderKeyboardPanel(),
        this.renderFunctionRowPanel(),
        this.renderProfilePanel(),
      );
      shell.append(main);
    }

    shell.append(
      element("footer", {
        text: "Keep this app open for remapping to stay active. Closing the window stops remapping. Firmware is never modified.",
      }),
    );
    this.root.append(shell);
  }

  private renderHeader(): HTMLElement {
    const header = element("header", { className: "page-header" });
    const titleGroup = element("div");
    titleGroup.append(
      element("p", { className: "eyebrow", text: "Magic Keyboard" }),
      element("h1", { text: "Settings" }),
      element("p", {
        className: "lede",
        text: "Manage connected keyboards, profiles, and key behavior.",
      }),
    );

    const refresh = element("button", {
      className: "secondary-button",
      text: this.busy ? "Refreshing…" : "Refresh devices",
      attributes: { type: "button" },
    });
    refresh.disabled = this.busy;
    refresh.addEventListener("click", () => void this.refreshDevices());
    header.append(titleGroup, refresh);
    return header;
  }

  private renderDiagnosticsPanel(): HTMLElement {
    const section = element("section", {
      className: "panel",
      attributes: { "aria-labelledby": "diagnostics-heading" },
    });
    const diag = this.snapshot?.diagnostics;
    section.append(
      element("h2", {
        text: "Remapper diagnostics",
        attributes: { id: "diagnostics-heading" },
      }),
      element("p", {
        className: "section-copy",
        text: "Press keys on your Magic Keyboard. The last captured key should appear here. Use Test volume to verify output injection.",
      }),
    );

    const stats = element("dl", { className: "device-stats" });
    const rows: Array<[string, string]> = [
      ["Hook", diag?.hookInstalled ? "Installed" : "Missing"],
      ["Remapping", diag?.remappingEnabled ? "Enabled" : "Disabled"],
      ["Function lock", diag?.fnLock ? "ON (F-keys standard)" : "OFF (specials default)"],
      ["Fn held", diag?.fnHeld ? "Yes (Right Ctrl or Apple Fn)" : "No — hold Right Ctrl for F-keys"],
      ["Bindings", String(diag?.bindingCount ?? 0)],
      ["Keys seen", String(diag?.keysSeen ?? 0)],
      [
        "Raw Input",
        diag?.rawInputReady
          ? `Ready (${diag.rawMagicDevices ?? 0} Apple device handles)`
          : "Not ready",
      ],
      ["Last key", diag?.lastKeyName ?? "None yet — press A or F12"],
      ["Last action", diag?.lastAction ?? "None"],
      ["Last status", diag?.lastStatus ?? "None"],
      ["Last error", diag?.lastError ?? "None"],
      ["Recent keys", (diag?.recentKeys ?? []).join(", ") || "None"],
    ];
    for (const [label, value] of rows) {
      stats.append(element("dt", { text: label }), element("dd", { text: value }));
    }
    section.append(stats);

    const actions = element("div", { className: "form-actions" });
    const refreshDiag = element("button", {
      className: "secondary-button",
      text: "Refresh diagnostics",
      attributes: { type: "button" },
    });
    refreshDiag.addEventListener("click", () => void this.refreshDiagnostics());
    const testVolume = element("button", {
      className: "primary-button",
      text: "Test volume up",
      attributes: { type: "button" },
    });
    testVolume.addEventListener("click", () => void this.testVolumeUp());
    const testDelete = element("button", {
      className: "secondary-button",
      text: "Test Delete",
      attributes: { type: "button" },
    });
    testDelete.addEventListener("click", () => void this.testDelete());
    actions.append(refreshDiag, testVolume, testDelete);
    section.append(actions);
    return section;
  }

  private renderDevicePanel(): HTMLElement {
    const section = element("section", {
      className: "panel",
      attributes: { "aria-labelledby": "device-heading" },
    });
    section.append(
      element("h2", {
        text: "Device status",
        attributes: { id: "device-heading" },
      }),
    );

    const device = this.selectedDevice();
    const selectGroup = element("div", { className: "field" });
    const label = element("label", {
      text: "Keyboard",
      attributes: { for: "device-select" },
    });
    const select = element("select", {
      attributes: { id: "device-select" },
    });
    for (const candidate of this.snapshot?.devices ?? []) {
      select.append(
        option(
          candidate.id,
          `${candidate.name} — ${formatConnection(candidate.connection)}`,
          candidate.id === this.snapshot?.selectedDeviceId,
        ),
      );
    }
    select.disabled = this.busy;
    select.addEventListener("change", () => void this.selectDevice(select.value));
    selectGroup.append(label, select);
    section.append(selectGroup);

    if (!device) {
      section.append(
        element("p", {
          className: "empty-state",
          text: "No keyboards have been discovered.",
        }),
      );
      return section;
    }

    const stats = element("dl", { className: "device-stats" });
    stats.append(
      element("dt", { text: "Connection" }),
      this.statusValue(device.connection),
      element("dt", { text: "Battery" }),
      this.batteryValue(device),
    );
    section.append(stats);

    const controls = element("div", { className: "device-controls" });
    const fnField = element("label", {
      className: "toggle-field",
      attributes: { for: "fn-lock" },
    });
    const fnToggle = element("input", {
      attributes: { type: "checkbox", id: "fn-lock" },
    });
    fnToggle.checked = device.fnLock;
    fnToggle.disabled = this.busy || device.connection !== "connected";
    fnToggle.addEventListener("change", () => {
      void this.setFnLock(device.id, fnToggle.checked);
    });
    const fnCopy = element("span");
    fnCopy.append(
      element("strong", { text: "Use F1–F12 as standard function keys" }),
      element("small", {
        text: "Off: F11 = volume down. Hold Right Ctrl + F11 for real F11 (Apple Fn is blocked by Windows). On: F-keys are standard; hold Right Ctrl for specials.",
      }),
    );
    fnField.append(fnToggle, fnCopy);

    const startupField = element("label", {
      className: "toggle-field",
      attributes: { for: "launch-at-startup" },
    });
    const startupToggle = element("input", {
      attributes: { type: "checkbox", id: "launch-at-startup" },
    });
    startupToggle.checked = this.snapshot?.launchAtStartup ?? true;
    startupToggle.disabled = this.busy;
    startupToggle.addEventListener("change", () => {
      void this.setLaunchAtStartup(startupToggle.checked);
    });
    const startupCopy = element("span");
    startupCopy.append(
      element("strong", { text: "Launch at Windows startup" }),
      element("small", {
        text: "Keep remapping active after you sign in. The installer also enables this by default.",
      }),
    );
    startupField.append(startupToggle, startupCopy);

    const profileField = element("div", { className: "field" });
    profileField.append(
      element("label", {
        text: "Active profile",
        attributes: { for: "active-profile" },
      }),
    );
    const profileSelect = element("select", {
      attributes: { id: "active-profile" },
    });
    for (const profile of this.snapshot?.profiles ?? []) {
      profileSelect.append(
        option(profile.id, profile.name, profile.id === device.activeProfileId),
      );
    }
    profileSelect.disabled =
      this.busy ||
      device.connection !== "connected" ||
      (this.snapshot?.profiles.length ?? 0) === 0;
    profileSelect.addEventListener("change", () => {
      void this.setActiveProfile(device.id, profileSelect.value);
    });
    profileField.append(profileSelect);
    controls.append(fnField, startupField, profileField);
    section.append(controls);
    return section;
  }

  private renderKeyboardPanel(): HTMLElement {
    const section = element("section", {
      className: "panel keyboard-panel",
      attributes: { "aria-labelledby": "keyboard-heading" },
    });
    const device = this.selectedDevice();
    const detectedModel = keyboardModel(
      device?.modelId ?? "magic-keyboard-compact",
    );
    if (detectedModel.id === "magic-keyboard-compact") {
      section.append(
        element("p", {
          className: "banner banner-warning",
          text: "A1644 Eject needs the HID lower filter (driver/). Until that is installed and test-signed, Caps Lock → Delete. User-mode alone cannot see Eject.",
        }),
      );
    }
    const activeModel = keyboardModel(
      this.keyboardModelOverride ?? detectedModel.id,
    );

    const heading = element("div", { className: "section-heading-row" });
    const copy = element("div");
    copy.append(
      element("h2", {
        text: "Keyboard layout",
        attributes: { id: "keyboard-heading" },
      }),
      element("p", {
        className: "section-copy",
        text: "Select a key on the keyboard, then choose what it should do.",
      }),
    );
    const modelField = element("div", { className: "field model-picker" });
    modelField.append(
      element("label", {
        text: "Keyboard model",
        attributes: { for: "keyboard-model" },
      }),
    );
    const modelSelect = element("select", {
      attributes: { id: "keyboard-model" },
    });
    modelSelect.append(
      option(
        "auto",
        `Auto-detect — ${detectedModel.name}`,
        this.keyboardModelOverride === null,
      ),
    );
    for (const model of KEYBOARD_MODELS) {
      modelSelect.append(
        option(model.id, model.name, this.keyboardModelOverride === model.id),
      );
    }
    modelSelect.addEventListener("change", () => {
      this.keyboardModelOverride =
        modelSelect.value === "auto"
          ? null
          : (modelSelect.value as KeyboardModelId);
      this.render();
    });
    modelField.append(modelSelect);
    heading.append(copy, modelField);
    section.append(heading);

    const boardViewport = element("div", { className: "keyboard-viewport" });
    const board = element("div", {
      className: "keyboard",
      attributes: {
        role: "group",
        "aria-label": `${activeModel.name} key layout`,
      },
    });
    for (const rowKeys of activeModel.rows) {
      const row = element("div", { className: "keyboard-row" });
      for (const keyboardKey of rowKeys) {
        const mapping = this.mappingForKey(keyboardKey.code);
        const button = element("button", {
          className: [
            "keyboard-key",
            mapping ? "is-mapped" : "",
            this.selectedKeyboardKey === keyboardKey.code ? "is-selected" : "",
          ]
            .filter(Boolean)
            .join(" "),
          attributes: {
            type: "button",
            title: keyboardKey.unavailable
              ? `${keyboardKey.label} is not configurable on Windows`
              : mapping
                ? `${keyboardKey.label}: ${describeAction(mapping.action)}`
                : `${keyboardKey.label}: pass through`,
            "aria-pressed": String(this.selectedKeyboardKey === keyboardKey.code),
          },
        });
        button.style.setProperty("--key-width", String(keyboardKey.width ?? 1));
        button.disabled = keyboardKey.unavailable ?? false;
        button.append(
          element("span", { className: "key-secondary", text: keyboardKey.secondary }),
          element("span", { className: "key-primary", text: keyboardKey.label }),
        );
        button.addEventListener("click", () => {
          this.selectedKeyboardKey = keyboardKey.code;
          this.render();
        });
        row.append(button);
      }
      board.append(row);
    }
    boardViewport.append(board);
    section.append(boardViewport, this.renderSelectedKeyEditor());
    return section;
  }

  private renderSelectedKeyEditor(): HTMLElement {
    const editor = element("div", { className: "key-editor" });
    editor.append(
      element("div", {
        className: "selected-key-name",
        text: this.selectedKeyboardKey,
      }),
    );

    if (!this.draftProfile) {
      editor.append(
        element("p", {
          className: "field-note",
          text: "Assign a profile before editing key mappings.",
        }),
      );
      return editor;
    }

    const mapping = this.mappingForKey(this.selectedKeyboardKey);
    const typeField = element("div", { className: "field" });
    typeField.append(
      element("label", {
        text: "Maps to",
        attributes: { for: "visual-key-action" },
      }),
    );
    const typeSelect = element("select", {
      attributes: { id: "visual-key-action" },
    });
    typeSelect.append(option("pass", "Pass through", !mapping));
    const types: KeyAction["type"][] = [
      "disabled",
      "key",
      "shortcut",
      "media",
      "launch",
    ];
    for (const type of types) {
      typeSelect.append(
        option(type, actionLabel(type), mapping?.action.type === type),
      );
    }
    typeSelect.addEventListener("change", () => {
      if (typeSelect.value === "pass") {
        this.removeMappingForKey(this.selectedKeyboardKey);
      } else {
        this.setFunctionMapping(
          this.selectedKeyboardKey,
          defaultAction(typeSelect.value as KeyAction["type"]),
        );
      }
    });
    typeField.append(typeSelect);
    editor.append(typeField);

    if (mapping) {
      const index = this.draftProfile.mappings.indexOf(mapping);
      editor.append(this.renderActionValue(mapping, index, "visual-key"));
    } else {
      editor.append(
        element("p", {
          className: "field-note key-pass-through",
          text: "Windows receives this key unchanged.",
        }),
      );
    }
    return editor;
  }

  private renderFunctionRowPanel(): HTMLElement {
    const section = element("section", {
      className: "panel function-row-panel",
      attributes: { "aria-labelledby": "function-row-heading" },
    });
    const heading = element("div", { className: "section-heading-row" });
    const copy = element("div");
    copy.append(
      element("h2", {
        text: "Function row",
        attributes: { id: "function-row-heading" },
      }),
      element("p", {
        className: "section-copy",
        text: "Set F1–F12 to the controls printed on a modern Magic Keyboard, or choose another action.",
      }),
    );
    const restore = element("button", {
      className: "secondary-button",
      text: "Restore Magic Keyboard defaults",
      attributes: { type: "button" },
    });
    restore.disabled = this.busy || !this.draftProfile;
    restore.addEventListener("click", () => this.restoreFunctionRowDefaults());
    heading.append(copy, restore);
    section.append(heading);

    if (!this.draftProfile) {
      section.append(
        element("p", {
          className: "empty-state",
          text: "Select a device with an assigned profile to configure its function row.",
        }),
      );
      return section;
    }

    const grid = element("div", {
      className: "function-key-grid",
      attributes: { role: "group", "aria-label": "F1 through F12 controls" },
    });
    for (const definition of FUNCTION_ROW_DEFAULTS) {
      const mapping = this.draftProfile.mappings.find(
        (candidate) => candidate.sourceKey.toUpperCase() === definition.key,
      );
      const selectedAction = mapping?.action ?? definition.action;
      const card = element("div", { className: "function-key-card" });
      const cap = element("div", { className: "function-key-cap" });
      cap.append(
        element("span", { className: "function-symbol", text: definition.symbol }),
        element("kbd", { text: definition.key }),
      );

      const id = `function-${definition.key.toLowerCase()}`;
      const field = element("div", { className: "field" });
      field.append(
        element("label", {
          text: definition.label,
          attributes: { for: id },
        }),
      );
      const select = element("select", { attributes: { id } });
      const choices: Array<{ label: string; action: KeyAction }> = [
        { label: definition.label, action: definition.action },
        { label: `Standard ${definition.key}`, action: { type: "key", key: definition.key } },
        ...FUNCTION_ROW_DEFAULTS.filter((candidate) => candidate.key !== definition.key).map(
          (candidate) => ({ label: candidate.label, action: candidate.action }),
        ),
        { label: "Disabled", action: { type: "disabled" } },
      ];
      const seen = new Set<string>();
      for (const choice of choices) {
        const value = JSON.stringify(choice.action);
        if (seen.has(value)) continue;
        seen.add(value);
        select.append(option(value, choice.label, actionsEqual(choice.action, selectedAction)));
      }
      if (!seen.has(JSON.stringify(selectedAction))) {
        select.append(
          option(
            JSON.stringify(selectedAction),
            "Custom action (edit below)",
            true,
          ),
        );
      }
      select.disabled = this.busy;
      select.addEventListener("change", () => {
        this.setFunctionMapping(definition.key, JSON.parse(select.value) as KeyAction);
      });
      field.append(select);
      card.append(cap, field);
      grid.append(card);
    }
    section.append(
      grid,
      element("p", {
        className: "field-note function-row-note",
        text: "These choices are saved with the active profile. Use the detailed mappings below for shortcuts or application launches.",
      }),
    );
    return section;
  }

  private statusValue(connection: DeviceStatus["connection"]): HTMLElement {
    const value = element("dd");
    value.append(
      element("span", {
        className: `status-badge status-${connection}`,
        text: formatConnection(connection),
      }),
    );
    return value;
  }

  private batteryValue(device: DeviceStatus): HTMLElement {
    const value = element("dd");
    if (!device.battery) {
      value.textContent = "Unavailable";
      return value;
    }

    const meter = element("meter", {
      attributes: {
        min: "0",
        max: "100",
        value: String(device.battery.percent),
        "aria-label": `Battery ${device.battery.percent} percent`,
      },
    });
    meter.textContent = `${device.battery.percent}%`;
    value.append(
      meter,
      document.createTextNode(
        ` ${device.battery.percent}%${device.battery.charging ? " · Charging" : ""}`,
      ),
    );
    return value;
  }

  private renderProfilePanel(): HTMLElement {
    const section = element("section", {
      className: "panel profile-panel",
      attributes: { "aria-labelledby": "profile-heading" },
    });
    section.append(
      element("h2", {
        text: "Profile configuration",
        attributes: { id: "profile-heading" },
      }),
      element("p", {
        className: "section-copy",
        text: "Map a physical key to another key, a shortcut, media control, or an application.",
      }),
    );

    if (!this.draftProfile) {
      section.append(
        element("p", {
          className: "empty-state",
          text: "Select a device with an assigned profile to edit mappings.",
        }),
      );
      return section;
    }

    if (this.issues.length > 0) {
      section.append(this.renderValidationSummary());
    }

    const form = element("form", {
      attributes: { novalidate: "", "aria-label": "Profile configuration" },
    });
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      void this.saveProfile();
    });

    const nameField = element("div", { className: "field" });
    nameField.append(
      element("label", {
        text: "Profile name",
        attributes: { for: "profile-name" },
      }),
    );
    const nameInput = element("input", {
      attributes: {
        id: "profile-name",
        name: "profile-name",
        maxlength: "40",
        autocomplete: "off",
      },
    });
    nameInput.value = this.draftProfile.name;
    this.applyValidity(nameInput, "profile.name");
    nameInput.addEventListener("input", () => {
      if (this.draftProfile) this.draftProfile.name = nameInput.value;
    });
    nameField.append(nameInput);
    form.append(nameField);

    const mappings = element("fieldset", { className: "mapping-list" });
    mappings.append(element("legend", { text: "Key mappings" }));
    this.draftProfile.mappings.forEach((mapping, index) => {
      mappings.append(this.renderMapping(mapping, index));
    });

    if (this.draftProfile.mappings.length === 0) {
      mappings.append(
        element("p", {
          className: "empty-state",
          text: "No custom mappings. Unmapped keys keep their normal behavior.",
        }),
      );
    }

    const addMapping = element("button", {
      className: "secondary-button",
      text: "Add mapping",
      attributes: { type: "button", id: "add-mapping" },
    });
    addMapping.disabled = this.busy || this.draftProfile.mappings.length >= 128;
    addMapping.addEventListener("click", () => this.addMapping());
    mappings.append(addMapping);
    form.append(mappings);

    const actions = element("div", { className: "form-actions" });
    const save = element("button", {
      className: "primary-button",
      text: this.busy ? "Saving…" : "Save profile",
      attributes: { type: "submit" },
    });
    save.disabled = this.busy;
    actions.append(save);
    form.append(actions);
    section.append(form);
    return section;
  }

  private renderMapping(mapping: KeyMapping, index: number): HTMLElement {
    const group = element("fieldset", { className: "mapping-row" });
    group.append(element("legend", { text: `Mapping ${index + 1}` }));

    const sourceId = `mapping-${index}-source`;
    const sourceField = element("div", { className: "field" });
    sourceField.append(
      element("label", {
        text: "Source key",
        attributes: { for: sourceId },
      }),
    );
    const source = element("input", {
      attributes: { id: sourceId, autocomplete: "off" },
    });
    source.value = mapping.sourceKey;
    this.applyValidity(source, `profile.mappings[${index}].sourceKey`);
    source.addEventListener("input", () => {
      mapping.sourceKey = source.value;
    });
    sourceField.append(source);

    const typeId = `mapping-${index}-action`;
    const typeField = element("div", { className: "field" });
    typeField.append(
      element("label", {
        text: "Action",
        attributes: { for: typeId },
      }),
    );
    const typeSelect = element("select", { attributes: { id: typeId } });
    const types: KeyAction["type"][] = [
      "disabled",
      "key",
      "shortcut",
      "media",
      "launch",
    ];
    for (const type of types) {
      typeSelect.append(option(type, actionLabel(type), type === mapping.action.type));
    }
    typeSelect.addEventListener("change", () => {
      mapping.action = defaultAction(typeSelect.value as KeyAction["type"]);
      this.issues = [];
      this.render();
    });
    typeField.append(typeSelect);

    const actionField = this.renderActionValue(mapping, index);
    const remove = element("button", {
      className: "text-button danger",
      text: "Remove",
      attributes: {
        type: "button",
        "aria-label": `Remove mapping ${index + 1} for ${mapping.sourceKey || "unnamed key"}`,
      },
    });
    remove.addEventListener("click", () => {
      this.draftProfile?.mappings.splice(index, 1);
      this.issues = [];
      this.render();
    });

    group.append(sourceField, typeField, actionField, remove);
    return group;
  }

  private renderActionValue(
    mapping: KeyMapping,
    index: number,
    idPrefix = `mapping-${index}`,
  ): HTMLElement {
    const field = element("div", { className: "field mapping-value" });
    const id = `${idPrefix}-action-value`;
    if (mapping.action.type === "disabled") {
      field.append(
        element("p", {
          className: "field-note",
          text: "This key will not send an action.",
        }),
      );
      return field;
    }

    field.append(
      element("label", {
        text:
          mapping.action.type === "key"
            ? "Destination key"
            : mapping.action.type === "shortcut"
              ? "Shortcut keys"
              : mapping.action.type === "media"
                ? "Media action"
                : "Application target",
        attributes: { for: id },
      }),
    );

    if (mapping.action.type === "media") {
      const select = element("select", { attributes: { id } });
      for (const command of MEDIA_COMMANDS) {
        select.append(
          option(
            command,
            command.replaceAll("-", " "),
            command === mapping.action.command,
          ),
        );
      }
      this.applyValidity(select, `profile.mappings[${index}].action`);
      select.addEventListener("change", () => {
        if (mapping.action.type === "media") {
          mapping.action.command = select.value as typeof mapping.action.command;
        }
        this.render();
      });
      field.append(select);
      return field;
    }

    const action = mapping.action;
    if (action.type === "key") {
      const select = element("select", { attributes: { id } });
      const known = new Set(DESTINATION_KEYS.map((entry) => entry.value.toLowerCase()));
      for (const entry of DESTINATION_KEYS) {
        select.append(
          option(entry.value, entry.label, entry.value === action.key),
        );
      }
      if (action.key && !known.has(action.key.toLowerCase())) {
        select.append(option(action.key, `${action.key} (custom)`, true));
      }
      this.applyValidity(select, `profile.mappings[${index}].action`);
      select.addEventListener("change", () => {
        if (mapping.action.type === "key") {
          mapping.action.key = select.value;
        }
        this.render();
      });
      field.append(
        select,
        element("small", {
          className: "field-note",
          text: "Pick a destination key from the list, then save the profile.",
        }),
      );
      return field;
    }

    if (action.type === "shortcut") {
      const select = element("select", { attributes: { id } });
      const current = action.keys.join("+").toLowerCase();
      let matched = false;
      for (const preset of SHORTCUT_PRESETS) {
        const value = preset.keys.join("+");
        const selected = value.toLowerCase() === current;
        matched ||= selected;
        select.append(option(value, preset.label, selected));
      }
      select.append(option("__custom__", "Custom shortcut…", !matched));
      select.addEventListener("change", () => {
        if (mapping.action.type !== "shortcut") return;
        if (select.value === "__custom__") {
          mapping.action.keys = ["Control", "C"];
        } else {
          mapping.action.keys = select.value.split("+").filter(Boolean);
        }
        this.render();
      });
      field.append(select);

      if (!matched || select.value === "__custom__") {
        const input = element("input", {
          attributes: {
            id: `${id}-custom`,
            autocomplete: "off",
            placeholder: "Control + Shift + P",
            "aria-describedby": `${idPrefix}-shortcut-help`,
          },
        });
        input.value = action.keys.join(" + ");
        input.addEventListener("change", () => {
          if (mapping.action.type === "shortcut") {
            mapping.action.keys = input.value
              .split("+")
              .map((key) => key.trim())
              .filter(Boolean);
          }
        });
        field.append(
          input,
          element("small", {
            text: "Separate keys with +.",
            attributes: { id: `${idPrefix}-shortcut-help` },
          }),
        );
        this.applyValidity(input, `profile.mappings[${index}].action`);
      } else {
        this.applyValidity(select, `profile.mappings[${index}].action`);
      }
      return field;
    }

    if (action.type === "launch") {
      const select = element("select", { attributes: { id } });
      const current = action.target.trim().toLowerCase();
      let matched = false;
      for (const preset of LAUNCH_PRESETS) {
        const selected = preset.value === current;
        matched ||= selected;
        select.append(option(preset.value, preset.label, selected));
      }
      select.append(option("__custom__", "Custom path…", !matched));
      select.addEventListener("change", () => {
        if (mapping.action.type !== "launch") return;
        mapping.action.target =
          select.value === "__custom__" ? "" : select.value;
        this.render();
      });
      field.append(select);
      if (!matched) {
        const input = element("input", {
          attributes: {
            id: `${id}-custom`,
            autocomplete: "off",
            placeholder: "C:\\Path\\Application.exe",
          },
        });
        input.value = action.target;
        input.addEventListener("input", () => {
          if (mapping.action.type === "launch") {
            mapping.action.target = input.value;
          }
        });
        field.append(input);
        this.applyValidity(input, `profile.mappings[${index}].action`);
      } else {
        this.applyValidity(select, `profile.mappings[${index}].action`);
      }
      return field;
    }

    return field;
  }

  private renderValidationSummary(): HTMLElement {
    const summary = element("div", {
      className: "banner banner-error validation-summary",
      attributes: { role: "alert", id: "validation-summary", tabindex: "-1" },
    });
    summary.append(
      element("strong", { text: "Check the profile before saving:" }),
    );
    const list = element("ul");
    for (const issue of this.issues) {
      const item = element("li");
      const targetId = this.fieldIdForIssue(issue.path);
      if (targetId) {
        const link = element("a", {
          text: issue.message,
          attributes: { href: `#${targetId}` },
        });
        item.append(link);
      } else {
        item.textContent = issue.message;
      }
      list.append(item);
    }
    summary.append(list);
    return summary;
  }

  private applyValidity(control: HTMLElement, pathPrefix: string): void {
    if (this.issues.some((issue) => issue.path.startsWith(pathPrefix))) {
      control.setAttribute("aria-invalid", "true");
      const describedBy = control.getAttribute("aria-describedby");
      control.setAttribute(
        "aria-describedby",
        [describedBy, "validation-summary"].filter(Boolean).join(" "),
      );
    }
  }

  private fieldIdForIssue(path: string): string | null {
    if (path === "profile.name") return "profile-name";
    const mappingMatch = /^profile\.mappings\[(\d+)]\.(.+)$/.exec(path);
    if (!mappingMatch) return path === "profile.mappings" ? "add-mapping" : null;
    const [, index, field] = mappingMatch;
    return field === "sourceKey"
      ? `mapping-${index}-source`
      : `mapping-${index}-action-value`;
  }

  private selectedDevice(): DeviceStatus | null {
    return (
      this.snapshot?.devices.find(
        (device) => device.id === this.snapshot?.selectedDeviceId,
      ) ?? null
    );
  }

  private mappingForKey(sourceKey: string): KeyMapping | undefined {
    return this.draftProfile?.mappings.find(
      (mapping) => mapping.sourceKey.toLowerCase() === sourceKey.toLowerCase(),
    );
  }

  private syncDraftToSelectedDevice(): void {
    const device = this.selectedDevice();
    const profile = this.snapshot?.profiles.find(
      (candidate) => candidate.id === device?.activeProfileId,
    );
    this.draftProfile = profile ? cloneProfile(profile) : null;
    this.issues = [];
  }

  private replaceDevice(updatedDevice: DeviceStatus): void {
    if (!this.snapshot) return;
    this.snapshot.devices = this.snapshot.devices.map((device) =>
      device.id === updatedDevice.id ? updatedDevice : device,
    );
  }

  private async refreshDevices(): Promise<void> {
    await this.perform(async () => {
      this.snapshot = await this.client.refreshDevices();
      this.syncDraftToSelectedDevice();
      this.notice = "Device status refreshed.";
    });
  }

  private async refreshDiagnostics(): Promise<void> {
    await this.perform(async () => {
      if (!this.snapshot) return;
      this.snapshot.diagnostics = await this.client.getDiagnostics();
      this.notice = "Diagnostics refreshed.";
    });
  }

  private async testVolumeUp(): Promise<void> {
    await this.perform(async () => {
      await this.client.testVolumeUp();
      if (this.snapshot) {
        this.snapshot.diagnostics = await this.client.getDiagnostics();
      }
      this.notice = "Sent a volume-up test command.";
    });
  }

  private async testDelete(): Promise<void> {
    await this.perform(async () => {
      await this.client.testDelete();
      if (this.snapshot) {
        this.snapshot.diagnostics = await this.client.getDiagnostics();
      }
      this.notice =
        "Sent a Delete test key. Focus a text field first — it should delete forward.";
    });
  }

  private async selectDevice(deviceId: string): Promise<void> {
    await this.perform(async () => {
      this.snapshot = await this.client.selectDevice(deviceId);
      this.keyboardModelOverride = null;
      this.selectedKeyboardKey = "F1";
      this.syncDraftToSelectedDevice();
      this.notice = "Selected device changed.";
    });
  }

  private async setFnLock(deviceId: string, enabled: boolean): Promise<void> {
    await this.perform(async () => {
      this.replaceDevice(await this.client.setFnLock(deviceId, enabled));
      this.notice = `Function lock ${enabled ? "enabled" : "disabled"}.`;
    });
  }

  private async setLaunchAtStartup(enabled: boolean): Promise<void> {
    await this.perform(async () => {
      this.snapshot = await this.client.setLaunchAtStartup(enabled);
      this.syncDraftToSelectedDevice();
      this.notice = enabled
        ? "Magic Keyboard will launch when you sign in to Windows."
        : "Startup launch disabled.";
    });
  }

  private async setActiveProfile(
    deviceId: string,
    profileId: string,
  ): Promise<void> {
    await this.perform(async () => {
      this.replaceDevice(
        await this.client.setActiveProfile(deviceId, profileId),
      );
      this.syncDraftToSelectedDevice();
      this.notice = "Active profile changed.";
    });
  }

  private async saveProfile(): Promise<void> {
    if (!this.draftProfile) return;
    this.issues = validateProfile(this.draftProfile);
    if (this.issues.length > 0) {
      this.notice = "";
      this.render();
      this.root
        .querySelector<HTMLElement>("#validation-summary")
        ?.focus();
      return;
    }

    const profileId = this.draftProfile.id;
    await this.perform(async () => {
      this.snapshot = await this.client.saveProfile(
        cloneProfile(this.draftProfile as Profile),
      );
      const saved = this.snapshot.profiles.find(
        (profile) => profile.id === profileId,
      );
      this.draftProfile = saved ? cloneProfile(saved) : null;
      this.issues = [];
      this.notice = "Profile saved.";
    });
  }

  private addMapping(): void {
    if (!this.draftProfile) return;
    const existing = new Set(
      this.draftProfile.mappings.map((mapping) =>
        mapping.sourceKey.toLowerCase(),
      ),
    );
    let sourceKey = "F1";
    for (let index = 1; index <= 24; index += 1) {
      const candidate = `F${index}`;
      if (!existing.has(candidate.toLowerCase())) {
        sourceKey = candidate;
        break;
      }
    }
    this.draftProfile.mappings.push({
      sourceKey,
      action: { type: "disabled" },
    });
    this.issues = [];
    this.render();
    this.root
      .querySelector<HTMLInputElement>(
        `#mapping-${this.draftProfile.mappings.length - 1}-source`,
      )
      ?.focus();
  }

  private setFunctionMapping(sourceKey: string, action: KeyAction): void {
    if (!this.draftProfile) return;
    const existing = this.draftProfile.mappings.find(
      (mapping) => mapping.sourceKey.toLowerCase() === sourceKey.toLowerCase(),
    );
    if (existing) {
      existing.action = cloneAction(action);
    } else {
      this.draftProfile.mappings.push({ sourceKey, action: cloneAction(action) });
    }
    this.issues = [];
    this.render();
  }

  private restoreFunctionRowDefaults(): void {
    if (!this.draftProfile) return;
    const functionKeys = new Set(FUNCTION_ROW_DEFAULTS.map(({ key }) => key));
    const otherMappings = this.draftProfile.mappings.filter(
      (mapping) => !functionKeys.has(mapping.sourceKey.toUpperCase()),
    );
    this.draftProfile.mappings = [
      ...FUNCTION_ROW_DEFAULTS.map(({ key, action }) => ({
        sourceKey: key,
        action: cloneAction(action),
      })),
      ...otherMappings,
    ];
    this.issues = [];
    this.notice = "Magic Keyboard function-row defaults restored. Save the profile to apply them.";
    this.render();
  }

  private removeMappingForKey(sourceKey: string): void {
    if (!this.draftProfile) return;
    this.draftProfile.mappings = this.draftProfile.mappings.filter(
      (mapping) => mapping.sourceKey.toLowerCase() !== sourceKey.toLowerCase(),
    );
    this.issues = [];
    this.render();
  }

  private async perform(work: () => Promise<void>): Promise<void> {
    this.busy = true;
    this.errorMessage = "";
    this.render();
    try {
      await work();
    } catch (error) {
      this.errorMessage = this.messageFrom(error);
    } finally {
      this.busy = false;
      this.render();
    }
  }

  private messageFrom(error: unknown): string {
    return error instanceof Error
      ? error.message
      : "An unexpected settings error occurred.";
  }
}

export function createSettingsApp(
  root: HTMLElement,
  adapter: IpcAdapter,
): SettingsApp {
  return new SettingsApp(root, adapter);
}
