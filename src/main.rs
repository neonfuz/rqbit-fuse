use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(name = "torrent-fuse")]
#[command(about = "A FUSE filesystem for accessing torrents via rqbit")]
struct Cli {
    #[arg(short, long, help = "Increase verbosity")]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(if cli.verbose {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        })
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    torrent_fuse::run().await
}
