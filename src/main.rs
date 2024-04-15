use std::str::FromStr;

use anyhow::Result;
use config::{
    get_configuration,
    pbaas::{self, pbaas_chain_configs},
};
use discord::DiscordMessage;
use poise::serenity_prelude::futures::{future::join_all, stream::FuturesUnordered};
use rpc::Client;
use tokio::sync::mpsc;
use tracing::*;
use tracing_subscriber::{
    fmt::{self, writer::MakeWriterExt},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};
use vrsc_rpc::{
    client::RpcApi,
    json::{vrsc::Address, TransactionVout},
};
use zmq::{listen_block_notifications, ZMQMessage};

mod config;
mod discord;
mod rpc;
mod zmq;

#[tokio::main]
async fn main() -> Result<()> {
    logging()?;
    let _config = get_configuration()?;

    let handles = FuturesUnordered::new();

    let (discord_tx, discord_rx) = mpsc::unbounded_channel::<DiscordMessage>();

    for pbaas_config in pbaas_chain_configs()? {
        let (tx, rx) = mpsc::unbounded_channel::<ZMQMessage>();
        let zmq_url = pbaas_config.zmq_block_hash_url.clone();
        let cashback_checker = CashbackChecker::new(pbaas_config, rx, discord_tx.clone())?;

        handles.push(tokio::spawn(async move {
            if let Err(e) = cashback_checker.run(tx, zmq_url).await {
                error!("error: {e:?}");
            }
        }));
    }

    join_all(handles).await;

    Ok(())
}

#[allow(unused)]
#[derive(Debug)]
pub struct CashbackChecker {
    currency_id: Address,
    client: Client,
    referral_id: Address,
    rx: mpsc::UnboundedReceiver<ZMQMessage>,
    tx: mpsc::UnboundedSender<DiscordMessage>,
}

impl CashbackChecker {
    pub fn new(
        config: pbaas::Config,
        rx: mpsc::UnboundedReceiver<ZMQMessage>,
        tx: mpsc::UnboundedSender<DiscordMessage>,
    ) -> Result<Self> {
        let client: Client = config.clone().try_into()?;
        let currency_id = config.currency_id.clone();
        let referral_id = config.referral_currency_id.clone();

        Ok(Self {
            currency_id,
            client,
            referral_id,
            rx,
            tx,
        })
    }

    #[instrument(level = "trace", skip(self, tx, url), fields(chain = self.currency_id.to_string()))]
    pub async fn run(mut self, tx: mpsc::UnboundedSender<ZMQMessage>, url: String) -> Result<()> {
        // Spawn a listener for ZMQ messages
        tokio::spawn(async move {
            if let Err(e) = listen_block_notifications(tx, &url).await {
                error!("{e:?}");
            }
        });

        // Receive messages from ZMQ
        while let Some(message) = self.rx.recv().await {
            match message {
                ZMQMessage::NewBlock(block_hash) => {
                    debug!("getting block for blockhash {}", block_hash);

                    let block = self.client.client.get_block(&block_hash, 2)?;

                    for tx in block.tx {
                        for vout in tx.vout {
                            if self.tx_has_referral(&vout)? {
                                // store tx in database
                                // send message to discord
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    #[instrument(level = "trace", skip(self, vout))]
    fn tx_has_referral(&self, vout: &TransactionVout) -> Result<bool> {
        if let Some(identity_reservation) = &vout.script_pubkey.identity_reservation {
            debug!("{identity_reservation:#?}");
            if let Some(referral) = &identity_reservation.referral {
                let used_referral_address = Address::from_str(&referral)?;

                if self.referral_id == used_referral_address {
                    trace!("referral used");

                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
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
