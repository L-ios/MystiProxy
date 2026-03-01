//! Example: Using MystiProxy Local Management
//!
//! This example demonstrates how to use the local management module
//! with synchronization capabilities.

#[cfg(feature = "local-management")]
use http_proxy::management::{
    LocalManagement, LocalManagementBuilder, LocalManagementConfig,
    MockConfiguration, HttpMethod, CreateMockRequest, MockRepository,
};
#[cfg(feature = "local-management")]
use uuid::Uuid;

#[cfg(feature = "local-management")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    println!("=== MystiProxy Local Management Example ===\n");
    
    // Example 1: Basic local management (no sync)
    println!("1. Creating basic local management instance...");
    let config = LocalManagementBuilder::new()
        .enabled(true)
        .db_path("/tmp/mystiproxy_example.db")
        .listen_addr("127.0.0.1:9090")
        .build();
    
    let mgmt = LocalManagement::init(config).await?;
    println!("   Instance ID: {}", mgmt.instance_id());
    println!("   Sync enabled: {}", mgmt.is_sync_enabled());
    
    // Create a mock configuration
    println!("\n2. Creating a mock configuration...");
    let repo = mgmt.repository();
    let request = CreateMockRequest {
        name: "Test API".to_string(),
        path: "/api/test".to_string(),
        method: HttpMethod::Get,
        matching_rules: Default::default(),
        response_config: Default::default(),
        is_active: true,
    };
    
    let config = repo.create(request).await?;
    println!("   Created mock: {} ({})", config.name, config.id);
    
    // List all mocks
    println!("\n3. Listing all mocks...");
    let mocks = repo.find_all(Default::default()).await?;
    println!("   Found {} mocks", mocks.len());
    for mock in &mocks {
        println!("   - {} [{}] {}", mock.name, mock.method, mock.path);
    }
    
    // Example 2: With sync enabled
    println!("\n4. Creating instance with sync enabled...");
    let instance_id = Uuid::new_v4();
    let sync_config = LocalManagementBuilder::new()
        .enabled(true)
        .db_path("/tmp/mystiproxy_sync_example.db")
        .with_sync("http://central.example.com", instance_id)
        .sync_interval(60)
        .api_key("test-api-key")
        .offline_queue(true)
        .max_queue_size(500)
        .build();
    
    let sync_mgmt = LocalManagement::init(sync_config).await?;
    println!("   Instance ID: {}", sync_mgmt.instance_id());
    println!("   Sync enabled: {}", sync_mgmt.is_sync_enabled());
    
    // Create API router
    println!("\n5. Creating API router...");
    let _router = sync_mgmt.create_router();
    println!("   Router created successfully");
    
    println!("\n=== Example completed successfully! ===");
    Ok(())
}

#[cfg(not(feature = "local-management"))]
fn main() {
    println!("This example requires the 'local-management' feature to be enabled.");
    println!("Run with: cargo run --example local_management_example --features local-management");
}
