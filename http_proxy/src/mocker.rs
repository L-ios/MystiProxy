use futures::future::Map;

#[allow(dead_code)]
pub enum Method {
    Get,
    Post,
    Put,
    Options,
    Delete,
}

#[allow(dead_code)]
pub struct Request {
    method: Method,
    uri: String,
    header: Option<Map<String, String>>,
    body: Option<String>,
}

#[allow(dead_code)]
pub struct Uri {
    path: String,
    query: Option<Map<String, String>>,
}

#[allow(dead_code)]
pub struct Response {
    header: Option<Map<String, String>>,
    body: Option<String>,
}
