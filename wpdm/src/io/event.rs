use std::path::PathBuf;

use rand::Rng;
use wpdm_common::{CliRequest, TransitionOrigin, TransitionType};

#[derive(Debug)]
pub enum WpdmIoEvent {
    Render(WpdmIoRenderEvent),
    ConfigureOutput(WpdmIoOutputEvent),
    NewOutput(WpdmIoOutputEvent),
    DestroyOutput(WpdmIoOutputEvent),
    CliRequest(CliRequest)
}

#[derive(Debug, Clone)]
pub struct WpdmOutputInfo {
    pub name: String,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug)]
pub struct WpdmIoOutputEvent {
    pub oi: WpdmOutputInfo
}

#[derive(Debug)]
pub struct WpdmIoRenderEvent {
    pub oi: WpdmOutputInfo
}

#[derive(Debug)]
pub enum WpdmIoRequest {
    StartTransition(StartTransitionReq),
    Render(WpdmIoRenderRequest),
    CliMonitorQuery
}

#[derive(Debug)]
pub struct StartTransitionReq {
    pub oi: WpdmOutputInfo,
    pub from: Option<PathBuf>,
    pub to: PathBuf,
    pub anim: TransitionParams
}

/// Transition parameters resolved for the shader: origin in screen pixel
/// coordinates (top-left, y-down), angle in degrees, duration in frames.
#[derive(Debug, Clone, Copy)]
pub struct TransitionParams {
    pub tran_type: TransitionType,
    pub origin: [f32; 2],
    pub angle: f32,
    pub n_frames: u32
}

impl TransitionParams {
    /// Resolves the transition parameters the way the CPU transitions did:
    /// enum origins become pixel coordinates, Random picks random ones (and
    /// a random angle for the wipe).
    pub fn new(
        tran_type: TransitionType,
        tran_origin: TransitionOrigin,
        angle: f32,
        width: u32,
        height: u32
    ) -> Self {
        let width_f32 = width as f32;
        let height_f32 = height as f32;
        let center_x = width_f32 / 2.0;
        let center_y = height_f32 / 2.0;

        let origin = match tran_origin {
            TransitionOrigin::Center => [center_x, center_y],
            TransitionOrigin::Left => [0.0, center_y],
            TransitionOrigin::Right => [width_f32, center_y],
            TransitionOrigin::Coord(x, y) => [x as f32, y as f32],
            TransitionOrigin::Random => {
                let mut rng = rand::rng();
                let rand_x: u32 = rng.random_range(0..=width);
                let rand_y: u32 = rng.random_range(0..=height);
                [rand_x as f32, rand_y as f32]
            }
        };

        let angle = if tran_origin == TransitionOrigin::Random {
            let mut rng = rand::rng();
            let angle: u32 = rng.random_range(0..=90);
            angle as f32
        } else {
            angle
        };

        Self { tran_type, origin, angle, n_frames: 60 }
    }
}

#[derive(Debug)]
pub struct WpdmIoRenderRequest {
    pub oi: WpdmOutputInfo,
    pub params: TransitionParams,
    pub progress: f32,
    pub render_next: bool
}
