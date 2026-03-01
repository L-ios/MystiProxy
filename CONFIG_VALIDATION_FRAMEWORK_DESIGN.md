# MystiProxy 配置验证框架设计方案

## 1. 概述

本方案旨在为 MystiProxy 设计并实现一个完整的配置验证框架，提升配置的安全性、可靠性和用户体验，同时支持热重载功能。

## 2. 现状分析

### 2.1 当前配置结构
```rust
// src/arg.rs
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

#[derive(Debug, Deserialize)]
pub struct Config {
    pub service: Vec<MystiEngine>,
}
```

### 2.2 存在的问题
1. 缺乏配置验证机制
2. 错误处理不够友好
3. 不支持配置热重载
4. 安全性检查不足
5. 缺乏配置版本管理

## 3. 整体架构设计

### 3.1 核心组件架构

```
┌─────────────────┐    ┌──────────────────┐    ┌──────────────────┐
│   ConfigLoader  │───▶│  ConfigValidator │───▶│  ConfigManager   │
└─────────────────┘    └──────────────────┘    └──────────────────┘
         │                       │                       │
         ▼                       ▼                       ▼
┌─────────────────┐    ┌──────────────────┐    ┌──────────────────┐
│  FileWatcher    │    │ ValidationRules  │    │  HotReloader     │
└─────────────────┘    └──────────────────┘    └──────────────────┘
```

### 3.2 组件职责说明

#### ConfigLoader（配置加载器）
- 负责从不同来源加载配置（文件、环境变量、命令行参数）
- 支持多种格式（YAML、JSON、TOML）
- 提供配置缓存和版本控制

#### ConfigValidator（配置验证器）
- 实现多层次验证机制
- 内置安全规则检查
- 自定义验证规则扩展
- 依赖关系验证

#### ConfigManager（配置管理器）
- 统一配置访问接口
- 配置变更通知机制
- 运行时配置更新
- 配置回滚支持

#### FileWatcher（文件监控器）
- 基于 `notify` crate 实现
- 监控配置文件变化
- 触发热重载流程

## 4. 技术选型

### 4.1 核心依赖库
```toml
# 配置验证相关
validator = "0.16"
serde = { version = "1.0", features = ["derive"] }
serde_yaml = "0.9"
config = "0.13"

# 文件监控
notify = { version = "6.1", features = ["serde"] }

# 错误处理
thiserror = "1.0"
anyhow = "1.0"

# 日志系统
log = "0.4"
env_logger = "0.10"

# 异步运行时
tokio = { version = "1.0", features = ["full"] }
```

### 4.2 架构模式选择
- **观察者模式**：用于配置变更通知
- **策略模式**：用于不同的验证策略
- **工厂模式**：用于验证规则创建
- **装饰器模式**：用于验证规则组合

## 5. 详细实现方案

### 5.1 配置模型增强

```rust
// src/config/mod.rs
pub mod validation;
pub mod loader;
pub mod manager;
pub mod watcher;

// src/config/model.rs
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProxyConfig {
    #[validate(length(min = 1, max = 50))]
    pub name: String,
    
    #[validate(custom = "validate_listen_address")]
    pub listen: ListenAddress,
    
    #[validate(custom = "validate_target_address")]
    pub target: TargetAddress,
    
    #[serde(default)]
    #[validate(custom = "validate_uri_mapping")]
    pub uri_mapping: Option<UriMapping>,
    
    #[serde(default = "default_timeout")]
    #[validate(range(min = 1, max = 300))]
    pub timeout_seconds: u32,
    
    #[serde(default)]
    pub headers: HashMap<String, String>,
    
    #[serde(default)]
    pub security: SecurityConfig,
    
    #[serde(default)]
    pub monitoring: MonitoringConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListenAddress {
    pub protocol: Protocol,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_tls_enabled")]
    pub tls_enabled: bool,
    
    #[serde(default)]
    pub allowed_ips: Vec<String>,
    
    #[serde(default)]
    pub rate_limit: Option<RateLimitConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub requests_per_second: u32,
    pub burst_size: u32,
}
```

### 5.2 验证规则实现

