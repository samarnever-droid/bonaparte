/**
 * Kaya ⚡ client: the plugin's keys live client-side (localStorage, never
 * in the project file) and ride per-call. Analysis works with zero keys;
 * transcription and narration unlock with them.
 */
import { command } from "./bridge";
import { editor, activeComp, accept, notify, queued } from "./store.svelte";
import type { Snapshot, SnapshotPatch } from "./model";

const STORE_KEY = "bonaparte.kaya.keys";
type KayaKeys = { openaiKey?: string; sarvamKey?: string; elevenLabsKey?: string };

export function kayaKeys(): KayaKeys {
  try {
    return JSON.parse(localStorage.getItem(STORE_KEY) ?? "{}") as KayaKeys;
  } catch {
    return {};
  }
}
export function saveKayaKeys(keys: KayaKeys) {
  localStorage.setItem(STORE_KEY, JSON.stringify(keys));
}

export async function kayaAnalyze(assetId: number) {
  await queued(async () => {
    const reply = await command<Snapshot | SnapshotPatch>("kaya.analyze", {
      delta: !!editor.deltaProtocol,
      baseRevision: editor.revision,
      assetId,
    });
    accept(reply);
    const extra = reply as { bpm?: number | null; beats?: number; silence?: unknown[] };
    const bpm = extra.bpm ? `${Math.round(extra.bpm)} BPM` : "no steady beat";
    notify(`Kaya ⚡ ${bpm}, ${extra.beats ?? 0} beats, ${extra.silence?.length ?? 0} silent spans.`);
  });
}

export async function kayaTranscribe(assetId: number, openaiKey: string) {
  await queued(async () => {
    const reply = await command<Snapshot | SnapshotPatch>("kaya.transcribe", {
      delta: !!editor.deltaProtocol,
      baseRevision: editor.revision,
      assetId,
      openaiKey,
    });
    accept(reply);
    const extra = reply as { words?: number; annotation?: string };
    notify(`Kaya ⚡ ${extra.words ?? 0} words: ${extra.annotation ?? ""}`);
  });
}

export async function narratorSpeak(opts: {
  text: string;
  startSecs: number;
  provider: "sarvam" | "elevenlabs";
  voice?: string;
}) {
  const comp = activeComp();
  if (!comp) return;
  const keys = kayaKeys();
  const apiKey =
    opts.provider === "sarvam" ? keys.sarvamKey ?? "" : keys.elevenLabsKey ?? "";
  await queued(async () => {
    const reply = await command<Snapshot | SnapshotPatch>("narrator.speak", {
      delta: !!editor.deltaProtocol,
      baseRevision: editor.revision,
      compId: comp.id,
      text: opts.text,
      startSecs: opts.startSecs,
      provider: opts.provider,
      apiKey,
      voice: opts.voice ?? "",
    });
    accept(reply);
    const extra = reply as { durationSecs?: number; provider?: string };
    notify(
      `Narrator ⚡ ${extra.durationSecs?.toFixed(1) ?? "?"}s placed at ${opts.startSecs.toFixed(1)}s via ${extra.provider}.`,
    );
  });
}
