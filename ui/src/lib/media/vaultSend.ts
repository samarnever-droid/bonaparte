/**
 * "Send to vault" — every asset in a project can hop onto the global shelf
 * with one right-click. Footage clips are copied by the native side straight
 * from disk (no bytes through the UI); embedded images and audio are
 * re-materialized as real files from what the document already carries.
 */
import { command } from "../bridge";
import { notify } from "../store.svelte";
import { vaultSave } from "../vault-client";
import { assetPngBlob } from "./thumbs";
import type { MediaAsset } from "../model";

function base(name: string): string {
  const stem = name.replace(/[\\/]/g, "_").replace(/\.[a-z0-9]+$/i, "");
  return stem.trim() || "asset";
}

function bytesFromBase64(b64: string): Uint8Array {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}

function sniffAudioExt(bytes: Uint8Array): string {
  const head = String.fromCharCode(...bytes.subarray(0, 4));
  if (head === "RIFF" && String.fromCharCode(...bytes.subarray(8, 12)) === "WAVE") return "wav";
  if (head.slice(0, 3) === "ID3" || (bytes[0] === 0xff && (bytes[1] & 0xe0) === 0xe0)) return "mp3";
  if (head === "fLaC") return "flac";
  if (head === "OggS") return "ogg";
  return "wav";
}

export async function sendAssetToVault(asset: MediaAsset): Promise<void> {
  try {
    if (asset.embedded) {
      const blob = await assetPngBlob(asset);
      if (!blob) return notify("Could not re-encode this image", true);
      const name = `${base(asset.name)}.png`;
      await vaultSave("images", name, new File([blob], name, { type: "image/png" }));
      return notify(`Sent ${name} to the vault`);
    }
    if (asset.audio || asset.footage?.path) {
      // Server-side: audio may be an Astra extent (its bytes are not in the
      // UI payload) and footage may be gigabytes — both copy without ever
      // touching the browser.
      const reply = await command<{ saved?: string }>("vault_save_asset", { mediaId: asset.id });
      return notify(reply?.saved ? `Sent ${reply.saved} to the vault` : "Sent to the vault");
    }
    if (asset.video?.frames_base64.length) {
      const blob = await assetPngBlob(asset);
      if (!blob) return notify("Could not re-encode the poster frame", true);
      const name = `${base(asset.name)} poster.png`;
      await vaultSave("images", name, new File([blob], name, { type: "image/png" }));
      return notify(`Saved the ${name} poster to the vault`);
    }
    notify("This asset has no portable file to copy — nothing to send", true);
  } catch (error) {
    notify(String(error), true);
  }
}
