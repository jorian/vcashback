use std::sync::Arc;

use ::serenity::all::ChannelId;
use poise::serenity_prelude as serenity;
use tokio::sync::mpsc;
use tracing::*;

struct Data {} // User data, which is stored and accessible in all command invocations
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

#[poise::command(slash_command, prefix_command)]
async fn age(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

pub async fn run(token: String, mut rx: mpsc::UnboundedReceiver<DiscordMessage>) {
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![age()],
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            let http = Arc::clone(&ctx.http);
            tokio::spawn(async move {
                while let Some(message) = rx.recv().await {
                    match message {
                        DiscordMessage::CashbackReceived => {
                            info!("got message to send to discord");

                            // vrsctest
                            ChannelId::new(1227894258216734782)
                                .send_message(
                                    &http,
                                    serenity::CreateMessage::new().content("Kaching"),
                                )
                                .await
                                .unwrap();
                        }
                    }
                }
            });

            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {})
            })
        })
        .build();

    let intents = serenity::GatewayIntents::non_privileged()
        | serenity::GatewayIntents::GUILD_MEMBERS
        | serenity::GatewayIntents::MESSAGE_CONTENT;

    let client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await;

    client.unwrap().start().await.unwrap();
}

pub enum DiscordMessage {
    CashbackReceived,
}
