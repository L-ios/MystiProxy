use clap::Parser;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct CliArg {
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

    /// protocol
    ///
    /// 代理协议
    #[arg(short, long, default_value = None)]
    pub protocol: Option<String>,

    /// config file
    ///
    /// config file
    #[arg(short, long, default_value = None)]
    pub config: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MystiEngine {
    pub name: String,
    pub listen: String,
    pub target: String,
    pub protocol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri_mapping: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub service: Vec<MystiEngine>,
}
