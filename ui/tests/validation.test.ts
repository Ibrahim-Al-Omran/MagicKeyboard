import { describe, expect, it } from "vitest";
import type { KeyboardConfig } from "../src/model";
import {
  ConfigurationError,
  assertValidConfig,
  validateConfig,
  validateProfile,
} from "../src/validation";

const validConfig = (): KeyboardConfig => ({
  version: 1,
  profiles: [
    {
      id: "standard",
      name: "Standard",
      mappings: [
        {
          sourceKey: "CapsLock",
          action: { type: "shortcut", keys: ["Control", "Shift", "P"] },
        },
        {
          sourceKey: "F6",
          action: { type: "media", command: "play-pause" },
        },
      ],
    },
  ],
  deviceProfiles: { "magic-keyboard-1": "standard" },
});

describe("profile validation", () => {
  it("accepts supported mapping actions", () => {
    const profile = validConfig().profiles[0];
    expect(profile).toBeDefined();
    expect(validateProfile(profile!)).toEqual([]);
  });

  it("reports duplicate source keys and malformed shortcuts", () => {
    const profile = validConfig().profiles[0]!;
    profile.mappings = [
      { sourceKey: "F1", action: { type: "disabled" } },
      {
        sourceKey: "f1",
        action: { type: "shortcut", keys: ["Control", "control"] },
      },
    ];

    const issues = validateProfile(profile);
    expect(issues.map((issue) => issue.path)).toContain(
      "profile.mappings[1].sourceKey",
    );
    expect(issues.map((issue) => issue.path)).toContain(
      "profile.mappings[1].action.keys",
    );
  });

  it("requires launch targets", () => {
    const profile = validConfig().profiles[0]!;
    profile.mappings = [
      { sourceKey: "F12", action: { type: "launch", target: " " } },
    ];

    expect(validateProfile(profile)).toContainEqual({
      path: "profile.mappings[0].action.target",
      message: "Enter an application target of 260 characters or fewer.",
    });
  });

  it("counts user-facing limits in JavaScript UTF-16 code units", () => {
    const profile = validConfig().profiles[0]!;
    profile.name = "😀".repeat(20);
    expect(validateProfile(profile)).toEqual([]);

    profile.name = "😀".repeat(21);
    expect(validateProfile(profile).map((issue) => issue.path)).toContain(
      "profile.name",
    );
  });
});

describe("configuration validation", () => {
  it("accepts a complete configuration", () => {
    expect(validateConfig(validConfig())).toEqual([]);
    expect(() => assertValidConfig(validConfig())).not.toThrow();
  });

  it("rejects duplicate profiles and missing device assignments", () => {
    const config = validConfig();
    config.profiles.push({
      id: "standard",
      name: "standard",
      mappings: [],
    });
    config.deviceProfiles["other-keyboard"] = "missing";

    const messages = validateConfig(config).map((issue) => issue.message);
    expect(messages).toContain("Profile IDs must be unique.");
    expect(messages).toContain("Profile names must be unique.");
    expect(messages).toContain("Assigned profile does not exist.");
    expect(() => assertValidConfig(config)).toThrow(ConfigurationError);
  });
});
