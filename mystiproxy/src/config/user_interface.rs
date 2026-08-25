//! 用户界面增强（友好的错误提示和修复建议）

use colored::*;
use validator::ValidationErrors;

use crate::config::validation::ConfigValidationError;

/// 用户界面配置
pub struct ConfigUserInterface {
    pub verbose: bool,
    pub color_output: bool,
}

impl Default for ConfigUserInterface {
    fn default() -> Self {
        Self {
            verbose: false,
            color_output: true,
        }
    }
}

impl ConfigUserInterface {
    /// 创建新的用户界面
    pub fn new(verbose: bool, color_output: bool) -> Self {
        Self {
            verbose,
            color_output,
        }
    }

    /// 打印验证结果
    pub fn print_validation_result(&self, result: &Result<(), ConfigValidationError>) {
        match result {
            Ok(()) => {
                if self.verbose {
                    self.print_success("Configuration validated successfully");
                }
            }
            Err(error) => {
                self.print_error(&format!("Configuration validation failed: {}", error));
                self.print_fix_suggestions(error);
            }
        }
    }

    /// 打印成功消息
    fn print_success(&self, msg: &str) {
        if self.color_output {
            println!("{} {}", "✓".green(), msg.green());
        } else {
            println!("✓ {}", msg);
        }
    }

    /// 打印错误消息
    fn print_error(&self, msg: &str) {
        if self.color_output {
            println!("{} {}", "✗".red(), msg.red());
        } else {
            println!("✗ {}", msg);
        }
    }

    /// 打印修复建议
    fn print_fix_suggestions(&self, error: &ConfigValidationError) {
        let suggestions = match error {
            ConfigValidationError::Validation(errors) => {
                self.extract_validation_suggestions(errors)
            }
            ConfigValidationError::Security(msg) => {
                vec![format!("Security issue: {}", msg)]
            }
            ConfigValidationError::Load(msg) => {
                vec![format!("Load error: {}", msg)]
            }
            ConfigValidationError::Parse(msg) => {
                vec![format!("Parse error: {}", msg)]
            }
            ConfigValidationError::HotReload(msg) => {
                vec![format!("Hot reload error: {}", msg)]
            }
            ConfigValidationError::Watch(msg) => {
                vec![format!("Watch error: {}", msg)]
            }
        };

        if self.color_output {
            println!("{}", "Suggested fixes:".yellow());
        } else {
            println!("Suggested fixes:");
        }

        for suggestion in suggestions {
            if self.color_output {
                println!("  • {}", suggestion.blue());
            } else {
                println!("  • {}", suggestion);
            }
        }
    }

