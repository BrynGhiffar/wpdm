use std::path::PathBuf;

pub enum RenderCmdTy {
    CircleTr,
    DiagTr,
}

// Render transition origin
pub enum RenderTrOrigin {
    Center,
    Left,
    Right,
    Random,
    Coord(u64, u64),
}

pub struct RenderCmd {
    pub monitor: String,
    pub src_argb_buff_path: PathBuf,
    pub dest_argb_buff_path: PathBuf,
    pub origin: RenderTrOrigin,
    pub tr_ty: RenderCmdTy,
}
