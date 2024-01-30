use std::{error::Error, fs::File, time::Duration};

use clap::{command, Args, Parser, Subcommand};
use csv::Reader;
use log::{error, info};
use stats::{mean, median, stddev};
use tokio::{join, time::sleep};
mod narwhal {
    tonic::include_proto!("narwhal");
}

mod exporter {
    tonic::include_proto!("exporter");
}

mod listener;
mod spammer;

const DEFAULT_SPAMMER_ENDPOINT: &str = "http://127.0.0.1:64013";
const DEFAULT_LISTENER_ENDPOINT: &str = "http://127.0.0.1:64011";

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Adds files to myapp
    Spammer(SpammerArgs),
    Listener(ListenerArgs),
    Benchmark(BenchmarkArgs),
}

#[derive(Args)]
struct BenchmarkArgs {
    /// Worker's gRPC endpoint to connect to
    #[arg(short, long, default_value_t=(DEFAULT_SPAMMER_ENDPOINT.parse()).unwrap())]
    spammer_endpoint: String,
    /// Exporter's gRPC endpoint to connect to
    /// Worker's gRPC endpoint to connect to
    #[arg(short, long, default_value_t=(DEFAULT_LISTENER_ENDPOINT.parse()).unwrap())]
    listener_endpoint: String,
    /// Sleep duration, ms
    #[arg(short = 'z', long, default_value_t = 1000)]
    sleep: u64,
    /// Min transaction size, bytes
    #[arg(long, default_value_t = 1024)]
    min_size: u32,
    /// Max transaction size, bytes
    #[arg(long, default_value_t = 32768)]
    max_size: u32,
    /// Subdag id from which to receive updates
    #[arg(short, long, default_value_t = 0)]
    from_id: u64,
    /// Path to csv file to store transaction stats
    #[arg(short, long)]
    tx_output: Option<String>,
    /// The amount of time to run the benchmark for, in seconds
    #[arg(short, long, default_value_t = 30)]
    run_for: u64,
}

/// Simple transactions generator that connects to a worker
/// and sends generated transactions with a given interval
#[derive(Args)]
struct SpammerArgs {
    /// Worker's gRPC endpoint to connect to
    #[arg(short, long, default_value_t=(DEFAULT_SPAMMER_ENDPOINT.parse()).unwrap())]
    endpoint: String,
    /// Sleep duration, ms
    #[arg(short, long, default_value_t = 1000)]
    sleep: u64,
    /// Min transaction size, bytes
    #[arg(long, default_value_t = 1024)]
    min_size: u32,
    /// Max transaction size, bytes
    #[arg(long, default_value_t = 32768)]
    max_size: u32,
}

impl Into<SpammerArgs> for &BenchmarkArgs {
    fn into(self) -> SpammerArgs {
        SpammerArgs {
            endpoint: self.spammer_endpoint.clone(),
            sleep: self.sleep,
            min_size: self.min_size,
            max_size: self.max_size,
        }
    }
}

impl Into<ListenerArgs> for &BenchmarkArgs {
    fn into(self) -> ListenerArgs {
        ListenerArgs {
            endpoint: self.listener_endpoint.clone(),
            from_id: self.from_id,
            tx_output: self.tx_output.clone(),
        }
    }
}

