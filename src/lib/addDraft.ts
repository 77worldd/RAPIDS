import type { TagId, TaskAddInput } from "../types";

/** Editable text stays separate from the host's strictly typed request. */
export interface AddDraft {
  readonly url: string;
  readonly destDir: string;
  readonly category: TagId;
  readonly priority: string;
  readonly connections: string;
  readonly speedLimitBps: string;
  readonly expectedSha256: string;
  readonly scheduleId: string;
}

export function createAddDraft(): AddDraft {
  return {
    url: "",
    destDir: "",
    category: "other",
    priority: "0",
    connections: "",
    speedLimitBps: "",
    expectedSha256: "",
    scheduleId: "",
  };
}

function optionalInteger(value: string, label: string): number | undefined {
  const text = value.trim();
  if (text === "") return undefined;
  const number = Number(text);
  if (!/^-?\d+$/.test(text) || !Number.isSafeInteger(number)) {
    throw new Error(`${label} must be a whole number.`);
  }
  return number;
}

/** The desktop app runs on Windows; relative folders would land in its working directory. */
export function isAbsoluteFolder(folder: string): boolean {
  return /^[a-zA-Z]:[\\/]/.test(folder) || /^\\\\[^\\]+\\[^\\]+(?:\\|$)/.test(folder);
}

export function toAddRequest(draft: AddDraft): TaskAddInput {
  const url = draft.url.trim();
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    throw new Error("Enter a complete HTTP or HTTPS URL.");
  }
  if ((parsed.protocol !== "http:" && parsed.protocol !== "https:") || !parsed.hostname) {
    throw new Error("Only HTTP and HTTPS download URLs are accepted.");
  }
  if (parsed.username !== "" || parsed.password !== "") {
    throw new Error("A download URL cannot contain a username or password.");
  }

  const destDir = draft.destDir.trim();
  if (destDir !== "" && !isAbsoluteFolder(destDir)) {
    throw new Error("The destination folder must be an absolute Windows path.");
  }

  const priority = optionalInteger(draft.priority, "Priority");
  const connections = optionalInteger(draft.connections, "Connections");
  if (connections !== undefined && (connections < 1 || connections > 32)) {
    throw new Error("Connections must be between 1 and 32 for one download.");
  }
  const speedLimitBps = optionalInteger(draft.speedLimitBps, "Speed ceiling");
  if (speedLimitBps !== undefined && speedLimitBps < 1) {
    throw new Error("A speed ceiling must be at least 1 byte per second.");
  }
  const expectedSha256 = draft.expectedSha256.trim();
  if (expectedSha256 !== "" && !/^[a-fA-F0-9]{64}$/.test(expectedSha256)) {
    throw new Error("The SHA-256 checksum must be exactly 64 hexadecimal characters.");
  }
  const scheduleId = optionalInteger(draft.scheduleId, "Start window");
  if (scheduleId !== undefined && scheduleId < 1) {
    throw new Error("Choose a valid start window.");
  }

  return {
    url,
    category: draft.category,
    ...(destDir !== "" ? { destDir } : {}),
    ...(priority !== undefined ? { priority } : {}),
    ...(connections !== undefined ? { connections } : {}),
    ...(speedLimitBps !== undefined ? { speedLimitBps } : {}),
    ...(expectedSha256 !== "" ? { expectedSha256: expectedSha256.toLowerCase() } : {}),
    ...(scheduleId !== undefined ? { scheduleId } : {}),
  };
}
