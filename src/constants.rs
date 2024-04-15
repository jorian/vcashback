use serde::{Deserialize, Serialize};
use vrsc_rpc::{bitcoin::Txid, json::vrsc::Address};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cashback {
    pub currency_id: Address,
    pub name_id: Address,
    pub name: String,
    pub txid: Option<Txid>,
}
