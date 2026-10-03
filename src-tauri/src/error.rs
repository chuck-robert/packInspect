//! 统一错误类型。
//!
//! Tauri command 的 `Result<T, Error>` 要求错误可序列化；这里保留结构化 code，
//! 前端可以据此做差异化提示（例如 `NOT_INSTALLED` 走「未安装」空态而不是报错红条）。

use serde::{Serialize, Serializer};

#[derive(Debug, Clone)]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
}

impl AppError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    /// 参数非法（前端传入的数据没通过校验）
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("INVALID_INPUT", message)
    }

    /// 被安全策略拦截（白名单不允许的操作）
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new("FORBIDDEN", message)
    }

    pub fn not_installed(manager: &str) -> Self {
        Self::new("NOT_INSTALLED", format!("未在系统中检测到 {manager}"))
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::new("IO_ERROR", message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("INTERNAL", message)
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("code", self.code)?;
        s.serialize_field("message", &self.message)?;
        s.end()
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::io(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::new("PARSE_ERROR", format!("解析命令输出失败: {e}"))
    }
}

pub type AppResult<T> = Result<T, AppError>;
