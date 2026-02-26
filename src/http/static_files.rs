//! 静态文件服务模块
//!
//! 提供静态文件服务功能，包括目录映射、文件读取和 MIME 类型检测

use std::path::{Path, PathBuf};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{header, Response, StatusCode};
use tracing::{debug, warn};

use crate::error::{MystiProxyError, Result};
use crate::http::handler::BoxBody;

/// 静态文件服务配置
#[derive(Debug, Clone)]
pub struct StaticFileConfig {
    /// 根目录
    pub root: PathBuf,
    /// 索引文件列表
    pub index_files: Vec<String>,
    /// 是否启用目录列表
    pub enable_directory_listing: bool,
}

impl Default for StaticFileConfig {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            index_files: vec!["index.html".to_string(), "index.htm".to_string()],
            enable_directory_listing: false,
        }
    }
}

/// 静态文件服务
#[derive(Debug, Clone)]
pub struct StaticFileService {
    /// 配置
    config: StaticFileConfig,
}

impl StaticFileService {
    /// 创建新的静态文件服务
    pub fn new(root: PathBuf) -> Self {
        Self {
            config: StaticFileConfig {
                root,
                ..Default::default()
            },
        }
    }

    /// 使用配置创建静态文件服务
    pub fn with_config(config: StaticFileConfig) -> Self {
        Self { config }
    }

    /// 获取根目录
    pub fn root(&self) -> &Path {
        &self.config.root
    }

    /// 将 URI 转换为文件路径
    ///
    /// # 参数
    /// - `uri`: 请求的 URI 路径
    ///
    /// # 返回
    /// 返回对应的文件系统路径，如果是目录则查找索引文件
    pub fn uri_to_path(&self, uri: &str) -> PathBuf {
        // 移除查询字符串
        let path = uri.split('?').next().unwrap_or(uri);

        // 解码 URL 编码（简单处理）
        let decoded_path = url_decode(path);

        // 移除开头的斜杠并构建完整路径
        let relative_path = decoded_path.trim_start_matches('/');
        let mut full_path = self.config.root.join(relative_path);

        // 规范化路径
        full_path = canonicalize_path(&full_path);

        // 如果是目录，查找索引文件
        if full_path.is_dir() {
            for index in &self.config.index_files {
                let index_path = full_path.join(index);
                if index_path.exists() && index_path.is_file() {
                    debug!("Found index file: {:?}", index_path);
                    return index_path;
                }
            }
        }

        full_path
    }

    /// 提供静态文件服务
    ///
    /// # 参数
    /// - `uri`: 请求的 URI 路径
    ///
    /// # 返回
    /// 返回 HTTP 响应
    pub async fn serve(&self, uri: &str) -> Result<Response<BoxBody>> {
        self.serve_with_range(uri, None).await
    }

    /// 提供静态文件服务（支持范围请求）
    ///
    /// # 参数
    /// - `uri`: 请求的 URI 路径
    /// - `range_header`: 可选的 Range 头部
    ///
    /// # 返回
    /// 返回 HTTP 响应
    pub async fn serve_with_range(
        &self,
        uri: &str,
        range_header: Option<&str>,
    ) -> Result<Response<BoxBody>> {
        self.serve_internal(uri, range_header).await
    }

