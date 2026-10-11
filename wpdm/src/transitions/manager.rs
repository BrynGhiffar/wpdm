use crate::io::event::TransitionParams;

pub struct Transition {
    pub monitor: String,
    pub frame: u32,
    pub params: TransitionParams
}

pub struct TransitionManager {
    pub transitions: Vec<Transition>,
}

impl TransitionManager {
    pub fn new() -> Self {
        Self {
            transitions: vec![],
        }
    }

    /// Advance the transition for `monitor` by one frame, synchronized to
    /// the global minimum frame across all active transitions so monitors
    /// stay in lockstep and complete together. Returns the params and
    /// progress to render this frame; the final call returns progress 1.0
    /// and removes the transition. Returns None when no transition is
    /// running for this monitor.
    pub fn advance(&mut self, monitor: &str) -> Option<(TransitionParams, f32)> {
        let tr_idx = self
            .transitions
            .iter()
            .position(|tr| tr.monitor.eq(monitor))?;

        // No monitor is left behind!
        let frame = self.transitions.iter().map(|tr| tr.frame).min()?;
        let tr = self.transitions.get_mut(tr_idx)?;

        let done = frame >= tr.params.n_frames;
        let progress = if done {
            1.0
        } else {
            frame as f32 / tr.params.n_frames as f32
        };
        let params = tr.params;

        if done {
            self.transitions.remove(tr_idx);
        } else {
            tr.frame = frame + 1;
        }

        Some((params, progress))
    }

    pub fn has_transitions(&self) -> bool {
        !self.transitions.is_empty()
    }
}
