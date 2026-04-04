//! HTTP Mocker module
//!
//! Provides mock request and response structures for testing.

use futures::future::Map;

/// HTTP method enum
#[allow(dead_code)]
pub enum Method {
    Get,
    Post,
    Put,
    Options,
    Delete,
}

/// HTTP request structure
#[allow(dead_code)]
pub struct Request {
    method: Method,
    uri: String,
    header: Option<Map<String, String>>,
    body: Option<String>,
}

/// URI structure
#[allow(dead_code)]
pub struct Uri {
    path: String,
    query: Option<Map<String, String>>,
}

/// HTTP response structure
#[allow(dead_code)]
pub struct Response {
    header: Option<Map<String, String>>,
    body: Option<String>,
}
