use clap::Parser;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
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

#[derive(Debug, Deserialize)]
pub struct MystiEngine {
    pub name: String,
    pub listen: String,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri_mapping: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<HashMap<String, String>>,
}

impl Default for MystiEngine {
    fn default() -> Self {
        todo!()
    }
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub service: Vec<MystiEngine>,
}
