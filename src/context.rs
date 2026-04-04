//! 线程上下文模块
//!
//! 提供线程级别的上下文信息存储，包括引擎名称和线程 ID

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

/// 全局线程 ID 计数器，从 1 开始
static THREAD_COUNTER: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// 当前线程关联的引擎名称
    static ENGINE_NAME: RefCell<Option<String>> = RefCell::new(None);
    
    /// 当前线程的唯一 ID
    static THREAD_ID: RefCell<Option<u64>> = RefCell::new(None);
}

/// 获取或分配当前线程的唯一 ID
fn get_or_assign_thread_id() -> u64 {
    THREAD_ID.with(|id| {
        if id.borrow().is_none() {
            let new_id = THREAD_COUNTER.fetch_add(1, Ordering::SeqCst);
            *id.borrow_mut() = Some(new_id);
            new_id
        } else {
            id.borrow().unwrap()
        }
    })
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

/// 获取当前线程的唯一 ID
pub fn get_thread_id() -> u64 {
    get_or_assign_thread_id()
}

/// 获取当前线程的完整标识（引擎名称:线程ID:线程名称）
pub fn thread_identity() -> String {
    let engine_name = get_engine_name();
    let thread_id = get_thread_id();
    let thread_name = thread::current()
        .name()
        .unwrap_or("unknown")
        .to_string();

    match engine_name {
        Some(engine) => format!("{}:{}:{}", engine, thread_id, thread_name),
        None => format!("{}:{}", thread_id, thread_name),
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
    fn test_thread_id_increments() {
        let id1 = get_thread_id();
        assert!(id1 >= 1);
    }

    #[test]
    fn test_thread_identity_with_engine() {
        set_engine_name("test-engine");
        let identity = thread_identity();
        assert!(identity.starts_with("test-engine:"));
        assert!(identity.contains(":"));
    }

    #[test]
    fn test_thread_identity_without_engine() {
        ENGINE_NAME.with(|n| {
            *n.borrow_mut() = None;
        });
        let identity = thread_identity();
        assert!(!identity.contains("test-engine"));
        assert!(identity.contains(":"));
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