    /// 内部实现：提供静态文件服务
    async fn serve_internal(
        &self,
        uri: &str,
        range_header: Option<&str>,
    ) -> Result<Response<BoxBody>> {
        let path = self.uri_to_path(uri);

        // 安全检查：防止路径遍历攻击
        let canonical_root = self.config.root.canonicalize().map_err(|e| {
            MystiProxyError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Root directory not found: {}", e),
            ))
        })?;

        let canonical_path = match path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                warn!("File not found: {:?}, error: {}", path, e);
                return self.serve_not_found();
            }
        };

        // 检查路径是否在根目录下
        if !canonical_path.starts_with(&canonical_root) {
            warn!("Path traversal detected: {:?}", canonical_path);
            return self.serve_forbidden();
        }

        // 检查文件是否存在
        if !canonical_path.exists() {
            return self.serve_not_found();
        }

        // 检查是否为文件
        if !canonical_path.is_file() {
            if canonical_path.is_dir() {
                // 目录但没有索引文件
                if self.config.enable_directory_listing {
                    return self.serve_directory_listing(&canonical_path, uri).await;
                } else {
                    return self.serve_forbidden();
                }
            } else {
                return self.serve_not_found();
            }
        }

        // 获取文件元数据
        let metadata = tokio::fs::metadata(&canonical_path).await?;
        let file_size = metadata.len();

        // 确定 MIME 类型
        let mime_type = self.get_mime_type(&canonical_path);

        // 处理范围请求
        if let Some(range) = range_header {
            return self.serve_range(&canonical_path, range, file_size, mime_type).await;
        }

        // 读取整个文件
        let content = tokio::fs::read(&canonical_path).await?;

        debug!(
            "Serving file: {:?}, size: {} bytes, mime: {}",
            canonical_path,
            content.len(),
            mime_type
        );

        // 构建响应
        let response = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime_type)
            .header(header::CONTENT_LENGTH, content.len())
            .header(header::ACCEPT_RANGES, "bytes")
            .body(Self::full_body(Bytes::from(content)))
            .map_err(MystiProxyError::Http)?;

        Ok(response)
    }

    /// 处理范围请求
    fn serve_range<'a>(
        &'a self,
        path: &'a Path,
        range_header: &'a str,
        file_size: u64,
        mime_type: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response<BoxBody>>> + 'a>> {
        Box::pin(async move {
            // 解析 Range 头部
            let ranges = parse_range_header(range_header, file_size)?;

            if ranges.is_empty() {
                // 无效的范围请求，返回整个文件
                let content = tokio::fs::read(path).await?;

                debug!("Serving full file due to invalid range: {} bytes", content.len());

                let response = Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, mime_type)
                    .header(header::CONTENT_LENGTH, content.len())
                    .header(header::ACCEPT_RANGES, "bytes")
                    .body(Self::full_body(Bytes::from(content)))
                    .map_err(MystiProxyError::Http)?;

                return Ok(response);
            }

            // 目前只支持单个范围请求
            let (start, end) = ranges[0];
            let content_length = end - start + 1;

            // 读取指定范围的数据
            let content = read_file_range(path, start, end).await?;

            debug!(
                "Serving range: {}-{} / {} bytes",
                start, end, file_size
            );

            // 构建响应
            let response = Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, mime_type)
                .header(header::CONTENT_LENGTH, content_length)
                .header(
                    header::CONTENT_RANGE,
                    format!("bytes {}-{}/{}", start, end, file_size),
                )
                .header(header::ACCEPT_RANGES, "bytes")
                .body(Self::full_body(Bytes::from(content)))
                .map_err(MystiProxyError::Http)?;

            Ok(response)
        })
    }

    /// 提供目录列表
    async fn serve_directory_listing(
        &self,
        dir_path: &Path,
        uri: &str,
    ) -> Result<Response<BoxBody>> {
        let mut entries = tokio::fs::read_dir(dir_path).await?;
        let mut html = String::new();

        html.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
        html.push_str("<meta charset=\"utf-8\">\n");
        html.push_str(&format!("<title>Index of {}</title>\n", uri));
        html.push_str("</head>\n<body>\n");
        html.push_str(&format!("<h1>Index of {}</h1>\n", uri));
        html.push_str("<hr>\n<ul>\n");

        // 添加父目录链接
        if uri != "/" {
            html.push_str("<li><a href=\"../\">../</a></li>\n");
        }

        // 读取目录条目
        let mut dirs = Vec::new();
        let mut files = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().await?.is_dir();

            if is_dir {
                dirs.push(name);
            } else {
                files.push(name);
            }
        }

        // 排序
        dirs.sort();
        files.sort();

        // 添加目录
        for dir in dirs {
            html.push_str(&format!("<li><a href=\"{}/\">{}/</a></li>\n", dir, dir));
        }

        // 添加文件
        for file in files {
            html.push_str(&format!("<li><a href=\"{}\">{}</a></li>\n", file, file));
        }

        html.push_str("</ul>\n<hr>\n</body>\n</html>");

        let response = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .header(header::CONTENT_LENGTH, html.len())
            .body(Self::full_body(Bytes::from(html)))
            .map_err(MystiProxyError::Http)?;

        Ok(response)
    }

    /// 返回 404 响应
    fn serve_not_found(&self) -> Result<Response<BoxBody>> {
        let body = b"<html><body><h1>404 Not Found</h1></body></html>";
        let response = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "text/html")
            .header(header::CONTENT_LENGTH, body.len())
            .body(Self::full_body(Bytes::from_static(body)))
            .map_err(MystiProxyError::Http)?;
        Ok(response)
    }

    /// 返回 403 响应
    fn serve_forbidden(&self) -> Result<Response<BoxBody>> {
        let body = b"<html><body><h1>403 Forbidden</h1></body></html>";
        let response = Response::builder()
            .status(StatusCode::FORBIDDEN)
            .header(header::CONTENT_TYPE, "text/html")
            .header(header::CONTENT_LENGTH, body.len())
            .body(Self::full_body(Bytes::from_static(body)))
            .map_err(MystiProxyError::Http)?;
        Ok(response)
    }

    /// 获取 MIME 类型
    ///
    /// 根据文件扩展名确定 Content-Type
    fn get_mime_type(&self, path: &Path) -> &'static str {
        match path.extension().and_then(|e| e.to_str()) {
            Some("html") | Some("htm") => "text/html; charset=utf-8",
            Some("css") => "text/css; charset=utf-8",
            Some("js") => "application/javascript",
            Some("json") => "application/json",
            Some("xml") => "application/xml",
            Some("txt") => "text/plain; charset=utf-8",
            Some("png") => "image/png",
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("gif") => "image/gif",
            Some("svg") => "image/svg+xml",
            Some("ico") => "image/x-icon",
            Some("webp") => "image/webp",
            Some("woff") => "font/woff",
            Some("woff2") => "font/woff2",
            Some("ttf") => "font/ttf",
            Some("otf") => "font/otf",
            Some("eot") => "application/vnd.ms-fontobject",
            Some("pdf") => "application/pdf",
            Some("zip") => "application/zip",
            Some("tar") => "application/x-tar",
            Some("gz") => "application/gzip",
            Some("mp3") => "audio/mpeg",
            Some("mp4") => "video/mp4",
            Some("webm") => "video/webm",
            Some("avi") => "video/x-msvideo",
            Some("doc") => "application/msword",
            Some("docx") => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            Some("xls") => "application/vnd.ms-excel",
            Some("xlsx") => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            Some("ppt") => "application/vnd.ms-powerpoint",
            Some("pptx") => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            _ => "application/octet-stream",
        }
    }

    /// 创建完整响应体
    fn full_body(bytes: Bytes) -> BoxBody {
        Full::new(bytes)
            .map_err(|never| match never {})
            .boxed()
    }
}

