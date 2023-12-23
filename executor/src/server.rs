use std::net::SocketAddr;
use std::time::Duration;
use async_trait::async_trait;
use tonic::{Request, Response, Status, transport::Server};

mod executor {
    tonic::include_proto!("executor");
}
use executor::executor_service_server::{ExecutorService, ExecutorServiceServer};
use executor::{OkMessage, PayloadMessage};

struct ExecutorServiceImpl;

#[async_trait]
impl ExecutorService for ExecutorServiceImpl {
    async fn execute(&self, preblock: Request<PayloadMessage>) -> Result<Response<OkMessage>, Status> {
        println!("Preblock with {} txs received", preblock.into_inner().transactions.iter().count());
        std::thread::sleep(Duration::from_millis(500));
        println!("Preblock executed");
        Ok(Response::new(OkMessage {}))
    }
}

pub async fn run_executor_server(addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    Server::builder()
        .add_service(ExecutorServiceServer::new(ExecutorServiceImpl {}))
        .serve(addr)
        .await?;
    Ok(())
}