    /// 从验证错误提取建议
    fn extract_validation_suggestions(&self, errors: &ValidationErrors) -> Vec<String> {
        let mut suggestions = Vec::new();

        // 使用 field_errors() 获取字段级错误（Vec<ValidationError>）
        let field_errors = errors.field_errors();

        for (field, error_vec) in field_errors {
            for error in error_vec {
                let code: &str = error.code.as_ref();
                let suggestion = match (field.as_ref(), code) {
                    ("listen", "listen_empty") => "Listen address cannot be empty".to_string(),
                    ("listen", "invalid_tcp_address") => {
                        "Invalid TCP address format, use 'tcp://host:port'".to_string()
                    }
                    ("listen", "empty_unix_socket_path") => {
                        "Unix socket path cannot be empty".to_string()
                    }
                    ("listen", "unsupported_protocol") => {
                        "Supported protocols: tcp://, unix://".to_string()
                    }
                    ("target", "target_empty") => "Target address cannot be empty".to_string(),
                    ("target", "invalid_tcp_target") => {
                        "Invalid TCP target format, use 'tcp://host:port'".to_string()
                    }
                    ("target", "empty_unix_target_path") => {
                        "Unix target path cannot be empty".to_string()
                    }
                    ("target", "unsupported_target_protocol") => {
                        "Supported target protocols: tcp://, unix://, http://, https://".to_string()
                    }
                    ("proxy_type", "tcp_proxy_requires_tcp_addresses") => {
                        "TCP proxy requires both listen and target to use tcp://".to_string()
                    }
                    ("proxy_type", "http_proxy_requires_tcp_or_unix_listen") => {
                        "HTTP proxy listen must be tcp:// or unix://".to_string()
                    }
                    ("proxy_type", "http_proxy_requires_tcp_or_unix_target") => {
                        "HTTP proxy target must be tcp:// or unix://".to_string()
                    }
                    ("proxy_type", "forward_proxy_requires_tcp_listen") => {
                        "Forward proxy requires tcp:// listen address".to_string()
                    }
                    ("locations", _) => format!("Location configuration error in '{}'", field),
                    ("tls", "tls_cert_path_empty") => {
                        "TLS certificate path cannot be empty".to_string()
                    }
                    ("tls", "tls_key_path_empty") => {
                        "TLS private key path cannot be empty".to_string()
                    }
                    ("tls", "tls_cert_file_not_found") => {
                        "TLS certificate file not found".to_string()
                    }
                    ("tls", "tls_key_file_not_found") => {
                        "TLS private key file not found".to_string()
                    }
                    ("tls", "tls_client_ca_file_not_found") => {
                        "TLS client CA file not found".to_string()
                    }
                    ("auth", "header_auth_requires_expected_value") => {
                        "Header authentication requires expected_value".to_string()
                    }
                    ("auth", "jwt_auth_requires_secret") => {
                        "JWT authentication requires jwt_secret".to_string()
                    }
                    ("auth", "unsupported_auth_type") => {
                        "Supported auth types: header, jwt".to_string()
                    }
                    ("upstream", "invalid_upstream_proxy_url") => {
                        "Invalid upstream proxy URL format".to_string()
                    }
                    _ => format!(
                        "Validation error in '{}': {}",
                        field,
                        error.message.as_deref().unwrap_or(code)
                    ),
                };
                suggestions.push(suggestion);
            }
        }

        suggestions
    }

