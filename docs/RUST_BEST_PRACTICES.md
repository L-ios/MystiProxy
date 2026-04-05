# Rust 最佳实践指南

本文档汇总了 Rust 官方推荐的最佳实践资源，帮助开发者编写高质量的地道 Rust 代码。

---

## 核心资源

### 1. Idiomatic Rust (Manning)

**书籍**: https://www.manning.com/books/idiomatic-rust

**核心内容**:
- Rust 特有的设计模式和惯用写法
- 经典设计模式在 Rust 中的应用
- 反模式（Anti-patterns）和常见错误

**关键要点**:

| 主题 | 内容 |
|------|------|
| **核心模式** | RAII、构造函数、错误处理、全局状态 |
| **设计模式** | Builder 模式、Fluent 接口、观察者模式 |
| **高级模式** | Newtype 模式、Zero-cost abstractions |
| **反模式** | 滥用 Clone、OOP 思维、把借用检查器当敌人 |

**适用场景**:
- 想深入理解 Rust 惯用写法
- 从 OOP 语言（C++/Java/Python）转 Rust
- 学习 Rust 特有的设计模式

---

### 2. Rust API Guidelines (官方)

**文档**: https://rust-lang.github.io/api-guidelines/

**核心内容**:
- Rust 库 API 设计规范
- 命名、互操作性、可预测性、灵活性
- 类型安全、可靠性、可调试性

**15 条核心规范**:

| 编号 | 规范 | 说明 |
|------|------|------|
| C-CASE | 命名遵循 RFC 430 | PascalCase/snake_case/SCREAMING_SNAKE_CASE |
| C-DEBUG | 所有公共类型实现 Debug | 便于调试输出 |
| C-COMMON-TRAITS | 积极实现常见 trait | Clone, Debug, Eq, PartialEq, Hash, Default |
| C-CONV-TRAITS | 使用标准转换 trait | From, Into, AsRef, AsMut |
| C-VALIDATE | 函数验证参数 | 静态或动态检查输入 |
| C-BUILDER | 复杂值使用 Builder | 配置选项多时使用构建器模式 |
| C-STRUCT-PRIVATE | 结构体包含私有字段 | 确保封装性和未来兼容性 |
| C-GOOD-ERR | 错误类型有意义 | 提供有价值的错误信息 |
| C-SEND-SYNC | 类型尽可能实现 Send/Sync | 支持并发 |
| C-CRATE-DOC |  crate 级文档详尽 | 包含示例 |

**适用场景**:
- 开发 Rust 库/框架
- 设计公共 API
- 需要与生态系统互操作

---

### 3. 6 条 Rust 最佳实践 (2025)

**核心要点**:

#### 1. 使用 Clippy 进行代码质量检查
```bash
cargo clippy -- -D warnings
```
- 启用所有警告作为错误
- 在 CI/CD 中集成 clippy

#### 2. 使用自定义错误类型
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("无效数据: {0}")]
    InvalidData(String),
}

pub type Result<T> = std::result::Result<T, AppError>;
```

#### 3. 使用 async/await 进行并发
```rust
#[tokio::main]
async fn main() {
    let handle = tokio::spawn(async {
        // 异步任务
    });

    let result = tokio::join!(handle1, handle2);
}
```

#### 4. 利用类型系统
```rust
// Newtype 模式
struct Percentage(u8);
impl Percentage {
    pub fn new(value: u8) -> Result<Self, String> {
        if value <= 100 {
            Ok(Self(value))
        } else {
            Err("Percentage must be 0-100".to_string())
        }
    }
}
```

#### 5. 编写全面的测试
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_percentage() {
        assert!(Percentage::new(50).is_ok());
    }

    #[test]
    fn test_invalid_percentage() {
        assert!(Percentage::new(101).is_err());
    }
}
```

#### 6. 使用 Cargo 工作区
```toml
[workspace]
members = ["crate1", "crate2", "crate3"]
```

---

## 快速参考

### 命名规范
```
类型/结构体: PascalCase    →  MyStruct, HttpClient
函数/方法:   snake_case    →  handle_request, get_data
变量:        snake_case    →  user_data, connection
常量:        SCREAMING     →  MAX_CONNECTIONS
```

### 错误处理层级
```rust
// 1. 快速原型: unwrap()
let value = compute().unwrap();

// 2. 提供上下文: expect()
let value = compute().expect("计算应该成功");

// 3. 传播错误: ?
let value = compute()?;

// 4. 显式处理: match
match compute() {
    Ok(v) => v,
    Err(e) => return Err(e),
}
```

### 状态共享
```rust
// 不可变共享
Arc::new(data)

// 可变共享
Arc::new(Mutex::new(state))
Arc::new(RwLock::new(state))

// 线程间通信
use std::sync::mpsc;
```

### 异步模式
```rust
// Spawn 任务
tokio::spawn(async move {
    // 并发执行
});

// 等待多个任务
tokio::join!(task1, task2);

// 超时
tokio::time::timeout(Duration::from_secs(5), async_op()).await?;
```

---

## 参考链接

### 官方资源
- [The Rust Programming Language Book](https://doc.rust-lang.org/book/)
- [Rust By Example](https://doc.rust-lang.org/rust-by-example/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Clippy Lints](https://doc.rust-lang.org/clippy/)

### 社区资源
- [Idiomatic Rust (Manning)](https://www.manning.com/books/idiomatic-rust)
- [Rust Design Patterns](https://rust-unofficial.github.io/patterns/)
- [Microsoft Rust Guidelines](http://microsoft.github.io/rust-guidelines/)

### 工具链
- `cargo clippy` - 代码质量检查
- `cargo fmt` - 代码格式化
- `cargo test` - 单元测试
- `cargo doc --open` - 文档生成

---

**最后更新**: 2026-04-04
