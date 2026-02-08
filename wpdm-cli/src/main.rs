use core::fmt;

use clap::Parser;
use wpdm_common::{CliRequest, TransitionOrigin, TransitionType, WallpaperCmd};

use crate::cache::CachedImg;
use crate::util::group_by_size;
mod cache;
mod util;

#[derive(clap::ValueEnum, Debug, Clone, Copy)]
#[clap(rename_all = "kebab-case")]
enum ArgTransitionType {
    Circle,
    Wipe,
    None
}

impl std::fmt::Display for ArgTransitionType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ArgTransitionType::Circle => write!(f, "circle"),
            ArgTransitionType::Wipe => write!(f, "wipe"),
            ArgTransitionType::None => write!(f, "none")
        }
    }
}

#[derive(clap::ValueEnum, Debug, Clone, Copy)]
#[clap(rename_all = "kebab-case")]
enum ArgTransitionOrigin {
    Center,
    Left,
    Right,
    Rand
}

impl ToString for ArgTransitionOrigin {
    fn to_string(&self) -> String {
        match self {
            ArgTransitionOrigin::Center => "center".to_string(),
            ArgTransitionOrigin::Left => "left".to_string(),
            ArgTransitionOrigin::Right => "right".to_string(),
            ArgTransitionOrigin::Rand => "rand".to_string()
        }
    }
}

#[derive(Parser)]
struct Args {
    #[arg(short, long)]
    image_path: String,
    #[arg(short, long, default_value_t = ArgTransitionType::Circle)]
    transition: ArgTransitionType,
    #[arg(short, long, default_value_t = ArgTransitionOrigin::Center)]
    origin: ArgTransitionOrigin,
    #[arg(short, long, default_value_t = 30.0)]
    angle: f32,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();
    let args = Args::parse();
    let mut client = wpdm_common::WpdmClient::new()?;

    let mut cached_img = CachedImg::new(&args.image_path)?;

    let tran_type = match args.transition {
        ArgTransitionType::Circle => TransitionType::Circle,
        ArgTransitionType::None => TransitionType::None,
        ArgTransitionType::Wipe => TransitionType::Wipe
    };
    let tran_origin = match args.origin {
        ArgTransitionOrigin::Center => TransitionOrigin::Center,
        ArgTransitionOrigin::Left => TransitionOrigin::Left,
        ArgTransitionOrigin::Right => TransitionOrigin::Right,
        ArgTransitionOrigin::Rand => TransitionOrigin::Random,
    };

    // Get all active monitors
    let monitors = client.get_monitors()?;

    // Group monitors by size
    for (width, height, monitors) in group_by_size(monitors) {
        let img_path = cached_img.get_image(width, height)?;

        let req = CliRequest::WallpaperCmd(WallpaperCmd {
            path: img_path,
            monitors,
            tran_origin,
            tran_type,
            angle: args.angle
        });

        client.send(req)?;
    }

    Ok(())
}

