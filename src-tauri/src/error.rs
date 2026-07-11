use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("存储错误：{0}")]
    Storage(String),
    #[error("扫描目录无效：{0}")]
    InvalidScanRoot(String),
    #[error("扫描范围设置无效：{0}")]
    InvalidScanSettings(String),
    #[error("数据序列化失败：{0}")]
    Serialization(String),
    #[error("本机命令执行失败：{0}")]
    Command(String),
    #[error("扫描已取消")]
    ScanCancelled,
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Storage(value.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serialization(value.to_string())
    }
}
