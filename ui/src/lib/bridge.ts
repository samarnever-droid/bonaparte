import { invoke, isTauri } from "@tauri-apps/api/core";
export const desktop = isTauri();
export { invoke };

async function post(command: string, args: unknown): Promise<Response> {
  const response = await fetch(`/api/${command}`, {
    method: "POST",
    headers: { "Content-Type": "application/json", "X-Bonaparte-Client": "editor" },
    body: JSON.stringify(args),
  });
  if (!response.ok) {
    const data = await response.json().catch(() => null);
    throw new Error(data?.error ?? `The Rust service returned ${response.status}.`);
  }
  return response;
}
export async function command<T>(name: string, args: unknown = {}): Promise<T> {
  if (desktop) return invoke<T>("editor_command", { command: name, args });
  return (await post(name, args)).json();
}
export async function binary(name: string, args: unknown): Promise<ArrayBuffer> {
  if (desktop) {
    const data = await invoke<ArrayBuffer | number[]>(name, { args });
    return data instanceof ArrayBuffer ? data : new Uint8Array(data).buffer;
  }
  return (await post(name, args)).arrayBuffer();
}

/* ── Disk-streamed downloads ──────────────────────────────────────────
   With the File System Access API (Chromium desktop), the binary bridge
   reply is piped from the socket straight into the chosen file — exports
   of any size, bounded by disk, not by RAM. Everything else keeps the
   classic blob path. Resolves false only when the user cancels the
   native picker (nothing was produced, no retry needed). */
type SaveHandle = {
  createWritable(): Promise<{
    write(data: Uint8Array | Blob): Promise<void>;
    close(): Promise<void>;
    abort(): Promise<void>;
  }>;
};
type SavePicker = (options: {
  suggestedName: string;
  types?: { description: string; accept: Record<string, string[]> }[];
}) => Promise<SaveHandle>;

export async function saveOrDownload(
  name: string,
  args: unknown,
  filename: string,
  mime: string,
): Promise<boolean> {
  const response = await post(name, args);
  const declared = Number(response.headers.get("content-length") ?? "0");
  const picker = (window as unknown as { showSaveFilePicker?: SavePicker })
    .showSaveFilePicker;
  // Only reach for the native picker when the reply is too big to hold as a
  // blob — everything smaller keeps the classic <a download> behaviour.
  if (picker && !desktop && response.body && declared > 256 * 1024 * 1024) {
    let handle: SaveHandle;
    try {
      handle = await picker({
        suggestedName: filename,
        types: [
          {
            description: "Bonaparte export",
            accept: { [mime]: [`.${filename.split(".").pop() ?? "bin"}`] },
          },
        ],
      });
    } catch (error) {
      if ((error as DOMException)?.name === "AbortError") return false;
      // Picker unavailable (unsupported context): the unread body still works.
      downloadBlob(await response.arrayBuffer(), filename, mime);
      return true;
    }
    const writable = await handle.createWritable();
    try {
      await response.body.pipeTo(writable as unknown as WritableStream);
    } catch {
      await writable.abort().catch(() => undefined);
      // The body is spent; re-request for the blob fallback.
      downloadBlob(await binary(name, args), filename, mime);
    }
    return true;
  }
  downloadBlob(await response.arrayBuffer(), filename, mime);
  return true;
}

function downloadBlob(data: ArrayBuffer, filename: string, mime: string) {
  const url = URL.createObjectURL(new Blob([data], { type: mime }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 30_000);
}
