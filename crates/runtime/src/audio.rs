//! Shared audio transport/import/export boundary. Heavy import work is prepared
//! outside the document mutex; one validated Op commits the resulting asset/clip.
use crate::{commit, EditorSession};
use base64::{engine::general_purpose::STANDARD, Engine};
use bonaparte_audio::{AudioSource, AudioSources, MixPlan};
use bonaparte_media::audio::{decode, decode_embedded, DecodedAudio};
use bonaparte_model::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
#[derive(Clone, Default)]
pub struct DecodedAudios {
    pub sources: BTreeMap<MediaId, Arc<DecodedAudio>>,
    hashes: BTreeMap<MediaId, String>,
}
impl AudioSources for DecodedAudios {
    fn source(&self, id: MediaId) -> Option<Arc<dyn AudioSource>> {
        self.sources
            .get(&id)
            .map(|s| s.clone() as Arc<dyn AudioSource>)
    }
}
pub fn decode_project_audio(project: &Project) -> Result<DecodedAudios, String> {
    let mut bank = DecodedAudios::default();
    bank.refresh(project)?;
    Ok(bank)
}
impl DecodedAudios {
    pub fn refresh(&mut self, project: &Project) -> Result<(), String> {
        self.sources
            .retain(|id, _| project.media.get(id).is_some_and(|m| m.audio.is_some()));
        self.hashes.retain(|id, _| self.sources.contains_key(id));
        for (id, asset) in &project.media {
            if let Some(audio) = &asset.audio {
                if self.hashes.get(id) == Some(&audio.sha256) {
                    continue;
                }
                self.sources.insert(*id, decode_embedded(audio)?);
                self.hashes.insert(*id, audio.sha256.clone());
            }
        }
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioImportRequest {
    pub comp_id: CompId,
    pub name: String,
    pub data_base64: String,
    #[serde(default)]
    pub track_id: Option<String>,
    #[serde(default)]
    pub start_frame: i64,
}
pub struct PreparedAudioImport {
    pub request: AudioImportRequest,
    pub audio: EmbeddedAudio,
    pub source: Arc<DecodedAudio>,
}
pub fn prepare_import(args: Value) -> Result<PreparedAudioImport, String> {
    let request: AudioImportRequest = serde_json::from_value(args).map_err(|e| e.to_string())?;
    if request.name.is_empty()
        || request.name.len() > 256
        || request.start_frame < 0
        || request.start_frame > MAX_AUDIO_TIMELINE
    {
        return Err("Invalid audio import name or placement".into());
    }
    // No size wall here: sources above the inline limit stream into the
    // Astra store inside decode() and the project carries a hash extent.
    if request.data_base64.len() > bonaparte_media::audio::MAX_AUDIO_FILE_BYTES.div_ceil(3) * 4 {
        return Err("Audio import exceeds the 2 GiB sanity bound".into());
    }
    let bytes = STANDARD
        .decode(request.data_base64.as_bytes())
        .map_err(|e| format!("Invalid audio upload: {e}"))?;
    let (audio, source) = decode(&bytes)?;
    Ok(PreparedAudioImport {
        request,
        audio,
        source,
    })
}
fn fresh(prefix: &str, mut used: impl FnMut(&str) -> bool) -> String {
    for i in 1..10000 {
        let id = format!("{prefix}-{i}");
        if !used(&id) {
            return id;
        }
    }
    format!("{prefix}-exhausted")
}
impl EditorSession {
    pub fn import_audio(&mut self, prepared: PreparedAudioImport) -> Result<Value, String> {
        let req = prepared.request;
        let comp = self
            .project
            .comp(req.comp_id)
            .ok_or("Audio destination composition was removed")?;
        let mut arrangement = comp.audio.clone();
        let media = self.project.next_media;
        let index = if let Some(id) = req.track_id {
            arrangement
                .tracks
                .iter()
                .position(|t| t.id == id)
                .ok_or("Audio destination track was removed")?
        } else {
            let id = fresh("track", |id| arrangement.tracks.iter().any(|t| t.id == id));
            let name = format!("Audio {}", arrangement.tracks.len() + 1);
            arrangement.tracks.push(AudioTrack::new(id, name));
            arrangement.tracks.len() - 1
        };
        if arrangement.tracks[index].locked {
            return Err("Unlock the destination audio track first".into());
        }
        let id = fresh("clip", |id| {
            arrangement
                .tracks
                .iter()
                .flat_map(|t| &t.clips)
                .any(|c| c.id == id)
        });
        let available = audio_duration(comp.duration).saturating_sub(req.start_frame as u64);
        if available == 0 {
            return Err("Place audio before the composition's end".into());
        }
        let mut clip = AudioClip::new(
            id,
            req.name.clone(),
            media,
            prepared.audio.frames.min(available),
        );
        clip.start_frame = req.start_frame;
        let clip_id = clip.id.clone();
        let track_id = arrangement.tracks[index].id.clone();
        arrangement.tracks[index].clips.push(clip);
        let duration =
            Time(((prepared.audio.frames as u128 * 120000).div_ceil(AUDIO_RATE as u128)) as i64);
        let asset = MediaAsset {
            id: MediaId(0),
            name: req.name,
            path: None,
            kind: MediaKind::Audio { duration },
            embedded: None,
            audio: Some(prepared.audio),
            slot: None,
            alias: None,
            perception: None,
            video: None,
            footage: None,
        };
        commit(
            &mut self.project,
            &mut self.history,
            &self.registry,
            Op::Batch {
                label: "Imported audio clip".into(),
                ops: vec![
                    Op::AddMedia { asset },
                    Op::SetCompAudio {
                        comp: req.comp_id,
                        audio: arrangement,
                    },
                ],
            },
        )?;
        self.audio.sources.insert(media, prepared.source);
        let mut result = self.changed()?;
        result["importedAudio"] = json!({"mediaId":media,"trackId":track_id,"clipId":clip_id});
        Ok(result)
    }
    pub fn audio_input(&self, comp_id: CompId) -> Result<AudioInput, String> {
        if !self.project.comps.contains_key(&comp_id) {
            return Err("Audio composition not found".into());
        }
        Ok(AudioInput {
            project: self.preview_snapshot.clone(),
            sources: self.audio.clone(),
            comp_id,
            revision: self.audio_revision,
            cache: self.audio_plans.clone(),
        })
    }
    pub fn audio_waveform(&self, args: Value) -> Result<Value, String> {
        let media = MediaId(args["mediaId"].as_u64().ok_or("Missing audio media ID")?);
        let source = self
            .audio
            .sources
            .get(&media)
            .ok_or("Audio source unavailable")?;
        let start = args["startFrame"].as_u64().unwrap_or(0);
        let end = args["endFrame"].as_u64().unwrap_or(source.frames());
        let points = args["points"].as_u64().unwrap_or(512) as usize;
        serde_json::to_value(source.waveform(start, end, points)?).map_err(|e| e.to_string())
    }
    pub fn split_audio(&mut self, args: Value) -> Result<Value, String> {
        let comp = CompId(args["compId"].as_u64().ok_or("Missing composition")?);
        let track = args["trackId"].as_str().ok_or("Missing audio track")?;
        let clip = args["clipId"].as_str().ok_or("Missing audio clip")?;
        let at = args["atFrame"].as_i64().ok_or("Missing sample position")?;
        let mut audio = self
            .project
            .comp(comp)
            .ok_or("Composition not found")?
            .audio
            .clone();
        let id = fresh("clip", |id| {
            audio
                .tracks
                .iter()
                .flat_map(|t| &t.clips)
                .any(|c| c.id == id)
        });
        let track = audio
            .tracks
            .iter_mut()
            .find(|t| t.id == track)
            .ok_or("Track not found")?;
        if track.locked {
            return Err("Track is locked".into());
        }
        let i = track
            .clips
            .iter()
            .position(|c| c.id == clip)
            .ok_or("Clip not found")?;
        let (left, right) = track.clips[i].split(at, id)?;
        track.clips[i] = left;
        track.clips.insert(i + 1, right);
        commit(
            &mut self.project,
            &mut self.history,
            &self.registry,
            Op::SetCompAudio { comp, audio },
        )?;
        self.changed()
    }
}
pub type MixCache = Arc<Mutex<BTreeMap<(u64, CompId), Arc<MixPlan>>>>;
pub struct AudioInput {
    pub project: Arc<Project>,
    pub sources: DecodedAudios,
    pub comp_id: CompId,
    pub revision: u64,
    pub cache: MixCache,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioChunkRequest {
    pub comp_id: CompId,
    pub start_frame: i64,
    pub frame_count: usize,
    pub sample_rate: u32,
    #[serde(default)]
    pub revision: Option<u64>,
}
impl AudioInput {
    pub fn plan(&self) -> Result<Arc<MixPlan>, String> {
        let key = (self.revision, self.comp_id);
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "Audio plan cache unavailable")?;
        if let Some(plan) = cache.get(&key) {
            return Ok(plan.clone());
        }
        let plan = Arc::new(MixPlan::new(&self.project, self.comp_id, &self.sources)?);
        if cache.len() >= 128 {
            cache.clear();
        }
        cache.insert(key, plan.clone());
        Ok(plan)
    }
    pub fn packet(&self, request: AudioChunkRequest) -> Result<Vec<u8>, String> {
        if request.comp_id != self.comp_id || request.revision.is_some_and(|r| r != self.revision) {
            return Err("Audio revision changed; request a fresh transport generation".into());
        }
        let block = match self.plan()?.render(
            request.start_frame,
            request.frame_count,
            request.sample_rate,
        ) {
            Ok(block) => block,
            Err(error) if error.starts_with("DSP_WARMUP:") => {
                let ready = error[11..].parse::<u64>().unwrap_or(0);
                let mut packet = b"BAW1".to_vec();
                packet.extend_from_slice(&ready.to_le_bytes());
                return Ok(packet);
            }
            Err(error) => return Err(error),
        };
        let metadata=serde_json::to_vec(&json!({"sampleRate":request.sample_rate,"channels":2,"startFrame":request.start_frame,"frameCount":request.frame_count,"revision":self.revision,"meter":block.meter,"tracks":block.tracks,"processing":block.processing,"compensatedLatency":block.compensated_latency})).map_err(|e|e.to_string())?;
        let mut bytes = Vec::with_capacity(8 + metadata.len() + block.samples.len() * 4);
        bytes.extend_from_slice(b"BAP1");
        bytes.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
        bytes.extend(metadata);
        for sample in block.samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        Ok(bytes)
    }
    pub fn wav(&self, path: &std::path::Path) -> Result<(), String> {
        let duration = self
            .project
            .comp(self.comp_id)
            .ok_or("Composition not found")?
            .duration;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let temp = tempfile::Builder::new()
            .prefix(".bonaparte-audio-")
            .suffix(".wav")
            .tempfile_in(parent)
            .map_err(|e| e.to_string())?
            .into_temp_path();
        bonaparte_media::audio::write_mix_wav(
            self.plan()?.as_ref(),
            &temp,
            0,
            audio_duration(duration),
        )?;
        temp.persist(path).map_err(|e| e.to_string())?;
        Ok(())
    }
}
