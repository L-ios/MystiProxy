use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use mystiproxy::config::{EngineConfig, ProxyType};
use mystiproxy::io::{SocketStream, StreamListener};
use mystiproxy::proxy::ProxyServer;

async fn start_echo_server() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind echo server");

    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        loop {
            if let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    loop {
                        match stream.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                if stream.write_all(&buf[..n]).await.is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
        }
    });

    port
}

async fn get_available_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind for port discovery");
    listener.local_addr().unwrap().port()
}

async fn wait_for_readiness() {
    tokio::time::sleep(Duration::from_millis(50)).await;
}

#[tokio::test]
async fn test_tcp_proxy_forwarding() {
    let echo_port = start_echo_server().await;
    let proxy_port = get_available_port().await;

    let config = EngineConfig {
        listen: format!("tcp://127.0.0.1:{}", proxy_port),
        target: format!("tcp://127.0.0.1:{}", echo_port),
        proxy_type: ProxyType::Tcp,
        request_timeout: None,
        connection_timeout: None,
        header: None,
        locations: None,
    };

    let mut server = ProxyServer::from_engine_config(&config).expect("failed to create ProxyServer");
    server.start().await.expect("failed to start ProxyServer");

    tokio::spawn(async move {
        let _ = server.run().await;
    });

    wait_for_readiness().await;

    let mut client =
        tokio::net::TcpStream::connect(format!("127.0.0.1:{}", proxy_port))
            .await
            .expect("failed to connect to proxy");

    let payload = b"hello proxy";
    client.write_all(payload).await.expect("write failed");

    let mut buf = vec![0u8; payload.len()];
    client
        .read_exact(&mut buf)
        .await
        .expect("read failed");

    assert_eq!(&buf[..], payload);
}

#[tokio::test]
async fn test_tcp_proxy_with_timeout() {
    let echo_port = start_echo_server().await;
    let proxy_port = get_available_port().await;

    let config = EngineConfig {
        listen: format!("tcp://127.0.0.1:{}", proxy_port),
        target: format!("tcp://127.0.0.1:{}", echo_port),
        proxy_type: ProxyType::Tcp,
        request_timeout: Some(Duration::from_secs(5)),
        connection_timeout: None,
        header: None,
        locations: None,
    };

    let mut server = ProxyServer::from_engine_config(&config).expect("failed to create ProxyServer");
    server.start().await.expect("failed to start ProxyServer");

    tokio::spawn(async move {
        let _ = server.run().await;
    });

    wait_for_readiness().await;

    let mut client =
        tokio::net::TcpStream::connect(format!("127.0.0.1:{}", proxy_port))
            .await
            .expect("failed to connect to proxy");

    let payload = b"timed hello";
    client.write_all(payload).await.expect("write failed");

    let mut buf = vec![0u8; payload.len()];
    client.read_exact(&mut buf).await.expect("read failed");

    assert_eq!(&buf[..], payload);
}

#[tokio::test]
async fn test_stream_listener_tcp() {
    let port = get_available_port().await;
    let listen_addr = format!("tcp://127.0.0.1:{}", port);

    let listener = StreamListener::new(listen_addr)
        .await
        .expect("failed to create TCP StreamListener");

    let accept_task = tokio::spawn(async move {
        let (stream, _addr) = listener
            .accept()
            .await
            .expect("failed to accept connection");
        stream
    });

    wait_for_readiness().await;

    let _connected =
        tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .expect("failed to connect");

    let stream = tokio::time::timeout(Duration::from_secs(2), accept_task)
        .await
        .expect("accept timed out")
        .expect("accept task panicked");

    match stream {
        SocketStream::Tcp(_) => {}
        SocketStream::Uds(_) => panic!("expected TCP stream, got UDS"),
    }
}

#[tokio::test]
async fn test_stream_listener_invalid_address() {
    let result = StreamListener::new("invalid://nope".to_string()).await;
    assert!(result.is_err(), "expected error for invalid address scheme");
}

