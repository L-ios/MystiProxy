//! HTTP 鉴权模块
//!
//! 提供请求认证功能，支持 Header 鉴权和 JWT 验证

use crate::Result;
use http::header::HeaderMap;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, warn};

/// 鉴权类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AuthType {
    /// Header 鉴权
    Header,
    /// JWT 鉴权
    Jwt {
        /// JWT 密钥
        secret: String,
        /// 发行者（可选）
        issuer: Option<String>,
        /// 受众（可选）
        audience: Option<String>,
    },
}

/// 鉴权配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// 鉴权类型
    pub auth_type: AuthType,
    /// Header 名称
    #[serde(default = "default_header_name")]
    pub header_name: String,
    /// 期望的值（用于 Header 鉴权）
    pub expected_value: Option<String>,
    /// 是否启用
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_header_name() -> String {
    "Authorization".to_string()
}

fn default_enabled() -> bool {
    true
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            auth_type: AuthType::Header,
            header_name: default_header_name(),
            expected_value: None,
            enabled: true,
        }
    }
}

/// 鉴权结果
#[derive(Debug, Clone)]
pub struct AuthResult {
    /// 是否认证成功
    pub authenticated: bool,
    /// 用户标识
    pub user: Option<String>,
    /// JWT claims（如果是 JWT 鉴权）
    pub claims: Option<HashMap<String, serde_json::Value>>,
}

impl Default for AuthResult {
    fn default() -> Self {
        Self {
            authenticated: false,
            user: None,
            claims: None,
        }
    }
}

/// JWT Claims 结构
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// 主题（Subject）- 通常为用户 ID
    pub sub: String,
    /// 过期时间（Expiration Time）
    pub exp: usize,
    /// 签发时间（Issued At）
    pub iat: usize,
    /// 发行者（Issuer）
    pub iss: Option<String>,
    /// 受众（Audience）
    pub aud: Option<String>,
    /// 其他自定义 claims
    #[serde(flatten)]
    pub other: HashMap<String, serde_json::Value>,
}

/// 认证器
#[derive(Debug, Clone)]
pub struct Authenticator {
    /// 配置
    config: AuthConfig,
}

impl Authenticator {
    /// 创建新的认证器
    pub fn new(config: AuthConfig) -> Self {
        Self { config }
    }

    /// 获取配置
    pub fn config(&self) -> &AuthConfig {
        &self.config
    }

    /// 验证请求
    ///
    /// # 参数
    /// - `headers`: HTTP 请求头
    ///
    /// # 返回
    /// - `Ok(AuthResult)`: 认证结果
    /// - `Err(MystiProxyError)`: 认证过程中的错误
    pub fn authenticate(&self, headers: &HeaderMap) -> Result<AuthResult> {
        // 如果未启用认证，直接返回成功
        if !self.config.enabled {
            debug!("认证未启用，跳过认证");
            return Ok(AuthResult {
                authenticated: true,
                user: None,
                claims: None,
            });
        }

        match &self.config.auth_type {
            AuthType::Header => self.authenticate_header(headers),
            AuthType::Jwt {
                secret,
                issuer,
                audience,
            } => self.authenticate_jwt(headers, secret, issuer.as_deref(), audience.as_deref()),
        }
    }

    /// Header 鉴权
    ///
    /// 检查请求头中是否包含指定的认证信息
    fn authenticate_header(&self, headers: &HeaderMap) -> Result<AuthResult> {
        let header_value = headers
            .get(&self.config.header_name)
            .and_then(|v| v.to_str().ok());

        match header_value {
            Some(value) => {
                // 如果设置了期望值，则进行匹配
                if let Some(expected) = &self.config.expected_value {
                    if value == expected {
                        debug!("Header 鉴权成功");
                        Ok(AuthResult {
                            authenticated: true,
                            user: Some(value.to_string()),
                            claims: None,
                        })
                    } else {
                        warn!("Header 鉴权失败: 值不匹配");
                        Ok(AuthResult {
                            authenticated: false,
                            user: None,
                            claims: None,
                        })
                    }
                } else {
                    // 如果没有设置期望值，只要 header 存在就认为认证成功
                    debug!("Header 鉴权成功（无期望值检查）");
                    Ok(AuthResult {
                        authenticated: true,
                        user: Some(value.to_string()),
                        claims: None,
                    })
                }
            }
            None => {
                warn!("Header 鉴权失败: 缺少认证头 '{}'", self.config.header_name);
                Ok(AuthResult {
                    authenticated: false,
                    user: None,
                    claims: None,
                })
            }
        }
    }

