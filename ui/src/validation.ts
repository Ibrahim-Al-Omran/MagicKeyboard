import {
  MEDIA_COMMANDS,
  type KeyboardConfig,
  type KeyAction,
  type Profile,
} from "./model";

export interface ValidationIssue {
  path: string;
  message: string;
}

const ID_PATTERN = /^[a-z0-9][a-z0-9._-]{0,63}$/i;
const KEY_PATTERN = /^[a-z0-9][a-z0-9_-]{0,31}$/i;

function actionIssues(action: KeyAction, path: string): ValidationIssue[] {
  switch (action.type) {
    case "disabled":
      return [];
    case "key":
      return KEY_PATTERN.test(action.key)
        ? []
        : [{ path: `${path}.key`, message: "Choose a valid destination key." }];
    case "shortcut": {
      const normalized = action.keys.map((key) => key.trim().toLowerCase());
      const issues: ValidationIssue[] = [];
      if (action.keys.length < 1 || action.keys.length > 4) {
        issues.push({
          path: `${path}.keys`,
          message: "A shortcut must contain between one and four keys.",
        });
      }
      if (action.keys.some((key) => !KEY_PATTERN.test(key))) {
        issues.push({
          path: `${path}.keys`,
          message: "Every shortcut key must be a valid key name.",
        });
      }
      if (new Set(normalized).size !== normalized.length) {
        issues.push({
          path: `${path}.keys`,
          message: "A shortcut cannot contain the same key twice.",
        });
      }
      return issues;
    }
    case "media":
      return MEDIA_COMMANDS.includes(action.command)
        ? []
        : [{ path: `${path}.command`, message: "Choose a valid media action." }];
    case "launch": {
      const target = action.target.trim();
      return target.length > 0 && target.length <= 260
        ? []
        : [
            {
              path: `${path}.target`,
              message: "Enter an application target of 260 characters or fewer.",
            },
          ];
    }
    default: {
      const exhaustive: never = action;
      return [
        {
          path,
          message: `Unsupported action: ${String(exhaustive)}`,
        },
      ];
    }
  }
}

export function validateProfile(
  profile: Profile,
  path = "profile",
): ValidationIssue[] {
  const issues: ValidationIssue[] = [];

  if (!ID_PATTERN.test(profile.id)) {
    issues.push({
      path: `${path}.id`,
      message: "Profile ID must be a short, URL-safe identifier.",
    });
  }

  const name = profile.name.trim();
  if (name.length < 1 || name.length > 40) {
    issues.push({
      path: `${path}.name`,
      message: "Profile name must contain between 1 and 40 characters.",
    });
  }

  if (profile.mappings.length > 128) {
    issues.push({
      path: `${path}.mappings`,
      message: "A profile can contain at most 128 mappings.",
    });
  }

  const seenKeys = new Set<string>();
  profile.mappings.forEach((mapping, index) => {
    const mappingPath = `${path}.mappings[${index}]`;
    const normalizedKey = mapping.sourceKey.trim().toLowerCase();

    if (!KEY_PATTERN.test(mapping.sourceKey)) {
      issues.push({
        path: `${mappingPath}.sourceKey`,
        message: "Choose a valid source key.",
      });
    } else if (seenKeys.has(normalizedKey)) {
      issues.push({
        path: `${mappingPath}.sourceKey`,
        message: "Each source key can be mapped only once per profile.",
      });
    }

    seenKeys.add(normalizedKey);
    issues.push(...actionIssues(mapping.action, `${mappingPath}.action`));
  });

  return issues;
}

export function validateConfig(config: KeyboardConfig): ValidationIssue[] {
  const issues: ValidationIssue[] = [];

  if (config.version !== 1) {
    issues.push({
      path: "version",
      message: "Only configuration version 1 is supported.",
    });
  }

  if (config.profiles.length < 1) {
    issues.push({
      path: "profiles",
      message: "At least one profile is required.",
    });
  }

  const profileIds = new Set<string>();
  const profileNames = new Set<string>();
  config.profiles.forEach((profile, index) => {
    issues.push(...validateProfile(profile, `profiles[${index}]`));

    const normalizedName = profile.name.trim().toLowerCase();
    if (profileIds.has(profile.id)) {
      issues.push({
        path: `profiles[${index}].id`,
        message: "Profile IDs must be unique.",
      });
    }
    if (profileNames.has(normalizedName)) {
      issues.push({
        path: `profiles[${index}].name`,
        message: "Profile names must be unique.",
      });
    }
    profileIds.add(profile.id);
    profileNames.add(normalizedName);
  });

  for (const [deviceId, profileId] of Object.entries(config.deviceProfiles)) {
    if (!ID_PATTERN.test(deviceId)) {
      issues.push({
        path: `deviceProfiles.${deviceId}`,
        message: "Device ID must be a short, URL-safe identifier.",
      });
    }
    if (!profileIds.has(profileId)) {
      issues.push({
        path: `deviceProfiles.${deviceId}`,
        message: "Assigned profile does not exist.",
      });
    }
  }

  return issues;
}

export function assertValidConfig(config: KeyboardConfig): void {
  const issues = validateConfig(config);
  if (issues.length > 0) {
    throw new ConfigurationError(issues);
  }
}

export class ConfigurationError extends Error {
  constructor(public readonly issues: ValidationIssue[]) {
    super(issues.map((issue) => `${issue.path}: ${issue.message}`).join("\n"));
    this.name = "ConfigurationError";
  }
}
