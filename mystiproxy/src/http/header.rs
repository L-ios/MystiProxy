//! Header 转换模块
//!
//! 提供 HTTP header 的转换和处理功能

use crate::config::{HeaderAction, HeaderActionType};
use crate::Result;
use hyper::header::{HeaderName, HeaderMap};
use std::collections::HashMap;

/// Header 转换器
///
/// 负责根据配置的规则对 HTTP headers 进行转换操作
pub struct HeaderTransformer {
    /// Header 动作映射，key 为 header 名称
    actions: HashMap<String, HeaderAction>,
}

impl HeaderTransformer {
    /// 创建新的 Header 转换器
    ///
    /// # 参数
    /// - `actions`: Header 动作映射
    ///
    /// # 返回
    /// 返回新创建的 HeaderTransformer 实例
    pub fn new(actions: HashMap<String, HeaderAction>) -> Self {
        Self { actions }
    }

    /// 应用所有 header 转换
    ///
    /// 遍历所有配置的 header 动作并依次应用
    ///
    /// # 参数
    /// - `headers`: 需要修改的 HeaderMap
    ///
    /// # 返回
    /// 成功返回 Ok(())，失败返回错误信息
    pub fn apply(&self, headers: &mut HeaderMap) -> Result<()> {
        for (name, action) in &self.actions {
            self.apply_action(headers, name, action)?;
        }
        Ok(())
    }

    /// 应用单个 header 动作
    ///
    /// 根据条件判断是否执行动作，并调用相应的动作类型进行处理
    ///
    /// # 参数
    /// - `headers`: 需要修改的 HeaderMap
    /// - `name`: Header 名称
    /// - `action`: Header 动作配置
    ///
    /// # 返回
    /// 成功返回 Ok(())，失败返回错误信息
    fn apply_action(
        &self,
        headers: &mut HeaderMap,
        name: &str,
        action: &HeaderAction,
    ) -> Result<()> {
        // 评估条件，如果条件不满足则跳过
        if !self.evaluate_condition(&action.condition, headers) {
            return Ok(());
        }

        // 应用动作
        action.action.apply(headers, name, &action.value);
        Ok(())
    }

    /// 评估条件表达式
    ///
    /// 支持简单的条件判断：
    /// - 条件为空或 None 时默认返回 true
    /// - 条件为非空字符串时，检查 header 是否存在该值
    ///
    /// # 参数
    /// - `condition`: 条件表达式（可选）
    /// - `headers`: 当前 headers
    ///
    /// # 返回
    /// 条件满足返回 true，否则返回 false
    fn evaluate_condition(&self, condition: &Option<String>, headers: &HeaderMap) -> bool {
        match condition {
            None => true,
            Some(cond) if cond.is_empty() => true,
            Some(cond) => {
                // 简单条件判断：检查是否存在指定格式的条件
                // 格式：header_name=value 或 header_name
                if let Some(eq_pos) = cond.find('=') {
                    // 格式：header_name=value
                    let header_name = &cond[..eq_pos];
                    let expected_value = &cond[eq_pos + 1..];
                    
                    headers
                        .get(header_name)
                        .map(|v| v.to_str().unwrap_or("") == expected_value)
                        .unwrap_or(false)
                } else {
                    // 格式：header_name（仅检查是否存在）
                    headers.contains_key(cond)
                }
            }
        }
    }
}