/// URL 解码（简单实现）
fn url_decode(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            } else {
                result.push('%');
                result.push_str(&hex);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }

    result
}

/// 规范化路径（移除 `.` 和 `..`）
fn canonicalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            std::path::Component::CurDir => {
                // 忽略 `.`
            }
            std::path::Component::ParentDir => {
                // 回退一级
                if !components.is_empty() {
                    components.pop();
                }
            }
            _ => {
                components.push(component);
            }
        }
    }

    components.iter().collect()
}

/// 解析 Range 头部
///
/// 格式: bytes=start-end 或 bytes=start- 或 bytes=-end
fn parse_range_header(range_header: &str, file_size: u64) -> Result<Vec<(u64, u64)>> {
    let range_header = range_header.trim();

    // 检查是否以 "bytes=" 开头
    if !range_header.starts_with("bytes=") {
        return Err(MystiProxyError::Other("Invalid range header".to_string()));
    }

    let range_spec = &range_header[6..]; // 移除 "bytes="
    let mut ranges = Vec::new();

    for part in range_spec.split(',') {
        let part = part.trim();

        if part.contains('-') {
            let parts: Vec<&str> = part.split('-').collect();

            if parts.len() != 2 {
                continue;
            }

            let start = if parts[0].is_empty() {
                // 格式: -end (最后 end 个字节)
                None
            } else {
                Some(parts[0].parse::<u64>().map_err(|_| {
                    MystiProxyError::Other(format!("Invalid range start: {}", parts[0]))
                })?)
            };

            let end = if parts[1].is_empty() {
                // 格式: start- (从 start 到文件末尾)
                None
            } else {
                Some(parts[1].parse::<u64>().map_err(|_| {
                    MystiProxyError::Other(format!("Invalid range end: {}", parts[1]))
                })?)
            };

            let (range_start, range_end) = match (start, end) {
                (Some(s), Some(e)) => {
                    // bytes=start-end
                    if s > e || s >= file_size {
                        continue;
                    }
                    (s, e.min(file_size - 1))
                }
                (Some(s), None) => {
                    // bytes=start-
                    if s >= file_size {
                        continue;
                    }
                    (s, file_size - 1)
                }
                (None, Some(e)) => {
                    // bytes=-end (最后 end 个字节)
                    if e == 0 {
                        continue;
                    }
                    let start = file_size.saturating_sub(e);
                    (start, file_size - 1)
                }
                (None, None) => {
                    continue;
                }
            };

            ranges.push((range_start, range_end));
        }
    }

    Ok(ranges)
}

