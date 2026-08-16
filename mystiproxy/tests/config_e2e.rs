//! E2E tests for configuration parsing and validation.
//!
//! These tests exercise the full config pipeline: YAML string → MystiConfig struct,
//! covering all config types, duration parsing, header actions, provider types,
//! and serialization round-trips.

use std::collections::HashMap;
use std::time::Duration;

use mystiproxy::config::{
    AuthConfig, BodyConfig, BodyType, EngineConfig, HeaderAction, HeaderActionType, JsonBodyAction,
    JsonBodyConfig, LocationConfig, MatchMode, MystiConfig, ProviderType, ProxyType, RequestConfig,
    ResponseConfig, UriConfig,
};

// ---------------------------------------------------------------------------
// Full YAML config parsing
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_full_config_with_multiple_engines() {
    let yaml = r#"
mysti:
  engine:
    web:
      listen: tcp://0.0.0.0:8080
      target: tcp://127.0.0.1:3000
      proxy_type: http
      request_timeout: 30s
      locations:
        - location: /api/health
          mode: Full
          provider: mock
          response:
            status: 200
        - location: /static
          mode: Prefix
          provider: static
          root: /var/www
    database:
      listen: tcp://0.0.0.0:3306
      target: tcp://10.0.0.1:3306
      proxy_type: tcp
      connection_timeout: 5s
cert: []
"#;

    let config = MystiConfig::from_yaml(yaml).expect("failed to parse config");
    assert_eq!(config.mysti.engine.len(), 2);

    let web = &config.mysti.engine["web"];
    assert_eq!(web.proxy_type, ProxyType::Http);
    assert_eq!(web.request_timeout, Some(Duration::from_secs(30)));
    assert!(web.locations.is_some());
    let locations = web.locations.as_ref().unwrap();
    assert_eq!(locations.len(), 2);
    assert_eq!(locations[0].provider, Some(ProviderType::Mock));
    assert_eq!(locations[1].provider, Some(ProviderType::Static));

    let db = &config.mysti.engine["database"];
    assert_eq!(db.proxy_type, ProxyType::Tcp);
    assert_eq!(db.connection_timeout, Some(Duration::from_secs(5)));
}

#[test]
fn test_e2e_config_with_auth_and_tls() {
    let yaml = r#"
mysti:
  engine:
    secure:
      listen: tcp://0.0.0.0:443
      target: tcp://127.0.0.1:8443
      proxy_type: http
      auth:
        auth_type: header
        header_name: X-API-Key
        expected_value: secret123
        enabled: true
      tls:
        cert_path: /etc/ssl/cert.pem
        key_path: /etc/ssl/key.pem
        mutual_auth: false
cert: []
"#;

    let config = MystiConfig::from_yaml(yaml).expect("failed to parse config");
    let engine = &config.mysti.engine["secure"];
    assert!(engine.auth.is_some());
    let auth = engine.auth.as_ref().unwrap();
    assert_eq!(auth.auth_type, "header");
    assert_eq!(auth.header_name, "X-API-Key");
    assert_eq!(auth.expected_value.as_deref(), Some("secret123"));
    assert!(auth.enabled);

    assert!(engine.tls.is_some());
    let tls = engine.tls.as_ref().unwrap();
    assert_eq!(tls.cert_path, "/etc/ssl/cert.pem");
    assert_eq!(tls.key_path, "/etc/ssl/key.pem");
    assert!(!tls.mutual_auth);
}