    /// 打印配置摘要
    pub fn print_config_summary(&self, config: &crate::config::MystiConfig) {
        if self.color_output {
            println!(
                "{}",
                "=== MystiProxy Configuration Summary ===".cyan().bold()
            );
        } else {
            println!("=== MystiProxy Configuration Summary ===");
        }

        println!("Engines: {}", config.mysti.engine.len());
        for (name, engine) in &config.mysti.engine {
            if self.color_output {
                println!(
                    "  {}: {} -> {} ({})",
                    name.cyan(),
                    engine.listen.yellow(),
                    engine.target.yellow(),
                    format!("{:?}", engine.proxy_type).green()
                );
            } else {
                println!(
                    "  {}: {} -> {} ({:?})",
                    name, engine.listen, engine.target, engine.proxy_type
                );
            }

            if let Some(locations) = &engine.locations {
                println!("    Locations: {}", locations.len());
                for loc in locations {
                    println!(
                        "      {} [{:?}] -> {:?}",
                        loc.location, loc.mode, loc.provider
                    );
                }
            }
        }

        if !config.cert.is_empty() {
            println!("Certificates: {}", config.cert.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EngineConfig, Mysti, MystiConfig, ProxyType};
    use std::collections::HashMap;

    fn make_config(engines: usize) -> MystiConfig {
        let mut map = HashMap::new();
        for i in 0..engines {
            map.insert(
                format!("e{i}"),
                EngineConfig {
                    listen: format!("tcp://0.0.0.0:{} ", 8000 + i).trim().to_string(),
                    target: format!("tcp://127.0.0.1:{} ", 9000 + i).trim().to_string(),
                    proxy_type: ProxyType::Http,
                    request_timeout: None,
                    connection_timeout: None,
                    header: None,
                    locations: None,
                    tls: None,
                    auth: None,
                    upstream: None,
                    allow: None,
                    deny: None,
                    management: None,
                },
            );
        }
        MystiConfig {
            mysti: Mysti { engine: map },
            cert: vec![],
        }
    }

    // ---------- T1 构造 ----------

    #[test]
    fn test_ui_creation_verbose_no_color() {
        let ui = ConfigUserInterface::new(true, false);
        assert!(ui.verbose);
        assert!(!ui.color_output);
    }

    #[test]
    fn test_ui_creation_silent_with_color() {
        let ui = ConfigUserInterface::new(false, true);
        assert!(!ui.verbose);
        assert!(ui.color_output);
    }

    #[test]
    fn test_ui_default_is_silent_with_color() {
        let ui = ConfigUserInterface::default();
        assert!(!ui.verbose);
        assert!(ui.color_output);
    }

    // ---------- T2 print_validation_result ----------

    #[test]
    fn test_print_validation_result_ok_silent() {
        // verbose=false 时成功路径不打印
        let ui = ConfigUserInterface::new(false, false);
        // 仅验证不 panic（stdout 不可断言）
        ui.print_validation_result(&Ok(()));
    }

    #[test]
    fn test_print_validation_result_ok_verbose() {
        let ui = ConfigUserInterface::new(true, false);
        ui.print_validation_result(&Ok(()));
    }

    #[test]
    fn test_print_validation_result_load_error() {
        let ui = ConfigUserInterface::new(false, false);
        let err = ConfigValidationError::Load("file not found".to_string());
        ui.print_validation_result(&Err(err));
    }

    #[test]
    fn test_print_validation_result_parse_error() {
        let ui = ConfigUserInterface::new(true, false);
        let err = ConfigValidationError::Parse("invalid yaml".to_string());
        ui.print_validation_result(&Err(err));
    }

    #[test]
    fn test_print_validation_result_security_error() {
        let ui = ConfigUserInterface::new(false, false);
        let err = ConfigValidationError::Security("ssrf blocked".to_string());
        ui.print_validation_result(&Err(err));
    }

    #[test]
    fn test_print_validation_result_hot_reload_error() {
        let ui = ConfigUserInterface::new(false, false);
        let err = ConfigValidationError::HotReload("watcher died".to_string());
        ui.print_validation_result(&Err(err));
    }

    #[test]
    fn test_print_validation_result_watch_error() {
        let ui = ConfigUserInterface::new(false, false);
        let err = ConfigValidationError::Watch("notify error".to_string());
        ui.print_validation_result(&Err(err));
    }

    #[test]
    fn test_print_validation_result_validation_error_with_field_errors() {
        let ui = ConfigUserInterface::new(true, false);
        // 构造一个含字段错误的 ValidationErrors
        let mut errs = validator::ValidationErrors::new();
        errs.add("listen", validator::ValidationError::new("listen_empty"));
        errs.add(
            "target",
            validator::ValidationError {
                code: "invalid_tcp_target".into(),
                message: Some("bad target".into()),
                params: HashMap::new(),
            },
        );
        let err = ConfigValidationError::Validation(errs);
        ui.print_validation_result(&Err(err));
    }

    // ---------- T3 print_config_summary ----------

    #[test]
    fn test_print_config_summary_empty_engines() {
        let ui = ConfigUserInterface::new(false, false);
        let cfg = make_config(0);
        // 仅验证不 panic
        ui.print_config_summary(&cfg);
    }

    #[test]
    fn test_print_config_summary_with_engines() {
        let ui = ConfigUserInterface::new(true, true);
        let cfg = make_config(3);
        ui.print_config_summary(&cfg);
    }

    #[test]
    fn test_print_config_summary_with_certs() {
        let ui = ConfigUserInterface::new(false, false);
        let mut cfg = make_config(1);
        cfg.cert = vec![crate::config::CertConfig {
            name: "root".to_string(),
            root_key: "k".to_string(),
        }];
        ui.print_config_summary(&cfg);
    }

    // ---------- T4 extract_validation_suggestions 映射 ----------

    #[test]
    fn test_extract_suggestions_listen_codes() {
        let ui = ConfigUserInterface::new(false, false);
        let cases: &[(&str, &str)] = &[
            ("listen", "listen_empty"),
            ("listen", "invalid_tcp_address"),
            ("listen", "empty_unix_socket_path"),
            ("listen", "unsupported_protocol"),
        ];
        for (field, code) in cases {
            let mut errs = validator::ValidationErrors::new();
            errs.add(field, validator::ValidationError::new(code));
            let err = ConfigValidationError::Validation(errs);
            // 走 print 路径间接触发 extract_validation_suggestions
            ui.print_validation_result(&Err(err));
        }
    }

    #[test]
    fn test_extract_suggestions_target_codes() {
        let ui = ConfigUserInterface::new(false, false);
        let cases: &[(&str, &str)] = &[
            ("target", "target_empty"),
            ("target", "invalid_tcp_target"),
            ("target", "empty_unix_target_path"),
            ("target", "unsupported_target_protocol"),
        ];
        for (field, code) in cases {
            let mut errs = validator::ValidationErrors::new();
            errs.add(field, validator::ValidationError::new(code));
            let err = ConfigValidationError::Validation(errs);
            ui.print_validation_result(&Err(err));
        }
    }

    #[test]
    fn test_extract_suggestions_proxy_type_codes() {
        let ui = ConfigUserInterface::new(false, false);
        let cases: &[&str] = &[
            "tcp_proxy_requires_tcp_addresses",
            "http_proxy_requires_tcp_or_unix_listen",
            "http_proxy_requires_tcp_or_unix_target",
            "forward_proxy_requires_tcp_listen",
        ];
        for code in cases {
            let mut errs = validator::ValidationErrors::new();
            errs.add("proxy_type", validator::ValidationError::new(code));
            let err = ConfigValidationError::Validation(errs);
            ui.print_validation_result(&Err(err));
        }
    }

    #[test]
    fn test_extract_suggestions_tls_codes() {
        let ui = ConfigUserInterface::new(false, false);
        let cases: &[&str] = &[
            "tls_cert_path_empty",
            "tls_key_path_empty",
            "tls_cert_file_not_found",
            "tls_key_file_not_found",
            "tls_client_ca_file_not_found",
        ];
        for code in cases {
            let mut errs = validator::ValidationErrors::new();
            errs.add("tls", validator::ValidationError::new(code));
            let err = ConfigValidationError::Validation(errs);
            ui.print_validation_result(&Err(err));
        }
    }

    #[test]
    fn test_extract_suggestions_auth_codes() {
        let ui = ConfigUserInterface::new(false, false);
        let cases: &[&str] = &[
            "header_auth_requires_expected_value",
            "jwt_auth_requires_secret",
            "unsupported_auth_type",
        ];
        for code in cases {
            let mut errs = validator::ValidationErrors::new();
            errs.add("auth", validator::ValidationError::new(code));
            let err = ConfigValidationError::Validation(errs);
            ui.print_validation_result(&Err(err));
        }
    }

    #[test]
    fn test_extract_suggestions_upstream_code() {
        let ui = ConfigUserInterface::new(false, false);
        let mut errs = validator::ValidationErrors::new();
        errs.add(
            "upstream",
            validator::ValidationError::new("invalid_upstream_proxy_url"),
        );
        let err = ConfigValidationError::Validation(errs);
        ui.print_validation_result(&Err(err));
    }

    #[test]
    fn test_extract_suggestions_locations_fallback() {
        let ui = ConfigUserInterface::new(false, false);
        let mut errs = validator::ValidationErrors::new();
        errs.add(
            "locations",
            validator::ValidationError::new("any_unknown_code"),
        );
        let err = ConfigValidationError::Validation(errs);
        ui.print_validation_result(&Err(err));
    }

    #[test]
    fn test_extract_suggestions_unknown_field_fallback() {
        let ui = ConfigUserInterface::new(false, false);
        let mut errs = validator::ValidationErrors::new();
        errs.add(
            "unknown_field",
            validator::ValidationError {
                code: "unknown_code".into(),
                message: Some("custom message".into()),
                params: HashMap::new(),
            },
        );
        let err = ConfigValidationError::Validation(errs);
        ui.print_validation_result(&Err(err));
    }

    // ---------- T5 着色输出 ----------

    #[test]
    fn test_color_output_path_no_panic() {
        let ui = ConfigUserInterface::new(true, true);
        let err = ConfigValidationError::Security("test".to_string());
        ui.print_validation_result(&Err(err));
        let cfg = make_config(2);
        ui.print_config_summary(&cfg);
    }
}
