// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use bytes::Bytes;
use narwhal_types::{TransactionProto, TransactionsClient};
use tonic::transport::Channel;

#[derive(Clone, Debug)]
pub struct WorkerClient {
    pub endpoint: String,
    client: Option<TransactionsClient<Channel>>,
}

impl WorkerClient {
    pub fn new(endpoint: String) -> Self {
        Self {
            endpoint,
            client: None,
        }
    }

    async fn client(&mut self) -> anyhow::Result<&mut TransactionsClient<Channel>> {
        if self.client.is_none() {
            self.client = Some(TransactionsClient::connect(self.endpoint.clone()).await?);
        }
        self.client
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Failed to initialize gRPC client"))
    }

    pub async fn send_transaction(&mut self, payload: Vec<u8>) -> anyhow::Result<()> {
        let tx = TransactionProto {
            transaction: Bytes::from(payload),
        };
        self.client().await?.submit_transaction(tx).await?;
        Ok(())
    }
}
