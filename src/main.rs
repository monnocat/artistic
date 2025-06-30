mod commands;
mod config;
mod database;
mod handlers;
mod init_tracing;
mod types;
mod util;

use std::env;

use color_eyre::{Result, eyre::Context as _};
use poise::{Framework, FrameworkOptions, builtins::register_in_guild, serenity_prelude::*};
use tracing::info;

use config::Config;
use handlers::{error_handler, post_announcements};
use types::Data;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install().wrap_err("failed to install color_eyre")?;
    dotenvy::dotenv().expect("failed to load .env file");
    init_tracing::init().wrap_err("failed to initialize tracing formatter")?;

    info!("Connecting to the database...");
    let pool = database::connect().await?;

    info!("Loading config...");
    let config = Config::load()?;

    let framework = Framework::builder()
        .options(FrameworkOptions {
            commands: commands::get(),
            on_error: |err| Box::pin(error_handler(err)),
            event_handler: |ctx, event, framework, data| {
                Box::pin(handlers::event_handler(ctx, event, framework, data))
            },
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                info!("Registering commands...");
                register_in_guild(ctx, &framework.options().commands, config.guild).await?;

                info!("Setting up data...");
                let data = Data::new(pool, config)
                    .await
                    .wrap_err("failed to load data")
                    .unwrap();

                tokio::spawn(post_announcements(ctx.clone(), data.clone()));

                info!("Done!");

                Ok(data)
            })
        })
        .build();

    let token = env::var("DISCORD_TOKEN").expect("environment variable DISCORD_TOKEN missing");

    let mut client = ClientBuilder::new(token, GatewayIntents::GUILDS)
        .framework(framework)
        .await
        .wrap_err("failed to create client")?;

    info!("Starting the client...");

    client
        .start()
        .await
        .wrap_err("the client encountered an error")
}
