//! e2e tests for mock provider static body support (`response.body.content`).
//!
//! Verifies the YAML schema documented in config.example.yaml actually works:
//! ```yaml
//! response:
//!   status: 201
//!   body:
//!     type: static
//!     content: '{"id": 42}'
//! ```

use std::sync::Arc;
use std::time::Duration;

use http_body_util::BodyExt;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use mystiproxy::config::MystiConfig;
use mystiproxy::config::{EngineConfig, LocationConfig, MatchMode, ProviderType, ProxyType};
use mystiproxy::http::{create_handler, BoxBody, HttpServer, HttpServerConfig};

async fn start_mock_engine(yaml: &str, port: u16) {
    let cfg: MystiConfig = serde_yaml::from_str(yaml).expect("valid yaml");
    let (_name, engine) = cfg.mysti.engine.into_iter().next().expect("one engine");
    let handler = create_handler(Arc::new(engine)).expect("handler");
    let mut server = HttpServer::new(
        HttpServerConfig::new(
            format!("tcp://127.0.0.1:{port}"),
            Some(Duration::from_secs(2)),
        ),
        handler,
        None,
    );
    server.start().await.expect("start");
    tokio::spawn(async move {
        let _ = server.run().await;
    });
}

async fn get(port: u16, path: &str) -> (u16, String) {
    let client: Client<_, BoxBody> = Client::builder(TokioExecutor::new()).build_http();
    let uri = format!("http://127.0.0.1:{port}{path}")
        .parse()
        .expect("uri");
    let resp = client.get(uri).await.expect("request");
    let status = resp.status().as_u16();
    let body = resp
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes()
        .to_stringLossy()
        .to_string();
    (status, body)
}

#[tokio::test]
async fn test_e2e_mock_static_body_via_yaml() {
    let yaml = format!(
        r#"
mysti:
  engine:
    mock1:
      proxy_type: http
      listen: tcp://127.0.0.1:19201
      target: tcp://127.0.0.1:1
      locations:
        - location: /api/data
          mode: Full
          provider: mock
          response:
            status: 201
            headers:
              Content-Type:
                value: application/json
                action: overwrite
            body:
              type: static
              content: '{{"id": 42, "name": "mock-data"}}'
cert: []
"#
    );
    start_mock_engine(&yaml, 19201).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (status, body) = get(19201, "/api/data").await;
    assert_eq!(status, 201, "custom mock status");
    assert_eq!(
        body, r#"{"id": 42, "name": "mock-data"}"#,
        "static body content"
    );
}

#[tokio::test]
async fn test_e2e_mock_empty_body_still_works() {
    let yaml = r#"
mysti:
  engine:
    mock2:
      proxy_type: http
      listen: tcp://127.0.0.1:19202
      target: tcp://127.0.0.1:1
      locations:
        - location: /empty
          mode: Full
          provider: mock
          response:
            status: 204
cert: []
"#;
    start_mock_engine(yaml, 19202).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (status, body) = get(19202, "/empty").await;
    assert_eq!(status, 204);
    assert!(body.is_empty(), "no content => empty body, got {body:?}");
}

#[tokio::test]
async fn test_e2e_mock_static_body_struct_config() {
    // 程序化构造（不经YAML）同样生效
    use mystiproxy::config::{
        BodyConfig, BodyType, HeaderAction, HeaderActionType, ResponseConfig,
    };
    use std::collections::HashMap;

    let engine = EngineConfig {
        listen: "tcp://127.0.0.1:19203".to_string(),
        target: "tcp://127.0.0.1:1".to_string(),
        proxy_type: ProxyType::Http,
        request_timeout: Some(Duration::from_secs(2)),
        connection_timeout: None,
        header: None,
        locations: Some(vec![LocationConfig {
            location: "/hello".to_string(),
            mode: MatchMode::Full,
            provider: Some(ProviderType::Mock),
            root: None,
            response: Some(ResponseConfig {
                conditions: None,
                status: Some(200),
                headers: Some(HashMap::from([(
                    "X-From".to_string(),
                    HeaderAction {
                        value: "struct-mock".to_string(),
                        action: HeaderActionType::Overwrite,
                        condition: None,
                    },
                )])),
                body: Some(BodyConfig {
                    template: None,
                    json: None,
                    body_type: Some(BodyType::Static),
                    content: Some("hello from struct".to_string()),
                }),
            }),
            request: None,
            index_files: None,
            enable_directory_listing: None,
        }]),
        auth: None,
        tls: None,
        upstream: None,
        allow: None,
        deny: None,
        management: None,
    };

    let handler = create_handler(Arc::new(engine)).expect("handler");
    let mut server = HttpServer::new(
        HttpServerConfig::new(
            "tcp://127.0.0.1:19203".to_string(),
            Some(Duration::from_secs(2)),
        ),
        handler,
        None,
    );
    server.start().await.expect("start");
    tokio::spawn(async move {
        let _ = server.run().await;
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (status, body) = get(19203, "/hello").await;
    assert_eq!(status, 200);
    assert_eq!(body, "hello from struct");
}

#[tokio::test]
async fn test_e2e_mock_body_json_type_without_content() {
    // body_type=json 但无 content：行为与旧版一致（空体），不 panic
    use mystiproxy::config::{BodyConfig, BodyType, ResponseConfig};

    let engine = EngineConfig {
        listen: "tcp://127.0.0.1:19204".to_string(),
        target: "tcp://127.0.0.1:1".to_string(),
        proxy_type: ProxyType::Http,
        request_timeout: Some(Duration::from_secs(2)),
        connection_timeout: None,
        header: None,
        locations: Some(vec![LocationConfig {
            location: "/".to_string(),
            mode: MatchMode::Prefix,
            provider: Some(ProviderType::Mock),
            root: None,
            response: Some(ResponseConfig {
                conditions: None,
                status: Some(200),
                headers: None,
                body: Some(BodyConfig {
                    template: None,
                    json: None,
                    body_type: Some(BodyType::Json),
                    content: None,
                }),
            }),
            request: None,
            index_files: None,
            enable_directory_listing: None,
        }]),
        auth: None,
        tls: None,
        upstream: None,
        allow: None,
        deny: None,
        management: None,
    };

    let handler = create_handler(Arc::new(engine)).expect("handler");
    let mut server = HttpServer::new(
        HttpServerConfig::new(
            "tcp://127.0.0.1:19204".to_string(),
            Some(Duration::from_secs(2)),
        ),
        handler,
        None,
    );
    server.start().await.expect("start");
    tokio::spawn(async move {
        let _ = server.run().await;
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (status, body) = get(19204, "/anything").await;
    assert_eq!(status, 200);
    assert!(body.is_empty(), "json type without content => empty body");
}

/// hyper Response body collect helper（消除 to_stringLossy 依赖差异）
trait BytesToString {
    fn to_stringLossy(&self) -> String;
}

impl BytesToString for hyper::body::Bytes {
    fn to_stringLossy(&self) -> String {
        String::from_utf8_lossy(self).to_string()
    }
}
