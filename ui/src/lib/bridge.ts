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
