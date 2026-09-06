import { binary } from "../bridge";
import workletUrl from "./worklet.js?url";
export interface AudioTransportState {
  compId: number;
  revision: number;
  durationTicks: number;
  loop: boolean;
}
export interface AudioMeter {
  peak: [number, number];
  rms: [number, number];
  clippedSamples?: number;
}
export interface AudioChunkMeta {
  sampleRate: number;
  channels: number;
  startFrame: number;
  frameCount: number;
  revision: number;
  meter: AudioMeter;
  tracks: { compId: number; trackId: string; meter: AudioMeter }[];
  processing?: {
    compId: number;
    id: string;
    kind: string;
    reductionDb: number;
    meter: AudioMeter;
  }[];
  compensatedLatency?: number;
}

function decodeChunk(
  data: ArrayBuffer,
  request: { revision: number; sampleRate: number; startFrame: number; frameCount: number },
): { pcm: Float32Array<ArrayBuffer>; meta: AudioChunkMeta } {
  if (data.byteLength < 8) throw new Error("Incomplete audio packet");
  const view = new DataView(data);
  if (view.getUint32(0, true) !== 0x31504142) throw new Error("Unsupported audio stream");
  const n = view.getUint32(4, true);
  if (n > 262144 || 8 + n > data.byteLength) throw new Error("Invalid audio metadata");
  const meta: AudioChunkMeta = JSON.parse(new TextDecoder().decode(new Uint8Array(data, 8, n)));
  if (
    meta.revision !== request.revision ||
    meta.channels !== 2 ||
    meta.sampleRate !== request.sampleRate ||
    meta.startFrame !== request.startFrame ||
    meta.frameCount !== request.frameCount ||
    data.byteLength !== 8 + n + meta.frameCount * 8
  )
    throw new Error("Audio stream timing or payload mismatch");
  const bytes = data.slice(8 + n);
  const pcm = new Float32Array(bytes);
  // Current browser targets are little-endian; keep an explicit portable fallback.
  if (new Uint8Array(new Uint16Array([1]).buffer)[0] !== 1) {
    for (let i = 0; i < pcm.length; i++) pcm[i] = view.getFloat32(8 + n + i * 4, true);
  }
  return { pcm, meta };
}

