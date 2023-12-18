use tonic::transport::Channel;

mod executor {
    tonic::include_proto!("executor");
}
use executor::executor_service_client::ExecutorServiceClient;
use executor::PayloadMessage;

/// Simple client for interaction with Executor gRPC server
/// Example of use
/// ```rust,ignore
///     let mut client = ExecutorClient::connect("http://127.0.0.1:5000".to_string()).await.unwrap();
///     client.execute(vec![vec![]]).await.unwrap();
/// ```
pub struct ExecutorClient {
    client: ExecutorServiceClient<Channel>
}

impl ExecutorClient {
    /// Connects to the executor gRPC server at the specified endpoint (e.g. `http://127.0.0.1:5000`)
    pub async fn connect(endpoint: String) -> Result<ExecutorClient, Box<dyn std::error::Error>> {
        let client = ExecutorServiceClient::connect(endpoint).await?;
        Ok(ExecutorClient {client})
    }

    /// Sends a batch of transactions to the executor
    pub async fn execute(&mut self, transactions: Vec<Vec<u8>>) -> Result<(), Box<dyn std::error::Error>> {
        self.client.execute(PayloadMessage{transactions}).await?;
        Ok(())
    }
}