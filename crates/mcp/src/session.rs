//! Session state for Bonaparte MCP server.

use bonaparte_model::{CompId, FrameRate, History, Project, Time};

/// Holds the live project state, mutation history, and currently selected composition.
pub struct McpSession {
    pub project: Project,
    pub history: History,
    pub active_comp: Option<CompId>,
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
            history: History::new(),
            active_comp: Some(comp_id),
        }
    }

    /// Replaces the active session with a newly loaded project, resetting history.
    pub fn with_project(project: Project) -> Self {
        let active_comp = project.comps.keys().next().copied();
        Self {
            project,
            history: History::new(),
            active_comp,
        }
    }

    /// Returns the active composition ID, or the first available composition.
    pub fn target_comp(&self, requested: Option<u64>) -> Option<CompId> {
        if let Some(id) = requested {
            let cid = CompId(id);
            if self.project.comps.contains_key(&cid) {
                return Some(cid);
            }
        }
        if let Some(active) = self.active_comp {
            if self.project.comps.contains_key(&active) {
                return Some(active);
            }
        }
        self.project.comps.keys().next().copied()
    }
}