#[test]
fn test_e2e_config_with_request_modifications() {
    let yaml = r#"
mysti:
  engine:
    rewrite:
      listen: tcp://0.0.0.0:8080
      target: tcp://127.0.0.1:9000
      proxy_type: http
      header:
        X-Forwarded-For:
          value: proxy.mysti.local
          action: overwrite
      locations:
        - location: /api/v1
          mode: Prefix
          provider: proxy
          request:
            method: POST
            uri:
              path: /api/v2
              query: rewrote=true
            headers:
              X-Rewritten:
                value: "yes"
                action: overwrite
            body:
              type: json
              json:
                path: $.version
                value: "2"
                action: overwrite
"#;

    let config = MystiConfig::from_yaml(yaml).expect("failed to parse config");
    let engine = &config.mysti.engine["rewrite"];

    assert!(engine.header.is_some());
    let header = engine.header.as_ref().unwrap();
    assert!(header.contains_key("X-Forwarded-For"));
    let action = &header["X-Forwarded-For"];
    assert_eq!(action.value, "proxy.mysti.local");
    assert_eq!(action.action, HeaderActionType::Overwrite);

    let locations = engine.locations.as_ref().unwrap();
    let loc = &locations[0];
    assert!(loc.request.is_some());
    let req = loc.request.as_ref().unwrap();
    assert_eq!(req.method.as_deref(), Some("POST"));

    let uri = req.uri.as_ref().unwrap();
    assert_eq!(uri.path.as_deref(), Some("/api/v2"));
    assert_eq!(uri.query.as_deref(), Some("rewrote=true"));

    let body = req.body.as_ref().unwrap();
    assert_eq!(body.body_type, Some(BodyType::Json));
    let json = body.json.as_ref().unwrap();
    assert_eq!(json.path, "$.version");
    assert_eq!(json.value, "2");
    assert_eq!(json.action, JsonBodyAction::Overwrite);
}

// ---------------------------------------------------------------------------
// Duration parsing variants
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_duration_parsing_variants() {
    let cases = [
        ("100ms", Duration::from_millis(100)),
        ("5s", Duration::from_secs(5)),
        ("2m", Duration::from_secs(120)),
        ("1h", Duration::from_secs(3600)),
        ("1.5s", Duration::from_millis(1500)),
        ("0.5m", Duration::from_secs(30)),
    ];

    for (input, expected) in cases {
        let yaml = format!(
            r#"
mysti:
  engine:
    test:
      listen: tcp://0.0.0.0:0
      target: tcp://127.0.0.1:1
      proxy_type: tcp
      request_timeout: {input}
cert: []
"#
        );
        let config = MystiConfig::from_yaml(&yaml).expect("failed to parse");
        let engine = &config.mysti.engine["test"];
        assert_eq!(
            engine.request_timeout,
            Some(expected),
            "failed for duration '{input}'"
        );
    }
}

#[test]
fn test_e2e_backward_compatible_timeout_alias() {
    let yaml = r#"
mysti:
  engine:
    legacy:
      listen: tcp://0.0.0.0:0
      target: tcp://127.0.0.1:1
      proxy_type: tcp
      timeout: 15s
cert: []
"#;
    let config = MystiConfig::from_yaml(yaml).expect("failed to parse");
    let engine = &config.mysti.engine["legacy"];
    assert_eq!(engine.request_timeout, Some(Duration::from_secs(15)));
}

// ---------------------------------------------------------------------------
// Serialization round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_config_serialization_round_trip() {
    let mut header = HashMap::new();
    header.insert(
        "X-Test".to_string(),
        HeaderAction {
            value: "abc".to_string(),
            action: HeaderActionType::Overwrite,
            condition: None,
        },
    );

    let original = EngineConfig {
        listen: "tcp://0.0.0.0:8080".to_string(),
        target: "tcp://127.0.0.1:9000".to_string(),
        proxy_type: ProxyType::Http,
        request_timeout: Some(Duration::from_secs(10)),
        connection_timeout: Some(Duration::from_secs(3)),
        header: Some(header),
        locations: Some(vec![LocationConfig {
            location: "/api".to_string(),
            mode: MatchMode::Prefix,
            provider: Some(ProviderType::Mock),
            root: None,
            response: Some(ResponseConfig {
                status: Some(204),
                headers: None,
                body: None,
                conditions: None,
            }),
            request: None,
            index_files: None,
            enable_directory_listing: None,
        }]),
        auth: None,
        upstream: None,
        allow: None,
        deny: None,
        management: None,
        tls: None,
    };

    // Duration is now serialized as a human-readable string, so full round-trip works.
    let yaml = serde_yaml::to_string(&original).expect("serialize failed");
    let parsed: EngineConfig =
        serde_yaml::from_str(&yaml).unwrap_or_else(|e| panic!("deserialize failed: {e}"));

    assert_eq!(parsed.listen, original.listen);
    assert_eq!(parsed.target, original.target);
    assert_eq!(parsed.proxy_type, original.proxy_type);
    assert_eq!(parsed.request_timeout, original.request_timeout);
    assert_eq!(parsed.connection_timeout, original.connection_timeout);
    assert!(parsed.header.is_some());
    assert!(parsed.locations.is_some());
    let locs = parsed.locations.as_ref().unwrap();
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].mode, MatchMode::Prefix);
    assert_eq!(locs[0].provider, Some(ProviderType::Mock));
}

