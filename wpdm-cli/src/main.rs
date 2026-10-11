use core::fmt;
use std::fmt::Display;

use clap::Parser;
use wpdm_common::{CliRequest, TransitionOrigin, TransitionType, WallpaperCmd};

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

impl Display for ArgTransitionOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArgTransitionOrigin::Center => write!(f, "center"),
            ArgTransitionOrigin::Left => write!(f, "left"),
            ArgTransitionOrigin::Right => write!(f, "right"),
            ArgTransitionOrigin::Rand => write!(f, "rand")
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

    // Image decoding and monitor handling happen in the daemon; the cli
    // only forwards the command. The monitor query doubles as a liveness
    // check: a dead daemon fails the cli here instead of silently no-op'ing.
    let monitors = client.get_monitors()?;
    let names = monitors.into_iter().map(|mon| mon.name).collect();

    client.send(CliRequest::WallpaperCmd(WallpaperCmd {
        path: args.image_path.into(),
        monitors: names,
        tran_origin,
        tran_type,
        angle: args.angle
    }))?;

    Ok(())
}
