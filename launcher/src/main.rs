use clap::Parser;
use std::env;
use std::path::Path;
use std::process::Command;

/// Simple launcher that runs narwhal primary with a single worker
/// with preconfigured parameters, keys, and committee
#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    /// Id of the node to run (from 1 to 7)
    #[arg(long, default_value_t = 1)]
    id: u8,
    /// Tracing log level (0 - error, 1 - warn, 2 - info, 3 - debug, 4 - trace)
    #[arg(long, default_value_t = 2)]
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

    let mut cmd = Command::new("cargo");

    cmd.args(vec!["run", "--bin", "narwhal-node", "--"]);

    if args.log_level > 0 {
        let verbosity = format!("-{}", (0..args.log_level).map(|_| "v").collect::<String>());
        cmd.arg(verbosity);
    }

    cmd.args(vec![
        "run-comb",
        "--primary-keys",
        format!("./launcher/defaults/primary-{}.key", args.id).as_str(),
        "--primary-network-keys",
        format!("./launcher/defaults/primary-network-{}.key", args.id).as_str(),
        "--worker-keys",
        format!("./launcher/defaults/worker-network-{}.key", args.id).as_str(),
        "--committee",
        "./launcher/defaults/committee.json",
        "--workers",
        "./launcher/defaults/workers.json",
        "--primary-store",
        format!("./.db/primary-{}", args.id).as_str(),
        "--worker-store",
        format!("./.db/worker-{}", args.id).as_str(),
    ]);

    // run and wait for exit
    let status = cmd.status();

    println!("Node exited with {:?}", status);
}
