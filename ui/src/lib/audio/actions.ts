import {
  editor,
  activeComp,
  arrangement,
  applyOp,
  accept,
  queued,
  clone,
  notify,
  pause,
  seekAudio,
  setWorkspace,
} from "../store.svelte";
import { command } from "../bridge";
import type { Snapshot, SnapshotPatch } from "../model";
import {
  AUDIO_RATE,
  emptyAudio,
  newTrack,
  newClip,
  newBus,
  envelope,
  type AudioArrangement,
  type AudioClip,
  type AudioTrack,
} from "./model";
export function selectedAudio() {
  const a = arrangement(),
    s = editor.audioSelection;
  const track = a.tracks.find((t) => t.id === s?.track);
  return { track, clip: track?.clips.find((c) => c.id === s?.clip) };
}
export async function editAudio(label: string, edit: (audio: AudioArrangement) => void) {
  const comp = activeComp();
  if (!comp) return false;
  return applyOp((project) => {
    const c = project.comps[String(comp.id)];
    if (!c) return null;
    const audio = clone(c.audio ?? emptyAudio());
    edit(audio);
    return { type: "batch", label, ops: [{ type: "setCompAudio", comp: comp.id, audio }] };
  });
}
export async function importAudio(file?: File, trackId?: string) {
  if (!editor.audioProtocol) {
    notify("Restart the audio-enabled runtime to import sound.", true);
    return;
  }
  if (!file) {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".wav,.mp3,.flac,.ogg,.oga,.aiff,.aif,.m4a,.aac";
    input.multiple = true;
    input.onchange = () => {
      void (async () => {
        for (const f of Array.from(input.files ?? [])) await importAudio(f, trackId);
      })();
    };
    input.click();
    return;
  }
  const comp = activeComp();
  if (!comp) return;
  if (file.size > 128 * 1024 * 1024) {
    notify(
      "That file is over the 128 MB import size. Trim or compress it first — the project itself stays portable either way.",
      true,
    );
    return;
  }
  pause();
  editor.audioImporting = true;
  editor.audioStatus = "Decoding source and building waveforms…";
  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    let raw = "";
    for (let i = 0; i < bytes.length; i += 8192)
      raw += String.fromCharCode(...bytes.subarray(i, i + 8192));
    const ok = await queued(async () => {
      const result = await command<
        Snapshot & { importedAudio: { mediaId: number; trackId: string; clipId: string } }
      >("import_audio", {
        compId: comp.id,
        name: file.name,
        dataBase64: btoa(raw),
        trackId: trackId ?? null,
        startFrame: Math.max(0, editor.audioCursor),
      });
      accept(result);
      editor.audioSelection = {
        track: result.importedAudio.trackId,
        clip: result.importedAudio.clipId,
      };
      setWorkspace("Audio");
    });
    if (!ok) return;
    notify(
      `${file.name} imported. Original encoded source retained; working audio is 48 kHz float.`,
    );
  } catch (e) {
    notify(String(e), true);
  } finally {
    editor.audioImporting = false;
    editor.audioStatus = "Ready";
  }
}
export async function addAudioTrack() {
  const track = newTrack(`Audio ${arrangement().tracks.length + 1}`);
  if (await editAudio("Added audio track", (a) => a.tracks.push(track)))
    editor.audioSelection = { track: track.id, clip: null };
}
export async function addAssetClip(media: number, trackId?: string) {
  const asset = editor.project?.media[String(media)],
    comp = activeComp();
  if (!asset?.audio || !comp) return;
  const start = Math.min(Math.max(0, editor.audioCursor), Math.ceil(comp.duration * 0.4) - 1);
  const clip = newClip(
    media,
    asset.name,
    Math.min(asset.audio.frames, Math.ceil(comp.duration * 0.4) - start),
    start,
  );
  let id = trackId;
  if (
    await editAudio("Added audio clip", (a) => {
      let track = a.tracks.find((t) => t.id === id);
      if (!track) {
        track = newTrack(`Audio ${a.tracks.length + 1}`);
        a.tracks.push(track);
      }
      if (track.locked) throw new Error("Track is locked");
      id = track.id;
      track.clips.push(clip);
    })
  )
    editor.audioSelection = { track: id!, clip: clip.id };
}
export async function updateClip(label: string, fn: (clip: AudioClip) => void) {
  const s = editor.audioSelection;
  if (!s?.clip) return;
  return editAudio(label, (a) => {
    const track = a.tracks.find((t) => t.id === s.track);
    if (!track || track.locked) return;
    const clip = track.clips.find((c) => c.id === s.clip);
    if (clip) fn(clip);
  });
}
export async function updateTrack(label: string, fn: (track: AudioTrack) => void) {
  const s = editor.audioSelection;
  if (!s) return;
  return editAudio(label, (a) => {
    const track = a.tracks.find((t) => t.id === s.track);
    if (track) fn(track);
  });
}
export async function splitAudio() {
  const s = editor.audioSelection,
    comp = activeComp();
  if (!s?.clip || !comp) return;
  pause();
  await queued(async () =>
    accept(
      await command<Snapshot | SnapshotPatch>("split_audio_clip", {
        delta: !!editor.deltaProtocol,
        baseRevision: editor.revision,
        compId: comp.id,
        trackId: s.track,
        clipId: s.clip,
        atFrame: editor.audioCursor,
      }),
    ),
  );
}
export async function deleteAudio() {
  const s = editor.audioSelection;
  if (!s) return;
  const { track } = selectedAudio();
  if (track?.locked) {
    notify("Unlock this track first.", true);
    return;
  }
  if (
    await editAudio(s.clip ? "Deleted audio clip" : "Deleted audio track", (a) => {
      if (s.clip) {
        const t = a.tracks.find((t) => t.id === s.track);
        if (t) t.clips = t.clips.filter((c) => c.id !== s.clip);
      } else a.tracks = a.tracks.filter((t) => t.id !== s.track);
    })
  ) {
    editor.audioSelection = s.clip ? { track: s.track, clip: null } : null;
  }
}
export async function duplicateAudio() {
  const { track, clip } = selectedAudio();
  if (!track || track.locked || !clip) return;
  const copy = clone(clip);
  copy.id = crypto.randomUUID();
  copy.name += " copy";
  copy.start_frame = clip.start_frame + clip.duration_frames;
  if (
    await editAudio("Duplicated audio clip", (a) =>
      a.tracks.find((t) => t.id === track.id)?.clips.push(copy),
    )
  )
    editor.audioSelection = { track: track.id, clip: copy.id };
}
export async function reverseAudio() {
  return updateClip("Reversed audio", (c) => {
    c.source_offset += (c.duration_frames - 1) * c.rate;
    c.rate = -c.rate;
  });
}
export async function normalizeAudio() {
  const { clip } = selectedAudio();
  if (!clip) return;
  const peak = editor.project?.media[String(clip.media)]?.audio?.peak;
  if (!peak) {
    notify("This source has no measurable peak.");
    return;
  }
  return updateClip("Peak-normalized clip", (c) => {
    c.gain_db = Math.max(-96, Math.min(24, -1 - 20 * Math.log10(peak)));
    c.gain_points = [];
  });
}
export async function toggleGainKey() {
  const { clip } = selectedAudio();
  if (!clip) return;
  const frame = Math.max(
    0,
    Math.min(clip.duration_frames - 1, editor.audioCursor - clip.start_frame),
  );
  return updateClip("Edited gain automation", (c) => {
    const existing = c.gain_points.find((p) => p.frame === frame);
    if (existing) c.gain_points = c.gain_points.filter((p) => p !== existing);
    else c.gain_points.push({ frame, value: envelope(c.gain_points, frame, c.gain_db) });
    c.gain_points.sort((a, b) => a.frame - b.frame);
  });
}
export async function crossfadeAudio() {
  const { track, clip } = selectedAudio();
  if (!track || !clip) return;
  const next = [...track.clips]
    .filter(
      (c) =>
        c.id !== clip.id &&
        c.start_frame > clip.start_frame &&
        c.start_frame < clip.start_frame + clip.duration_frames,
    )
    .sort((a, b) => a.start_frame - b.start_frame)[0];
  if (!next) {
    notify("Overlap this clip with a later clip on the same track first.", true);
    return;
  }
  const overlap = Math.min(
    clip.start_frame + clip.duration_frames - next.start_frame,
    next.duration_frames,
  );
  await editAudio("Equal-power audio crossfade", (a) => {
    const t = a.tracks.find((t) => t.id === track.id)!;
    const left = t.clips.find((c) => c.id === clip.id)!,
      right = t.clips.find((c) => c.id === next.id)!;
    left.fade_out = {
      start: left.duration_frames - overlap,
      end: left.duration_frames,
      shape: "equal_power",
    };
    right.fade_in = { start: 0, end: overlap, shape: "equal_power" };
  });
}
export async function addMarker() {
  const frame = editor.audioCursor;
  await editAudio("Added timeline marker", (a) =>
    a.markers.push({ id: crypto.randomUUID(), frame, name: `Marker ${a.markers.length + 1}` }),
  );
}
export function snapAudio(frame: number, bypass = false) {
  if (bypass || editor.audioSnap === "sample") return Math.round(frame);
  const c = activeComp();
  if (!c) return Math.round(frame);
  const a = arrangement();
  if (editor.audioSnap === "beat") {
    const step = (AUDIO_RATE * 60) / a.bpm;
    return Math.round(Math.round((frame - a.beat_offset) / step) * step + a.beat_offset);
  }
  const step = (AUDIO_RATE * c.fps.den) / c.fps.num;
  return Math.round(Math.round(frame / step) * step);
}
export function goAudioStart() {
  pause();
  seekAudio(0);
}

export async function addMixBus() {
  const bus = newBus(`Bus ${(arrangement().buses?.length ?? 0) + 1}`);
  if (
    await editAudio("Added mix bus", (a) => {
      a.buses ??= [];
      a.buses.push(bus);
    })
  ) {
    editor.audioSelection = null;
    editor.audioBusSelection = bus.id;
  }
}
export async function removeMixBus(id: string) {
  await editAudio("Removed mix bus and disconnected routes", (a) => {
    a.buses = (a.buses ?? []).filter((b) => b.id !== id);
    for (const strip of [...a.tracks, ...a.buses]) {
      if (strip.output === id) strip.output = null;
      strip.sends = (strip.sends ?? []).filter((s) => s.bus !== id);
    }
  });
  if (editor.audioBusSelection === id) editor.audioBusSelection = null;
}
