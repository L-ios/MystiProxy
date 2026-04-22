//! HTTP Body 转换模块
//!
//! 提供 JSON Body 的解析、查询和修改功能

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use serde_json::Value;

use crate::config::{BodyConfig, JsonBodyAction, JsonBodyConfig};
use crate::error::{MystiProxyError, Result};

/// Body 转换器
pub struct BodyTransformer;

impl BodyTransformer {
    /// 转换 JSON body
    ///
    /// # 参数
    /// - `body`: JSON 值的可变引用
    /// - `config`: Body 配置
    ///
    /// # 返回
    /// 成功返回 Ok(())，失败返回错误
    pub fn transform(body: &mut Value, config: &BodyConfig) -> Result<()> {
        if let Some(json_config) = &config.json {
            Self::transform_json(body, json_config)?;
        }
        Ok(())
    }

    /// 使用 JSONPath 转换 JSON
    ///
    /// # 参数
    /// - `body`: JSON 值的可变引用
    /// - `config`: JSON Body 配置
    ///
    /// # 返回
    /// 成功返回 Ok(())，失败返回错误
    fn transform_json(body: &mut Value, config: &JsonBodyConfig) -> Result<()> {
        // 解析要设置的值
        let new_value = Self::parse_value(&config.value)?;

        match config.action {
            JsonBodyAction::Overwrite => {
                // 覆盖值
                Self::apply_overwrite(body, &config.path, new_value)?;
            }
            JsonBodyAction::Add => {
                // 添加值
                Self::apply_add(body, &config.path, new_value)?;
            }
            JsonBodyAction::Delete => {
                // 删除值
                Self::apply_delete(body, &config.path)?;
            }
        }

        Ok(())
    }

    /// 解析值字符串为 JSON Value
    ///
    /// 尝试将字符串解析为 JSON，如果失败则作为普通字符串处理
    fn parse_value(value_str: &str) -> Result<Value> {
        // 首先尝试解析为 JSON
        if let Ok(value) = serde_json::from_str::<Value>(value_str) {
            return Ok(value);
        }
        // 如果解析失败，作为字符串处理
        Ok(Value::String(value_str.to_string()))
    }

    /// 应用覆盖操作
    ///
    /// 使用路径查找并覆盖值
    fn apply_overwrite(body: &mut Value, path: &str, new_value: Value) -> Result<()> {
        Self::set_value_by_path(body, path, new_value)?;
        Ok(())
    }

    /// 应用添加操作
    ///
    /// 向数组添加元素或向对象添加字段
    fn apply_add(body: &mut Value, path: &str, new_value: Value) -> Result<()> {
        // 如果路径不存在，创建新字段
        Self::set_value_by_path(body, path, new_value)?;
        Ok(())
    }

    /// 应用删除操作
    ///
    /// 删除路径匹配的值
    fn apply_delete(body: &mut Value, path: &str) -> Result<()> {
        Self::delete_value_by_path(body, path)?;
        Ok(())
    }

    /// 根据路径设置值
    fn set_value_by_path(body: &mut Value, path: &str, new_value: Value) -> Result<()> {
        // 简化实现：支持基本的点号路径
        if path == "$" {
            *body = new_value;
            return Ok(());
        }

        let path = path
            .strip_prefix("$.")
            .ok_or_else(|| MystiProxyError::JsonPath("Path must start with $.".to_string()))?;

        let parts: Vec<&str> = path.split('.').collect();
        Self::set_nested_value(body, &parts, new_value)
    }

    /// 设置嵌套值
    fn set_nested_value(body: &mut Value, parts: &[&str], new_value: Value) -> Result<()> {
        if parts.is_empty() {
            return Ok(());
        }

        if parts.len() == 1 {
            // 最后一层，直接设置值
            let part = parts[0];

            // 处理数组索引
            if part.contains('[') {
                Self::set_array_value(body, part, new_value)?;
            } else if let Some(obj) = body.as_object_mut() {
                obj.insert(part.to_string(), new_value);
            } else {
                return Err(MystiProxyError::JsonPath(
                    "Cannot set field on non-object value".to_string(),
                ));
            }

            return Ok(());
        }

        // 多层路径，递归处理
        let part = parts[0];
        let remaining = &parts[1..];

        // 处理数组索引
        if part.contains('[') {
            let current = body;
            Self::navigate_and_set_array(current, part, remaining, new_value)?;
        } else if let Some(obj) = body.as_object_mut() {
            if !obj.contains_key(part) {
                // 如果字段不存在，创建一个新的对象
                obj.insert(part.to_string(), Value::Object(serde_json::Map::new()));
            }
            if let Some(next_body) = obj.get_mut(part) {
                Self::set_nested_value(next_body, remaining, new_value)?;
            }
        } else {
            return Err(MystiProxyError::JsonPath(
                "Cannot navigate through non-object value".to_string(),
            ));
        }

        Ok(())
    }