```rust
// src/config/validation/rules.rs
use validator::{ValidationError, ValidationErrors};
use std::net::{IpAddr, SocketAddr};
use regex::Regex;

pub fn validate_listen_address(listen: &ListenAddress) -> Result<(), ValidationError> {
    match listen.protocol {
        Protocol::Tcp | Protocol::Http => {
            if listen.port == 0 || listen.port > 65535 {
                return Err(ValidationError::new("invalid_port"));
            }
            
            if !is_valid_host(&listen.host) {
                return Err(ValidationError::new("invalid_host"));
            }
        }
        Protocol::Unix => {
            if listen.host.is_empty() {
                return Err(ValidationError::new("empty_socket_path"));
            }
        }
    }
    Ok(())
}

pub fn validate_target_address(target: &TargetAddress) -> Result<(), ValidationError> {
    // 验证目标地址格式
    if target.url.is_empty() {
        return Err(ValidationError::new("empty_target_url"));
    }
    
    // 安全检查：防止 SSRF 攻击
    if is_internal_ip(&target.url) && !target.allow_internal {
        return Err(ValidationError::new("internal_target_not_allowed"));
    }
    
    Ok(())
}

fn is_valid_host(host: &str) -> bool {
    // 检查是否为有效的主机名或IP地址
    host.parse::<IpAddr>().is_ok() || 
    Regex::new(r"^[a-zA-Z0-9.-]+$").unwrap().is_match(host)
}

fn is_internal_ip(url: &str) -> bool {
    // 检查是否指向内部网络
    // 实现具体的内部IP检测逻辑
    false
}
```

### 5.3 配置加载器实现

```rust
// src/config/loader.rs
use std::path::Path;
use std::fs::File;
use std::io::BufReader;
use serde::de::DeserializeOwned;
use config::{Config, File as ConfigFile, Environment};
use crate::config::model::ProxyConfig;

pub struct ConfigLoader {
    config_path: Option<String>,
}

impl ConfigLoader {
    pub fn new(config_path: Option<String>) -> Self {
        Self { config_path }
    }
    
    pub fn load<T>(&self) -> Result<T, ConfigError>
    where
        T: DeserializeOwned,
    {
        let mut config_builder = Config::builder();
        
        // 加载默认配置
        config_builder = config_builder.add_source(ConfigFile::new("config/default", config::FileFormat::Yaml));
        
        // 加载用户配置文件
        if let Some(path) = &self.config_path {
            config_builder = config_builder.add_source(ConfigFile::new(path, config::FileFormat::Yaml));
        }
        
        // 加载环境变量
        config_builder = config_builder.add_source(Environment::with_prefix("MYSTI"));
        
        let config = config_builder.build()?;
        let parsed_config: T = config.try_deserialize()?;
        
        Ok(parsed_config)
    }
    
    pub fn load_with_validation(&self) -> Result<ProxyConfig, ConfigError> {
        let config: ProxyConfig = self.load()?;
        config.validate()?; // 执行验证
        Ok(config)
    }
}
```

### 5.4 配置管理器实现

```rust
// src/config/manager.rs
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use crate::config::model::ProxyConfig;
use crate::config::validation::ConfigValidator;

pub struct ConfigManager {
    current_config: Arc<RwLock<ProxyConfig>>,
    validator: ConfigValidator,
    reload_notifier: broadcast::Sender<ConfigReloadEvent>,
}

pub struct ConfigReloadEvent {
    pub old_config: ProxyConfig,
    pub new_config: ProxyConfig,
    pub timestamp: std::time::SystemTime,
}

impl ConfigManager {
    pub fn new(initial_config: ProxyConfig) -> Result<Self, ConfigError> {
        let (sender, _) = broadcast::channel(100);
        
        Ok(Self {
            current_config: Arc::new(RwLock::new(initial_config)),
            validator: ConfigValidator::new(),
            reload_notifier: sender,
        })
    }
    
    pub fn get_current(&self) -> ProxyConfig {
        self.current_config.read().unwrap().clone()
    }
    
    pub fn subscribe_reload_events(&self) -> broadcast::Receiver<ConfigReloadEvent> {
        self.reload_notifier.subscribe()
    }
    
    pub fn update_config(&self, new_config: ProxyConfig) -> Result<(), ConfigError> {
        self.validator.validate(&new_config)?;
        
        let old_config = self.get_current();
        let mut config_guard = self.current_config.write().unwrap();
        *config_guard = new_config.clone();
        
        // 通知所有订阅者
        let event = ConfigReloadEvent {
            old_config,
            new_config,
            timestamp: std::time::SystemTime::now(),
        };
        
        let _ = self.reload_notifier.send(event);
        Ok(())
    }
}
```

