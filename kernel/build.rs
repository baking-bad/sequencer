// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use narwhal_test_utils::CommitteeFixture;
use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use std::num::NonZeroUsize;

pub const COMMITTEE_SIZE: usize = 4;

#[derive(Default)]
pub struct NoRng {
    counter: u8
}

impl rand::RngCore for NoRng {
    fn next_u32(&mut self) -> u32 {
        self.counter += 1;
        self.counter as u32
    }

    fn next_u64(&mut self) -> u64 {
        self.counter += 1;
        self.counter as u64
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.counter += 1;
        dest.fill(self.counter)
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        self.counter += 1;
        Ok(dest.fill(self.counter))
    }
}

impl rand::CryptoRng for NoRng {}

#[derive(Debug, Serialize, Deserialize)]
struct Set {
    value: String,
    to: String
}

#[derive(Debug, Serialize, Deserialize)]
struct Instruction {
    pub set: Set,
}

#[derive(Debug, Serialize, Deserialize)]
struct KernelSetup {
    pub instructions: Vec<Instruction>
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut output_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    output_path.push("../bin/kernel_config.yaml");

    // if output_path.exists() {
    //     return Ok(())
    // }

    let fixture = CommitteeFixture::builder()
        .rng(NoRng::default())
        .committee_size(NonZeroUsize::new(COMMITTEE_SIZE).unwrap())
        .build();

    let epoch = 0;
    let authorities: Vec<pre_block::PublicKey> = fixture
        .authorities()
        .map(|auth| auth.public_key().as_ref().to_vec())
        .collect();
    let value = bcs::to_bytes(&authorities)?;

    let kernel_setup = KernelSetup {
        instructions: vec![
            Instruction {
                set: Set {
                    value: hex::encode(&value),
                    to: format!("/authorities/{}", epoch)
                }
            }
        ]
    };

    let file = std::fs::File::create(output_path).expect("Could not create file");
    serde_yaml::to_writer(file, &kernel_setup)?;

    Ok(())
}