    /// 设置数组元素值
    fn set_array_value(body: &mut Value, part: &str, new_value: Value) -> Result<()> {
        let idx_parts: Vec<&str> = part.split('[').collect();
        let field = idx_parts[0];

        let mut current = body;

        // 如果有字段名，先导航到字段
        if !field.is_empty() {
            if let Some(obj) = current.as_object_mut() {
                if !obj.contains_key(field) {
                    obj.insert(field.to_string(), Value::Array(Vec::new()));
                }
                current = obj.get_mut(field).ok_or_else(|| {
                    MystiProxyError::JsonPath(format!("Field '{field}' not found"))
                })?;
            } else {
                return Err(MystiProxyError::JsonPath(
                    "Cannot access field on non-object value".to_string(),
                ));
            }
        }

        // 处理数组索引
        for idx_part in idx_parts.iter().skip(1) {
            if let Some(idx_str) = idx_part.strip_suffix(']') {
                let idx: usize = idx_str.parse().map_err(|_| {
                    MystiProxyError::JsonPath(format!("Invalid array index: {idx_str}"))
                })?;

                if let Some(arr) = current.as_array_mut() {
                    if idx < arr.len() {
                        current = &mut arr[idx];
                    } else {
                        return Err(MystiProxyError::JsonPath(format!(
                            "Array index {idx} out of bounds"
                        )));
                    }
                } else {
                    return Err(MystiProxyError::JsonPath(
                        "Cannot index non-array value".to_string(),
                    ));
                }
            }
        }

        *current = new_value;
        Ok(())
    }

    /// 导航并设置数组中的值
    fn navigate_and_set_array(
        body: &mut Value,
        part: &str,
        remaining: &[&str],
        new_value: Value,
    ) -> Result<()> {
        let idx_parts: Vec<&str> = part.split('[').collect();
        let field = idx_parts[0];

        let mut current = body;

        // 如果有字段名，先导航到字段
        if !field.is_empty() {
            if let Some(obj) = current.as_object_mut() {
                if !obj.contains_key(field) {
                    obj.insert(field.to_string(), Value::Array(Vec::new()));
                }
                current = obj.get_mut(field).ok_or_else(|| {
                    MystiProxyError::JsonPath(format!("Field '{field}' not found"))
                })?;
            } else {
                return Err(MystiProxyError::JsonPath(
                    "Cannot access field on non-object value".to_string(),
                ));
            }
        }

        // 处理数组索引
        for idx_part in idx_parts.iter().skip(1) {
            if let Some(idx_str) = idx_part.strip_suffix(']') {
                let idx: usize = idx_str.parse().map_err(|_| {
                    MystiProxyError::JsonPath(format!("Invalid array index: {idx_str}"))
                })?;

                if let Some(arr) = current.as_array_mut() {
                    if idx < arr.len() {
                        current = &mut arr[idx];
                    } else {
                        return Err(MystiProxyError::JsonPath(format!(
                            "Array index {idx} out of bounds"
                        )));
                    }
                } else {
                    return Err(MystiProxyError::JsonPath(
                        "Cannot index non-array value".to_string(),
                    ));
                }
            }
        }

        // 递归处理剩余路径
        Self::set_nested_value(current, remaining, new_value)?;
        Ok(())
    }

