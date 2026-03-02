//! CLI client for the distributed rate limiter.

mod limiter {
    tonic::include_proto!("limiter");
}

use clap::{Parser, Subcommand};
use limiter::limiter_service_client::LimiterServiceClient;
use tonic::transport::Channel;

#[derive(Parser)]
#[command(name = "limiter-client")]
#[command(about = "Distributed rate limiter CLI client")]
struct Cli {
    #[arg(long, default_value = "http://127.0.0.1:50051")]
    server: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Check if a request is allowed (consumes 1 token)
    Allow {
        /// Rate limit key (e.g. user:123, api:key-abc)
        #[arg(short, long)]
        key: String,

        /// Tokens to consume (default 1)
        #[arg(short, long, default_value = "1")]
        tokens: u32,
    },
    /// Set rate limit for a key
    SetLimit {
        #[arg(short, long)]
        key: String,

        /// Max tokens (burst capacity)
        #[arg(short, long)]
        capacity: u64,

        /// Tokens per second (refill rate)
        #[arg(short, long)]
        refill_rate: f64,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = Cli::parse();

    let channel = Channel::from_shared(cli.server.clone())?
        .connect()
        .await?;

    let mut client = LimiterServiceClient::new(channel);

    match cli.command {
        Commands::Allow { key, tokens } => {
            let req = tonic::Request::new(limiter::AllowRequest { key, tokens });
            let res = client.allow(req).await?;
            let r = res.into_inner();
            if r.allowed {
                println!("allowed");
            } else {
                eprintln!("denied: {}", r.message);
                std::process::exit(1);
            }
        }
        Commands::SetLimit {
            key,
            capacity,
            refill_rate,
        } => {
            let req = tonic::Request::new(limiter::SetLimitRequest {
                key,
                capacity,
                refill_rate,
            });
            let res = client.set_limit(req).await?;
            let r = res.into_inner();
            if r.ok {
                println!("ok");
            } else {
                eprintln!("error: {}", r.error);
                std::process::exit(1);
            }
        }
    }

    Ok(())
}
