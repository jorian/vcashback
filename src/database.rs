use std::str::FromStr;

use anyhow::Result;
use sqlx::PgPool;
use vrsc_rpc::{bitcoin::Txid, json::vrsc::Address};

use crate::constants::Cashback;

#[derive(Debug)]
pub struct DbCashback {
    pub currency_id: String,
    pub name_id: String,
    pub name_str: String,
    pub txid: Option<String>,
}

impl TryFrom<DbCashback> for Cashback {
    type Error = sqlx::Error;

    fn try_from(value: DbCashback) -> Result<Self, Self::Error> {
        Ok(Self {
            currency_id: Address::from_str(&value.currency_id)
                .map_err(|e| sqlx::Error::Decode(e.into()))?,
            name_id: Address::from_str(&value.name_id)
                .map_err(|e| sqlx::Error::Decode(e.into()))?,
            name: value.name_str,
            txid: value
                .txid
                .map(|txid_str| Txid::from_str(&txid_str))
                .transpose()
                .map_err(|e| sqlx::Error::Decode(e.into()))?,
        })
    }
}

pub async fn store_cashback(
    pool: &PgPool,
    currency_id: &Address,
    name_id: &Address,
    name: &str,
) -> Result<()> {
    sqlx::query!(
        "INSERT INTO cashbacks (currency_id, name_id, name_str)
            VALUES ($1, $2, $3)",
        currency_id.to_string(),
        name_id.to_string(),
        name
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn update_cashback(
    pool: &PgPool,
    currency_id: &Address,
    name_id: &Address,
    txid: &Txid,
) -> Result<()> {
    sqlx::query!(
        "UPDATE cashbacks
        SET txid = $3
        WHERE currency_id = $1 and name_id = $2",
        currency_id.to_string(),
        name_id.to_string(),
        txid.to_string()
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_pending_cashbacks(pool: &PgPool) -> Result<Vec<Cashback>> {
    let rows = sqlx::query_as!(
        DbCashback,
        "SELECT currency_id, name_id, name_str, txid
        FROM cashbacks
        WHERE txid IS NULL"
    )
    .try_map(Cashback::try_from)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}
