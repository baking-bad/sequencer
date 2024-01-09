// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use narwhal_config::{Committee, Import};
use pre_block::{fixture::NarwhalFixture, PublicKey};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
struct Set {
    value: String,
    to: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Instruction {
    pub set: Set,
}

#[derive(Debug, Serialize, Deserialize)]
struct KernelSetup {
    pub instructions: Vec<Instruction>,
}

fn mock_setup() -> std::result::Result<KernelSetup, Box<dyn std::error::Error>> {
    let fixture = NarwhalFixture::default();

    let epoch = 0;
    let value = bcs::to_bytes(&fixture.authorities())?;

    let setup = KernelSetup {
        instructions: vec![Instruction {
            set: Set {
                value: hex::encode(&value),
                to: format!("/authorities/{}", epoch),
            },
        }],
    };
    Ok(setup)
}

fn real_setup() -> std::result::Result<KernelSetup, Box<dyn std::error::Error>> {
    let mut committee = Committee::import("../launcher/defaults/committee.json")?;
    committee.load();

    let authorities: Vec<PublicKey> = committee
        .authorities()
        .map(|auth| auth.protocol_key_bytes().0.to_vec())
        .collect();
    let value = bcs::to_bytes(&authorities)?;

    let setup = KernelSetup {
        instructions: vec![
            // Instruction {
            //     set: Set {
            //         value: hex::encode(&value),
            //         to: format!("/authorities/{}", committee.epoch()),
            //     },
            // },
            Instruction {
                set: Set {
                    value: hex::encode(&(0u64.to_be_bytes())),
                    to: format!("/index"),
                },
            },
        ],
    };
    Ok(setup)
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut output_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    output_path.push("../bin/kernel_config.yaml");

    let kernel_setup = real_setup()?;

    let file = std::fs::File::create(output_path).expect("Could not create file");
    serde_yaml::to_writer(file, &kernel_setup)?;

    Ok(())
}
