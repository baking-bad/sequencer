use std::{
    fs::File,
    time::{SystemTime, UNIX_EPOCH},
};

use log::warn;

use super::SubDag;

#[derive(Debug)]
pub struct Stats {
    pub subdag_time: u128,
    pub num_txs: usize,
    pub payload_size: usize,
    pub avg_latency: u128,
    pub cert_time_delta: u128,
    pub cert_round_delta: u64,
}

pub fn stats(subdag: &SubDag) -> Stats {
    let subdag_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let mut num_txs = 0;
    let mut payload_size = 0;
    let mut sum_latency = 0;

    let first_cert_ts = subdag.certificates[0].clone().header.unwrap().created_at as u128;
    let last_cert_ts = subdag.leader.clone().unwrap().header.unwrap().created_at as u128;

    let first_cert_round = subdag.certificates[0].clone().header.unwrap().round;
    let last_cert_round = subdag.leader.clone().unwrap().header.unwrap().round;

    for payload in subdag.payloads.iter() {
        for batch in payload.batches.iter() {
            num_txs += batch.transactions.len();
            for tx in batch.transactions.iter() {
                payload_size += tx.len();
                let tx_time_bytes: [u8; 16] = match tx.get(0..16) {
                    Some(value) => value.try_into().unwrap(),
                    None => {
                        warn!("Foreign transaction {}", hex::encode(tx));
                        continue;
                    }
                };
                let tx_time = u128::from_be_bytes(tx_time_bytes);
                sum_latency += subdag_time - tx_time;
            }
        }
    }

    Stats {
        subdag_time,
        num_txs,
        payload_size,
        avg_latency: if num_txs > 0 {
            sum_latency / (num_txs as u128)
        } else {
            0
        },
        cert_time_delta: last_cert_ts - first_cert_ts,
        cert_round_delta: last_cert_round - first_cert_round,
    }
}

pub fn write_tx_stats(subdag: &SubDag, writer: &mut csv::Writer<File>) {
    let received_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let first_cert_round = subdag.certificates[0].clone().header.unwrap().round;
    let last_cert_round = subdag.leader.clone().unwrap().header.unwrap().round;
    let num_rounds = last_cert_round - first_cert_round + 1;
    let num_blocks = subdag.payloads.len();

    for payload in subdag.payloads.iter() {
        for batch in payload.batches.iter() {
            for tx in batch.transactions.iter() {
                let tx_time_bytes: [u8; 16] = match tx.get(0..16) {
                    Some(value) => value.try_into().unwrap(),
                    None => {
                        warn!("Foreign transaction {}", hex::encode(tx));
                        continue;
                    }
                };
                let tx_time = u128::from_be_bytes(tx_time_bytes);

                writer.write_field(tx_time.to_string()).unwrap();
                writer.write_field(received_at.to_string()).unwrap();
                writer.write_field(tx.len().to_string()).unwrap();
                writer.write_field(num_rounds.to_string()).unwrap();
                writer.write_field(num_blocks.to_string()).unwrap();
                writer.write_record(None::<&[u8]>).unwrap();
            }
        }
    }
}
