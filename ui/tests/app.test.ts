import { beforeEach, describe, expect, it } from "vitest";
import { createSettingsApp } from "../src/app";
import { MockIpcAdapter } from "../src/ipc";

async function waitUntil(assertion: () => boolean): Promise<void> {
  for (let attempt = 0; attempt < 30; attempt += 1) {
    if (assertion()) return;
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  throw new Error("Timed out waiting for the UI to update.");
}

describe("settings screen", () => {
  beforeEach(() => {
    document.body.innerHTML = '<div id="app"></div>';
  });

  it("renders semantic, labelled device and profile controls", async () => {
    const root = document.querySelector<HTMLElement>("#app")!;
    await createSettingsApp(root, new MockIpcAdapter()).start();

    expect(root.querySelector("h1")?.textContent).toBe("Settings");
    expect(root.querySelector("main")).not.toBeNull();
    expect(root.querySelector('[aria-labelledby="device-heading"]')).not.toBeNull();
    expect(root.querySelector('label[for="device-select"]')?.textContent).toBe(
      "Keyboard",
    );
    expect(root.querySelector('meter[aria-label="Battery 84 percent"]')).not.toBeNull();
    expect(root.querySelector('form[aria-label="Profile configuration"]')).not.toBeNull();
    expect(root.querySelector('label[for="fn-lock"]')).not.toBeNull();
    expect(root.querySelector("footer")?.textContent).toContain(
      "Keep this app open",
    );
  });

  it("updates function lock through the mock boundary", async () => {
    const root = document.querySelector<HTMLElement>("#app")!;
    await createSettingsApp(root, new MockIpcAdapter()).start();

    const toggle = root.querySelector<HTMLInputElement>("#fn-lock")!;
    toggle.checked = true;
    toggle.dispatchEvent(new Event("change", { bubbles: true }));

    await waitUntil(
      () =>
        root.getAttribute("aria-busy") === "false" &&
        root.querySelector<HTMLInputElement>("#fn-lock")?.checked === true,
    );
    expect(root.querySelector('[role="status"]')?.textContent).toContain(
      "enabled",
    );
  });

  it("shows the Magic Keyboard function row and restores its defaults", async () => {
    const root = document.querySelector<HTMLElement>("#app")!;
    await createSettingsApp(root, new MockIpcAdapter()).start();

    expect(root.querySelectorAll(".function-key-card")).toHaveLength(12);
    expect(root.querySelector('label[for="function-f1"]')?.textContent).toBe(
      "Brightness down",
    );
    expect(root.querySelector('label[for="function-f2"]')?.textContent).toBe(
      "Brightness up",
    );
    expect(
      root.querySelector<HTMLSelectElement>("#function-f12")?.selectedOptions[0]
        ?.textContent,
    ).toBe("Volume up");

    const restore = [...root.querySelectorAll<HTMLButtonElement>("button")].find(
      (button) => button.textContent === "Restore Magic Keyboard defaults",
    )!;
    restore.click();

    expect(root.querySelector('[role="status"]')?.textContent).toContain(
      "defaults restored",
    );
  });

  it("auto-detects a keyboard model and maps keys from its visual layout", async () => {
    const root = document.querySelector<HTMLElement>("#app")!;
    await createSettingsApp(root, new MockIpcAdapter()).start();

    const model = root.querySelector<HTMLSelectElement>("#keyboard-model")!;
    expect(model.selectedOptions[0]?.textContent).toContain(
      "Auto-detect — Magic Keyboard A1644 (compact)",
    );
    expect(
      root.querySelector(
        '[aria-label="Magic Keyboard A1644 (compact) key layout"]',
      ),
    ).not.toBeNull();
    expect(
      [...root.querySelectorAll<HTMLButtonElement>(".keyboard-key")].some(
        (button) =>
          button.disabled &&
          button.title.toLowerCase().includes("eject"),
      ),
    ).toBe(true);

    const keyA = [...root.querySelectorAll<HTMLButtonElement>(".keyboard-key")].find(
      (button) => button.title.startsWith("A:"),
    )!;
    keyA.click();
    expect(root.querySelector(".selected-key-name")?.textContent).toBe("KeyA");

    const action = root.querySelector<HTMLSelectElement>("#visual-key-action")!;
    action.value = "disabled";
    action.dispatchEvent(new Event("change", { bubbles: true }));
    expect(
      [...root.querySelectorAll<HTMLButtonElement>(".keyboard-key")].find(
        (button) => button.title === "A: Disabled",
      ),
    ).not.toBeUndefined();

    root.querySelector<HTMLSelectElement>("#keyboard-model")!.value =
      "magic-keyboard-numeric";
    root
      .querySelector<HTMLSelectElement>("#keyboard-model")!
      .dispatchEvent(new Event("change", { bubbles: true }));
    expect(
      root.querySelector(
        '[aria-label="Magic Keyboard with Numeric Keypad key layout"]',
      ),
    ).not.toBeNull();
  });

  it("shows an accessible validation summary and marks invalid fields", async () => {
    const root = document.querySelector<HTMLElement>("#app")!;
    await createSettingsApp(root, new MockIpcAdapter()).start();

    const name = root.querySelector<HTMLInputElement>("#profile-name")!;
    name.value = "";
    name.dispatchEvent(new Event("input", { bubbles: true }));
    root
      .querySelector<HTMLFormElement>('form[aria-label="Profile configuration"]')!
      .dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));

    await waitUntil(() => root.querySelector("#validation-summary") !== null);
    expect(
      root.querySelector("#validation-summary")?.getAttribute("role"),
    ).toBe("alert");
    expect(
      root.querySelector<HTMLInputElement>("#profile-name")?.getAttribute(
        "aria-invalid",
      ),
    ).toBe("true");
    expect(document.activeElement?.id).toBe("validation-summary");
  });

  it("disables device mutations for disconnected keyboards", async () => {
    const root = document.querySelector<HTMLElement>("#app")!;
    await createSettingsApp(root, new MockIpcAdapter()).start();

    const deviceSelect =
      root.querySelector<HTMLSelectElement>("#device-select")!;
    deviceSelect.value = "travel-keyboard";
    deviceSelect.dispatchEvent(new Event("change", { bubbles: true }));

    await waitUntil(
      () =>
        root.getAttribute("aria-busy") === "false" &&
        root.querySelector<HTMLSelectElement>("#device-select")?.value ===
          "travel-keyboard",
    );
    expect(root.querySelector<HTMLInputElement>("#fn-lock")?.disabled).toBe(
      true,
    );
    expect(
      root.querySelector<HTMLSelectElement>("#active-profile")?.disabled,
    ).toBe(true);
  });
});
