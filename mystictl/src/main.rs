use kube::Client;

mod k8s;

#[tokio::main]
pub async fn main() {
    let client = Client::try_default().await;
}
