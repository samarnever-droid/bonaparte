import test from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import { readFileSync } from "node:fs";
const source = readFileSync(new URL("../../src/lib/audio/worklet.js", import.meta.url), "utf8");
function processor() {
  let Constructor;
  const messages = [];
  const context = {
    sampleRate: 48000,
    currentFrame: 0,
    AudioWorkletProcessor: class {
      constructor() {
        this.port = { postMessage: (m) => messages.push(m), onmessage: null };
      }
    },
    registerProcessor: (name, c) => {
      Constructor = c;
    },
    Math,
  };
  vm.runInNewContext(source, context);
  const p = new Constructor();
  return {
    p,
    messages,
    send: (m) => p.port.onmessage({ data: m }),
    run: (n) => {
      const out = [new Float32Array(n), new Float32Array(n)];
      p.process([], [out]);
      context.currentFrame += n;
      return out;
    },
  };
}
function chunk(start, frames, value, generation = 1) {
  return { type: "chunk", generation, start, pcm: new Float32Array(frames * 2).fill(value) };
}
test("underflow emits silence without advancing composition time", () => {
  const t = processor();
  t.send({ type: "reset", generation: 1, start: 0, end: 10000, loop: false, mode: "play" });
  t.send({ type: "start", generation: 1 });
  t.run(512);
  assert.equal(t.p.cursor, 0);
  assert.equal(t.p.underflows, 512);
  t.send(chunk(0, 1024, 0.25));
  const out = t.run(512);
  assert.equal(t.p.cursor, 512);
  assert.equal(out[0][0], 0.25);
});
test("obsolete generations cannot inject old audio after a seek", () => {
  const t = processor();
  t.send({ type: "reset", generation: 2, start: 500, end: 10000, loop: false, mode: "play" });
  t.send(chunk(500, 1000, 0.8, 1));
  t.send({ type: "start", generation: 2 });
  assert.equal(t.run(128)[0][0], 0);
  assert.equal(t.p.cursor, 500);
  t.send(chunk(500, 1000, 0.2, 2));
  assert.ok(Math.abs(t.run(128)[0][0] - 0.2) < 1e-6);
});
test("bounded queue never evicts the block currently being played", () => {
  const t = processor();
  t.send({ type: "reset", generation: 1, start: 100, end: 100000, loop: true, mode: "play" });
  t.send(chunk(0, 1000, 0.3));
  for (let i = 1; i < 25; i++) t.send(chunk(i * 1000, 1000, 0.1));
  assert.equal(t.p.blocks.length, 16);
  t.send({ type: "start", generation: 1 });
  assert.ok(Math.abs(t.run(128)[0][0] - 0.3) < 1e-6);
  assert.equal(t.p.underflows, 0);
});
test("looping crosses boundaries without inserted samples or gaps", () => {
  const t = processor();
  t.send({ type: "reset", generation: 1, start: 0, end: 256, loop: true, mode: "play" });
  t.send(chunk(0, 128, 0.2));
  t.send(chunk(128, 128, 0.4));
  t.send({ type: "start", generation: 1 });
  const out = t.run(512)[0];
  assert.equal(out[0], out[256]);
  assert.equal(out[128], out[384]);
  assert.equal(t.p.underflows, 0);
  assert.equal(t.p.loops, 1);
});
test("master meters remain pre-monitor so low listening volume does not hide clipping", () => {
  const t = processor();
  t.send({ type: "reset", generation: 1, start: 0, end: 2000, loop: false, mode: "play" });
  t.send(chunk(0, 2000, 1.25));
  t.send({ type: "monitor", value: 0 });
  t.send({ type: "start", generation: 1 });
  const out = t.run(512)[0];
  const clock = t.messages.find((m) => m.type === "clock");
  assert.equal(clock.peak[0], 1.25);
  assert.ok(out[511] < 0.2);
});

test("a processing hot-swap preserves the sample cursor and declicks the transition", () => {
  const t = processor();
  t.send({ type: "reset", generation: 1, start: 100, end: 10000, loop: false, mode: "play" });
  t.send(chunk(0, 10000, 0.5));
  t.send({ type: "start", generation: 1 });
  t.run(128);
  assert.equal(t.p.cursor, 228);
  t.send({
    type: "swap",
    generation: 2,
    end: 10000,
    loop: false,
    blocks: [{ start: 0, pcm: new Float32Array(20000).fill(0.1) }],
  });
  assert.equal(t.p.cursor, 228);
  const out = t.run(512)[0];
  assert.ok(out[0] > 0.49);
  assert.ok(Math.abs(out[511] - 0.1) < 1e-6);
  assert.equal(t.p.cursor, 740);
  assert.equal(t.p.underflows, 0);
});