    /// 根据路径删除值
    fn delete_value_by_path(body: &mut Value, path: &str) -> Result<()> {
        if path == "$" {
            // 删除根节点，设置为 null
            *body = Value::Null;
            return Ok(());
        }

        let path = path
            .strip_prefix("$.")
            .ok_or_else(|| MystiProxyError::JsonPath("Path must start with $.".to_string()))?;

        let parts: Vec<&str> = path.split('.').collect();

        if parts.is_empty() {
            return Ok(());
        }

        // 找到父节点并删除
        if parts.len() == 1 {
            let part = parts[0];

            // 处理数组索引
            if part.contains('[') {
                Self::delete_array_element(body, part)?;
            } else if let Some(obj) = body.as_object_mut() {
                obj.remove(part);
            }
            return Ok(());
        }

        // 多层路径，先找到父节点
        let parent_parts = &parts[..parts.len() - 1];
        let last_part = parts[parts.len() - 1];

        let mut current = body;
        for part in parent_parts {
            if part.contains('[') {
                current = Self::navigate_array(current, part)?;
            } else if let Some(obj) = current.as_object_mut() {
                current = obj.get_mut(*part).ok_or_else(|| {
                    MystiProxyError::JsonPath(format!("Field '{part}' not found"))
                })?;
            } else {
                return Err(MystiProxyError::JsonPath(
                    "Cannot navigate through non-object value".to_string(),
                ));
            }
        }

        // 删除最后一个字段或数组元素
        if last_part.contains('[') {
            Self::delete_array_element(current, last_part)?;
        } else if let Some(obj) = current.as_object_mut() {
            obj.remove(last_part);
        }

        Ok(())
    }