### 5.5 热重载机制实现

```rust
// src/config/watcher.rs
use notify::{RecommendedWatcher, RecursiveMode, Watcher, Config};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::config::loader::ConfigLoader;
use crate::config::manager::ConfigManager;

pub struct ConfigWatcher {
    watcher: RecommendedWatcher,
    config_loader: ConfigLoader,
    config_manager: Arc<ConfigManager>,
}

impl ConfigWatcher {
    pub fn new(
        config_path: String,
        config_loader: ConfigLoader,
        config_manager: Arc<ConfigManager>,
    ) -> Result<Self, ConfigError> {
        let (tx, rx) = mpsc::channel(1);
        
        let mut watcher = RecommendedWatcher::new(
            move |res| {
                futures::executor::block_on(async {
                    tx.send(res).await.unwrap();
                })
            },
            Config::default(),
        )?;
        
        watcher.watch(Path::new(&config_path), RecursiveMode::NonRecursive)?;
        
        Ok(Self {
            watcher,
            config_loader,
            config_manager,
        })
    }
    
    pub async fn watch(&mut self) -> Result<(), ConfigError> {
        loop {
            tokio::select! {
                event = self.rx.recv() => {
                    match event {
                        Some(Ok(event)) => {
                            if self.should_reload(&event) {
                                self.reload_config().await?;
                            }
                        }
                        Some(Err(e)) => {
                            log::error!("Config watcher error: {:?}", e);
                        }
                        None => break,
                    }
                }
            }
        }
        Ok(())
    }
    
    async fn reload_config(&self) -> Result<(), ConfigError> {
        match self.config_loader.load_with_validation() {
            Ok(new_config) => {
                self.config_manager.update_config(new_config)?;
                log::info!("Configuration reloaded successfully");
            }
            Err(e) => {
                log::error!("Failed to reload configuration: {:?}", e);
                // 可以发送告警或记录错误
            }
        }
        Ok(())
    }
}
```

## 6. 安全性增强措施

### 6.1 输入验证强化
```rust
// 安全头验证
pub fn validate_security_headers(headers: &HashMap<String, String>) -> Result<(), ValidationError> {
    let dangerous_headers = [
        "content-length",
        "transfer-encoding",
        "connection",
        "upgrade",
    ];
    
    for header_name in headers.keys() {
        let normalized = header_name.to_lowercase();
        if dangerous_headers.contains(&normalized.as_str()) {
            return Err(ValidationError::new("dangerous_header"));
        }
    }
    Ok(())
}

// 路径遍历防护
pub fn validate_file_path(path: &str) -> Result<(), ValidationError> {
    if path.contains("..") || path.starts_with('/') {
        return Err(ValidationError::new("unsafe_file_path"));
    }
    Ok(())
}
```

### 6.2 访问控制
```rust
// IP白名单检查
pub fn check_ip_whitelist(ip: &str, whitelist: &[String]) -> bool {
    if whitelist.is_empty() {
        return true; // 默认允许所有IP
    }
    
    whitelist.iter().any(|allowed| {
        if let Ok(allowed_ip) = allowed.parse::<IpAddr>() {
            if let Ok(request_ip) = ip.parse::<IpAddr>() {
                return allowed_ip == request_ip;
            }
        }
        false
    })
}
```

## 7. 错误处理和用户体验优化

### 7.1 结构化错误类型
```rust
// src/error.rs
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_yaml::Error),
    
    #[error("Validation error: {0}")]
    Validation(String),
    
    #[error("File watching error: {0}")]
    Watch(#[from] notify::Error),
    
    #[error("Configuration not found")]
    NotFound,
}
```