// ---------------------------------------------------------------------------
// Match mode serialization
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_all_match_modes_round_trip() {
    for mode in [
        MatchMode::Full,
        MatchMode::Prefix,
        MatchMode::Regex,
        MatchMode::PrefixRegex,
    ] {
        let yaml = serde_yaml::to_string(&mode).expect("serialize failed");
        let parsed: MatchMode = serde_yaml::from_str(&yaml).expect("deserialize failed");
        assert_eq!(parsed, mode);
    }
}

#[test]
fn test_e2e_all_provider_types_round_trip() {
    for provider in [
        ProviderType::Static,
        ProviderType::Mock,
        ProviderType::Proxy,
    ] {
        let yaml = serde_yaml::to_string(&provider).expect("serialize failed");
        let parsed: ProviderType = serde_yaml::from_str(&yaml).expect("deserialize failed");
        assert_eq!(parsed, provider);
    }
}

// ---------------------------------------------------------------------------
// Invalid configs
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_empty_engine_map() {
    let yaml = r#"
mysti:
  engine: {}
cert: []
"#;
    let config = MystiConfig::from_yaml(yaml).expect("failed to parse");
    assert!(config.mysti.engine.is_empty());
}

#[test]
fn test_e2e_invalid_duration_unit() {
    let yaml = r#"
mysti:
  engine:
    bad:
      listen: tcp://0.0.0.0:0
      target: tcp://127.0.0.1:1
      proxy_type: tcp
      request_timeout: 10xyz
cert: []
"#;
    let result = MystiConfig::from_yaml(yaml);
    assert!(result.is_err(), "should fail with invalid duration unit");
}