interface Clock {
  cursor: number;
  origin: number;
  loops: number;
  contextFrame: number;
  playing: boolean;
  buffering: boolean;
  mode: string;
}
export class AudioTransport {
  context: AudioContext | null = null;
  node: AudioWorkletNode | null = null;
  private generation = 0;
  private state: AudioTransportState | null = null;
  private cache = new Map<number, { pcm: Float32Array<ArrayBuffer>; meta: AudioChunkMeta }>();
  private loading = new Map<number, Promise<void>>();
  private clock: Clock | null = null;
  private running = false;
  private chunkSize = 24000;
  private mode = "play";
  private end = 0;
  private filling = false;
  private monitor = 1;
  private updating = false;
  private updateTicket = 0;
  private scrubTimer: ReturnType<typeof setTimeout> | null = null;
  onMeter: (
    meter: AudioMeter,
    tracks: AudioChunkMeta["tracks"],
    processing?: AudioChunkMeta["processing"],
  ) => void = () => {};
  onStatus: (status: string, underruns: number) => void = () => {};
  onEnd: () => void = () => {};
  onError: (message: string) => void = () => {};
  private deviceInit: Promise<void> | null = null;
  async unlock() {
    if (!this.deviceInit)
      this.deviceInit = this.initialize().catch((error) => {
        void this.context?.close();
        this.context = null;
        this.node = null;
        this.deviceInit = null;
        throw error;
      });
    await this.deviceInit;
    await this.context!.resume();
  }
  private async initialize() {
    if (!this.context) {
      this.context = new AudioContext({ latencyHint: "interactive", sampleRate: 48000 });
      if (!this.context.audioWorklet)
        throw new Error(
          "AudioWorklet is required for synchronized audio playback. Use a current Chromium/WebView browser.",
        );
      await this.context.audioWorklet.addModule(workletUrl);
      this.node = new AudioWorkletNode(this.context, "bonaparte-audio", {
        numberOfInputs: 0,
        numberOfOutputs: 1,
        outputChannelCount: [2],
      });
      this.node.connect(this.context.destination);
      this.node.port.onmessage = ({ data: m }) => {
        if (m.generation !== this.generation) return;
        if (m.type === "ended") {
          this.running = false;
          this.onStatus("Ready", 0);
          if (m.mode === "play") this.onEnd();
          return;
        }
        if (m.type === "clock") {
          this.clock = m;
          const entry = [...this.cache.values()].find(
            (b) =>
              m.cursor >= b.meta.startFrame && m.cursor < b.meta.startFrame + b.meta.frameCount,
          );
          this.onMeter(
            { peak: m.peak, rms: m.rms },
            entry?.meta.tracks ?? [],
            entry?.meta.processing,
          );
          if (this.running) {
            this.onStatus(
              m.buffering
                ? "Buffering audio…"
                : this.mode === "scrub"
                  ? "Scrub audition"
                  : "Audio clock locked",
              m.underflows,
            );
            void this.fill(m.cursor);
          }
        }
      };
    }
    await this.context.resume();
    this.node?.port.postMessage({ type: "monitor", value: this.monitor });
  }
  private async requestChunk(
    request: {
      compId: number;
      revision: number;
      startFrame: number;
      frameCount: number;
      sampleRate: number;
    },
    valid: () => boolean,
  ) {
    for (let i = 0; i < 6000; i++) {
      if (!valid()) throw new Error("Audio preparation superseded");
      try {
        const data = await binary("audio_chunk", request);
        if (data.byteLength >= 4 && new DataView(data).getUint32(0, true) === 0x31574142) {
          this.onStatus("Preparing processed audio…", 0);
          continue;
        }
        return data;
      } catch (error) {
        if (!String(error).includes("DSP_WARMUP:")) throw error;
        this.onStatus("Preparing processed audio…", 0);
      }
    }
    throw new Error("Processed audio preparation exceeded its bounded retry limit");
  }
  private async fetch(start: number, token: number) {
    if (this.cache.has(start) || this.loading.has(start)) return this.loading.get(start);
    const state = this.state,
      context = this.context;
    if (!state || !context || start >= this.end) return;
    const request = {
      compId: state.compId,
      revision: state.revision,
      startFrame: start,
      frameCount: Math.min(this.chunkSize, this.end - start),
      sampleRate: context.sampleRate,
    };
    const promise = (async () => {
      const data = await this.requestChunk(request, () => token === this.generation);
      if (token !== this.generation) return;
      const { pcm, meta } = decodeChunk(data, request);
      this.cache.set(start, { pcm, meta });
      const copy = pcm.slice();
      this.node?.port.postMessage({ type: "chunk", generation: token, start, pcm: copy }, [
        copy.buffer,
      ]);
    })().finally(() => {
      if (token === this.generation) this.loading.delete(start);
    });
    this.loading.set(start, promise);
    return promise;
  }
  private async fill(cursor: number) {
    if (this.filling || !this.running || this.updating) return;
    this.filling = true;
    const token = this.generation;
    try {
      const wanted = new Set<number>();
      const base = Math.floor(cursor / this.chunkSize) * this.chunkSize;
      for (let i = 0; i < 6; i++) {
        let start = base + i * this.chunkSize;
        if (start >= this.end) {
          if (!this.state?.loop || this.mode === "scrub") break;
          start %= Math.ceil(this.end / this.chunkSize) * this.chunkSize;
        }
        if (start < this.end) wanted.add(start);
      }
      for (const start of wanted) {
        if (token !== this.generation) return;
        await this.fetch(start, token);
      }
      if (this.cache.size > 16) {
        for (const key of this.cache.keys()) {
          if (!wanted.has(key)) {
            this.cache.delete(key);
            if (this.cache.size <= 12) break;
          }
        }
      }
    } catch (error) {
      if (token === this.generation && !this.updating) {
        this.onError(String(error));
        this.stop();
        this.onEnd();
      }
    } finally {
      if (token === this.generation) this.filling = false;
    }
  }
  async update(state: AudioTransportState) {
    if (
      !this.running ||
      !this.context ||
      !this.node ||
      this.mode !== "play" ||
      this.state?.compId !== state.compId
    )
      return this.start(state, this.positionTicks() ?? 0);
    const ticket = ++this.updateTicket;
    this.updating = true;
    this.onStatus("Preparing updated sound…", 0);
    const rate = this.context.sampleRate,
      total = Math.ceil((state.durationTicks * rate) / 120000);
    const load = async (cursor: number) => {
      const starts = new Set<number>();
      const base = Math.floor(Math.min(total - 1, cursor) / this.chunkSize) * this.chunkSize;
      for (let i = 0; i < 6; i++) {
        let at = base + i * this.chunkSize;
        if (at >= total) {
          if (!state.loop) break;
          at %= Math.ceil(total / this.chunkSize) * this.chunkSize;
        }
        if (at < total) starts.add(at);
      }
      return Promise.all(
        [...starts].map(async (start) => {
          const request = {
            compId: state.compId,
            revision: state.revision,
            startFrame: start,
            frameCount: Math.min(this.chunkSize, total - start),
            sampleRate: rate,
          };
          return decodeChunk(
            await this.requestChunk(request, () => ticket === this.updateTicket && this.running),
            request,
          );
        }),
      );
    };
    try {
      let ready = await load(this.clock?.cursor ?? 0);
      if (ticket !== this.updateTicket || !this.running) return false;
      const cursor = Math.min(total - 1, this.clock?.cursor ?? 0);
      if (
        !ready.some(
          (b) => cursor >= b.meta.startFrame && cursor < b.meta.startFrame + b.meta.frameCount,
        )
      ) {
        ready = await load(cursor);
        if (ticket !== this.updateTicket || !this.running) return false;
      }
      const token = ++this.generation;
      this.state = state;
      this.end = total;
      this.loading.clear();
      this.cache.clear();
      for (const b of ready) this.cache.set(b.meta.startFrame, b);
      const blocks = ready.map((b) => ({ start: b.meta.startFrame, pcm: b.pcm.slice() }));
      this.node.port.postMessage(
        { type: "swap", generation: token, end: total, loop: state.loop, blocks },
        blocks.map((b) => b.pcm.buffer),
      );
      this.updating = false;
      this.filling = false;
      this.onStatus("Audio clock locked", 0);
      void this.fill(cursor);
      return true;
    } catch (error) {
      if (ticket === this.updateTicket) {
        this.updating = false;
        this.onError(String(error));
        this.stop();
        this.onEnd();
      }
      return false;
    }
  }
  async start(state: AudioTransportState, ticks: number, mode: "play" | "scrub" = "play") {
    this.stop();
    const token = ++this.generation;
    this.onStatus("Preparing audio…", 0);
    await this.unlock();
    if (token !== this.generation) return false;
    this.state = state;
    this.mode = mode;
    this.chunkSize = Math.round(this.context!.sampleRate / 2);
    const total = Math.ceil((state.durationTicks * this.context!.sampleRate) / 120000);
    const from = Math.max(
      0,
      Math.min(total - 1, Math.round((ticks * this.context!.sampleRate) / 120000)),
    );
    this.end =
      mode === "scrub"
        ? Math.min(total, from + Math.round(this.context!.sampleRate * 0.09))
        : total;
    this.cache.clear();
    this.loading.clear();
    this.clock = null;
    this.filling = false;
    this.node!.port.postMessage({
      type: "reset",
      generation: token,
      start: from,
      end: this.end,
      loop: state.loop && mode === "play",
      mode,
    });
    const first = Math.floor(from / this.chunkSize) * this.chunkSize;
    await this.fetch(first, token);
    let next = first + this.chunkSize;
    if (next >= this.end && state.loop && mode === "play") next = 0;
    if (next < this.end) await this.fetch(next, token);
    if (token !== this.generation) return false;
    this.running = true;
    this.node!.port.postMessage({ type: "start", generation: token });
    this.onStatus(mode === "play" ? "Audio clock locked" : "Scrub audition", 0);
    void this.fill(from);
    return true;
  }
  stop() {
    this.updateTicket++;
    this.updating = false;
    this.generation++;
    this.running = false;
    this.filling = false;
    this.node?.port.postMessage({ type: "stop" });
    this.cache.clear();
    this.loading.clear();
    this.clock = null;
    if (this.scrubTimer) {
      clearTimeout(this.scrubTimer);
      this.scrubTimer = null;
    }
    this.onMeter({ peak: [0, 0], rms: [0, 0] }, []);
    this.onStatus("Ready", 0);
  }
  audition(state: AudioTransportState, ticks: number) {
    if (this.scrubTimer) clearTimeout(this.scrubTimer);
    this.scrubTimer = setTimeout(() => {
      this.scrubTimer = null;
      void this.start({ ...state, loop: false }, ticks, "scrub").catch((e) =>
        this.onError(String(e)),
      );
    }, 45);
  }
  positionTicks(): number | null {
    if (!this.clock || !this.context || !this.state || this.mode !== "play") return null;
    let cursor = this.clock.cursor;
    if (this.clock.playing && !this.clock.buffering) {
      const stamp = this.context.getOutputTimestamp?.();
      const contextTime =
        stamp?.contextTime && stamp.contextTime > 0
          ? stamp.contextTime
          : Math.max(0, this.context.currentTime - (this.context.baseLatency ?? 0));
      cursor += contextTime * this.context.sampleRate - this.clock.contextFrame;
    }
    if (!this.clock.loops) cursor = Math.max(this.clock.origin, cursor);
    if (this.state.loop) cursor = ((cursor % this.end) + this.end) % this.end;
    else cursor = Math.max(0, Math.min(this.end - 1, cursor));
    return (cursor * 120000) / this.context.sampleRate;
  }
  setMonitor(value: number) {
    this.monitor = Math.max(0, Math.min(1, value));
    this.node?.port.postMessage({ type: "monitor", value: this.monitor });
  }
  diagnostics() {
    return {
      state: this.context?.state ?? "not started",
      sampleRate: this.context?.sampleRate ?? null,
      baseLatency: this.context?.baseLatency ?? null,
      cachedChunks: this.cache.size,
      generation: this.generation,
    };
  }
}
