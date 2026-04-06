//! 配置模块

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// 顶层配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MystiConfig {
    /// Mysti 引擎配置
    pub mysti: Mysti,
    /// 证书配置
    #[serde(default)]
    pub cert: Vec<CertConfig>,
}

/// Mysti 引擎容器
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mysti {
    /// 引擎配置映射，key 为引擎名称
    pub engine: HashMap<String, EngineConfig>,
}

/// 引擎配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    /// 监听地址 (支持 tcp://, unix://)
    pub listen: String,
    /// 目标地址 (支持 tcp://, unix://)
    pub target: String,
    /// 代理类型
    pub proxy_type: ProxyType,
    /// 请求超时时间（完整代理操作）
    #[serde(
        default,
        deserialize_with = "deserialize_option_duration",
        alias = "timeout"
    )]
    pub request_timeout: Option<Duration>,
    /// 连接超时时间
    #[serde(default, deserialize_with = "deserialize_option_duration")]
    pub connection_timeout: Option<Duration>,
    /// 请求头配置
    #[serde(default)]
    pub header: Option<HashMap<String, HeaderAction>>,
    /// 位置配置
    #[serde(default)]
    pub locations: Option<Vec<LocationConfig>>,
    /// TLS 配置
    #[serde(default)]
    pub tls: Option<TlsConfig>,
    /// HTTP 鉴权配置
    #[serde(default)]
    pub auth: Option<AuthConfig>,
}

/// TLS 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// 证书文件路径
    pub cert_path: String,
    /// 私钥文件路径
    pub key_path: String,
    /// 客户端 CA 证书路径（用于双向认证）
    #[serde(default)]
    pub client_ca_path: Option<String>,
    /// 是否启用双向认证
    #[serde(default)]
    pub mutual_auth: bool,
}

/// HTTP 鉴权配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// 鉴权类型
    pub auth_type: String,
    /// 鉴权头部名称
    #[serde(default = "default_auth_header")]
    pub header_name: String,
    /// 期望的鉴权值
    pub expected_value: Option<String>,
    /// JWT 密钥
    pub jwt_secret: Option<String>,
    /// 是否启用
    #[serde(default = "default_auth_enabled")]
    pub enabled: bool,
}

fn default_auth_header() -> String {
    "Authorization".to_string()
}

fn default_auth_enabled() -> bool {
    true
}

/// 证书配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertConfig {
    /// 证书名称
    pub name: String,
    /// 根密钥
    #[serde(default)]
    pub root_key: String,
}

/// 代理类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProxyType {
    /// TCP 代理
    Tcp,
    /// HTTP 代理
    Http,
}

/// 位置配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationConfig {
    /// 位置路径
    pub location: String,
    /// 匹配模式
    pub mode: MatchMode,
    /// 提供者类型
    #[serde(default)]
    pub provider: Option<ProviderType>,
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default)]
    pub response: Option<ResponseConfig>,
    #[serde(default)]
    pub request: Option<RequestConfig>,
}

/// 匹配模式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum MatchMode {
    /// 完全匹配
    Full,
    /// 前缀匹配
    Prefix,
    /// 正则匹配
    Regex,
    /// 前缀正则匹配
    #[serde(rename = "PrefixRegex")]
    PrefixRegex,
}

/// 提供者类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    /// 静态提供者
    Static,
    /// Mock 提供者
    Mock,
    /// 代理提供者
    Proxy,
}

/// 头部动作配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderAction {
    /// 头部值
    pub value: String,
    /// 动作类型
    pub action: HeaderActionType,
    /// 条件
    #[serde(default)]
    pub condition: Option<String>,
}

/// 头部动作类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HeaderActionType {
    /// 覆盖
    #[serde(rename = "overwrite")]
    Overwrite,
    /// 缺失时添加
    #[serde(rename = "missed")]
    Missed,
    /// 强制删除
    #[serde(rename = "forceDelete")]
    ForceDelete,
}

/// 响应配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseConfig {
    /// 状态码
    #[serde(default)]
    pub status: Option<u16>,
    /// 响应头
    #[serde(default)]
    pub headers: Option<HashMap<String, HeaderAction>>,
    /// 响应体
    #[serde(default)]
    pub body: Option<BodyConfig>,
}

/// 请求配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestConfig {
    /// 请求方法
    #[serde(default)]
    pub method: Option<String>,
    /// URI 配置
    #[serde(default)]
    pub uri: Option<UriConfig>,
    /// 请求头
    #[serde(default)]
    pub headers: Option<HashMap<String, HeaderAction>>,
    /// 请求体
    #[serde(default)]
    pub body: Option<BodyConfig>,
}