#[test]
fn test_e2e_invalid_yaml_syntax() {
    let result = MystiConfig::from_yaml("{{invalid yaml");
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Struct builder e2e
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_build_config_from_structs() {
    let config = MystiConfig {
        mysti: mystiproxy::config::Mysti {
            engine: {
                let mut m = HashMap::new();
                m.insert(
                    "default".to_string(),
                    EngineConfig {
                        listen: "tcp://0.0.0.0:8080".to_string(),
                        target: "tcp://127.0.0.1:3000".to_string(),
                        proxy_type: ProxyType::Http,
                        request_timeout: None,
                        connection_timeout: None,
                        header: None,
                        locations: Some(vec![LocationConfig {
                            location: "/health".to_string(),
                            mode: MatchMode::Full,
                            provider: Some(ProviderType::Mock),
                            root: None,
                            response: Some(ResponseConfig {
                                status: Some(200),
                                headers: None,
                                body: Some(BodyConfig {
                                    json: None,
                                    content: None,
                                    template: None,
                                    body_type: Some(BodyType::Static),
                                }),
                                conditions: None,
                            }),
                            request: Some(RequestConfig {
                                method: None,
                                uri: Some(UriConfig {
                                    path: Some("/internal/health".to_string()),
                                    query: None,
                                }),
                                headers: None,
                                body: None,
                            }),
                            index_files: None,
                            enable_directory_listing: None,
                        }]),
                        auth: Some(AuthConfig {
                            auth_type: "header".to_string(),
                            header_name: "Authorization".to_string(),
                            expected_value: Some("token".to_string()),
                            jwt_secret: None,
                            enabled: true,
                        }),
                        tls: None,
                        upstream: None,
                        allow: None,
                        deny: None,
                        management: None,
                    },
                );
                m
            },
        },
        cert: vec![],
    };

    let engine = &config.mysti.engine["default"];
    assert_eq!(engine.proxy_type, ProxyType::Http);
    let loc = &engine.locations.as_ref().unwrap()[0];
    assert_eq!(loc.location, "/health");
    assert_eq!(loc.mode, MatchMode::Full);
    assert!(loc.response.is_some());
    assert!(loc.request.is_some());
    assert!(engine.auth.is_some());
}

#[cfg(test)]
mod f8b_validation_wiring {
    use mystiproxy::config::{MystiConfig, ValidationLevel};

    fn bad_cidr_yaml() -> String {
        r#"
mysti:
  engine:
    web:
      listen: tcp://127.0.0.1:18099
      target: tcp://127.0.0.1:18081
      proxy_type: http
      allow: ["10.0.0.0/99"]
cert: []
"#
        .to_string()
    }

    fn good_yaml() -> String {
        r#"
mysti:
  engine:
    web:
      listen: tcp://127.0.0.1:18099
      target: tcp://127.0.0.1:18081
      proxy_type: http
cert: []
"#
        .to_string()
    }

    #[test]
    fn test_strict_rejects_bad_cidr_yaml() {
        let dir = std::env::temp_dir().join("f8b_test_strict.yaml");
        std::fs::write(&dir, bad_cidr_yaml()).unwrap();
        let r =
            MystiConfig::load_validated_with_level(dir.to_str().unwrap(), ValidationLevel::Strict);
        assert!(r.is_err(), "strict must reject /99 CIDR");
    }

    #[test]
    fn test_none_level_allows_bad_cidr_yaml() {
        let dir = std::env::temp_dir().join("f8b_test_none.yaml");
        std::fs::write(&dir, bad_cidr_yaml()).unwrap();
        let r =
            MystiConfig::load_validated_with_level(dir.to_str().unwrap(), ValidationLevel::None);
        assert!(r.is_ok(), "none must allow: {:?}", r.err());
    }

    #[test]
    fn test_good_yaml_passes_strict() {
        let dir = std::env::temp_dir().join("f8b_test_good.yaml");
        std::fs::write(&dir, good_yaml()).unwrap();
        let r =
            MystiConfig::load_validated_with_level(dir.to_str().unwrap(), ValidationLevel::Strict);
        assert!(r.is_ok(), "good config must pass: {:?}", r.err());
    }
}

#[cfg(test)]
mod f9_management_config {
    use mystiproxy::config::{ManagementConfig, MystiConfig};

    #[test]
    fn test_management_default_not_effective() {
        let m = ManagementConfig::default();
        assert!(!m.is_effective(), "no listen -> disabled");
    }

    #[test]
    fn test_management_listen_effective() {
        let m = ManagementConfig {
            listen: Some("tcp://127.0.0.1:8081".into()),
            ..Default::default()
        };
        assert!(m.is_effective());
    }

    #[test]
    fn test_management_explicit_disable() {
        let m = ManagementConfig {
            listen: Some("tcp://127.0.0.1:8081".into()),
            enabled: Some(false),
            ..Default::default()
        };
        assert!(!m.is_effective());
    }

    #[test]
    fn test_yaml_management_section_parsed() {
        let yaml = r#"
mysti:
  engine:
    web:
      listen: tcp://127.0.0.1:18080
      target: tcp://127.0.0.1:18081
      proxy_type: http
      management:
        listen: tcp://127.0.0.1:18091
        db_path: /tmp/f9-mgmt.db
        central_url: http://127.0.0.1:18090
        sync_interval: 15
cert: []
"#;
        let c = MystiConfig::from_yaml(yaml).unwrap();
        let m = c.mysti.engine["web"].management.as_ref().unwrap();
        assert_eq!(m.listen.as_deref(), Some("tcp://127.0.0.1:18091"));
        assert_eq!(m.db_path.as_deref(), Some("/tmp/f9-mgmt.db"));
        assert_eq!(m.central_url.as_deref(), Some("http://127.0.0.1:18090"));
        assert_eq!(m.sync_interval, Some(15));
        assert!(m.is_effective());
    }

    #[test]
    fn test_yaml_without_management_is_none() {
        let yaml = r#"
mysti:
  engine:
    web: {listen: tcp://127.0.0.1:18080, target: tcp://127.0.0.1:18081, proxy_type: http}
cert: []
"#;
        let c = MystiConfig::from_yaml(yaml).unwrap();
        assert!(c.mysti.engine["web"].management.is_none());
    }
}
