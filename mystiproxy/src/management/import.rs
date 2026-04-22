//! Configuration file import module
//!
//! Supports importing mock configurations from YAML and JSON files.

use std::path::Path;
use tracing::{info, warn};

use super::error::{ManagementError, Result};
use super::models::{
    BodyMatchType, ResponseBodyType as BodyType, CreateMockRequest, MatchingRules, MockConfiguration, ResponseBody,
    ResponseConfig,
};
use super::repository::MockRepository;

/// Mock configuration file format
#[derive(Debug, Clone, serde::Deserialize)]
pub struct MockConfigFile {
    /// API version
    #[serde(default = "default_version")]
    pub version: String,
    
    /// Mock configurations
    pub mocks: Vec<MockEntry>,
}

fn default_version() -> String {
    "v1".to_string()
}

/// Single mock entry in config file
#[derive(Debug, Clone, serde::Deserialize)]
pub struct MockEntry {
    /// Human-readable name
    pub name: String,
    
    /// URL path pattern
    pub path: String,
    
    /// HTTP method (GET, POST, etc.)
    #[serde(default = "default_method")]
    pub method: String,
    
    /// Matching rules (optional)
    #[serde(default)]
    pub matching: Option<MatchingEntry>,
    
    /// Response configuration
    pub response: ResponseEntry,
    
    /// Whether this mock is active
    #[serde(default = "default_active")]
    pub active: bool,
}

fn default_method() -> String {
    "GET".to_string()
}

fn default_active() -> bool {
    true
}

/// Matching rules entry
#[derive(Debug, Clone, serde::Deserialize)]
pub struct MatchingEntry {
    /// Path pattern type
    #[serde(default)]
    pub path_pattern_type: Option<String>,
    
    /// Header matching rules
    #[serde(default)]
    pub headers: Vec<HeaderMatchEntry>,
    
    /// Query parameter matching rules
    #[serde(default)]
    pub query_params: Vec<QueryParamMatchEntry>,
    
    /// Body matching rule
    #[serde(default)]
    pub body: Option<BodyMatchEntry>,
}

/// Header matching entry
#[derive(Debug, Clone, serde::Deserialize)]
pub struct HeaderMatchEntry {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub match_type: Option<String>,
}

/// Query parameter matching entry
#[derive(Debug, Clone, serde::Deserialize)]
pub struct QueryParamMatchEntry {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub match_type: Option<String>,
}

/// Body matching entry
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BodyMatchEntry {
    pub json_path: Option<String>,
    pub value: Option<String>,
    #[serde(default)]
    pub match_type: Option<String>,
}

/// Response configuration entry
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ResponseEntry {
    /// HTTP status code
    #[serde(default = "default_status")]
    pub status: u16,
    
    /// Response headers
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
    
    /// Response body
    #[serde(default)]
    pub body: Option<BodyEntry>,
    
    /// Response delay in milliseconds
    #[serde(default)]
    pub delay_ms: Option<u32>,
}

fn default_status() -> u16 {
    200
}

/// Response body entry
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BodyEntry {
    /// Body type
    #[serde(rename = "type", default)]
    pub body_type: Option<String>,
    
    /// Body content
    #[serde(default)]
    pub content: Option<String>,
    
    /// Template variables
    #[serde(default)]
    pub template_vars: Vec<TemplateVarEntry>,
}

/// Template variable entry
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TemplateVarEntry {
    pub name: String,
    pub source: String,
    pub path: String,
}

impl MockEntry {
    /// Convert to CreateMockRequest
    pub fn to_create_request(&self) -> Result<CreateMockRequest> {
        let method = self.method.parse()
            .map_err(|e: String| ManagementError::import(format!("Invalid HTTP method '{}': {}", self.method, e)))?;
        
        Ok(CreateMockRequest {
            name: self.name.clone(),
            path: self.path.clone(),
            method,
            matching_rules: self.matching.as_ref().map(|m| m.to_matching_rules(&self.path)).unwrap_or_default(),
            response_config: self.response.to_response_config()?,
            is_active: self.active,
        })
    }
}

impl MatchingEntry {
    /// Convert to MatchingRules
    pub fn to_matching_rules(&self, path: &str) -> MatchingRules {
        use super::models::{HeaderMatch, QueryParamMatch, BodyMatch, MatchType, PathPatternType, TemplateVarSource};
        
        let path_pattern_type = self.path_pattern_type.as_ref().map(|t| {
            match t.to_lowercase().as_str() {
                "prefix" => PathPatternType::Prefix,
                "regex" => PathPatternType::Regex,
                _ => PathPatternType::Exact,
            }
        }).unwrap_or(PathPatternType::Exact);
        
        let headers = self.headers.iter().map(|h| {
            let match_type = h.match_type.as_ref().map(|t| {
                match t.to_lowercase().as_str() {
                    "regex" => MatchType::Regex,
                    "exists" => MatchType::Exists,
                    _ => MatchType::Exact,
                }
            }).unwrap_or(MatchType::Exact);
            
            HeaderMatch {
                name: h.name.clone(),
                value: h.value.clone(),
                match_type,
            }
        }).collect();
        
        let query_params = self.query_params.iter().map(|q| {
            let match_type = q.match_type.as_ref().map(|t| {
                match t.to_lowercase().as_str() {
                    "regex" => MatchType::Regex,
                    "exists" => MatchType::Exists,
                    _ => MatchType::Exact,
                }
            }).unwrap_or(MatchType::Exact);
            
            QueryParamMatch {
                name: q.name.clone(),
                value: q.value.clone(),
                match_type,
            }
        }).collect();
        
        let body = self.body.as_ref().map(|b| {
            let match_type = b.match_type.as_ref().map(|t| {
                match t.to_lowercase().as_str() {
                    "regex" => BodyMatchType::Regex,
                    "json_path" => BodyMatchType::JsonPath,
                    _ => BodyMatchType::Exact,
                }
            }).unwrap_or(BodyMatchType::Exact);
            
            BodyMatch {
                json_path: b.json_path.clone(),
                value: b.value.clone(),
                match_type,
            }
        });
        
        MatchingRules {
            path_pattern: Some(path.to_string()),
            path_pattern_type,
            headers,
            query_params,
            body,
        }
    }
}

