# 完成 Rust 文件中的 TODO 项

## 发现的 TODO 项

通过搜索项目中的 rs 文件，发现以下 TODO 项：

### 1. src/main.rs (第 16-17 行) ✅ 已完成
```rust
// TODO: 加载配置
// TODO: 启动代理服务器
```
**解决方案**:
- 添加命令行参数解析 (使用 clap)
- 加载 YAML 配置文件
- 根据配置启动多个代理引擎
- 实现优雅关闭

### 2. src/arg.rs (第 42 行) ✅ 已完成
```rust
impl Default for MystiEngine {
    fn default() -> Self {
        todo!()
    }
}
```
**解决方案**:
- 实现合理的默认值

### 3. src/io/stream.rs (第 16 行) ✅ 已完成
```rust
if !addr.contains("://") {
    todo!("invalid url")
}
```
**解决方案**:
- 返回错误而不是 panic

### 4. src/io/stream.rs (第 24 行) ✅ 已完成
```rust
_ => todo!("not for support {}", protocol),
```
**解决方案**:
- 返回错误而不是 panic

### 5. src/io/listener.rs (第 24 行) ✅ 已完成
```rust
} else {
    todo!("Invalid listen")
}
```
**解决方案**:
- 返回错误而不是 panic

## 额外修复

### 文档测试 crate 名称修复 ✅ 已完成
将所有文档示例中的 `mysti_proxy` 替换为 `mystiproxy`，修复了 5 个文件：
- src/proxy/tcp.rs
- src/proxy/unix.rs
- src/proxy/address.rs
- src/proxy/forward.rs
- src/tls/mod.rs

## 测试结果
- 单元测试：72 passed ✅
- 文档测试：11 passed, 1 ignored ✅