### 7.2 友好的错误消息
```rust
impl ConfigError {
    pub fn user_friendly_message(&self) -> String {
        match self {
            ConfigError::Validation(details) => {
                format!("配置验证失败: {}", details)
            }
            ConfigError::NotFound => {
                "找不到配置文件，请检查路径是否正确".to_string()
            }
            ConfigError::Io(_) => {
                "读取配置文件时发生IO错误，请检查文件权限".to_string()
            }
            _ => format!("配置加载失败: {:?}", self),
        }
    }
}
```

## 8. 渐进式实施策略

### 第一阶段：基础验证框架
- [ ] 实现基本的配置验证结构
- [ ] 添加核心验证规则
- [ ] 集成到现有代码中
- [ ] 保持完全向后兼容

### 第二阶段：安全增强
- [ ] 实现安全验证规则
- [ ] 添加输入清理和转义
- [ ] 实现访问控制机制
- [ ] 添加日志审计功能

### 第三阶段：高级功能
- [ ] 实现配置热重载
- [ ] 添加配置版本管理
- [ ] 实现配置回滚机制
- [ ] 添加健康检查和监控

### 第四阶段：完善和优化
- [ ] 性能优化
- [ ] 文档完善
- [ ] 测试覆盖率提升
- [ ] 生产环境部署

## 9. 向后兼容性保证

### 9.1 兼容性策略
1. **渐进式迁移**：新验证规则默认不启用，通过配置开关控制
2. **宽松模式**：初期采用警告而非错误的方式处理验证失败
3. **版本检测**：自动检测配置版本并应用相应的验证规则
4. **降级处理**：当新功能不可用时自动降级到旧的行为

### 9.2 迁移工具
```rust
// 提供配置迁移工具
pub fn migrate_config(old_config: &Value) -> Result<Value, ConfigError> {
    let mut new_config = old_config.clone();
    
    // 迁移旧字段到新结构
    if let Some(services) = old_config.get("services") {
        new_config["proxy_configs"] = services.clone();
    }
    
    // 设置默认值
    if new_config.get("security").is_none() {
        new_config["security"] = json!({
            "tls_enabled": false,
            "rate_limit": null
        });
    }
    
    Ok(new_config)
}
```

## 10. 测试策略

### 10.1 单元测试
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_valid_config() {
        let config = ProxyConfig {
            name: "test-proxy".to_string(),
            listen: ListenAddress {
                protocol: Protocol::Tcp,
                host: "0.0.0.0".to_string(),
                port: 8080,
            },
            target: TargetAddress {
                url: "http://example.com".to_string(),
                allow_internal: false,
            },
            timeout_seconds: 30,
            ..Default::default()
        };
        
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_invalid_port() {
        let config = ProxyConfig {
            listen: ListenAddress {
                protocol: Protocol::Tcp,
                host: "0.0.0.0".to_string(),
                port: 70000, // 无效端口
            },
            ..Default::default()
        };
        
        assert!(config.validate().is_err());
    }
}
```

### 10.2 集成测试
- 配置加载和验证完整流程测试
- 热重载功能测试
- 安全验证测试
- 性能基准测试

## 11. 部署和监控

### 11.1 监控指标
```rust
// 配置相关指标
pub struct ConfigMetrics {
    pub reload_count: AtomicU64,
    pub validation_errors: AtomicU64,
    pub last_reload_time: AtomicU64,
    pub config_version: String,
}
```

### 11.2 告警机制
- 配置加载失败告警
- 验证错误次数阈值告警
- 热重载频率异常告警

## 12. 总结

本方案提供了一个完整的配置验证框架，具有以下特点：

1. **模块化设计**：各组件职责清晰，易于维护和扩展
2. **安全优先**：内置多重安全验证机制
3. **用户体验**：友好的错误提示和渐进式实施
4. **向后兼容**：确保现有配置继续工作
5. **可观察性**：完善的监控和告警机制

该框架将显著提升 MystiProxy 的配置管理能力，为用户提供更安全、可靠的代理服务。