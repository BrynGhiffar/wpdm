use clap::Parser;

#[derive(Parser)]
struct Args {
    #[arg(short, long)]
    image_path: String,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();
    let args = Args::parse();
    let mut client = wpdm_common::WpdmClient::new(None)?;
    client.set_wallpaper(args.image_path)?;
    Ok(())
}
