/**
 * Bonaparte Vault — the global asset shelf shared by every project.
 * List/upload/pull through the engine; pulled files flow through the
 * exact same import pipeline as drops.
 */
import { command } from "./bridge";
import { editor, accept } from "./store.svelte";
import type { Snapshot, SnapshotPatch } from "./model";

export interface VaultEntry {
  file: string;
  bytes: number;
  modified_secs: number;
}
export interface VaultFolder {
  name: string;
  entries: VaultEntry[];
}

export async function vaultList(): Promise<{ root: string; folders: VaultFolder[] }> {
  const reply = await command<Snapshot | SnapshotPatch>("vault_list", {
    delta: !!editor.deltaProtocol,
    baseRevision: editor.revision,
  });
  accept(reply);
  const extra = reply as { root?: string; folders?: VaultFolder[] };
  return { root: extra.root ?? "", folders: extra.folders ?? [] };
}

export async function vaultSave(folder: string, name: string, file: File) {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let raw = "";
  for (let i = 0; i < bytes.length; i += 8192)
    raw += String.fromCharCode(...bytes.subarray(i, i + 8192));
  const reply = await command<Snapshot | SnapshotPatch>("vault_save", {
    delta: !!editor.deltaProtocol,
    baseRevision: editor.revision,
    folder,
    name,
    dataBase64: btoa(raw),
  });
  accept(reply);
  return reply as { saved?: string };
}

export async function vaultRead(folder: string, name: string): Promise<File> {
  const reply = await command<{ name?: string; dataBase64?: string }>("vault_read", {
    delta: !!editor.deltaProtocol,
    baseRevision: editor.revision,
    folder,
    name,
  });
  const b64 = reply.dataBase64 ?? "";
  const binary = atob(b64);
  const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0));
  return new File([bytes], name, { type: kindType(folder, name) });
}

function kindType(folder: string, name: string): string {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  const table: Record<string, string> = {
    svg: "image/svg+xml",
    png: "image/png",
    jpg: "image/jpeg",
    jpeg: "image/jpeg",
    webp: "image/webp",
    gif: "image/gif",
    wav: "audio/wav",
    mp3: "audio/mpeg",
    flac: "audio/flac",
    ogg: "audio/ogg",
    m4a: "audio/mp4",
    aac: "audio/aac",
    aiff: "audio/aiff",
    aif: "audio/aiff",
    mp4: "video/mp4",
    webm: "video/webm",
    mov: "video/quicktime",
    json: folder === "lottie" ? "application/lottie+json" : "application/json",
    obj: "model/obj",
  };
  return table[ext] ?? "application/octet-stream";
}

/** Vault folder for a dropped/uploaded file, by extension. */
export function vaultFolderFor(name: string): string {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  if (["svg", "png", "jpg", "jpeg", "webp", "gif"].includes(ext)) return "images";
  if (["wav", "mp3", "flac", "ogg", "oga", "aif", "aiff", "m4a", "aac"].includes(ext))
    return "audio";
  if (["mp4", "m4v", "webm", "mov", "mkv", "avi"].includes(ext)) return "video";
  if (ext === "lottie") return "lottie";
  if (["otf", "ttf", "woff", "woff2"].includes(ext)) return "fonts";
  if (["obj", "ma", "aep"].includes(ext)) return "effects";
  return "effects";
}
