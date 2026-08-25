use std::path::PathBuf;

use smithay_client_toolkit::shm::slot::{Buffer, SlotPool};
use wpdm_common::{CliRequest, TransitionOrigin, TransitionType};

use crate::transitions::anim::TransitionAnim;

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
    pub slot: SlotPool,
    pub oi: WpdmOutputInfo
}

#[derive(Debug)]
pub enum WpdmIoRequest {
    StartTransition(StartTransitionReq),
    InitRender(WpdmOutputInfo),
    Render(WpdmIoRenderRequest),
    CliMonitorQuery
}

#[derive(Debug)]
pub struct StartTransitionReq {
    pub oi: WpdmOutputInfo,
    pub from: Option<PathBuf>,
    pub to: PathBuf,
    pub anim: TransitionAnim
}

#[derive(Debug)]
pub struct TransitionParams {
    pub tran_type: TransitionType,
    pub tran_origin: TransitionOrigin,
    pub angle: f32,
    pub n_frames: u32
}

#[derive(Debug)]
pub struct WpdmIoRenderRequest {
    pub slot: SlotPool,
    pub buffer: Buffer,
    pub oi: WpdmOutputInfo,
    pub render_next: bool
}