/// 读取文件的指定范围
async fn read_file_range(path: &Path, start: u64, end: u64) -> Result<Vec<u8>> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    let mut file = tokio::fs::File::open(path).await?;

    // 定位到起始位置
    file.seek(std::io::SeekFrom::Start(start)).await?;

    // 读取指定长度的数据
    let length = (end - start + 1) as usize;
    let mut buffer = vec![0u8; length];
    file.read_exact(&mut buffer).await?;

    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_url_decode() {
        assert_eq!(url_decode("hello%20world"), "hello world");
        assert_eq!(url_decode("test%2Fpath"), "test/path");
        assert_eq!(url_decode("a+b"), "a b");
    }

    #[test]
    fn test_canonicalize_path() {
        let path = PathBuf::from("/var/www/../html/./index.html");
        let canonical = canonicalize_path(&path);
        assert_eq!(canonical, PathBuf::from("/var/html/index.html"));

        // 测试连续的 ..
        let path = PathBuf::from("/var/www/../../html/index.html");
        let canonical = canonicalize_path(&path);
        assert_eq!(canonical, PathBuf::from("/html/index.html"));
    }

    #[test]
    fn test_parse_range_header() {
        let file_size = 1000;

        // bytes=0-499
        let ranges = parse_range_header("bytes=0-499", file_size).unwrap();
        assert_eq!(ranges, vec![(0, 499)]);

        // bytes=500-
        let ranges = parse_range_header("bytes=500-", file_size).unwrap();
        assert_eq!(ranges, vec![(500, 999)]);

        // bytes=-500
        let ranges = parse_range_header("bytes=-500", file_size).unwrap();
        assert_eq!(ranges, vec![(500, 999)]);

        // bytes=0-0
        let ranges = parse_range_header("bytes=0-0", file_size).unwrap();
        assert_eq!(ranges, vec![(0, 0)]);
    }

    #[test]
    fn test_get_mime_type() {
        let service = StaticFileService::new(PathBuf::from("."));

        assert_eq!(
            service.get_mime_type(Path::new("index.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(service.get_mime_type(Path::new("style.css")), "text/css; charset=utf-8");
        assert_eq!(
            service.get_mime_type(Path::new("app.js")),
            "application/javascript"
        );
        assert_eq!(service.get_mime_type(Path::new("image.png")), "image/png");
        assert_eq!(
            service.get_mime_type(Path::new("unknown.xyz")),
            "application/octet-stream"
        );
    }

    #[tokio::test]
    async fn test_uri_to_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();

        // 创建测试文件
        fs::write(root.join("index.html"), "<html></html>").unwrap();
        fs::create_dir_all(root.join("subdir")).unwrap();
        fs::write(root.join("subdir/index.htm"), "<html></html>").unwrap();

        let service = StaticFileService::new(root.clone());

        // 测试根目录索引文件
        let path = service.uri_to_path("/");
        assert!(path.ends_with("index.html"));

        // 测试子目录索引文件
        let path = service.uri_to_path("/subdir/");
        assert!(path.ends_with("index.htm"));

        // 测试普通文件
        let path = service.uri_to_path("/test.txt");
        assert_eq!(path, root.join("test.txt"));
    }
}
