use clap::Parser;

/// MystiProxy - 灵活的代理服务器
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
#[command(arg_required_else_help = true)]
pub struct MystiArg {
    /// 目标地址 (支持 tcp://, unix://)
    ///
    /// 例如: tcp://127.0.0.1:8080 或 unix:///var/run/docker.sock
    #[arg(short, long)]
    pub target: Option<String>,

    /// 监听地址 (支持 tcp://, unix://)
    ///
    /// 例如: tcp://0.0.0.0:3128 或 unix:///tmp/proxy.sock
    #[arg(short, long)]
    pub listen: Option<String>,

    /// 配置文件路径 (YAML 格式)
    ///
    /// 如果指定了配置文件，将忽略其他命令行参数
    #[arg(short, long)]
    pub config: Option<String>,

    /// 代理层 (transport, application)
    ///
    /// 默认为 transport
    #[arg(long, default_value = "transport")]
    pub layer: String,

    /// 连接超时时间
    ///
    /// 例如: 10s, 5m, 1h
    #[arg(long)]
    pub timeout: Option<String>,
}