impl HeaderActionType {
    /// 应用 header 动作
    ///
    /// 根据动作类型对 header 进行相应的操作
    ///
    /// # 参数
    /// - `headers`: 需要修改的 HeaderMap
    /// - `name`: Header 名称
    /// - `value`: Header 值
    pub fn apply(&self, headers: &mut HeaderMap, name: &str, value: &str) {
        // 尝试将 name 解析为 HeaderName
        let header_name = match name.parse::<HeaderName>() {
            Ok(n) => n,
            Err(_) => return, // 无效的 header 名称，直接返回
        };

        match self {
            HeaderActionType::Overwrite => {
                // 强制覆盖 header
                if let Ok(header_value) = value.parse() {
                    headers.insert(&header_name, header_value);
                }
            }
            HeaderActionType::Missed => {
                // 仅在 header 不存在时添加
                if !headers.contains_key(&header_name) {
                    if let Ok(header_value) = value.parse() {
                        headers.insert(&header_name, header_value);
                    }
                }
            }
            HeaderActionType::ForceDelete => {
                // 强制删除 header
                headers.remove(&header_name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_transformer_overwrite() {
        let mut actions = HashMap::new();
        actions.insert(
            "Host".to_string(),
            HeaderAction {
                value: "localhost".to_string(),
                action: HeaderActionType::Overwrite,
                condition: None,
            },
        );

        let transformer = HeaderTransformer::new(actions);
        let mut headers = HeaderMap::new();
        headers.insert("Host", "example.com".parse().unwrap());

        transformer.apply(&mut headers).unwrap();

        assert_eq!(headers.get("Host").unwrap().to_str().unwrap(), "localhost");
    }

    #[test]
    fn test_header_transformer_missed() {
        let mut actions = HashMap::new();
        actions.insert(
            "X-Custom".to_string(),
            HeaderAction {
                value: "custom-value".to_string(),
                action: HeaderActionType::Missed,
                condition: None,
            },
        );

        let transformer = HeaderTransformer::new(actions);

        // 测试 header 不存在时添加
        let mut headers1 = HeaderMap::new();
        transformer.apply(&mut headers1).unwrap();
        assert_eq!(
            headers1.get("X-Custom").unwrap().to_str().unwrap(),
            "custom-value"
        );

        // 测试 header 已存在时不覆盖
        let mut headers2 = HeaderMap::new();
        headers2.insert("X-Custom", "existing-value".parse().unwrap());
        transformer.apply(&mut headers2).unwrap();
        assert_eq!(
            headers2.get("X-Custom").unwrap().to_str().unwrap(),
            "existing-value"
        );
    }

    #[test]
    fn test_header_transformer_force_delete() {
        let mut actions = HashMap::new();
        actions.insert(
            "X-Delete-Me".to_string(),
            HeaderAction {
                value: "".to_string(),
                action: HeaderActionType::ForceDelete,
                condition: None,
            },
        );

        let transformer = HeaderTransformer::new(actions);
        let mut headers = HeaderMap::new();
        headers.insert("X-Delete-Me", "some-value".parse().unwrap());

        transformer.apply(&mut headers).unwrap();

        assert!(!headers.contains_key("X-Delete-Me"));
    }

    #[test]
    fn test_evaluate_condition_empty() {
        let actions = HashMap::new();
        let transformer = HeaderTransformer::new(actions);
        let headers = HeaderMap::new();

        // 条件为 None
        assert!(transformer.evaluate_condition(&None, &headers));

        // 条件为空字符串
        assert!(transformer.evaluate_condition(&Some("".to_string()), &headers));
    }

    #[test]
    fn test_evaluate_condition_with_value() {
        let actions = HashMap::new();
        let transformer = HeaderTransformer::new(actions);
        let mut headers = HeaderMap::new();
        headers.insert("Host", "localhost".parse().unwrap());

        // 条件满足
        assert!(transformer.evaluate_condition(
            &Some("Host=localhost".to_string()),
            &headers
        ));

        // 条件不满足
        assert!(!transformer.evaluate_condition(
            &Some("Host=example.com".to_string()),
            &headers
        ));
    }

    #[test]
    fn test_evaluate_condition_existence() {
        let actions = HashMap::new();
        let transformer = HeaderTransformer::new(actions);
        let mut headers = HeaderMap::new();
        headers.insert("X-Custom", "value".parse().unwrap());

        // 检查 header 是否存在
        assert!(transformer.evaluate_condition(&Some("X-Custom".to_string()), &headers));

        // 不存在的 header
        assert!(!transformer.evaluate_condition(&Some("X-Not-Exist".to_string()), &headers));
    }

    #[test]
    fn test_multiple_actions() {
        let mut actions = HashMap::new();
        
        // 覆盖 Host
        actions.insert(
            "Host".to_string(),
            HeaderAction {
                value: "localhost".to_string(),
                action: HeaderActionType::Overwrite,
                condition: None,
            },
        );
        
        // 添加缺失的 header
        actions.insert(
            "X-Added".to_string(),
            HeaderAction {
                value: "added-value".to_string(),
                action: HeaderActionType::Missed,
                condition: None,
            },
        );
        
        // 删除某个 header
        actions.insert(
            "X-Remove".to_string(),
            HeaderAction {
                value: "".to_string(),
                action: HeaderActionType::ForceDelete,
                condition: None,
            },
        );

        let transformer = HeaderTransformer::new(actions);
        let mut headers = HeaderMap::new();
        headers.insert("Host", "example.com".parse().unwrap());
        headers.insert("X-Remove", "remove-me".parse().unwrap());

        transformer.apply(&mut headers).unwrap();

        // 验证结果
        assert_eq!(headers.get("Host").unwrap().to_str().unwrap(), "localhost");
        assert_eq!(headers.get("X-Added").unwrap().to_str().unwrap(), "added-value");
        assert!(!headers.contains_key("X-Remove"));
    }

    #[test]
    fn test_conditional_action() {
        let mut actions = HashMap::new();
        actions.insert(
            "X-Conditional".to_string(),
            HeaderAction {
                value: "conditional-value".to_string(),
                action: HeaderActionType::Overwrite,
                condition: Some("Host=localhost".to_string()),
            },
        );

        let transformer = HeaderTransformer::new(actions);

        // 条件满足时应用
        let mut headers1 = HeaderMap::new();
        headers1.insert("Host", "localhost".parse().unwrap());
        transformer.apply(&mut headers1).unwrap();
        assert_eq!(
            headers1.get("X-Conditional").unwrap().to_str().unwrap(),
            "conditional-value"
        );

        // 条件不满足时不应用
        let mut headers2 = HeaderMap::new();
        headers2.insert("Host", "example.com".parse().unwrap());
        transformer.apply(&mut headers2).unwrap();
        assert!(!headers2.contains_key("X-Conditional"));
    }
}