/// URI 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UriConfig {
    /// 路径
    #[serde(default)]
    pub path: Option<String>,
    /// 查询参数
    #[serde(default)]
    pub query: Option<String>,
}

/// 请求体配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodyConfig {
    /// JSON 配置
    #[serde(default)]
    pub json: Option<JsonBodyConfig>,
    /// 类型
    #[serde(default, rename = "type")]
    pub body_type: Option<BodyType>,
}

/// JSON 请求体配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonBodyConfig {
    /// JSONPath 路径
    pub path: String,
    /// 值
    pub value: String,
    /// 动作
    pub action: JsonBodyAction,
}

/// JSON 请求体动作
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JsonBodyAction {
    /// 覆盖
    #[serde(rename = "overwrite")]
    Overwrite,
    /// 添加
    #[serde(rename = "add")]
    Add,
    /// 删除
    #[serde(rename = "delete")]
    Delete,
}

/// 请求体类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BodyType {
    /// 静态类型
    Static,
    /// JSON 类型
    Json,
}

/// 自定义 Duration 反序列化函数（支持 Option<Duration>）
fn deserialize_option_duration<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    match opt {
        Some(s) => {
            let duration = parse_duration(&s).map_err(serde::de::Error::custom)?;
            Ok(Some(duration))
        }
        None => Ok(None),
    }
}

/// 解析持续时间字符串（如 "10s", "5m", "1h"）
fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("duration string is empty".to_string());
    }

    // 提取数字部分和单位部分
    let mut num_str = String::new();
    let mut unit_str = String::new();

    for c in s.chars() {
        if c.is_ascii_digit() || c == '.' {
            num_str.push(c);
        } else {
            unit_str.push(c);
        }
    }

    let num: f64 = num_str
        .parse()
        .map_err(|e| format!("invalid duration number: {e}"))?;

    let duration = match unit_str.as_str() {
        "ms" => Duration::from_secs_f64(num / 1000.0),
        "s" => Duration::from_secs_f64(num),
        "m" => Duration::from_secs_f64(num * 60.0),
        "h" => Duration::from_secs_f64(num * 3600.0),
        _ => return Err(format!("unknown duration unit: {unit_str}")),
    };

    Ok(duration)
}

