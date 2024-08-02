use log::debug;
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::ops::Index;

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct UriMapping {
    #[serde(
        rename = "method",
        skip_serializing_if = "Vec::is_empty",
        serialize_with = "UriMapping::serialize_method",
        deserialize_with = "UriMapping::deserialize_method"
    )]
    methods: Vec<String>, // GET, POST, PUT, DELETE, etc
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<String>, // Full
    #[serde(skip_serializing_if = "Option::is_none")]
    service: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_service: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    var_pattern: Option<String>,
}

#[derive(Debug)]
struct UriVariable {
    name: String,
    pattern: Option<String>,
    regex: Regex,
    index: usize,
}

impl UriVariable {
    pub fn to_pattern(&self) -> String {
        match self.pattern.as_ref() {
            None => "[\\w|-]+".to_string(),
            Some(pattern) => pattern.to_string(),
        }
    }

    pub fn origin(&self) -> String {
        match self.pattern.as_ref() {
            None => format!("{{{}}}", self.name),
            Some(pattern) => format!("{{{}:{}}}", self.name, pattern),
        }
    }
}

#[derive(PartialEq, Debug)]
enum UriMatch {
    /// 全量匹配
    Exact,
    /// 前缀匹配
    Prefix,
    /// 变量形式匹配
    Variable,
    /// 变量前缀匹配
    VariablePrefix,
}

impl UriMapping {
    fn supports_method(&self, method: &str) -> bool {
        for mtd in &self.methods {
            if mtd.eq("*") {
                return true;
            }
            if mtd.eq_ignore_ascii_case(method) {
                return true;
            }
        }
        false
    }

    fn uri_variable(uri: &str) -> HashMap<String, UriVariable> {
        let re = Regex::new(r"/\{(\w+):?([^}]*)}").unwrap(); // 改进：可能存在特殊情况，需要修改正则表达式
        let mut variable_patterns = HashMap::new();
        let mut index = 1;
        for cap in re.captures_iter(uri) {
            // 如果没有提供正则，则默认匹配非斜杠字符
            let (pattern, regex) = if let Some(matched) = cap.get(2) {
                if matched.is_empty() {
                    (None, "\\w+")
                } else {
                    (Some(matched.as_str().to_string()), matched.as_str())
                }
            } else {
                (None, "\\w+")
            };
            let variable = UriVariable {
                name: (&cap[1]).to_string(),
                pattern,
                regex: Regex::new(regex).unwrap(),
                index,
            };
            variable_patterns.insert(variable.name.clone(), variable);
            index += 1;
        }

        return variable_patterns;
    }