#[derive(Args)]
struct ListenerArgs {
    /// Primary's gRPC endpoint to connect to
    #[arg(short, long, default_value_t=(DEFAULT_LISTENER_ENDPOINT.parse()).unwrap())]
    endpoint: String,
    /// Subdag id from which to receive updates
    #[arg(short, long, default_value_t = 0)]
    from_id: u64,
    /// Path to csv file to store transaction stats
    #[arg(short, long)]
    tx_output: Option<String>,
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Spammer(args) => {
            let (_tx_stop, rx_stop) = tokio::sync::broadcast::channel(1);
            spammer::run(args, rx_stop).await;
        }
        Commands::Listener(args) => {
            let (_tx_stop, rx_stop) = tokio::sync::broadcast::channel(1);
            listener::run(args, rx_stop).await;
        }
        Commands::Benchmark(args) => {
            let (tx_stop, rx_stop_spammer) = tokio::sync::broadcast::channel(1);
            let rx_stop_listener = tx_stop.subscribe();

            let spammer_args: SpammerArgs = (&args).into();
            let listener_args: ListenerArgs = (&args).into();

            let listener =
                tokio::spawn(async move { listener::run(listener_args, rx_stop_listener).await });
            info!("Listener started");

            let spammer =
                tokio::spawn(async move { spammer::run(spammer_args, rx_stop_spammer).await });
            info!("Spammer started");
            sleep(Duration::from_secs(args.run_for)).await;
            tx_stop
                .send(())
                .expect("Could not send signal to stop components");

            let (res_spammer, res_listener) = join!(spammer, listener);
            let () = res_spammer.expect("Spammer did not terminate correctly");
            let () = res_listener.expect("Listener did not terminate correctly");
            match args.tx_output {
                None => {
                    info!("No file for saving data specified. Benchmark will terminate");
                }
                Some(tx_output) => {
                    info!("Analizyng results");
                    TxStats::stats(tx_output).expect("Could not get statistics for results");
                }
            }
        }
    }
}

type Record = (u64, u64, u32, u32, u32);

struct TxStats {
    sent_timestamp: u64,
    received_timestamp: u64,
    _size: u32,
    _round: u32,
    _leader_round: u32,
}

impl From<Record> for TxStats {
    fn from(record: Record) -> TxStats {
        let (sent_timestamp, received_timestamp, size, round, leader_round) = record;
        TxStats {
            sent_timestamp,
            received_timestamp,
            _size: size,
            _round: round,
            _leader_round: leader_round,
        }
    }
}

impl TxStats {
    pub fn latency(&self) -> u64 {
        self.received_timestamp - self.sent_timestamp
    }

    pub fn latencies<'a>(
        rdr: &'a mut Reader<File>,
    ) -> Result<impl Iterator<Item = u64> + 'a, Box<dyn Error>> {
        let latencies = rdr
            .deserialize::<Record>()
            .filter_map(|record| match record {
                Ok(record) => Some(Into::<TxStats>::into(record).latency()),
                Err(e) => {
                    error!("Could not parse record: {}", e);
                    None
                }
            });

        Ok(latencies)
    }

    pub fn count(tx_output: String) -> Result<usize, Box<dyn Error>> {
        let mut rdr = Reader::from_path(tx_output.as_str())?;
        let records = Self::latencies(&mut rdr)?;
        Ok(records.count())
    }

    pub fn mean(tx_output: String) -> Result<f64, Box<dyn Error>> {
        let mut rdr = Reader::from_path(tx_output.as_str())?;
        let records = Self::latencies(&mut rdr)?;
        Ok(mean(records))
    }

    pub fn stddev(tx_output: String) -> Result<f64, Box<dyn Error>> {
        let mut rdr = Reader::from_path(tx_output.as_str())?;
        let records = Self::latencies(&mut rdr)?;
        Ok(stddev(records))
    }

    pub fn median(tx_output: String) -> Result<Option<f64>, Box<dyn Error>> {
        let mut rdr = Reader::from_path(tx_output.as_str())?;
        let records = Self::latencies(&mut rdr)?;
        Ok(median(records))
    }

    pub fn stats(tx_output: String) -> Result<(), Box<dyn Error>> {
        println!(
            "Transactions: {}, Mean: {}, Median: {:?}, Std-dev: {}",
            Self::count(tx_output.clone())?,
            Self::mean(tx_output.clone())?,
            Self::median(tx_output.clone())?,
            Self::stddev(tx_output)?
        );
        Ok(())
    }
}
