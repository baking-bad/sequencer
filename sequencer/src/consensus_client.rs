use narwhal_types::{TransactionsClient, TransactionProto};
use tonic::transport::Channel;
use bytes::Bytes;

#[derive(Clone, Debug)]
pub struct ConsensusClient {
    pub endpoint: String,
    client: Option<TransactionsClient<Channel>>,
}

impl ConsensusClient {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint, client: None }
    }

    async fn client(&mut self) -> anyhow::Result<&mut TransactionsClient<Channel>> {
        if self.client.is_none() {
            self.client = Some(TransactionsClient::connect(self.endpoint.clone()).await?);
        }
        self.client.as_mut().ok_or_else(|| anyhow::anyhow!("Failed to initialize gRPC client"))
    }

    pub async fn send_transaction(&mut self, payload: Vec<u8>) -> anyhow::Result<()> {
        let tx = TransactionProto {
            transaction: Bytes::from(payload),
        };
        self.client().await?.submit_transaction(tx).await?;
        Ok(())
    }
}

