//! 性能监控模块
//!
//! 提供 Prometheus 指标收集和导出功能。
//!
//! 指标注册到进程级 Registry（prometheus 全局 default registry 的独立实例封装），
//! 通过 `/metrics` 端点以 exposition 格式导出。

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use prometheus::{
    core::Collector, Encoder, Gauge, Histogram, HistogramOpts, IntCounter, IntCounterVec, Opts,
    Registry, TextEncoder,
};
use tracing::info;

/// 监控指标管理器
pub struct MetricsManager {
    registry: Registry,
    http_requests_total: IntCounterVec,
    http_request_duration_seconds: Histogram,
    tcp_connection_duration_seconds: Histogram,
    errors_total: IntCounter,
    memory_usage_bytes: Gauge,
}

fn register<C: Collector + Clone + 'static>(registry: &Registry, c: C) -> C {
    // Idempotent: ignore AlreadyReg so repeated construction is safe.
    let _ = registry.register(Box::new(c.clone()));
    c
}

impl MetricsManager {
    /// 创建新的监控指标管理器（指标即刻注册到私有 Registry）
    pub fn new() -> Self {
        let registry = Registry::new();

        let http_requests_total = register(
            &registry,
            IntCounterVec::new(
                Opts::new(
                    "http_requests_total",
                    "Total HTTP requests by method and status",
                ),
                &["method", "status"],
            )
            .unwrap(),
        );

        let http_request_duration_seconds = register(
            &registry,
            Histogram::with_opts(HistogramOpts::new(
                "http_request_duration_seconds",
                "HTTP request duration in seconds",
            ))
            .unwrap(),
        );

        let tcp_connection_duration_seconds = register(
            &registry,
            Histogram::with_opts(HistogramOpts::new(
                "tcp_connection_duration_seconds",
                "TCP connection duration in seconds",
            ))
            .unwrap(),
        );

        let errors_total = register(
            &registry,
            IntCounter::new("errors_total", "Total errors").unwrap(),
        );

        let memory_usage_bytes = register(
            &registry,
            Gauge::new("memory_usage_bytes", "Memory usage in bytes").unwrap(),
        );

        // CounterVec exposes no children until a label combination is used;
        // pre-touch a neutral combination so the metric always shows up in gather().
        http_requests_total.with_label_values(&["none", "0"]);

        Self {
            registry,
            http_requests_total,
            http_request_duration_seconds,
            tcp_connection_duration_seconds,
            errors_total,
            memory_usage_bytes,
        }
    }

    /// 初始化监控指标（保留 API；注册在 new 中已完成）
    pub fn init(&mut self) {
        info!("Metrics initialized");
    }

    /// Gather all registered metrics as Prometheus exposition text.
    pub fn gather(&self) -> String {
        let mut buf = Vec::new();
        let encoder = TextEncoder::new();
        if encoder.encode(&self.registry.gather(), &mut buf).is_err() {
            return String::new();
        }
        String::from_utf8_lossy(&buf).into_owned()
    }

    /// 启动指标导出服务器（GET /metrics）
    pub async fn start_server(self: Arc<Self>, addr: SocketAddr) {
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("Metrics server bind {addr} failed: {e}");
                return;
            }
        };
        info!("Metrics server started on {addr}");

        loop {
            let (stream, _) = match listener.accept().await {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("Metrics accept error: {e}");
                    continue;
                }
            };
            let metrics = self.clone();
            tokio::spawn(async move {
                let io = hyper_util::rt::TokioIo::new(stream);
                let service = service_fn(move |req: Request<Incoming>| {
                    let metrics = metrics.clone();
                    async move {
                        let resp: Response<Full<Bytes>> = if req.method()
                            == hyper::http::Method::GET
                            && req.uri().path() == "/metrics"
                        {
                            Response::builder()
                                .status(StatusCode::OK)
                                .header(
                                    hyper::http::header::CONTENT_TYPE,
                                    "text/plain; version=0.0.4",
                                )
                                .body(Full::new(Bytes::from(metrics.gather())))
                                .unwrap()
                        } else {
                            Response::builder()
                                .status(StatusCode::NOT_FOUND)
                                .body(Full::new(Bytes::new()))
                                .unwrap()
                        };
                        Ok::<_, std::convert::Infallible>(resp)
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, service)
                    .await;
            });
        }
    }

    /// 停止指标导出服务器（进程退出即止；保留 API 兼容）
    pub async fn stop_server(&mut self) {
        info!("Metrics server stopped");
    }

    /// 记录 HTTP 请求指标（method/status 维度；path 不做标签避免高基数）
    pub fn record_http_request(&self, method: &str, _path: &str, status: u16, duration: Duration) {
        self.http_requests_total
            .with_label_values(&[method, &status.to_string()])
            .inc();
        self.http_request_duration_seconds
            .observe(duration.as_secs_f64());
    }

    /// 记录 TCP 连接指标
    pub fn record_tcp_connection(&self, duration: Duration) {
        self.tcp_connection_duration_seconds
            .observe(duration.as_secs_f64());
    }

    /// 记录错误指标
    pub fn record_error(&self, _error_type: &str) {
        self.errors_total.inc();
    }

    /// 记录内存使用指标
    pub fn record_memory_usage(&self, used: u64, _total: u64) {
        self.memory_usage_bytes.set(used as f64);
    }
}

