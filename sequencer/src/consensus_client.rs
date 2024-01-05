// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use bytes::Bytes;
use log::{debug, info};
use narwhal_types::{TransactionProto, TransactionsClient};
use pre_block::{Certificate, CertificateHeader, PreBlock, SystemMessage};
use std::{collections::BTreeSet, sync::mpsc};
use tonic::transport::Channel;

mod exporter {
    tonic::include_proto!("exporter");
}
use exporter::exporter_client::ExporterClient;

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
            info!("[Worker client] connecting to gRPC ...");
            self.client = Some(TransactionsClient::connect(self.endpoint.clone()).await?);
        }
        info!("[Worker client] connected to {}", self.endpoint);
        self.client
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("[Worker client] Failed to initialize gRPC client"))
    }

    pub async fn send_transaction(&mut self, payload: Vec<u8>) -> anyhow::Result<()> {
        let tx = TransactionProto {
            transaction: Bytes::from(payload),
        };
        let res = self.client().await?.submit_transaction(tx).await?;
        debug!("[Worker client] Response {:#?}", res.metadata());
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct PrimaryClient {
    pub endpoint: String,
    client: Option<ExporterClient<Channel>>,
}

impl PrimaryClient {
    pub fn new(endpoint: String) -> Self {
        Self {
            endpoint,
            client: None,
        }
    }

    async fn client(&mut self) -> anyhow::Result<&mut ExporterClient<Channel>> {
        if self.client.is_none() {
            info!("[Primary client] connecting to gRPC ...");
            self.client = Some(ExporterClient::connect(self.endpoint.clone()).await?);
        }
        info!("[Primary client] connected to {}", self.endpoint);
        self.client
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("[Primary client] Failed to initialize gRPC client"))
    }

    pub async fn subscribe_pre_blocks(
        &mut self,
        from_id: u64,
        pre_blocks_tx: mpsc::Sender<PreBlock>,
    ) -> anyhow::Result<()> {
        let mut stream = self
            .client()
            .await?
            .export(exporter::ExportRequest { from_id })
            .await?
            .into_inner();

        while let Some(subdag) = stream.message().await? {
            let pre_block: PreBlock = subdag.into();
            info!("[Primary client] Pre-block #{} received", pre_block.index());

            pre_blocks_tx.send(pre_block)?;
        }

        Ok(())
    }
}

impl From<exporter::SystemMessage> for SystemMessage {
    fn from(msg: exporter::SystemMessage) -> Self {
        match msg.message.unwrap() {
            exporter::system_message::Message::DkgConfirmation(msg) => Self::DkgConfirmation(msg),
            exporter::system_message::Message::DkgMessage(msg) => Self::DkgMessage(msg),
            exporter::system_message::Message::RandomnessSignature(sig) => {
                Self::RandomnessSignature(sig.randomness_round, sig.bytes)
            }
        }
    }
}

impl From<exporter::Header> for CertificateHeader {
    fn from(header: exporter::Header) -> Self {
        Self {
            author: header.author as u16,
            round: header.round,
            epoch: header.epoch,
            created_at: header.created_at,
            payload: header
                .payload_info
                .into_iter()
                .map(|info| {
                    (
                        info.digest.try_into().unwrap(),
                        (info.worker_id, info.created_at),
                    )
                })
                .collect(),
            system_messages: header
                .system_messages
                .into_iter()
                .map(|msg| msg.into())
                .collect(),
            parents: BTreeSet::from_iter(
                header
                    .parents
                    .into_iter()
                    .map(|digest| digest.try_into().unwrap()),
            ),
        }
    }
}

impl From<exporter::Certificate> for Certificate {
    fn from(cert: exporter::Certificate) -> Self {
        Self {
            header: cert.header.unwrap().into(),
            signers: cert.signers,
            signature: cert.signature,
        }
    }
}

impl From<exporter::SubDag> for PreBlock {
    fn from(sub_dag: exporter::SubDag) -> Self {
        Self {
            batches: sub_dag
                .payloads
                .into_iter()
                .map(|payload| payload.batches)
                .map(|batches| {
                    batches
                        .into_iter()
                        .map(|batch| batch.transactions)
                        .collect()
                })
                .collect(),
            index: sub_dag.id,
            leader: sub_dag.leader.unwrap().into(),
            certificates: sub_dag
                .certificates
                .into_iter()
                .map(|cert| cert.into())
                .collect(),
        }
    }
}