    /// 导航到数组元素
    fn navigate_array<'a>(body: &'a mut Value, part: &str) -> Result<&'a mut Value> {
        let idx_parts: Vec<&str> = part.split('[').collect();
        let field = idx_parts[0];

        let mut current = body;

        // 如果有字段名，先导航到字段
        if !field.is_empty() {
            if let Some(obj) = current.as_object_mut() {
                current = obj.get_mut(field).ok_or_else(|| {
                    MystiProxyError::JsonPath(format!("Field '{field}' not found"))
                })?;
            } else {
                return Err(MystiProxyError::JsonPath(
                    "Cannot access field on non-object value".to_string(),
                ));
            }
        }

        // 处理数组索引
        for idx_part in idx_parts.iter().skip(1) {
            if let Some(idx_str) = idx_part.strip_suffix(']') {
                let idx: usize = idx_str.parse().map_err(|_| {
                    MystiProxyError::JsonPath(format!("Invalid array index: {idx_str}"))
                })?;

                if let Some(arr) = current.as_array_mut() {
                    if idx < arr.len() {
                        current = &mut arr[idx];
                    } else {
                        return Err(MystiProxyError::JsonPath(format!(
                            "Array index {idx} out of bounds"
                        )));
                    }
                } else {
                    return Err(MystiProxyError::JsonPath(
                        "Cannot index non-array value".to_string(),
                    ));
                }
            }
        }

        Ok(current)
    }

    /// 删除数组元素
    fn delete_array_element(body: &mut Value, part: &str) -> Result<()> {
        let idx_parts: Vec<&str> = part.split('[').collect();
        let field = idx_parts[0];

        let mut current = body;

        // 如果有字段名，先导航到字段
        if !field.is_empty() {
            if let Some(obj) = current.as_object_mut() {
                current = obj.get_mut(field).ok_or_else(|| {
                    MystiProxyError::JsonPath(format!("Field '{field}' not found"))
                })?;
            } else {
                return Err(MystiProxyError::JsonPath(
                    "Cannot access field on non-object value".to_string(),
                ));
            }
        }

        // 处理数组索引
        for idx_part in idx_parts.iter().skip(1) {
            if let Some(idx_str) = idx_part.strip_suffix(']') {
                let idx: usize = idx_str.parse().map_err(|_| {
                    MystiProxyError::JsonPath(format!("Invalid array index: {idx_str}"))
                })?;

                if let Some(arr) = current.as_array_mut() {
                    if idx < arr.len() {
                        arr.remove(idx);
                    }
                } else {
                    return Err(MystiProxyError::JsonPath(
                        "Cannot index non-array value".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }
}

/// 从 hyper body 读取 JSON
///
/// # 参数
/// - `body`: Hyper Incoming body
///
/// # 返回
/// 成功返回 JSON Value，失败返回错误
pub async fn read_json_body(body: Incoming) -> Result<Value> {
    let bytes = body
        .collect()
        .await
        .map_err(|e| MystiProxyError::Hyper(e.to_string()))?
        .to_bytes();

    if bytes.is_empty() {
        return Ok(Value::Null);
    }

    let value: Value = serde_json::from_slice(&bytes)?;
    Ok(value)
}

/// 将 JSON 写入 body
///
/// # 参数
/// - `value`: JSON 值引用
///
/// # 返回
/// 返回 BoxBody
pub fn write_json_body(value: &Value) -> super::BoxBody {
    let json_str = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    let bytes = Bytes::from(json_str);

    Full::new(bytes).map_err(|never| match never {}).boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_transform_overwrite() {
        let mut body = json!({
            "name": "old",
            "age": 20
        });

        let config = BodyConfig {
            json: Some(JsonBodyConfig {
                path: "$.name".to_string(),
                value: "new".to_string(),
                action: JsonBodyAction::Overwrite,
            }),
            body_type: None,
        };

        BodyTransformer::transform(&mut body, &config).unwrap();
        assert_eq!(body["name"], "new");
        assert_eq!(body["age"], 20);
    }

    #[test]
    fn test_transform_nested_overwrite() {
        let mut body = json!({
            "user": {
                "name": "old",
                "email": "old@example.com"
            }
        });

        let config = BodyConfig {
            json: Some(JsonBodyConfig {
                path: "$.user.name".to_string(),
                value: "new".to_string(),
                action: JsonBodyAction::Overwrite,
            }),
            body_type: None,
        };

        BodyTransformer::transform(&mut body, &config).unwrap();
        assert_eq!(body["user"]["name"], "new");
        assert_eq!(body["user"]["email"], "old@example.com");
    }

    #[test]
    fn test_transform_delete() {
        let mut body = json!({
            "name": "test",
            "age": 20
        });

        let config = BodyConfig {
            json: Some(JsonBodyConfig {
                path: "$.age".to_string(),
                value: String::new(),
                action: JsonBodyAction::Delete,
            }),
            body_type: None,
        };

        BodyTransformer::transform(&mut body, &config).unwrap();
        assert_eq!(body["name"], "test");
        assert!(body.get("age").is_none());
    }

    #[test]
    fn test_transform_json_value() {
        let mut body = json!({
            "data": {}
        });

        let config = BodyConfig {
            json: Some(JsonBodyConfig {
                path: "$.data".to_string(),
                value: r#"{"key": "value"}"#.to_string(),
                action: JsonBodyAction::Overwrite,
            }),
            body_type: None,
        };

        BodyTransformer::transform(&mut body, &config).unwrap();
        assert_eq!(body["data"]["key"], "value");
    }

    #[test]
    fn test_parse_value() {
        // 测试 JSON 对象
        let value = BodyTransformer::parse_value(r#"{"key": "value"}"#).unwrap();
        assert_eq!(value["key"], "value");

        // 测试 JSON 数组
        let value = BodyTransformer::parse_value(r#"[1, 2, 3]"#).unwrap();
        assert_eq!(value.as_array().unwrap().len(), 3);

        // 测试普通字符串
        let value = BodyTransformer::parse_value("just a string").unwrap();
        assert_eq!(value, "just a string");

        // 测试数字
        let value = BodyTransformer::parse_value("42").unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn test_set_nested_value() {
        let mut body = json!({});

        BodyTransformer::set_nested_value(&mut body, &["user", "name"], json!("test")).unwrap();
        assert_eq!(body["user"]["name"], "test");

        BodyTransformer::set_nested_value(&mut body, &["user", "age"], json!(25)).unwrap();
        assert_eq!(body["user"]["age"], 25);
    }

    #[test]
    fn test_delete_nested_value() {
        let mut body = json!({
            "user": {
                "name": "test",
                "age": 25
            }
        });

        BodyTransformer::delete_value_by_path(&mut body, "$.user.age").unwrap();
        assert!(body["user"].get("age").is_none());
        assert_eq!(body["user"]["name"], "test");
    }

    #[test]
    fn test_array_operations() {
        let mut body = json!({
            "items": [1, 2, 3]
        });

        // 修改数组元素
        let config = BodyConfig {
            json: Some(JsonBodyConfig {
                path: "$.items[0]".to_string(),
                value: "10".to_string(),
                action: JsonBodyAction::Overwrite,
            }),
            body_type: None,
        };

        BodyTransformer::transform(&mut body, &config).unwrap();
        assert_eq!(body["items"][0], 10);
    }

    #[test]
    fn test_create_nested_path() {
        let mut body = json!({});

        // 创建不存在的嵌套路径
        let config = BodyConfig {
            json: Some(JsonBodyConfig {
                path: "$.user.profile.name".to_string(),
                value: "test".to_string(),
                action: JsonBodyAction::Overwrite,
            }),
            body_type: None,
        };

        BodyTransformer::transform(&mut body, &config).unwrap();
        assert_eq!(body["user"]["profile"]["name"], "test");
    }
}
