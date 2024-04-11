use std::collections::HashMap;

use anyhow::{Context, Result};
use config::{get_configuration, pbaas::pbaas_chain_configs};
use rpc::Client;
use tokio::sync::mpsc;
use tracing::*;
use tracing_subscriber::{
    fmt::{self, writer::MakeWriterExt},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};
use vrsc_rpc::client::RpcApi;
use zmq::ZMQMessage;

mod config;
mod rpc;
mod zmq;

#[tokio::main]
async fn main() -> Result<()> {
    logging()?;
    let _config = get_configuration()?;

    let (tx, mut rx) = mpsc::unbounded_channel::<ZMQMessage>();

    let mut handles = vec![];
    let mut clients = HashMap::new();

    for pbaas_config in pbaas_chain_configs()? {
        let client: Client = pbaas_config.clone().try_into()?;
        clients.insert(pbaas_config.currency_id.clone(), client);

        handles.push(tokio::spawn({
            let zmq_url = pbaas_config.zmq_block_hash_url.clone();
            let tx = tx.clone();
            async move {
                let _ = zmq::listen_block_notifications(
                    &zmq_url,
                    pbaas_config.currency_id.clone(),
                    tx.clone(),
                )
                .await;
            }
        }));
    }

    while let Some(message) = rx.recv().await {
        info!("{message:?}");

        match message {
            ZMQMessage::NewBlock(currency_id, block_hash) => {
                let client = clients
                    .get(&currency_id)
                    .context("failed to get client from hashmap")?;

                let block = client.client.get_block(&block_hash, 2)?;

                debug!("{:#?}", block.tx)
            }
        }
    }

    Ok(())
}

fn logging() -> Result<()> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var(
            "RUST_LOG",
            "verusid_cashback=trace,vrsc-rpc=info,poise=info,serenity=info",
        )
    }

    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    let file_appender = tracing_appender::rolling::hourly("./logs", "error");

    tracing_subscriber::registry()
        // .with(opentelemetry)
        // Continue logging to stdout
        .with(filter_layer)
        .with(fmt::Layer::default())
        .with(
            fmt::Layer::new()
                .json()
                .with_ansi(false)
                .with_writer(file_appender.with_max_level(Level::ERROR)),
        )
        .try_init()?;

    Ok(())
}
