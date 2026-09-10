//! Session state for Bonaparte MCP server.

use bonaparte_model::{CompId, FrameRate, History, Project, Time};
use bonaparte_runtime::EditorSession;
use std::sync::{Arc, Mutex, MutexGuard};

/// Holds the live project state, mutation history, and currently selected composition.
///
/// A session is either standalone (the headless `bonaparte-mcp` binary: it
/// owns its project) or **hosted** — a live view of a running
/// [`EditorSession`]. Hosted sessions sync reads from the host and route
/// mutating tools through the host's own command surface, so AI edits land
/// in the editor's native undo history instead of a parallel universe.
pub struct McpSession {
    pub project: Project,
    pub history: History,
    pub active_comp: Option<CompId>,
    pub live: Option<Arc<Mutex<EditorSession>>>,
}

impl Default for McpSession {
    fn default() -> Self {
        Self::new()
    }
}

impl McpSession {
    /// Creates a new MCP session initialized with a default project and 1080p 30fps composition.
    pub fn new() -> Self {
        let mut project = Project::new("Untitled");
        let comp_id = project.create_comp(
            "Main",
            1920,
            1080,
            FrameRate::FPS_30,
            Time::from_secs_f64(10.0),
        );
        Self {
            project,
            history: bonaparte_runtime::journal::fresh_history(),
            active_comp: Some(comp_id),
            live: None,
        }
    }

    /// Replaces the active session with a newly loaded project, resetting history.
    pub fn with_project(project: Project) -> Self {
        let active_comp = project.comps.keys().next().copied();
        Self {
            project,
            history: bonaparte_runtime::journal::fresh_history(),
            active_comp,
            live: None,
        }
    }

    /// Builds a hosted session over a running editor. The host is the source
    /// of truth; this clone is refreshed before every request.
    pub fn hosted(live: Arc<Mutex<EditorSession>>) -> Self {
        let mut session = Self {
            project: Project::new("host"),
            history: bonaparte_runtime::journal::fresh_history(),
            active_comp: None,
            live: Some(live),
        };
        session.sync_from_host();
        session
    }

    /// Locks the host editor, recovering from a poisoned mutex: a panic in
    /// one request must never take the editor down with it. The document is
    /// validated on every commit, so a mid-panic state is still internally
    /// consistent.
    pub fn lock_host(&self) -> Result<MutexGuard<'_, EditorSession>, String> {
        let live = self.live.as_ref().ok_or("not a hosted session")?;
        match live.lock() {
            Ok(guard) => Ok(guard),
            Err(poisoned) => Ok(poisoned.into_inner()),
        }
    }

    /// Refreshes the local mirror from the host editor.
    pub fn sync_from_host(&mut self) {
        let (project, active) = match self.lock_host() {
            Ok(host) => {
                let active = host.project.comps.keys().next().copied();
                (host.project.clone(), active)
            }
            Err(_) => return,
        };
        self.project = project;
        self.active_comp = active;
    }

    /// True when this session edits a live editor rather than private state.
    pub fn is_hosted(&self) -> bool {
        self.live.is_some()
    }

    /// Returns the active composition ID, or the first available composition.
    pub fn target_comp(&self, requested: Option<u64>) -> Option<CompId> {
        if let Some(id) = requested {
            let cid = CompId(id);
            return self.project.comps.contains_key(&cid).then_some(cid);
        }
        if let Some(active) = self.active_comp {
            if self.project.comps.contains_key(&active) {
                return Some(active);
            }
        }
        self.project.comps.keys().next().copied()
    }
}
