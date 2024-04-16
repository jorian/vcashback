use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use poise::serenity_prelude as serenity;
use tokio::sync::mpsc;
use tracing::*;
use vrsc_rpc::json::vrsc::Address;

use crate::config::DiscordConfig;

struct Data {} // User data, which is stored and accessible in all command invocations
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

#[poise::command(slash_command, prefix_command)]
async fn age(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

pub async fn run(
    config: DiscordConfig,
    mut rx: mpsc::UnboundedReceiver<DiscordMessage>,
) -> Result<()> {
    let token = config.token;
    let mut channels = HashMap::new();
    channels.insert(
        Address::from_str("iJhCezBExJHvtyH3fGhNnt2NhU4Ztkf2yq")?,
        1227894258216734782,
    );
    channels.insert(
        Address::from_str("iExBJfZYK7KREDpuhj6PzZBzqMAKaFg7d2")?,
        1227869235942785035,
    );

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
                        DiscordMessage::CashbackInitiated(currency_id, (name, nameid)) => {
                            info!("got message to send to discord");

                            let channel_id = channels.get(&currency_id).unwrap();
                            serenity::ChannelId::new(*channel_id)
                                .send_message(
                                    &http,
                                    serenity::CreateMessage::new().content(format!(
                                        ":sparkles:  **{}@** ({}) initiated cashback",
                                        name, nameid
                                    )),
                                )
                                .await
                                .unwrap();
                        }
                        DiscordMessage::CashbackProcessed(currency_id, (name, name_id), explorer_link) => {
                            let channel_id = channels.get(&currency_id).unwrap();
                            serenity::ChannelId::new(*channel_id)
                                .send_message(
                                    &http,
                                    serenity::CreateMessage::new()
                                        .content(format!(":moneybag:  Cashback processed for **{name}@** ({name_id}): [{explorer_link}]")),
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

    client?.start().await?;

    Ok(())
}

pub enum DiscordMessage {
    CashbackInitiated(Address, (String, Address)),
    CashbackProcessed(Address, (String, Address), String),
}