impl MystiConfig {
    /// 从 YAML 文件加载配置
    pub fn from_yaml_file(path: &str) -> crate::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_yaml(&content)
    }

    /// 从 YAML 字符串解析配置
    pub fn from_yaml(yaml: &str) -> crate::Result<Self> {
        let config: MystiConfig = serde_yaml::from_str(yaml)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("10s").unwrap(), Duration::from_secs(10));
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
        assert_eq!(
            parse_duration("1.5s").unwrap(),
            Duration::from_secs_f64(1.5)
        );
    }

    #[test]
    fn test_mysti_config_from_yaml() {
        let yaml = r#"
mysti:
  engine:
    docker:
      listen: tcp://0.0.0.0:3128
      target: unix:///var/run/docker.sock
      proxy_type: http
      timeout: 10s
      header:
        Host:
          value: localhost
          action: overwrite
      locations:
        - location: '/a/b'
          mode: Prefix
          response:
            status: 200
            headers:
              test:
                value: good
                action: overwrite
            body:
              type: static
        - location: '/a/c'
          mode: Full
          request:
            method: get
            uri:
              path: '/a/d'
            body:
              type: json
              json:
                path: '$.name'
                value: 'test'
                action: overwrite
cert:
  - name: client1
    root_key: ""
"#;
        let config: MystiConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.mysti.engine.contains_key("docker"));
        let docker_config = &config.mysti.engine["docker"];
        assert_eq!(docker_config.listen, "tcp://0.0.0.0:3128");
        assert_eq!(docker_config.target, "unix:///var/run/docker.sock");
        assert_eq!(docker_config.proxy_type, ProxyType::Http);
        assert_eq!(docker_config.request_timeout, Some(Duration::from_secs(10)));
        assert!(docker_config.header.is_some());
        assert!(docker_config.locations.is_some());
        let locations = docker_config.locations.as_ref().unwrap();
        assert_eq!(locations.len(), 2);
        assert_eq!(locations[0].location, "/a/b");
        assert_eq!(locations[0].mode, MatchMode::Prefix);
        assert!(locations[0].response.is_some());
        let response = locations[0].response.as_ref().unwrap();
        assert_eq!(response.status, Some(200));
        assert!(response.headers.is_some());
        assert!(response.body.is_some());
        let body = response.body.as_ref().unwrap();
        assert_eq!(body.body_type, Some(BodyType::Static));
        assert_eq!(locations[1].location, "/a/c");
        assert_eq!(locations[1].mode, MatchMode::Full);
        assert!(locations[1].request.is_some());
        let request = locations[1].request.as_ref().unwrap();
        assert_eq!(request.method, Some("get".to_string()));
        assert!(request.uri.is_some());
        let uri = request.uri.as_ref().unwrap();
        assert_eq!(uri.path, Some("/a/d".to_string()));
        assert!(request.body.is_some());
        let body = request.body.as_ref().unwrap();
        assert_eq!(body.body_type, Some(BodyType::Json));
        assert!(body.json.is_some());
        let json = body.json.as_ref().unwrap();
        assert_eq!(json.path, "$.name");
        assert_eq!(json.value, "test");
        assert_eq!(json.action, JsonBodyAction::Overwrite);
        assert_eq!(config.cert.len(), 1);
        assert_eq!(config.cert[0].name, "client1");
        assert_eq!(config.cert[0].root_key, "");
    }

    #[test]
    fn test_proxy_type_serialization() {
        assert_eq!(
            serde_yaml::to_string(&ProxyType::Tcp).unwrap().trim(),
            "tcp"
        );
        assert_eq!(
            serde_yaml::to_string(&ProxyType::Http).unwrap().trim(),
            "http"
        );
    }

    #[test]
    fn test_match_mode_serialization() {
        assert_eq!(
            serde_yaml::to_string(&MatchMode::Full).unwrap().trim(),
            "Full"
        );
        assert_eq!(
            serde_yaml::to_string(&MatchMode::Prefix).unwrap().trim(),
            "Prefix"
        );
        assert_eq!(
            serde_yaml::to_string(&MatchMode::Regex).unwrap().trim(),
            "Regex"
        );
        assert_eq!(
            serde_yaml::to_string(&MatchMode::PrefixRegex)
                .unwrap()
                .trim(),
            "PrefixRegex"
        );
    }

    #[test]
    fn test_header_action_type_serialization() {
        assert_eq!(
            serde_yaml::to_string(&HeaderActionType::Overwrite)
                .unwrap()
                .trim(),
            "overwrite"
        );
        assert_eq!(
            serde_yaml::to_string(&HeaderActionType::Missed)
                .unwrap()
                .trim(),
            "missed"
        );
        assert_eq!(
            serde_yaml::to_string(&HeaderActionType::ForceDelete)
                .unwrap()
                .trim(),
            "forceDelete"
        );
    }

    #[test]
    fn test_full_yaml_config() {
        let yaml = r#"
mysti:
  engine:
    docker:
      listen: tcp://0.0.0.0:3128
      target: unix:///var/run/docker.sock
      proxy_type: http
      timeout: 10s
      header:
        Host:
          value: localhost
          action: overwrite
          condition: ''
      locations:
        - location: '/a/b'
          mode: Prefix
          response:
            status: 200
            headers:
              test:
                value: good
                action: overwrite
            body:
              type: static
        - location: '/a/c'
          mode: Full
          request:
            method: 'get'
            uri:
              path: '/a/d'
              query: 'a=b&c=d'
            headers:
              Host:
                value: localhost
                action: overwrite
            body:
              json:
                path: '$.name'
                value: 'test'
                action: overwrite
    containerd:
      listen: tcp://0.0.0.0:3129
      target: tcp://127.0.0.1:2765
      proxy_type: tcp

cert:
  - name: client1
    root_key: ""
"#;
        let config: MystiConfig = serde_yaml::from_str(yaml).unwrap();

        // 验证 docker 引擎配置
        assert!(config.mysti.engine.contains_key("docker"));
        let docker_config = &config.mysti.engine["docker"];
        assert_eq!(docker_config.listen, "tcp://0.0.0.0:3128");
        assert_eq!(docker_config.target, "unix:///var/run/docker.sock");
        assert_eq!(docker_config.proxy_type, ProxyType::Http);
        assert_eq!(docker_config.request_timeout, Some(Duration::from_secs(10)));

        // 验证 containerd 引擎配置
        assert!(config.mysti.engine.contains_key("containerd"));
        let containerd_config = &config.mysti.engine["containerd"];
        assert_eq!(containerd_config.listen, "tcp://0.0.0.0:3129");
        assert_eq!(containerd_config.target, "tcp://127.0.0.1:2765");
        assert_eq!(containerd_config.proxy_type, ProxyType::Tcp);

        // 验证证书配置
        assert_eq!(config.cert.len(), 1);
        assert_eq!(config.cert[0].name, "client1");
    }
}
