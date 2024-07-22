use futures::future::Map;

pub enum Method {
    Get,
    Post,
    Put,
    Options,
    Delete,
}

pub struct Request {
    method: Method,
    uri: String,
    header: Option<Map<String, String>>,
    body: Option<String>,
}

pub struct Uri {
    path: String,
    query: Option<Map<String, String>>,
}

pub struct Response {
    header: Option<Map<String, String>>,
    body: Option<String>,
}
