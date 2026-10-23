import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

export function isDesktopShell(): boolean {
  return (
    typeof window !== "undefined" &&
    Object.prototype.hasOwnProperty.call(window, "__TAURI_INTERNALS__")
  );
}

/** Opens the operating system's folder chooser and returns the chosen path. */
export async function pickFolder(
  current: string | null,
  title: string,
): Promise<string | null> {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      title,
      ...(current !== null && current.trim() !== ""
        ? { defaultPath: current.trim() }
        : {}),
    });
    if (selected === null) return null;
    if (typeof selected !== "string") {
      throw new Error("The folder chooser returned more than one folder.");
    }
    return selected;
  } catch (error) {
    throw new Error(`The folder chooser could not open: ${errorText(error)}`, {
      cause: error,
    });
  }
}

export async function openDownloadedFile(path: string): Promise<void> {
  try {
    await invoke("open_download_file", { path });
  } catch (error) {
    throw new Error(`The file could not be opened: ${errorText(error)}`, {
      cause: error,
    });
  }
}

export async function revealDownloadedFile(path: string): Promise<void> {
  try {
    await revealItemInDir(path);
  } catch (error) {
    throw new Error(
      `The containing folder could not be opened: ${errorText(error)}`,
      { cause: error },
    );
  }
}

export async function readClipboardText(): Promise<string> {
  try {
    if (navigator.clipboard === undefined) {
      throw new Error("Clipboard access is unavailable.");
    }
    return await navigator.clipboard.readText();
  } catch (error) {
    throw new Error(`RAPIDS could not read the clipboard: ${errorText(error)}`, {
      cause: error,
    });
  }
}

export async function writeClipboardText(text: string): Promise<void> {
  try {
    if (navigator.clipboard === undefined) {
      throw new Error("Clipboard access is unavailable.");
    }
    await navigator.clipboard.writeText(text);
  } catch (error) {
    throw new Error(`RAPIDS could not copy the link: ${errorText(error)}`, {
      cause: error,
    });
  }
}

function errorText(error: unknown): string {
  if (typeof error === "string" && error.trim() !== "") return error;
  if (error instanceof Error && error.message.trim() !== "") return error.message;
  return "the native bridge did not answer.";
}
