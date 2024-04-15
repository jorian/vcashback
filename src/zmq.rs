use std::str::FromStr;

use anyhow::Result;
use poise::serenity_prelude::futures::StreamExt;
use tokio::sync::mpsc::UnboundedSender;
use tracing::*;
use vrsc_rpc::bitcoin::BlockHash;

#[instrument(level = "trace")]
pub async fn listen_block_notifications(
    tx: UnboundedSender<ZMQMessage>,
    url: &str,
    // chain: Address,
) -> Result<()> {
    let mut socket = tmq::subscribe(&tmq::Context::new())
        .connect(url.as_ref())?
        .subscribe(b"hash")?;

    info!("listening on {url}");

    loop {
        if let Some(Ok(msg)) = socket.next().await {
            if let Some(hash) = msg.into_iter().nth(1) {
                let block_hash = hash
                    .iter()
                    .map(|byte| format!("{:02x}", *byte))
                    .collect::<Vec<_>>()
                    .join("");

                tx.send(ZMQMessage::NewBlock(BlockHash::from_str(&block_hash)?))?;
            } else {
                error!("not a valid message!");
            }
        } else {
            error!("no correct message received");
        }
    }
}

#[derive(Debug)]
pub enum ZMQMessage {
    NewBlock(BlockHash),
}
