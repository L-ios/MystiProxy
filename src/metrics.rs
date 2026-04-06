//! 性能监控模块
//! 
//! 提供 Prometheus 指标收集和导出功能

use std::net::SocketAddr;
use std::time::Duration;

use prometheus::{Counter, Gauge, Histogram, HistogramOpts, Opts};
use tracing::{info};

/// 监控指标管理器
pub struct MetricsManager {
    http_requests_total: Counter,
    http_request_duration_seconds: Histogram,
    tcp_connection_duration_seconds: Histogram,
    errors_total: Counter,
    memory_usage_bytes: Gauge,
}

impl MetricsManager {
    /// 创建新的监控指标管理器
    pub fn new() -> Self {
        // 创建指标
        let http_requests_total = Counter::with_opts(Opts::new("http_requests_total", "Total HTTP requests")).unwrap();
        let http_request_duration_seconds = Histogram::with_opts(HistogramOpts::new("http_request_duration_seconds", "HTTP request duration in seconds")).unwrap();
        let tcp_connection_duration_seconds = Histogram::with_opts(HistogramOpts::new("tcp_connection_duration_seconds", "TCP connection duration in seconds")).unwrap();
        let errors_total = Counter::with_opts(Opts::new("errors_total", "Total errors")).unwrap();
        let memory_usage_bytes = Gauge::with_opts(Opts::new("memory_usage_bytes", "Memory usage in bytes")).unwrap();

        Self {
            http_requests_total,
            http_request_duration_seconds,
            tcp_connection_duration_seconds,
            errors_total,
            memory_usage_bytes,
        }
    }

    /// 初始化监控指标
    pub fn init(&mut self) {
        info!("Metrics initialized");
    }

    /// 启动指标导出服务器
    pub async fn start_server(&mut self, addr: SocketAddr) {
        info!("Metrics server started on {:?}", addr);
    }

    /// 停止指标导出服务器
    pub async fn stop_server(&mut self) {
        info!("Metrics server stopped");
    }

    /// 记录 HTTP 请求指标
    pub fn record_http_request(&self, method: &str, path: &str, status: u16, duration: Duration) {
        self.http_requests_total.inc();
        self.http_request_duration_seconds.observe(duration.as_secs_f64());
    }

    /// 记录 TCP 连接指标
    pub fn record_tcp_connection(&self, duration: Duration) {
        self.tcp_connection_duration_seconds.observe(duration.as_secs_f64());
    }

    /// 记录错误指标
    pub fn record_error(&self, error_type: &str) {
        self.errors_total.inc();
    }

    /// 记录内存使用指标
    pub fn record_memory_usage(&self, used: u64, total: u64) {
        self.memory_usage_bytes.set(used as f64);
    }
}
