use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None, arg_required_else_help = true)]
pub struct MystiArg {
    /// target socket
    ///
    /// 目标的uds文件
    #[arg(short, long, default_value = None)]
    pub target: Option<String>,

    /// listen socket
    ///
    /// 代理的本地端口
    #[arg(short, long, default_value = None)]
    pub listen: Option<String>,

    /// config file
    ///
    /// config file
    #[arg(short, long, default_value = None)]
    pub config: Option<String>,
}
