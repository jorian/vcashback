use anyhow::Result;
use secrecy::{ExposeSecret, Secret};
use vrsc_rpc::json::vrsc::Address;

use crate::config::pbaas;

#[derive(Debug)]
pub struct Client {
    pub currency_id: Address,
    pub client: vrsc_rpc::client::Client,
}

impl Client {
    pub fn new(
        currency_id: Address,
        rpc_port: u16,
        rpc_user: String,
        rpc_password: Secret<String>,
    ) -> Result<Self> {
        Ok(Self {
            currency_id,
            client: vrsc_rpc::client::Client::rpc(vrsc_rpc::Auth::UserPass(
                format!("http://localhost:{}", rpc_port),
                rpc_user,
                rpc_password.expose_secret().to_owned(),
            ))?,
        })
    }
}

impl TryFrom<pbaas::Config> for Client {
    type Error = anyhow::Error;

    fn try_from(value: pbaas::Config) -> Result<Self, Self::Error> {
        Ok(Self::new(
            value.currency_id,
            value.rpc_port,
            value.rpc_user,
            Secret::new(value.rpc_password),
        )?)
    }
}