impl ResponseEntry {
    /// Convert to ResponseConfig
    pub fn to_response_config(&self) -> Result<ResponseConfig> {
        use super::models::{TemplateVar, TemplateVarSource};
        
        let body = self.body.as_ref().map(|b| {
            let body_type = b.body_type.as_ref().map(|t| {
                match t.to_lowercase().as_str() {
                    "template" => BodyType::Template,
                    "file" => BodyType::File,
                    "script" => BodyType::Script,
                    _ => BodyType::Static,
                }
            }).unwrap_or(BodyType::Static);
            
            let content = b.content.clone();
            
            let template_vars = b.template_vars.iter().map(|v| {
                let source = match v.source.to_lowercase().as_str() {
                    "path" => TemplateVarSource::Path,
                    "query" => TemplateVarSource::Query,
                    "header" => TemplateVarSource::Header,
                    "body" => TemplateVarSource::Body,
                    _ => TemplateVarSource::Path,
                };
                
                TemplateVar {
                    name: v.name.clone(),
                    source,
                    path: Some(v.path.clone()),
                }
            }).collect();
            
            ResponseBody {
                body_type,
                content,
                template_vars,
            }
        });
        
        Ok(ResponseConfig {
            status: self.status,
            headers: self.headers.clone(),
            body,
            delay_ms: self.delay_ms,
        })
    }
}

/// Import mock configurations from a file
pub async fn import_from_file<R: MockRepository>(
    path: &Path,
    repository: &R,
) -> Result<Vec<MockConfiguration>> {
    info!("Importing mock configurations from: {}", path.display());
    
    let content = std::fs::read_to_string(path)?;
    
    let config_file: MockConfigFile = if path.extension().map_or(false, |ext| ext == "yaml" || ext == "yml") {
        serde_yaml::from_str(&content)?
    } else {
        serde_json::from_str(&content)?
    };
    
    info!("Found {} mock entries in file", config_file.mocks.len());
    
    let mut imported = Vec::new();
    let mut errors = Vec::new();
    
    for (index, entry) in config_file.mocks.iter().enumerate() {
        match entry.to_create_request() {
            Ok(request) => {
                let mut config = MockConfiguration::new(
                    request.name,
                    request.path,
                    request.method,
                    request.matching_rules.clone(),
                    request.response_config.clone(),
                );
                config.is_active = request.is_active;
                config.update_content_hash();
                
                match repository.save(&config).await {
                    Ok(()) => {
                        info!("Imported mock: {} ({})", config.name, config.id);
                        imported.push(config);
                    }
                    Err(e) => {
                        warn!("Failed to save mock '{}': {}", entry.name, e);
                        errors.push((index, entry.name.clone(), e.to_string()));
                    }
                }
            }
            Err(e) => {
                warn!("Failed to parse mock entry {}: {}", index, e);
                errors.push((index, entry.name.clone(), e.to_string()));
            }
        }
    }
    
    if !errors.is_empty() {
        warn!("Import completed with {} errors:", errors.len());
        for (index, name, error) in &errors {
            warn!("  [{}] {}: {}", index, name, error);
        }
    }
    
    info!("Successfully imported {} mock configurations", imported.len());
    Ok(imported)
}

/// Import mock configurations from a string
pub fn parse_config_file(content: &str, format: &str) -> Result<MockConfigFile> {
    match format.to_lowercase().as_str() {
        "yaml" | "yml" => Ok(serde_yaml::from_str(content)?),
        "json" => Ok(serde_json::from_str(content)?),
        _ => Err(ManagementError::import(format!("Unsupported format: {}", format))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::management::db::create_memory_pool;
    use crate::management::repository::LocalMockRepository;
    
    #[tokio::test]
    async fn test_import_from_yaml() {
        let pool = create_memory_pool().await.unwrap();
        let repo = LocalMockRepository::with_random_instance_id(pool);
        
        let yaml_content = r#"
version: v1
mocks:
  - name: Test Mock
    path: /api/test
    method: GET
    response:
      status: 200
      body:
        type: static
        content: '{"message": "hello"}'
"#;
        
        let config_file = parse_config_file(yaml_content, "yaml").unwrap();
        assert_eq!(config_file.mocks.len(), 1);
        
        let entry = &config_file.mocks[0];
        let request = entry.to_create_request().unwrap();
        assert_eq!(request.name, "Test Mock");
        assert_eq!(request.path, "/api/test");
    }
    
    #[test]
    fn test_parse_json_config() {
        let json_content = r#"
{
  "version": "v1",
  "mocks": [
    {
      "name": "JSON Mock",
      "path": "/api/json",
      "method": "POST",
      "response": {
        "status": 201
      }
    }
  ]
}
"#;
        
        let config_file = parse_config_file(json_content, "json").unwrap();
        assert_eq!(config_file.mocks.len(), 1);
        assert_eq!(config_file.mocks[0].name, "JSON Mock");
    }
}
