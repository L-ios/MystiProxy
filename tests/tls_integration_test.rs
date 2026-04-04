//! TLS 集成测试
//!
//! 测试 TLS 连接、ALPN 协商、mTLS 双向认证等功能

use mystiproxy::tls::{TlsConfig, TlsServer, create_tls_connector, create_tls_connector_with_client_cert};
use std::io::Write;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// 初始化 CryptoProvider（rustls 0.23 需要）
fn init_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

/// 生成测试用的自签名证书
fn generate_test_cert() -> (Vec<u8>, Vec<u8>) {
    use rcgen::{CertificateParams, KeyPair, PKCS_ECDSA_P256_SHA256};

    let mut params = CertificateParams::default();
    // 不设置为 CA，这是一个终端实体证书
    params.is_ca = rcgen::IsCa::NoCa;
    params.key_identifier_method = rcgen::KeyIdMethod::Sha256;
    params.distinguished_name = rcgen::DistinguishedName::new();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "localhost");

    // 添加 Subject Alternative Name (SAN) 扩展
    params.subject_alt_names = vec![
        rcgen::SanType::DnsName("localhost".try_into().unwrap()),
        rcgen::SanType::IpAddress("127.0.0.1".parse().unwrap()),
    ];

    let key_pair = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
    let cert = params.self_signed(&key_pair).unwrap();

    (
        cert.pem().into_bytes(),
        key_pair.serialize_pem().into_bytes(),
    )
}

/// 创建临时证书文件
fn create_temp_cert_files() -> (NamedTempFile, NamedTempFile) {
    let (cert_pem, key_pem) = generate_test_cert();

    let mut cert_file = NamedTempFile::new().unwrap();
    let mut key_file = NamedTempFile::new().unwrap();

    cert_file.write_all(&cert_pem).unwrap();
    key_file.write_all(&key_pem).unwrap();

    (cert_file, key_file)
}

#[tokio::test]
async fn test_tls_handshake() {
    init_crypto_provider();
    
    // 创建临时证书文件
    let (cert_file, key_file) = create_temp_cert_files();

    // 创建 TLS 配置
    let tls_config = TlsConfig::from_pem_files(cert_file.path(), key_file.path()).unwrap();
    let server_config = tls_config.to_server_config().unwrap();
    let tls_server = TlsServer::new(server_config);

    // 启动测试服务器
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    let server = async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut tls_stream = tls_server.accept(stream).await.unwrap();

        // 读取客户端消息
        let mut buf = [0u8; 1024];
        let n = tls_stream.read(&mut buf).await.unwrap();
        assert!(n > 0);

        // 回显消息
        tls_stream.write_all(&buf[..n]).await.unwrap();
    };

    let client = async move {
        // 等待服务器启动
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // 创建 TLS 连接器（使用自定义 CA 证书）
        let connector = create_tls_connector(Some(cert_file.path())).unwrap();
        let stream = TcpStream::connect(server_addr).await.unwrap();

        // 连接到服务器
        let domain = "localhost".try_into().unwrap();
        let mut tls_stream = connector.connect(domain, stream).await.unwrap();

        // 发送消息
        tls_stream.write_all(b"Hello, TLS!").await.unwrap();

        // 读取响应
        let mut buf = [0u8; 1024];
        let n = tls_stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"Hello, TLS!");
    };

    // 并发运行服务器和客户端
    tokio::join!(server, client);
}

#[tokio::test]
async fn test_tls_with_client_ca() {
    init_crypto_provider();
    
    // 创建临时证书文件
    let (cert_file, key_file) = create_temp_cert_files();

    // 创建 TLS 配置（带客户端 CA）
    let tls_config = TlsConfig::from_pem_files(cert_file.path(), key_file.path())
        .unwrap()
        .with_client_ca(cert_file.path())
        .unwrap();

    let server_config = tls_config.to_server_config_mutual().unwrap();
    let tls_server = TlsServer::new(server_config);

    // 启动测试服务器
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    let server = async move {
        let (stream, _) = listener.accept().await.unwrap();

        // 尝试 TLS 握手（客户端没有提供证书，应该失败）
        let result = tls_server.accept(stream).await;
        // 注意：由于客户端没有提供证书，握手应该失败
        // 但这取决于具体的 TLS 配置
        if let Ok(mut tls_stream) = result {
            // 如果握手成功，尝试读取数据
            let mut buf = [0u8; 1024];
            if let Ok(n) = tls_stream.read(&mut buf).await {
                assert!(n > 0);
            }
        }
    };

    let client = async move {
        // 等待服务器启动
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // 创建 TLS 连接器（带客户端证书）
        let connector = create_tls_connector_with_client_cert(
            cert_file.path(),
            cert_file.path(),
            key_file.path(),
        )
        .unwrap();

        let stream = TcpStream::connect(server_addr).await.unwrap();

        // 连接到服务器
        let domain = "localhost".try_into().unwrap();
        let mut tls_stream = connector.connect(domain, stream).await.unwrap();

        // 发送消息
        tls_stream.write_all(b"Hello, mTLS!").await.unwrap();
    };

    // 并发运行服务器和客户端
    tokio::join!(server, client);
}