    /// JWT 鉴权
    ///
    /// 解析并验证 JWT token
    fn authenticate_jwt(
        &self,
        headers: &HeaderMap,
        secret: &str,
        issuer: Option<&str>,
        audience: Option<&str>,
    ) -> Result<AuthResult> {
        // 从 header 中获取 token
        let header_value = headers
            .get(&self.config.header_name)
            .and_then(|v| v.to_str().ok());

        let token = match header_value {
            Some(value) => {
                // 支持 "Bearer <token>" 格式
                if value.starts_with("Bearer ") {
                    value[7..].to_string()
                } else {
                    value.to_string()
                }
            }
            None => {
                warn!("JWT 鉴权失败: 缺少认证头 '{}'", self.config.header_name);
                return Ok(AuthResult {
                    authenticated: false,
                    user: None,
                    claims: None,
                });
            }
        };

        // 创建解码密钥
        let decoding_key = DecodingKey::from_secret(secret.as_bytes());

        // 创建验证配置
        let mut validation = Validation::new(Algorithm::HS256);

        // 设置发行者验证
        if let Some(iss) = issuer {
            validation.set_issuer(&[iss]);
        }

        // 设置受众验证
        if let Some(aud) = audience {
            validation.set_audience(&[aud]);
        }

        // 解码并验证 token
        match decode::<Claims>(&token, &decoding_key, &validation) {
            Ok(token_data) => {
                debug!("JWT 鉴权成功: user={}", token_data.claims.sub);

                // 将 claims 转换为 HashMap
                let mut claims_map = HashMap::new();
                claims_map.insert(
                    "sub".to_string(),
                    serde_json::Value::String(token_data.claims.sub.clone()),
                );
                claims_map.insert(
                    "exp".to_string(),
                    serde_json::Value::Number(token_data.claims.exp.into()),
                );
                claims_map.insert(
                    "iat".to_string(),
                    serde_json::Value::Number(token_data.claims.iat.into()),
                );
                if let Some(iss) = &token_data.claims.iss {
                    claims_map.insert("iss".to_string(), serde_json::Value::String(iss.clone()));
                }
                if let Some(aud) = &token_data.claims.aud {
                    claims_map.insert("aud".to_string(), serde_json::Value::String(aud.clone()));
                }
                // 添加其他自定义 claims
                for (k, v) in &token_data.claims.other {
                    claims_map.insert(k.clone(), v.clone());
                }

                Ok(AuthResult {
                    authenticated: true,
                    user: Some(token_data.claims.sub),
                    claims: Some(claims_map),
                })
            }
            Err(e) => {
                warn!("JWT 鉴权失败: {}", e);
                Ok(AuthResult {
                    authenticated: false,
                    user: None,
                    claims: None,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_config_default() {
        let config = AuthConfig::default();
        assert!(config.enabled);
        assert_eq!(config.header_name, "Authorization");
    }

    #[test]
    fn test_header_auth_success() {
        let config = AuthConfig {
            auth_type: AuthType::Header,
            header_name: "X-Auth-Token".to_string(),
            expected_value: Some("secret-token".to_string()),
            enabled: true,
        };
        let authenticator = Authenticator::new(config);

        let mut headers = HeaderMap::new();
        headers.insert("X-Auth-Token", "secret-token".parse().unwrap());

        let result = authenticator.authenticate(&headers).unwrap();
        assert!(result.authenticated);
        assert_eq!(result.user, Some("secret-token".to_string()));
    }

    #[test]
    fn test_header_auth_failure() {
        let config = AuthConfig {
            auth_type: AuthType::Header,
            header_name: "X-Auth-Token".to_string(),
            expected_value: Some("secret-token".to_string()),
            enabled: true,
        };
        let authenticator = Authenticator::new(config);

        let mut headers = HeaderMap::new();
        headers.insert("X-Auth-Token", "wrong-token".parse().unwrap());

        let result = authenticator.authenticate(&headers).unwrap();
        assert!(!result.authenticated);
    }

    #[test]
    fn test_auth_disabled() {
        let config = AuthConfig {
            auth_type: AuthType::Header,
            header_name: "Authorization".to_string(),
            expected_value: None,
            enabled: false,
        };
        let authenticator = Authenticator::new(config);

        let headers = HeaderMap::new();
        let result = authenticator.authenticate(&headers).unwrap();
        assert!(result.authenticated);
    }

    #[test]
    fn test_claims_serialization() {
        let claims = Claims {
            sub: "user123".to_string(),
            exp: 9999999999,
            iat: 1111111111,
            iss: Some("test-issuer".to_string()),
            aud: None,
            other: HashMap::new(),
        };

        let json = serde_json::to_string(&claims).unwrap();
        assert!(json.contains("user123"));
    }
}
