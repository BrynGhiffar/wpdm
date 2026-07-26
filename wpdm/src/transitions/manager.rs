use memmap2::Mmap;

#[allow(unused)]
use crate::loader::load_argb_buffer;
use crate::transitions::anim::TransitionAnim;

pub struct Transition {
    pub monitor: String,
    pub frame: u32,
    pub from_buffer: Mmap,
    pub to_buffer: Mmap,
    pub transition: TransitionAnim,
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

    pub fn render_transition(&mut self, monitor: &str, buffer: &mut [u8]) -> Option<()> {
        let tr_idx = self
            .transitions
            .iter()
            .position(|tr| tr.monitor.eq(monitor))?;

        // No monitor is left behind!
        let frame = self.transitions.iter().map(|tr| tr.frame).min()?;
        let tr = self.transitions.get_mut(tr_idx)?;

        let ret = tr
            .transition
            .render(frame, &tr.from_buffer, &tr.to_buffer, buffer);
        if !ret {
            tr.frame = frame + 1;
        } else {
            let Transition {
                from_buffer,
                to_buffer,
                ..
            } = self.transitions.remove(tr_idx);
            drop(from_buffer);
            drop(to_buffer);
        }
        Some(())
    }

    pub fn has_transitions(&self) -> bool {
        !self.transitions.is_empty()
    }
}
