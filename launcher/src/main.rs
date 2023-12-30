use std::{env, thread};
use std::path::Path;
use std::process::{Command, Child};
use std::sync::{Arc, atomic::AtomicBool, atomic::Ordering};
use std::time::Duration;
use clap::Parser;
use signal_hook::flag::register;
use signal_hook::consts::{SIGINT, SIGTERM};

/// Simple launcher that runs narwhal primary with a single worker
/// with preconfigured parameters, keys, and committee
#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    /// Id of the node to run (from 1 to 7)
    #[arg(long, default_value_t=1)]
    id: u8,
    /// Tracing log level (0 - error, 1 - warn, 2 - info, 3 - debug, 4 - trace)
    #[arg(long, default_value_t=2)]
    log_level: u8,
}

fn main() {
    let args = Args::parse();

    // set working dir to the repository root
    let repository_root = Path::new(&env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get repository path");
    std::env::set_current_dir(&repository_root.to_path_buf())
        .expect("Failed to set working directory");

    // handle termination signals to properly stop primary and worker processes
    let terminate = Arc::new(AtomicBool::new(false));
    register(SIGINT, Arc::clone(&terminate))
        .expect("Failed to register SIGINT handler");
    register(SIGTERM, Arc::clone(&terminate))
        .expect("Failed to register SIGTERM handler");

    // // run primary
    // let mut primary = run(NodeType::Primary, args.id, args.log_level)
    //     .expect("Failed to run primary");

    // println!("Primary started with pid {}", primary.id());

    // // run worker
    // let mut worker = run(NodeType::Worker, args.id, args.log_level)
    //     .expect("Failed to run worker");

    // println!("Worker started with pid {}", worker.id());

    // run comb
    let mut comb = run_comb(args.id, args.log_level)
        .expect("Failed to run comb");

    println!("Comb started with pid {}", comb.id());

    // wait for termination
    while !terminate.load(Ordering::Relaxed) {
        // // check if primary is ok
        // match primary.try_wait() {
        //     Ok(None) => {
        //         // primary is ok
        //     },
        //     Ok(Some(exit_code)) => {
        //         println!("Primary exited with {}", exit_code);
        //         break;
        //     },
        //     Err(e) => {
        //         println!("Failed to check primary's status: {:?}", e);
        //         break;
        //     }
        // }
        // // check if worker is ok
        // match worker.try_wait() {
        //     Ok(None) => {
        //         // worker is ok
        //     },
        //     Ok(Some(exit_code)) => {
        //         println!("Worker exited with {}", exit_code);
        //         break;
        //     },
        //     Err(e) => {
        //         println!("Failed to check worker's status: {:?}", e);
        //         break;
        //     }
        // }
        // check if comb is ok
        match comb.try_wait() {
            Ok(None) => {
                // comb is ok
            },
            Ok(Some(exit_code)) => {
                println!("Comb exited with {}", exit_code);
                break;
            },
            Err(e) => {
                println!("Failed to check comb's status: {:?}", e);
                break;
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
    
    println!("Killing child processes...");

    // primary.kill().expect("Failed to kill primary");
    // worker.kill().expect("Failed to kill worker");
    comb.kill().expect("Failed to kill worker");

    // println!("Primary exited with {}", primary.wait().unwrap());
    // println!("Worker exited with {}", worker.wait().unwrap());
    println!("Comb exited with {}", comb.wait().unwrap());
}

// enum NodeType {
//     Primary,
//     Worker,
// }

// fn run(node_type: NodeType, id: u8, log_level: u8) -> std::io::Result<Child> {
//     let mut cmd = Command::new("cargo");
    
//     cmd.args(vec![
//         "run",
//         "--bin", "narwhal-node",
//         "--",
//     ]);

//     if log_level > 0 {
//         let verbosity = format!("-{}", (0..log_level).map(|_| "v").collect::<String>());
//         cmd.arg(verbosity);
//     }

//     cmd.args(vec![
//         "run",
//         "--primary-keys", format!("./launcher/defaults/primary-{}.key", id).as_str(),
//         "--primary-network-keys", format!("./launcher/defaults/primary-network-{}.key", id).as_str(),
//         "--worker-keys", format!("./launcher/defaults/worker-network-{}.key", id).as_str(),
//         "--committee", "./launcher/defaults/committee.json",
//         "--workers", "./launcher/defaults/workers.json",
//     ]);

//     match node_type {
//         NodeType::Primary => {
//             cmd.args(vec![
//                 "--store", format!("./.db/primary-{}", id).as_str(),
//                 "primary",
//             ]);
//         }
//         NodeType::Worker => {
//             cmd.args(vec![
//                 "--store", format!("./.db/worker-{}", id).as_str(),
//                 "worker", "0",
//             ]);
//         }
//     }
    
//     cmd.spawn()
// }

fn run_comb(id: u8, log_level: u8) -> std::io::Result<Child> {
    let mut cmd = Command::new("cargo");
    
    cmd.args(vec![
        "run",
        "--bin", "narwhal-node",
        "--",
    ]);

    if log_level > 0 {
        let verbosity = format!("-{}", (0..log_level).map(|_| "v").collect::<String>());
        cmd.arg(verbosity);
    }

    cmd.args(vec![
        "run-comb",
        "--primary-keys", format!("./launcher/defaults/primary-{}.key", id).as_str(),
        "--primary-network-keys", format!("./launcher/defaults/primary-network-{}.key", id).as_str(),
        "--worker-keys", format!("./launcher/defaults/worker-network-{}.key", id).as_str(),
        "--committee", "./launcher/defaults/committee.json",
        "--workers", "./launcher/defaults/workers.json",
        "--primary-store", format!("./.db/primary-{}", id).as_str(),
        "--worker-store", format!("./.db/worker-{}", id).as_str(),
    ]);
    
    cmd.spawn()
}