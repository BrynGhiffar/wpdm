use std::path::PathBuf;

use wpdm_common::{TransitionOrigin, TransitionType};

pub struct RenderCmd {
    pub monitor: String,
    pub src_argb_buff_path: PathBuf,
    pub dest_argb_buff_path: PathBuf,
    pub tr_origin: TransitionOrigin,
    pub tr_type: TransitionType,
    pub angle: f32,
}
