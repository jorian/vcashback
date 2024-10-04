use std::{str::FromStr, time::Duration};

use anyhow::Result;
use config::{
    get_configuration,
    pbaas::{self, pbaas_chain_configs},
};
use discord::DiscordMessage;
use poise::serenity_prelude::futures::{future::join_all, stream::FuturesUnordered};
use rpc::Client;
use sqlx::PgPool;
use tokio::sync::mpsc;
use tracing::*;
use tracing_subscriber::{
    fmt::{self, writer::MakeWriterExt},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};
use vrsc_rpc::{
    bitcoin::Txid,
    client::{RpcApi, SendCurrencyOutput},
    json::{
        vrsc::{Address, Amount},
        TransactionVout,
    },
};
use zmq::{listen_block_notifications, ZMQMessage};

mod config;
mod constants;
mod database;
mod discord;
mod rpc;
mod zmq;

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging()?;

    let config = get_configuration()?;
    let pg_url = &config.database.connection_string();
    let pool = PgPool::connect_lazy(pg_url)?;

    let handles = FuturesUnordered::new();

    let (discord_tx, discord_rx) = mpsc::unbounded_channel::<DiscordMessage>();
    tokio::spawn(discord::run(config.discord, discord_rx));

    for pbaas_config in pbaas_chain_configs()? {
        let (tx, rx) = mpsc::unbounded_channel::<ZMQMessage>();
        let zmq_url = pbaas_config.zmq_block_hash_url.clone();

        let cashback_checker =
            CashbackChecker::new(pool.clone(), pbaas_config, rx, discord_tx.clone())?;

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
    pool: PgPool,
    currency_id: Address,
    client: Client,
    referral_id: Address,
    explorer_url: String,
    fee: u64,
    referral_amount: u64,
    rx: mpsc::UnboundedReceiver<ZMQMessage>,
    tx: mpsc::UnboundedSender<DiscordMessage>,
}

impl CashbackChecker {
    pub fn new(
        pool: PgPool,
        config: pbaas::Config,
        rx: mpsc::UnboundedReceiver<ZMQMessage>,
        tx: mpsc::UnboundedSender<DiscordMessage>,
    ) -> Result<Self> {
        let client: Client = config.clone().try_into()?;
        let currency_id = config.currency_id.clone();
        let referral_id = config.referral_currency_id.clone();
        let explorer_url = config.explorer_url.clone();
        let referral_amount = config.referral_amount;
        let fee = config.fee;

        Ok(Self {
            pool,
            currency_id,
            client,
            referral_id,
            explorer_url,
            fee,
            referral_amount,
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
                            if self.tx_has_referral(&vout).await? {
                                // store tx in database
                                // send message to discord
                            }
                        }
                    }

                    self.process_pending().await?;
                }
            }
        }

        Ok(())
    }

    #[instrument(level = "trace", skip(self, vout))]
    async fn tx_has_referral(&self, vout: &TransactionVout) -> Result<bool> {
        if let Some(identity_reservation) = &vout.script_pubkey.identity_reservation {
            debug!("{identity_reservation:#?}");
            if let Some(referral) = &identity_reservation.referral {
                let used_referral_address = Address::from_str(&referral)?;

                if self.referral_id == used_referral_address {
                    trace!("referral used");

                    database::store_cashback(
                        &self.pool,
                        &self.currency_id,
                        &identity_reservation.nameid,
                        &identity_reservation.name,
                    )
                    .await?;

                    self.tx
                        .send(DiscordMessage::CashbackInitiated(
                            self.currency_id.clone(),
                            (
                                identity_reservation.name.clone(),
                                identity_reservation.nameid.clone(),
                            ),
                        ))
                        .unwrap();

                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    #[instrument(level = "trace", skip(self))]
    async fn process_pending(&self) -> Result<()> {
        let pending = database::get_pending_cashbacks(&self.pool).await?;
        debug!("{pending:#?}");
        let blockheight = self.client.client.get_blockchain_info()?.blocks;

        for cashback in pending {
            let identity_hist = self.client.client.get_identity_history(
                &cashback.name_id.to_string(),
                0,
                99999999,
            )?;

            if (blockheight - identity_hist.blockheight as u64) < 10 {
                // wait 10 confirmations until payment
                return Ok(());
            }

            let tx = self.pool.begin().await?;

            let opid = self.client.client.send_currency(
                "*",
                vec![
                    SendCurrencyOutput {
                        currency: None,
                        amount: Amount::from_sat(self.referral_amount - self.fee),
                        address: cashback.name_id.to_string(),
                        convertto: None,
                        via: None,
                    },
                    SendCurrencyOutput {
                        currency: None,
                        amount: Amount::from_sat(self.fee - 20000),
                        address: self.referral_id.to_string(),
                        convertto: None,
                        via: None,
                    },
                ],
                None,
                None,
            )?;

            if let Some(txid) = wait_for_sendcurrency_finish(&self.client.client, &opid).await? {
                database::update_cashback(
                    &self.pool,
                    &cashback.currency_id,
                    &cashback.name_id,
                    &txid,
                )
                .await?;

                tx.commit().await?;

                self.tx
                    .send(DiscordMessage::CashbackProcessed(
                        self.currency_id.clone(),
                        (cashback.name.clone(), cashback.name_id.clone()),
                        format!("{}{}", self.explorer_url, txid),
                    ))
                    .unwrap();
            }
        }

        Ok(())
    }
}

fn setup_logging() -> Result<()> {
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

async fn wait_for_sendcurrency_finish(
    client: &vrsc_rpc::client::Client,
    opid: &str,
) -> Result<Option<Txid>> {
    loop {
        let operation_status = client.z_get_operation_status(vec![&opid])?;
        if let Some(Some(opstatus)) = operation_status.first() {
            debug!("op-status: {:#?}", opstatus);

            if ["queued", "executing"].contains(&opstatus.status.as_ref()) {
                tokio::time::sleep(Duration::from_millis(100)).await;
                trace!("opid still executing");

                continue;
            }

            if let Some(txid) = &opstatus.result {
                trace!(
                    "there was an operation_status, operation was executed with status: {}",
                    opstatus.status
                );

                return Ok(Some(txid.txid));
            }
        }
    }
}
