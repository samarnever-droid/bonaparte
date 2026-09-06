//! Stateful channel-strip graph with deterministic canonical blocks, bounded
//! PCM/checkpoint caches and compensated lookahead latency on every route.
use super::*;
use crate::dsp::{ChannelState, Delay, Limiter};
use std::sync::Mutex;
const QUANTUM: u64 = 256;
const PCM_BUDGET: usize = 32 * 1024 * 1024;
const STATE_BUDGET: usize = 16 * 1024 * 1024;
struct Node {
    comp: CompId,
    id: String,
    kind: &'static str,
    inputs: Vec<(usize, f64)>,
    dry: Option<MixPlan>,
    gain: f64,
    pan: [f64; 2],
    processing: AudioProcessing,
    limiter: AudioLimiter,
    gate: [f64; 2],
}
struct Builder<'a> {
    project: &'a Project,
    sources: &'a dyn AudioSources,
    nodes: Vec<Node>,
    instances: usize,
}
impl Builder<'_> {
    fn push(&mut self, node: Node) -> Result<usize, String> {
        if self.nodes.len() >= 512 {
            return Err("Processed audio graph exceeds 512 nodes".into());
        }
        let id = self.nodes.len();
        self.nodes.push(node);
        Ok(id)
    }
    fn comp(
        &mut self,
        id: CompId,
        offset: f64,
        begin: f64,
        end: f64,
        depth: usize,
    ) -> Result<usize, String> {
        self.instances += 1;
        if depth >= 32 || self.instances > 4096 {
            return Err("Processed audio nesting/instance limit exceeded".into());
        }
        let c = self
            .project
            .comp(id)
            .ok_or("Audio composition not found")?
            .clone();
        let a = c.audio.clone();
        let end = end.min(offset + c.duration.0 as f64 * 0.4);
        let solo = a.tracks.iter().any(|t| t.solo && !t.muted);
        let mut children = vec![];
        if !a.muted && !solo {
            for l in c.layers.values() {
                if l.visible {
                    if let LayerKind::PreComp { comp } = l.kind {
                        if !self.project.has_audio(comp) {
                            continue;
                        }
                        let start = offset + l.start.0 as f64 * 0.4;
                        let child_end = end.min(start + l.duration.0 as f64 * 0.4);
                        if child_end > begin.max(start) {
                            children.push(self.comp(
                                comp,
                                start,
                                begin.max(start),
                                child_end,
                                depth + 1,
                            )?);
                        }
                    }
                }
            }
        }
        let master = self.push(Node {
            comp: id,
            id: format!("master-{}", self.instances),
            kind: "master",
            inputs: children.into_iter().map(|i| (i, 1.0)).collect(),
            dry: None,
            gain: if a.muted { 0.0 } else { db_gain(a.gain_db) },
            pan: [1.0; 2],
            processing: a.processing.clone(),
            limiter: a.limiter.clone(),
            gate: [begin, end],
        })?;
        let mut buses = BTreeMap::new();
        for bus in &a.buses {
            let index = self.push(Node {
                comp: id,
                id: bus.id.clone(),
                kind: "bus",
                inputs: vec![],
                dry: None,
                gain: if bus.muted { 0.0 } else { db_gain(bus.gain_db) },
                pan: balance(bus.pan),
                processing: bus.processing.clone(),
                limiter: Default::default(),
                gate: [begin, end],
            })?;
            buses.insert(bus.id.clone(), index);
        }
        let route = |nodes: &mut Vec<Node>,
                     source: usize,
                     output: &Option<String>,
                     sends: &[AudioSend]|
         -> Result<(), String> {
            let target = if let Some(id) = output {
                *buses.get(id).ok_or("Mix bus target missing")?
            } else {
                master
            };
            nodes[target].inputs.push((source, 1.0));
            for send in sends {
                if send.enabled {
                    let target = *buses.get(&send.bus).ok_or("Audio send target missing")?;
                    nodes[target].inputs.push((source, db_gain(send.gain_db)));
                }
            }
            Ok(())
        };
        for track in &a.tracks {
            let mut voices = vec![];
            if !a.muted && !track.muted && (!solo || track.solo) {
                for clip in &track.clips {
                    if clip.muted {
                        continue;
                    }
                    let origin = offset + clip.start_frame as f64;
                    let vbegin = begin.max(origin);
                    let vend = end.min(origin + clip.duration_frames as f64);
                    if vend <= vbegin {
                        continue;
                    }
                    let source = self
                        .sources
                        .source(clip.media)
                        .ok_or("Audio source unavailable")?;
                    voices.push(Voice {
                        clip: clip.clone(),
                        source,
                        origin,
                        begin: vbegin,
                        end: vend,
                        gain: db_gain(track.gain_db),
                        track_pan: balance(track.pan),
                    });
                }
            }
            let dry = MixPlan {
                buses: vec![Bus {
                    comp: id,
                    id: track.id.clone(),
                    voices,
                }],
                duration_frames: end,
                master: 1.0,
                graph: None,
            };
            let index = self.push(Node {
                comp: id,
                id: track.id.clone(),
                kind: "track",
                inputs: vec![],
                dry: Some(dry),
                gain: 1.0,
                pan: [1.0; 2],
                processing: track.processing.clone(),
                limiter: Default::default(),
                gate: [begin, end],
            })?;
            route(&mut self.nodes, index, &track.output, &track.sends)?;
        }
        for bus in &a.buses {
            route(&mut self.nodes, buses[&bus.id], &bus.output, &bus.sends)?;
        }
        Ok(master)
    }
}
#[derive(Clone)]
struct NodeState {
    strip: ChannelState,
    limiter: Option<Limiter>,
    delays: Vec<Delay>,
    input_latency: usize,
    latency: usize,
}
#[derive(Clone)]
struct State {
    nodes: Vec<NodeState>,
    cursor: u64,
}
impl State {
    fn bytes(&self) -> usize {
        self.nodes
            .iter()
            .map(|n| {
                n.strip.bytes()
                    + n.limiter.as_ref().map(Limiter::bytes).unwrap_or(0)
                    + n.delays.iter().map(Delay::bytes).sum::<usize>()
            })
            .sum()
    }
}
#[derive(Clone)]
struct Level {
    index: usize,
    peak: [f64; 2],
    sum: [f64; 2],
    reduction: f64,
}
struct Block {
    samples: Vec<f32>,
    levels: Vec<Level>,
}
impl Block {
    fn bytes(&self) -> usize {
        self.samples.len() * 4 + self.levels.len() * std::mem::size_of::<Level>()
    }
}
struct Cache {
    state: State,
    checkpoints: BTreeMap<u64, State>,
    blocks: BTreeMap<u64, Arc<Block>>,
    bytes: usize,
    checkpoint_bytes: usize,
    root_latency: usize,
}
pub(crate) struct ProcessedGraph {
    nodes: Vec<Node>,
    order: Vec<usize>,
    root: usize,
    cache: Mutex<BTreeMap<u32, Cache>>,
}
impl ProcessedGraph {
    pub fn new(
        project: &Project,
        comp: CompId,
        sources: &dyn AudioSources,
    ) -> Result<Self, String> {
        let end = project
            .comp(comp)
            .ok_or("Composition not found")?
            .duration
            .0 as f64
            * 0.4;
        let mut builder = Builder {
            project,
            sources,
            nodes: vec![],
            instances: 0,
        };
        let root = builder.comp(comp, 0.0, 0.0, end, 0)?;
        let mut order = vec![];
        let mut marks = vec![0u8; builder.nodes.len()];
        fn visit(
            i: usize,
            nodes: &[Node],
            marks: &mut [u8],
            order: &mut Vec<usize>,
        ) -> Result<(), String> {
            if marks[i] == 1 {
                return Err("Audio processing graph contains a cycle".into());
            }
            if marks[i] == 2 {
                return Ok(());
            }
            marks[i] = 1;
            for (j, _) in &nodes[i].inputs {
                visit(*j, nodes, marks, order)?;
            }
            marks[i] = 2;
            order.push(i);
            Ok(())
        }
        visit(root, &builder.nodes, &mut marks, &mut order)?;
        Ok(Self {
            nodes: builder.nodes,
            order,
            root,
            cache: Mutex::new(BTreeMap::new()),
        })
    }
    fn initial(&self, rate: u32) -> State {
        let mut latencies = vec![0; self.nodes.len()];
        for &i in &self.order {
            let max = self.nodes[i]
                .inputs
                .iter()
                .map(|(j, _)| latencies[*j])
                .max()
                .unwrap_or(0);
            latencies[i] = max
                + if self.nodes[i].limiter.enabled {
                    Limiter::latency(rate)
                } else {
                    0
                };
        }
        let nodes = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let input_latency = n
                    .inputs
                    .iter()
                    .map(|(j, _)| latencies[*j])
                    .max()
                    .unwrap_or(0);
                NodeState {
                    strip: ChannelState::new(&n.processing, rate),
                    limiter: n.limiter.enabled.then(|| Limiter::new(&n.limiter, rate)),
                    delays: n
                        .inputs
                        .iter()
                        .map(|(j, _)| Delay::new(input_latency - latencies[*j]))
                        .collect(),
                    input_latency,
                    latency: latencies[i],
                }
            })
            .collect();
        State { nodes, cursor: 0 }
    }
    fn block(&self, state: &mut State, rate: u32) -> Result<Block, String> {
        let start = state.cursor;
        let mut outputs = vec![vec![[0.0f64; 2]; QUANTUM as usize]; self.nodes.len()];
        let mut levels = vec![];
        for &i in &self.order {
            let node = &self.nodes[i];
            let data = if let Some(plan) = &node.dry {
                Some(plan.render(start as i64, QUANTUM as usize, rate)?.samples)
            } else {
                None
            };
            let ns = &mut state.nodes[i];
            let mut level = Level {
                index: i,
                peak: [0.0; 2],
                sum: [0.0; 2],
                reduction: 0.0,
            };
            for frame in 0..QUANTUM as usize {
                let mut sample = if let Some(data) = &data {
                    [data[frame * 2] as f64, data[frame * 2 + 1] as f64]
                } else {
                    [0.0; 2]
                };
                for (k, (input, gain)) in node.inputs.iter().enumerate() {
                    let delayed = ns.delays[k].process(outputs[*input][frame]);
                    sample[0] += delayed[0] * gain;
                    sample[1] += delayed[1] * gain;
                }
                sample[0] *= node.gain * node.pan[0];
                sample[1] *= node.gain * node.pan[1];
                sample = ns.strip.process(sample);
                let time = (start as f64 + frame as f64 - ns.input_latency as f64)
                    * AUDIO_RATE as f64
                    / rate as f64;
                if time < node.gate[0] || time >= node.gate[1] {
                    sample = [0.0; 2];
                }
                if let Some(limiter) = &mut ns.limiter {
                    sample = limiter.process(sample);
                    level.reduction = level.reduction.max(limiter.reduction);
                }
                level.reduction = level.reduction.max(ns.strip.reduction);
                for c in 0..2 {
                    if !sample[c].is_finite() || sample[c].abs() > f32::MAX as f64 {
                        return Err("Processed audio exceeds floating-point headroom".into());
                    }
                    level.peak[c] = level.peak[c].max(sample[c].abs());
                    level.sum[c] += sample[c] * sample[c];
                }
                outputs[i][frame] = sample;
            }
            levels.push(level);
        }
        state.cursor += QUANTUM;
        Ok(Block {
            samples: outputs[self.root]
                .iter()
                .flat_map(|s| [s[0] as f32, s[1] as f32])
                .collect(),
            levels,
        })
    }
    pub fn render(&self, start: i64, count: usize, rate: u32) -> Result<MixedBlock, String> {
        let mut caches = self
            .cache
            .lock()
            .map_err(|_| "Audio processor cache unavailable")?;
        if !caches.contains_key(&rate) {
            if caches.len() >= 2 {
                caches.clear();
            }
            let state = self.initial(rate);
            let latency = state.nodes[self.root].latency;
            let bytes = state.bytes();
            if bytes > STATE_BUDGET {
                return Err("Audio processor state exceeds the 16 MiB checkpoint budget".into());
            }
            let mut checkpoints = BTreeMap::new();
            checkpoints.insert(0, state.clone());
            caches.insert(
                rate,
                Cache {
                    state,
                    checkpoints,
                    blocks: BTreeMap::new(),
                    bytes: 0,
                    checkpoint_bytes: bytes,
                    root_latency: latency,
                },
            );
        }
        let cache = caches.get_mut(&rate).unwrap();
        let wanted = start as u64 + cache.root_latency as u64;
        let last = wanted + count as u64;
        let first_block = wanted / QUANTUM * QUANTUM;
        let after = last.div_ceil(QUANTUM) * QUANTUM;
        let missing = (first_block..after)
            .step_by(QUANTUM as usize)
            .find(|i| !cache.blocks.contains_key(i));
        if let Some(first) = missing {
            if cache.state.cursor > first {
                cache.state = cache
                    .checkpoints
                    .range(..=first)
                    .next_back()
                    .map(|(_, s)| s.clone())
                    .unwrap_or_else(|| self.initial(rate));
            } else if let Some((position, state)) = cache.checkpoints.range(..=first).next_back() {
                if *position > cache.state.cursor {
                    cache.state = state.clone();
                }
            }
            let work_end = cache.state.cursor.saturating_add(rate as u64 * 15);
            while cache.state.cursor < after {
                if cache.state.cursor >= work_end {
                    return Err(format!(
                        "DSP_WARMUP:{}",
                        cache.state.cursor.saturating_sub(cache.root_latency as u64)
                    ));
                }
                let at = cache.state.cursor;
                let block = Arc::new(self.block(&mut cache.state, rate)?);
                if !cache.blocks.contains_key(&at) {
                    let size = block.bytes();
                    while cache.bytes + size > PCM_BUDGET {
                        let Some(key) = cache.blocks.keys().next().copied() else {
                            break;
                        };
                        cache.bytes -= cache.blocks.remove(&key).unwrap().bytes();
                    }
                    cache.bytes += size;
                    cache.blocks.insert(at, block);
                }
                if cache.state.cursor % 4096 == 0
                    && !cache.checkpoints.contains_key(&cache.state.cursor)
                {
                    let size = cache.state.bytes();
                    while cache.checkpoint_bytes + size > STATE_BUDGET {
                        let Some(key) = cache.checkpoints.keys().copied().find(|k| *k != 0) else {
                            break;
                        };
                        cache.checkpoint_bytes -= cache.checkpoints.remove(&key).unwrap().bytes();
                    }
                    if cache.checkpoint_bytes + size <= STATE_BUDGET {
                        cache
                            .checkpoints
                            .insert(cache.state.cursor, cache.state.clone());
                        cache.checkpoint_bytes += size;
                    }
                }
            }
        }
        let mut samples = Vec::with_capacity(count * 2);
        let mut levels: BTreeMap<usize, ([f64; 2], [f64; 2], usize, f64)> = BTreeMap::new();
        let mut cursor = wanted;
        while cursor < last {
            let key = cursor / QUANTUM * QUANTUM;
            let block = cache
                .blocks
                .get(&key)
                .ok_or("Processed audio cache window was evicted; request a shorter chunk")?;
            let offset = (cursor - key) as usize;
            let n = ((last - cursor) as usize).min(QUANTUM as usize - offset);
            samples.extend_from_slice(&block.samples[offset * 2..(offset + n) * 2]);
            for level in &block.levels {
                let e = levels
                    .entry(level.index)
                    .or_insert(([0.0; 2], [0.0; 2], 0, 0.0));
                for c in 0..2 {
                    e.0[c] = e.0[c].max(level.peak[c]);
                    e.1[c] += level.sum[c] * n as f64 / QUANTUM as f64;
                }
                e.2 += n;
                e.3 = e.3.max(level.reduction);
            }
            cursor += n as u64;
        }
        let exact: Vec<f64> = samples.iter().map(|v| *v as f64).collect();
        let meter = measure(&exact);
        let mut tracks = vec![];
        let mut strips = vec![];
        for (i, (peak, sum, n, reduction)) in levels {
            let node = &self.nodes[i];
            let meter = Meter {
                peak: [peak[0] as f32, peak[1] as f32],
                rms: [
                    (sum[0] / n.max(1) as f64).sqrt() as f32,
                    (sum[1] / n.max(1) as f64).sqrt() as f32,
                ],
                clipped_samples: 0,
            };
            if node.kind == "track" {
                tracks.push(TrackMeter {
                    comp_id: node.comp,
                    track_id: node.id.clone(),
                    meter: meter.clone(),
                });
            }
            strips.push(ProcessorMeter {
                comp_id: node.comp,
                id: node.id.clone(),
                kind: node.kind.into(),
                reduction_db: reduction as f32,
                meter,
            });
        }
        Ok(MixedBlock {
            samples,
            meter,
            tracks,
            processing: strips,
            compensated_latency: cache.root_latency,
        })
    }
}
