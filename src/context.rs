//! 线程上下文模块
//!
//! 提供线程级别的上下文信息存储，包括引擎名称等

use std::cell::RefCell;
use std::thread;

thread_local! {
    /// 当前线程关联的引擎名称
    static ENGINE_NAME: RefCell<Option<String>> = RefCell::new(None);
}

/// 设置当前线程的引擎名称
pub fn set_engine_name(name: impl Into<String>) {
    ENGINE_NAME.with(|n| {
        *n.borrow_mut() = Some(name.into());
    });
}

/// 获取当前线程的引擎名称
pub fn get_engine_name() -> Option<String> {
    ENGINE_NAME.with(|n| n.borrow().clone())
}

/// 获取当前线程的完整标识（引擎名称 + 线程名称）
pub fn thread_identity() -> String {
    let engine_name = get_engine_name();
    let thread_name = thread::current()
        .name()
        .unwrap_or("unknown")
        .to_string();

    match engine_name {
        Some(engine) => format!("{}:{}", engine, thread_name),
        None => thread_name,
    }
}

/// 在指定引擎上下文中执行闭包
pub fn with_engine<F, T>(engine_name: &str, f: F) -> T
where
    F: FnOnce() -> T,
{
    set_engine_name(engine_name);
    let result = f();
    ENGINE_NAME.with(|n| {
        *n.borrow_mut() = None;
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_get_engine_name() {
        set_engine_name("docker");
        assert_eq!(get_engine_name(), Some("docker".to_string()));

        set_engine_name("containerd");
        assert_eq!(get_engine_name(), Some("containerd".to_string()));
    }

    #[test]
    fn test_thread_identity() {
        set_engine_name("test-engine");
        let identity = thread_identity();
        assert!(identity.starts_with("test-engine:"));
    }

    #[test]
    fn test_with_engine() {
        let result = with_engine("my-engine", || {
            assert_eq!(get_engine_name(), Some("my-engine".to_string()));
            42
        });
        assert_eq!(result, 42);
        assert!(get_engine_name().is_none());
    }
}
