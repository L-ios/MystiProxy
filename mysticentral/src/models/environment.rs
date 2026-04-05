//! Environment model
//!
//! Represents deployment environments (dev, staging, prod, etc.)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Environment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub endpoints: HashMap<String, String>,
    pub is_template: bool,
    pub template_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Environment {
    /// Create a new environment
    pub fn new(name: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            description: None,
            endpoints: HashMap::new(),
            is_template: false,
            template_id: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create from a template
    pub fn from_template(name: String, template: &Environment) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            description: template.description.clone(),
            endpoints: template.endpoints.clone(),
            is_template: false,
            template_id: Some(template.id),
            created_at: now,
            updated_at: now,
        }
    }
}

/// Request to create a new environment
#[derive(Debug, Clone, Deserialize)]
pub struct EnvironmentCreateRequest {
    pub name: String,
    pub description: Option<String>,
    pub endpoints: Option<HashMap<String, String>>,
    pub template_id: Option<Uuid>,
}

/// Request to update an environment
#[derive(Debug, Clone, Deserialize)]
pub struct EnvironmentUpdateRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub endpoints: Option<HashMap<String, String>>,
}

/// Filter for listing environments
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EnvironmentFilter {
    pub is_template: Option<bool>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

impl EnvironmentFilter {
    pub fn page(&self) -> u32 {
        self.page.unwrap_or(1).max(1)
    }

    pub fn limit(&self) -> u32 {
        self.limit.unwrap_or(20).min(100).max(1)
    }

    pub fn offset(&self) -> u32 {
        (self.page() - 1) * self.limit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_environment_creation() {
        let env = Environment::new("development".to_string());
        assert_eq!(env.name, "development");
        assert!(!env.is_template);
        assert!(env.endpoints.is_empty());
    }

    #[test]
    fn test_environment_from_template() {
        let mut template = Environment::new("template".to_string());
        template.is_template = true;
        template
            .endpoints
            .insert("api".to_string(), "https://api.example.com".to_string());

        let env = Environment::from_template("dev".to_string(), &template);
        assert_eq!(env.name, "dev");
        assert!(!env.is_template);
        assert_eq!(env.template_id, Some(template.id));
        assert_eq!(
            env.endpoints.get("api"),
            Some(&"https://api.example.com".to_string())
        );
    }
}