/// 进程级共享实例：main.rs 的导出服务与各 engine handler 记录到同一 Registry
pub fn global_metrics() -> Arc<MetricsManager> {
    use std::sync::OnceLock;
    static GLOBAL: OnceLock<Arc<MetricsManager>> = OnceLock::new();
    GLOBAL
        .get_or_init(|| Arc::new(MetricsManager::new()))
        .clone()
}

impl Default for MetricsManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gather_contains_all_metrics_with_help_and_type() {
        let m = MetricsManager::new();
        let out = m.gather();
        for name in [
            "http_requests_total",
            "http_request_duration_seconds",
            "tcp_connection_duration_seconds",
            "errors_total",
            "memory_usage_bytes",
        ] {
            assert!(out.contains(name), "missing metric {name} in:\n{out}");
        }
        assert!(out.contains("# HELP http_requests_total"));
        assert!(out.contains("# TYPE http_requests_total counter"));
        assert!(out.contains("# TYPE memory_usage_bytes gauge"));
    }

    #[test]
    fn test_labeled_counter_records_by_method_status() {
        let m = MetricsManager::new();
        m.record_http_request("GET", "/a", 200, Duration::from_millis(5));
        m.record_http_request("POST", "/b", 404, Duration::from_millis(7));
        let out = m.gather();
        assert!(
            out.contains(r#"http_requests_total{method="GET",status="200"} 1"#),
            "{out}"
        );
        assert!(
            out.contains(r#"http_requests_total{method="POST",status="404"} 1"#),
            "{out}"
        );
    }

    #[test]
    fn test_counter_accumulates() {
        let m = MetricsManager::new();
        for _ in 0..3 {
            m.record_http_request("GET", "/x", 200, Duration::from_millis(1));
        }
        let out = m.gather();
        assert!(
            out.contains(r#"http_requests_total{method="GET",status="200"} 3"#),
            "{out}"
        );
    }

    #[test]
    fn test_repeated_construction_is_idempotent() {
        let _a = MetricsManager::new();
        let _b = MetricsManager::new(); // must not panic
        let c = MetricsManager::new();
        assert!(!c.gather().is_empty());
    }

    #[test]
    fn test_histogram_buckets_present() {
        let m = MetricsManager::new();
        m.record_http_request("GET", "/", 200, Duration::from_secs_f64(0.42));
        let out = m.gather();
        assert!(
            out.contains("http_request_duration_seconds_bucket{le="),
            "{out}"
        );
        m.record_tcp_connection(Duration::from_secs_f64(1.5));
        assert!(m
            .gather()
            .contains("tcp_connection_duration_seconds_bucket{le="));
    }

    #[test]
    fn test_memory_gauge_and_errors_counter() {
        let m = MetricsManager::new();
        m.record_memory_usage(1024, 2048);
        m.record_error("timeout");
        m.record_error("conn");
        let out = m.gather();
        assert!(out.contains("memory_usage_bytes 1024"), "{out}");
        assert!(out.contains("errors_total 2"), "{out}");
    }

    #[tokio::test]
    async fn test_metrics_server_serves_exposition() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let m = Arc::new(MetricsManager::new());
        let addr: SocketAddr = "127.0.0.1:19099".parse().unwrap();
        let server = tokio::spawn(m.clone().start_server(addr));

        tokio::time::sleep(Duration::from_millis(200)).await;

        m.record_http_request("GET", "/t", 200, Duration::from_millis(3));

        async fn http_get(path: &str) -> String {
            let mut stream = tokio::net::TcpStream::connect("127.0.0.1:19099")
                .await
                .expect("connect metrics server");
            stream
                .write_all(
                    format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                        .as_bytes(),
                )
                .await
                .unwrap();
            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).await.unwrap();
            String::from_utf8_lossy(&buf).into_owned()
        }

        let resp = http_get("/metrics").await;
        assert!(resp.starts_with("HTTP/1.1 200"), "{resp}");
        assert!(resp.contains("text/plain"), "{resp}");
        assert!(resp.contains("http_requests_total"), "{resp}");
        assert!(resp.contains("method=\"GET\""), "{resp}");

        let resp = http_get("/nope").await;
        assert!(resp.starts_with("HTTP/1.1 404"), "{resp}");

        server.abort();
    }
}
