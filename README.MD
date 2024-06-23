# TUDS
TUDS主要用于将UDS暴露为TCP请求，例如将 `/var/run/docker.sock` 暴露到2375端口上

## 配置
- uds文件路径
- 监听的端口

## 对比
nginx有此功能


现在有个函数 fn match_uri(baseUri: &str, in_uri: &str) -> Option<UriMatch>
enum UriMatch {
Full,
Prefix,
Regex,
PrefixRegex,
}

当baseUri = /时，in_uri = /a/b/c时，返回Some(Prefix)
当baseUri = /a/b/c时，in_uri = /a/b/c/d/e时，返回Some(Prefix)
当baseUri = /a/b/c时，in_uri = /a/b/c时，返回Some(Full)
当baseUri = /a/{id}/c时，in_uri = /a/b/c时，返回Some(Regex)，其中{id}是参数，匹配inUri中的b
当baseUri = /a/{id}/c时，in_uri = /a/b/c/d/e时，返回Some(PrefixRegex)，其中{id}是参数，匹配inUri中的b
当baseUri = /a/{id}/c时，in_uri = /a/b/d/e/f时，返回None
1. 提供ut，来测试ai写的代码