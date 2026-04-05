//! Authentication Service
//!
//! JWT-based authentication and RBAC authorization using industry-standard libraries.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::models::user::{LoginResponse, User, UserInfo, UserRole};

/// JWT claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid, // User ID
    pub username: String,
    pub role: UserRole,
    pub exp: usize, // Expiration time (Unix timestamp)
    pub iat: usize, // Issued at (Unix timestamp)
}

impl Claims {
    /// Create new claims for a user
    #[allow(dead_code)]
    pub fn new(user: &User, expires_in_hours: i64) -> Self {
        let now = Utc::now();
        let exp = now + Duration::hours(expires_in_hours);

        Self {
            sub: user.id,
            username: user.username.clone(),
            role: user.role,
            exp: exp.timestamp() as usize,
            iat: now.timestamp() as usize,
        }
    }
}

/// Authentication service
#[derive(Clone)]
pub struct AuthService {
    #[allow(dead_code)]
    jwt_secret: String,
    jwt_expiration_hours: i64,
    #[allow(dead_code)]
    encoding_key: EncodingKey,
    #[allow(dead_code)]
    decoding_key: DecodingKey,
}

impl AuthService {
    /// Create a new AuthService
    ///
    /// # Errors
    /// Returns an error if the JWT secret is empty or invalid
    pub fn new(jwt_secret: String, jwt_expiration_hours: i64) -> ApiResult<Self> {
        if jwt_secret.is_empty() {
            return Err(ApiError::Internal(anyhow::anyhow!(
                "JWT secret cannot be empty. Please set MYSTICENTRAL_JWT_SECRET environment variable."
            )));
        }

        if jwt_secret.len() < 32 {
            tracing::warn!(
                "JWT secret is shorter than recommended ({} chars). Consider using a secret with at least 32 characters.",
                jwt_secret.len()
            );
        }

        let encoding_key = EncodingKey::from_secret(jwt_secret.as_bytes());
        let decoding_key = DecodingKey::from_secret(jwt_secret.as_bytes());

        Ok(Self {
            jwt_secret,
            jwt_expiration_hours,
            encoding_key,
            decoding_key,
        })
    }

    /// Generate a JWT token for a user
    #[allow(dead_code)]
    pub fn generate_token(&self, user: &User) -> ApiResult<LoginResponse> {
        let claims = Claims::new(user, self.jwt_expiration_hours);
        let token = encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("Failed to generate token: {}", e)))?;

        Ok(LoginResponse {
            token,
            user: UserInfo::from(user.clone()),
            expires_at: Utc::now() + Duration::hours(self.jwt_expiration_hours),
        })
    }

    /// Validate a JWT token and return claims
    #[allow(dead_code)]
    pub fn validate_token(&self, token: &str) -> ApiResult<Claims> {
        let token_data = decode::<Claims>(token, &self.decoding_key, &Validation::default())
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                    ApiError::Unauthorized("Token has expired".to_string())
                }
                jsonwebtoken::errors::ErrorKind::InvalidToken => {
                    ApiError::Unauthorized("Invalid token".to_string())
                }
                jsonwebtoken::errors::ErrorKind::InvalidSignature => {
                    ApiError::Unauthorized("Invalid token signature".to_string())
                }
                _ => ApiError::Unauthorized(format!("Token validation failed: {}", e)),
            })?;

        Ok(token_data.claims)
    }

    /// Hash a password using Argon2
    ///
    /// Returns a PHC (Password Hashing Competition) format string that includes:
    /// - Algorithm identifier
    /// - Version
    /// - Parameters
    /// - Salt
    /// - Hash
    #[allow(dead_code)]
    pub fn hash_password(password: &str) -> ApiResult<String> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("Failed to hash password: {}", e)))
    }

    /// Verify a password against a hash
    #[allow(dead_code)]
    pub fn verify_password(password: &str, hash: &str) -> bool {
        let parsed_hash = match PasswordHash::new(hash) {
            Ok(h) => h,
            Err(_) => {
                tracing::warn!("Invalid password hash format");
                return false;
            }
        };

        Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok()
    }
}

impl std::fmt::Debug for AuthService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthService")
            .field("jwt_expiration_hours", &self.jwt_expiration_hours)
            .field("jwt_secret", &"[REDACTED]")
            .finish()
    }
}

/// Permission checking for RBAC
#[allow(dead_code)]
pub struct PermissionChecker;

