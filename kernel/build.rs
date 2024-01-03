// SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
//
// SPDX-License-Identifier: MIT

use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use pre_block::fixture::NarwhalFixture;

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

    let fixture = NarwhalFixture::default();

    let epoch = 0;
    let value = bcs::to_bytes(&fixture.authorities())?;

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