#[tokio::test]
async fn test_socket_stream_tcp_connect() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind failed");
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = vec![0u8; 1024];
            loop {
                match stream.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if stream.write_all(&buf[..n]).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    });

    wait_for_readiness().await;

    let mut stream =
        SocketStream::connect(format!("tcp://127.0.0.1:{}", port))
            .await
            .expect("SocketStream connect failed");

    let payload = b"socket stream test";
    stream.write_all(payload).await.expect("write failed");

    let mut buf = vec![0u8; payload.len()];
    stream.read_exact(&mut buf).await.expect("read failed");

    assert_eq!(&buf[..], payload);
}

#[tokio::test]
async fn test_socket_stream_invalid_protocol() {
    let result = SocketStream::connect("ftp://something".to_string()).await;
    assert!(result.is_err(), "expected error for unsupported protocol");
}

#[tokio::test]
async fn test_socket_stream_no_protocol() {
    let result = SocketStream::connect("no-protocol".to_string()).await;
    assert!(result.is_err(), "expected error for missing protocol separator");
}

#[tokio::test]
async fn test_stream_listener_unix() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let socket_path = temp_dir.path().join("test.sock");
    let listen_addr = format!("unix://{}", socket_path.display());

    let listener = StreamListener::new(listen_addr)
        .await
        .expect("failed to create UDS StreamListener");

    tokio::spawn(async move {
        loop {
            if let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 1024];
                    loop {
                        match stream.read(&mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                if stream.write_all(&buf[..n]).await.is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
        }
    });

    wait_for_readiness().await;

    let mut stream =
        SocketStream::connect(format!("unix://{}", socket_path.display()))
            .await
            .expect("failed to connect UDS SocketStream");

    let payload = b"uds hello";
    stream.write_all(payload).await.expect("write failed");

    let mut buf = vec![0u8; payload.len()];
    stream.read_exact(&mut buf).await.expect("read failed");

    assert_eq!(&buf[..], payload);
}

#[tokio::test]
async fn test_proxy_server_lifecycle() {
    let echo_port = start_echo_server().await;
    let proxy_port = get_available_port().await;

    let config = EngineConfig {
        listen: format!("tcp://127.0.0.1:{}", proxy_port),
        target: format!("tcp://127.0.0.1:{}", echo_port),
        proxy_type: ProxyType::Tcp,
        request_timeout: None,
        connection_timeout: None,
        header: None,
        locations: None,
    };

    let mut server =
        ProxyServer::from_engine_config(&config).expect("failed to create ProxyServer");

    server.start().await.expect("start failed");

    assert!(server.listen_addr().is_tcp(), "listen address should be TCP");
    assert!(server.target_addr().is_tcp(), "target address should be TCP");

    tokio::spawn(async move {
        let _ = server.run().await;
    });

    wait_for_readiness().await;

    let mut client =
        tokio::net::TcpStream::connect(format!("127.0.0.1:{}", proxy_port))
            .await
            .expect("failed to connect to proxy");

    let payload = b"lifecycle test";
    client.write_all(payload).await.expect("write failed");

    let mut buf = vec![0u8; payload.len()];
    client.read_exact(&mut buf).await.expect("read failed");

    assert_eq!(&buf[..], payload);
}

#[tokio::test]
async fn test_proxy_config_from_engine_config() {
    let config = EngineConfig {
        listen: "tcp://127.0.0.1:19000".to_string(),
        target: "tcp://127.0.0.1:19001".to_string(),
        proxy_type: ProxyType::Tcp,
        request_timeout: Some(Duration::from_secs(10)),
        connection_timeout: None,
        header: None,
        locations: None,
    };

    let server = ProxyServer::from_engine_config(&config).expect("creation failed");

    assert!(server.listen_addr().is_tcp());
    assert!(server.target_addr().is_tcp());
}