impl PermissionChecker {
    /// Check if a role has permission to perform an action
    #[allow(dead_code)]
    pub fn can(role: UserRole, action: Permission) -> bool {
        match role {
            UserRole::Admin => true, // Admin can do everything
            UserRole::Editor => matches!(
                action,
                Permission::ReadMock
                    | Permission::CreateMock
                    | Permission::UpdateMock
                    | Permission::DeleteMock
                    | Permission::ReadEnvironment
                    | Permission::CreateEnvironment
                    | Permission::UpdateEnvironment
                    | Permission::ExportMocks
                    | Permission::ImportMocks
            ),
            UserRole::Viewer => matches!(
                action,
                Permission::ReadMock | Permission::ReadEnvironment | Permission::ExportMocks
            ),
        }
    }

    /// Check if a user can modify a resource owned by another user
    #[allow(dead_code)]
    pub fn can_modify(actor_role: UserRole, _actor_id: Uuid, _owner_id: Option<Uuid>) -> bool {
        matches!(actor_role, UserRole::Admin | UserRole::Editor)
    }
}

/// Permissions for RBAC
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Permission {
    ReadMock,
    CreateMock,
    UpdateMock,
    DeleteMock,
    ReadEnvironment,
    CreateEnvironment,
    UpdateEnvironment,
    DeleteEnvironment,
    ManageUsers,
    ExportMocks,
    ImportMocks,
    ViewAnalytics,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_hashing() {
        let password = "test_password_123!";
        let hash = AuthService::hash_password(password).unwrap();

        // Hash should be different each time due to random salt
        let hash2 = AuthService::hash_password(password).unwrap();
        assert_ne!(hash, hash2);

        // Both should verify correctly
        assert!(AuthService::verify_password(password, &hash));
        assert!(AuthService::verify_password(password, &hash2));

        // Wrong password should fail
        assert!(!AuthService::verify_password("wrong_password", &hash));
    }

    #[test]
    fn test_password_hash_format() {
        let password = "test";
        let hash = AuthService::hash_password(password).unwrap();

        // Argon2 hashes should start with $argon2
        assert!(hash.starts_with("$argon2"));
    }

    #[test]
    fn test_token_generation_and_validation() {
        let service =
            AuthService::new("test-secret-key-for-testing-min-32-chars".to_string(), 24).unwrap();
        let user = User::new(
            "testuser".to_string(),
            "test@example.com".to_string(),
            "hash".to_string(),
            UserRole::Editor,
        );

        let response = service.generate_token(&user).unwrap();
        let claims = service.validate_token(&response.token).unwrap();

        assert_eq!(claims.sub, user.id);
        assert_eq!(claims.role, UserRole::Editor);
        assert_eq!(claims.username, user.username);
    }

    #[test]
    fn test_invalid_token() {
        let service =
            AuthService::new("test-secret-key-for-testing-min-32-chars".to_string(), 24).unwrap();

        // Invalid token format
        assert!(service.validate_token("invalid.token.here").is_err());

        // Token signed with different secret
        let other_service =
            AuthService::new("different-secret-key-for-testing-32-chars".to_string(), 24).unwrap();
        let user = User::new(
            "testuser".to_string(),
            "test@example.com".to_string(),
            "hash".to_string(),
            UserRole::Viewer,
        );
        let response = other_service.generate_token(&user).unwrap();

        assert!(service.validate_token(&response.token).is_err());
    }

    #[test]
    fn test_permission_checker() {
        assert!(PermissionChecker::can(
            UserRole::Admin,
            Permission::ManageUsers
        ));
        assert!(PermissionChecker::can(
            UserRole::Editor,
            Permission::CreateMock
        ));
        assert!(!PermissionChecker::can(
            UserRole::Editor,
            Permission::ManageUsers
        ));
        assert!(PermissionChecker::can(
            UserRole::Viewer,
            Permission::ReadMock
        ));
        assert!(!PermissionChecker::can(
            UserRole::Viewer,
            Permission::CreateMock
        ));
    }

    #[test]
    fn test_empty_jwt_secret() {
        let result = AuthService::new("".to_string(), 24);
        assert!(result.is_err());
    }

    #[test]
    fn test_claims_creation() {
        let user = User::new(
            "test".to_string(),
            "test@example.com".to_string(),
            "hash".to_string(),
            UserRole::Viewer,
        );

        let claims = Claims::new(&user, 1);
        assert_eq!(claims.sub, user.id);
        assert_eq!(claims.username, user.username);
        assert_eq!(claims.role, UserRole::Viewer);
    }
}
