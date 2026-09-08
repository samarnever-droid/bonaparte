/**
 * Kinetic ⚡ — the lyric-video engine's UI side. Type lines, pick a motion
 * style, and the engine lays every word on the beat grid with keyframes,
 * optionally synthesizing a sound-design lane (impact on downbeats).
 */
import { command } from "./bridge";
import { editor, activeComp, accept, notify, queued } from "./store.svelte";
import type { Snapshot, SnapshotPatch } from "./model";

export interface KineticOptions {
  assetId: number;
  lines: string[];
  style: "pop" | "rise" | "wave";
  sync: boolean;
  foley: boolean;
}

export async function generateKinetic(opts: KineticOptions) {
  const comp = activeComp();
  if (!comp) return;
  const lines = opts.lines.map((l) => l.trim()).filter(Boolean);
  if (!lines.length) {
    notify("Write at least one line of lyrics.", true);
    return;
  }
  if (opts.sync && !editor.project?.media[String(opts.assetId)]?.audio?.beat_grid) {
    notify("Detect beats on the music first — or untick Sync.", true);
    return;
  }
  await queued(async () => {
    const reply = await command<Snapshot | SnapshotPatch>("kinetic_lyrics", {
      delta: !!editor.deltaProtocol,
      baseRevision: editor.revision,
      compId: comp.id,
      audioAsset: opts.sync ? opts.assetId : null,
      lines,
      style: opts.style,
    });
    accept(reply);
    const extra = reply as { words?: number };
    if (opts.foley) {
      const foley = await command<Snapshot | SnapshotPatch>("sound_design", {
        delta: !!editor.deltaProtocol,
        baseRevision: editor.revision,
        compId: comp.id,
        audioAsset: opts.assetId,
        flavor: "impact",
        onDownbeats: true,
      });
      accept(foley);
      const foleyExtra = foley as { events?: number };
      notify(
        `Kinetic ⚡ ${extra.words ?? 0} words on the beat + ${foleyExtra.events ?? 0} auto impacts. Play it.`,
      );
    } else {
      notify(`Kinetic ⚡ ${extra.words ?? 0} words animated on the beat. Play it.`);
    }
  });
}