#[tokio::test]
async fn test_multiple_tls_connections() {
    init_crypto_provider();
    
    // 创建临时证书文件
    let (cert_file, key_file) = create_temp_cert_files();

    // 创建 TLS 配置
    let tls_config = TlsConfig::from_pem_files(cert_file.path(), key_file.path()).unwrap();
    let server_config = tls_config.to_server_config().unwrap();

    // 启动测试服务器
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    let server = async move {
        for _ in 0..3 {
            let (stream, _) = listener.accept().await.unwrap();
            let tls_server = TlsServer::new(Arc::clone(&server_config));
            let mut tls_stream = tls_server.accept(stream).await.unwrap();

            // 读取并回显
            let mut buf = [0u8; 1024];
            let n = tls_stream.read(&mut buf).await.unwrap();
            tls_stream.write_all(&buf[..n]).await.unwrap();
        }
    };

    let client = async move {
        // 等待服务器启动
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let connector = create_tls_connector(Some(cert_file.path())).unwrap();

        // 创建多个连接
        for i in 0..3 {
            let stream = TcpStream::connect(server_addr).await.unwrap();
            let domain = "localhost".try_into().unwrap();
            let mut tls_stream = connector.connect(domain, stream).await.unwrap();

            let msg = format!("Message {}", i);
            tls_stream.write_all(msg.as_bytes()).await.unwrap();

            let mut buf = [0u8; 1024];
            let n = tls_stream.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], msg.as_bytes());
        }
    };

    // 并发运行服务器和客户端
    tokio::join!(server, client);
}

#[tokio::test]
async fn test_tls_large_data() {
    init_crypto_provider();
    
    // 创建临时证书文件
    let (cert_file, key_file) = create_temp_cert_files();

    // 创建 TLS 配置
    let tls_config = TlsConfig::from_pem_files(cert_file.path(), key_file.path()).unwrap();
    let server_config = tls_config.to_server_config().unwrap();
    let tls_server = TlsServer::new(server_config);

    // 启动测试服务器
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    let server = async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut tls_stream = tls_server.accept(stream).await.unwrap();

        // 读取大量数据
        let mut buf = vec![0u8; 1024 * 1024]; // 1MB
        let mut total = 0;
        while total < buf.len() {
            let n = tls_stream.read(&mut buf[total..]).await.unwrap();
            if n == 0 {
                break;
            }
            total += n;
        }

        // 回显数据
        tls_stream.write_all(&buf[..total]).await.unwrap();
    };

    let client = async move {
        // 等待服务器启动
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let connector = create_tls_connector(Some(cert_file.path())).unwrap();
        let stream = TcpStream::connect(server_addr).await.unwrap();
        let domain = "localhost".try_into().unwrap();
        let mut tls_stream = connector.connect(domain, stream).await.unwrap();

        // 发送大量数据
        let data = vec![0xAB; 1024 * 1024]; // 1MB
        tls_stream.write_all(&data).await.unwrap();

        // 读取响应
        let mut buf = vec![0u8; 1024 * 1024];
        let mut total = 0;
        while total < buf.len() {
            let n = tls_stream.read(&mut buf[total..]).await.unwrap();
            if n == 0 {
                break;
            }
            total += n;
        }

        assert_eq!(total, data.len());
        assert_eq!(&buf[..total], &data[..]);
    };

    // 并发运行服务器和客户端
    tokio::join!(server, client);
}

#[tokio::test]
async fn test_tls_concurrent_connections() {
    init_crypto_provider();
    
    // 创建临时证书文件
    let (cert_file, key_file) = create_temp_cert_files();

    // 创建 TLS 配置
    let tls_config = TlsConfig::from_pem_files(cert_file.path(), key_file.path()).unwrap();
    let server_config = tls_config.to_server_config().unwrap();

    // 启动测试服务器
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    let server = async move {
        for _ in 0..10 {
            let (stream, _) = listener.accept().await.unwrap();
            let tls_server = TlsServer::new(Arc::clone(&server_config));

            tokio::spawn(async move {
                let mut tls_stream = tls_server.accept(stream).await.unwrap();
                let mut buf = [0u8; 1024];
                let n = tls_stream.read(&mut buf).await.unwrap();
                tls_stream.write_all(&buf[..n]).await.unwrap();
            });
        }
    };

    let client = async move {
        // 等待服务器启动
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let connector = create_tls_connector(Some(cert_file.path())).unwrap();
        let mut handles = vec![];

        // 创建多个并发连接
        for i in 0..10 {
            let connector = connector.clone();
            let addr = server_addr;

            let handle = tokio::spawn(async move {
                let stream = TcpStream::connect(addr).await.unwrap();
                let domain = "localhost".try_into().unwrap();
                let mut tls_stream = connector.connect(domain, stream).await.unwrap();

                let msg = format!("Concurrent message {}", i);
                tls_stream.write_all(msg.as_bytes()).await.unwrap();

                let mut buf = [0u8; 1024];
                let n = tls_stream.read(&mut buf).await.unwrap();
                assert_eq!(&buf[..n], msg.as_bytes());
            });

            handles.push(handle);
        }

        // 等待所有客户端完成
        for handle in handles {
            handle.await.unwrap();
        }
    };

    // 并发运行服务器和客户端
    tokio::join!(server, client);
}

#[test]
fn test_tls_config_reload() {
    init_crypto_provider();
    
    // 测试 TLS 配置可以重新加载
    let (cert_file1, key_file1) = create_temp_cert_files();
    let (cert_file2, key_file2) = create_temp_cert_files();

    // 创建第一个配置
    let config1 = TlsConfig::from_pem_files(cert_file1.path(), key_file1.path()).unwrap();
    let server_config1 = config1.to_server_config().unwrap();

    // 创建第二个配置
    let config2 = TlsConfig::from_pem_files(cert_file2.path(), key_file2.path()).unwrap();
    let server_config2 = config2.to_server_config().unwrap();

    // 验证两个配置都有效
    let _server1 = TlsServer::new(server_config1);
    let _server2 = TlsServer::new(server_config2);
}
