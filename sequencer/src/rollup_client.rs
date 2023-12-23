use serde::Deserialize;
use tezos_smart_rollup_encoding::smart_rollup::SmartRollupAddress;

use crate::da_batcher::DaBatch;

#[derive(Deserialize)]
pub struct DurableStorageError {
    pub kind: String,
    pub id: String,
    pub block: Option<String>,
    pub msg: Option<String>,
}

impl std::fmt::Display for DurableStorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.block, &self.msg) {
            (Some(hash), None) => f.write_fmt(format_args!("[{}] {}", self.id, hash)),
            (None, Some(msg)) => f.write_fmt(format_args!("[{}] {}", self.id, msg)),
            (None, None) => f.write_str(self.id.as_str()),
            _ => unreachable!(),
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum DurableStorageResponse {
    Value(String),
    Errors(Vec<DurableStorageError>),
}

#[derive(Clone, Debug)]
pub struct RollupClient {
    pub base_url: String,
    client: reqwest::Client,
}

impl RollupClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }

    pub async fn get_rollup_address(&self) -> anyhow::Result<SmartRollupAddress> {
        let res = self
            .client
            .get(format!("{}/global/smart_rollup_address", self.base_url))
            .send()
            .await?;

        if res.status() == 200 {
            let value: String = res.json().await?;
            Ok(SmartRollupAddress::from_b58check(&value)?)
        } else {
            Err(anyhow::anyhow!(
                "Response status {0}",
                res.status().as_u16()
            ))
        }
    }

    pub async fn store_get(&self, key: String) -> anyhow::Result<Vec<u8>> {
        let res = self
            .client
            .get(format!(
                "{}/global/block/head/durable/wasm_2_0_0/value?key={}",
                self.base_url, key
            ))
            .send()
            .await?;

        if res.status() == 200 || res.status() == 500 {
            let content: Option<DurableStorageResponse> = res.json().await?;
            match content {
                Some(DurableStorageResponse::Value(value)) => {
                    let payload = hex::decode(value)?;
                    Ok(payload)
                }
                Some(DurableStorageResponse::Errors(errors)) => {
                    let message = errors.first().unwrap().to_string();
                    Err(anyhow::anyhow!(message))
                }
                None => Err(anyhow::anyhow!("Key not found: {0}", key)),
            }
        } else {
            Err(anyhow::anyhow!(
                "Response status {0}",
                res.status().as_u16()
            ))
        }
    }

    pub async fn inject_batch(&self, batch: DaBatch) -> anyhow::Result<()> {
        let messages: Vec<String> = batch.into_iter().map(|msg| hex::encode(msg)).collect();

        let res = self
            .client
            .post(format!("{}/local/batcher/injection", self.base_url))
            .json(&messages)
            .send()
            .await?;

        if res.status() == 200 {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Response status {0}",
                res.status().as_u16()
            ))
        }
    }
}