    /// # uri的匹配模式
    ///
    /// 1. uri和当前的uri相同，则为Full
    ///
    /// 2. 前缀匹配，如果传入的uri的前缀和UriMapping中的相同，则为true
    ///
    /// # Arguments
    ///
    /// * `&self` - 对当前`UriMapping`实例的引用，包含了所有用于匹配的配置信息。
    /// * `uri: &str` - 需要被检验的URI字符串，代表了客户端请求的目标地址。
    ///
    /// # Returns
    ///
    /// * `bool` - 如果传入的`uri`与`UriMapping`的配置匹配，则返回`true`；否则返回`false`。
    fn match_uri(&self, in_uri: &str) -> Option<UriMatch> {
        match self.uri.as_ref() {
            None => None,
            Some(uri) => {
                debug!("uri: {}, in_uri: {}", uri, in_uri);
                let base_uri = uri.as_str();
                // 精确匹配
                if uri == in_uri {
                    return Some(UriMatch::Exact);
                }

                if uri == "/" && in_uri.len() > 1 {
                    return Some(UriMatch::Prefix);
                }

                let variable_patterns = Self::uri_variable(uri);
                if variable_patterns.len() == 0 {
                    // 前缀匹配
                    let prefix_uri = if uri.ends_with("/") {
                        format!("{}", uri)
                    } else {
                        format!("{}/", uri)
                    };
                    return if in_uri.starts_with(&prefix_uri) {
                        Some(UriMatch::Prefix)
                    } else {
                        None
                    };
                }

                // 处理路径变量，支持变量后面跟正则表达式，并识别带路径的前缀匹配
                let mut processed_base_uri = base_uri.to_string();
                for (_, regex_pattern) in variable_patterns {
                    processed_base_uri = processed_base_uri.replace(
                        &regex_pattern.origin(),
                        &format!(r"({})", regex_pattern.to_pattern()),
                    );
                }

                // 构造正则表达式并尝试匹配
                let regex = Regex::new(&format!("^{}\\/?.*$", processed_base_uri)).unwrap();
                let mut match_var = HashMap::new();
                if regex.is_match(in_uri) {
                    let mut end = 0;

                    for cap in regex.captures_iter(in_uri) {
                        for i in 1..cap.len() {
                            if let Some(matchs) = cap.get(i) {
                                debug!("{}: {}", i, matchs.as_str());
                                match_var.insert(i, matchs.as_str().to_string());
                                end = matchs.end();
                            }
                        }
                    }
                    if let Some(matched) = in_uri.get(0..end) {
                        if matched.len() == in_uri.len()
                            || (matched.len() + 1 == in_uri.len() && in_uri.ends_with("/"))
                        {
                            Some(UriMatch::Variable)
                        } else {
                            Some(UriMatch::VariablePrefix)
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        }
    }

    fn build_target_uri(&self, in_uri: &str) -> Option<String> {
        match self.match_uri(in_uri).unwrap() {
            UriMatch::Exact => self.target_uri.clone(),
            UriMatch::Prefix => Some(in_uri.replace(
                self.uri.clone().unwrap().as_str(),
                self.target_uri.clone().unwrap().as_str(),
            )),
            // UriMatch::Prefix => Some(in_uri.to_string()),
            UriMatch::Variable | UriMatch::VariablePrefix => {
                let base_uri = self.uri.as_ref().unwrap();
                let in_map = Self::uri_variable(base_uri);
                // 处理路径变量，支持变量后面跟正则表达式，并识别带路径的前缀匹配
                let mut processed_base_uri = base_uri.to_string();
                for (_, regex_pattern) in &in_map {
                    processed_base_uri = processed_base_uri.replace(
                        &regex_pattern.origin(),
                        &format!(r"({})", regex_pattern.to_pattern().as_str()),
                    );
                }

                // 构造正则表达式并尝试匹配
                let rest = Regex::new(&processed_base_uri).unwrap().replace(in_uri, "").to_string();
                let regex = Regex::new(&format!("^{}\\/?.*$", processed_base_uri)).unwrap();
                let mut match_var = HashMap::new();

                for cap in regex.captures_iter(in_uri) {
                    for i in 1..cap.len() {
                        match cap.get(i) {
                            Some(matchs) => {
                                match_var.insert(i, matchs.as_str().to_string());
                            }
                            _ => {
                                return None;
                            }
                        }
                    }
                }

                // 通过遍历map，转移target
                let mut target_uri = self.target_uri.as_ref().unwrap().to_string();
                let out_map = Self::uri_variable(self.target_uri.as_ref().unwrap());
                for (_, regex_pattern) in &out_map {
                    let name = regex_pattern.name.as_str();
                    match in_map.get(name) {
                        Some(variable) => {
                            let path = match_var.get(&variable.index).unwrap();
                            target_uri = target_uri.replace(&regex_pattern.origin(), path);
                        }
                        _ => {
                            return None;
                        }
                    }
                }
                if rest.len() == 0 {
                    return Some(target_uri)
                }
                let rest = if rest.starts_with("/") {
                    &rest[1..]
                } else {
                    &rest
                };

                if target_uri.ends_with("/")  {
                    Some(format!("{}{}", target_uri, rest))
                } else {
                    Some(format!("{}/{}", target_uri, rest))
                }
            }
        }
    }

    fn deserialize_method<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(|mtds| {
            let mut mtds = mtds
                .split(&[',', '|'])
                .map(|method| method.to_uppercase().to_string())
                .collect::<Vec<String>>();
            mtds.sort();
            mtds
        })
    }
    fn serialize_method<S>(methods: &Vec<String>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(methods.join(",").as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_mapping_serialize() {
        let mapping_string = r#"{
  "method": "GET,POST|put,*",
  "mode": "Full",
  "service": "test",
  "target_protocol": "http",
  "target_service": "test",
  "target_uri": "http://127.0.0.1:8080",
  "uri": "/test",
  "var_pattern": "test"
}"#;
        let mut mapping = serde_json::from_str::<UriMapping>(mapping_string).unwrap();
        assert_eq!(mapping.methods, vec!["*", "GET", "POST", "PUT"]);
        assert_eq!(mapping.mode, Some("Full".to_string()));
        assert_eq!(mapping.service, Some("test".to_string()));
    }

    #[test_case("/", "/" => Some(UriMatch::Exact); "1. exact match")]
    #[test_case("/", "/test" => Some(UriMatch::Prefix); "2. prefix match with root")]
    #[test_case("/api/users", "/api/users/123" => Some(UriMatch::Prefix); "3. prefix match with users")]
    #[test_case("/api/users/{id}", "/api/users/123" => Some(UriMatch::Variable); "4. variable match")] // 只匹配数字id
    #[test_case("/api/users/{id:[0-9]+}", "/api/users/123" => Some(UriMatch::Variable); "4. variable regex match")] // 只匹配数字id
    #[test_case("/api/users/{id:[0-9]+}", "/api/users/123/details" => Some(UriMatch::VariablePrefix); "5. variable match with nums")] // 匹配id后还有更多路径
    #[test_case("/api/users/{id:[0-9]+}/", "/api/users/123/details" => Some(UriMatch::VariablePrefix); "6. variable match with nums")] // 匹配id后还有更多路径
    #[test_case("/api/users/{id:[0-9]+}/", "/api/users/123" => None; "6. variable match with nums and with slash")] // 匹配id后还有更多路径
    #[test_case("/api/users/{id:[0-9]+}/", "/api/users/123/" => Some(UriMatch::Variable); "6. variable match with nums and with slash /")] // 匹配id后还有更多路径
    #[test_case("/api/users/{id:[0-9]+}", "/api/users/abc" => None; "7. variable match with non-nums")] // id不是数字，匹配失败
    #[test_case("/api/users/{id:[0-9]+}/records/{rid:[0-9a-z]+}", "/api/users/123/records/789abc" => Some(UriMatch::Variable); "8. variable match with more path")]
    #[test_case("/api/users/{id:[0-9]+}/records/{rid:[0-9]+}", "/api/users/123456789/records/987654321" => Some(UriMatch::Variable); "9. variable match with more path")]
    fn uri_matching(base_uri: &str, in_uri: &str) -> Option<UriMatch> {
        let mut mapping = UriMapping::default();
        mapping.uri = Some(base_uri.to_string());

        mapping.match_uri(in_uri)
    }

    #[test_case("/api/users/{id:[0-9]+}/records/{rid:[0-9]+}", "/user/{id}/record/{rid}",
    "/api/users/123/records/456" => Some("/user/123/record/456".to_string()); "user record transform")]
    #[test_case("/api/users/{rid}/records/{id}", "/record/{id}/user/{rid}",
    "/api/users/123/records/456" => Some("/record/456/user/123".to_string()); "user record transform with switch")]
    #[test_case("/api/users/{rid}/records/{id}", "/record/{id}/user/{rid}",
    "/api/users/123-456-789/records/456-789-123" => Some("/record/456-789-123/user/123-456-789".to_string()); "uuid in path")]
    #[test_case("/api/users/{rid}/records/{id}", "/record/{id}/user/{rid}",
    "/api/users/123-456-789/records/456-789-123/abc" => Some("/record/456-789-123/user/123-456-789/abc".to_string()); "uuid in path with variable prefix")]
    #[test_case("/api/users/{rid}/records/{id}/", "/record/{id}/user/{rid}/",
    "/api/users/123-456-789/records/456-789-123/abc" => Some("/record/456-789-123/user/123-456-789/abc".to_string()); "uuid in path with variable prefix with slash end")]
    #[test_case("/api/users/{rid}/records/{id}", "/record/{id}/user/{rid}/",
    "/api/users/123-456-789/records/456-789-123/abc" => Some("/record/456-789-123/user/123-456-789/abc".to_string()); "uuid in path with variable prefix target uri with slash end")]
    #[test_case("/api/users/{rid}/records/{id}/", "/record/{id}/user/{rid}",
    "/api/users/123-456-789/records/456-789-123/abc" => Some("/record/456-789-123/user/123-456-789/abc".to_string()); "uuid in path with variable prefix in uri with slash end")]
    fn uri_matching_test(in_pattern_uri: &str, target_pattern_uri: &str, in_uri: &str) -> Option<String> {
        let mut mapping = UriMapping::default();
        mapping.uri = Some(in_pattern_uri.to_string());
        mapping.target_uri = Some(target_pattern_uri.to_string());
        if mapping.match_uri(in_uri).is_some() {
            mapping.build_target_uri(in_uri)
        } else {
            None
        }

    }
}
