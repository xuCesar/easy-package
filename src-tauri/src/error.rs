use serde::ser::SerializeStruct;
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
    #[error("已有环境扫描正在进行")]
    ScanConflict,
    #[error("已有软件包操作正在进行")]
    ActionConflict,
    #[error("上次操作可能中断，请先完成一次环境扫描")]
    RecoveryRequired,
}

impl AppError {
    /// 稳定错误码：跨前后端的契约，前端只允许按 code 分支，message 仅用于展示。
    pub fn code(&self) -> &'static str {
        match self {
            Self::Storage(_) => "STORAGE",
            Self::InvalidScanRoot(_) => "INVALID_SCAN_ROOT",
            Self::InvalidScanSettings(_) => "INVALID_SCAN_SETTINGS",
            Self::Serialization(_) => "SERIALIZATION",
            Self::Command(_) => "COMMAND",
            Self::ScanCancelled => "SCAN_CANCELLED",
            Self::ScanConflict => "SCAN_ALREADY_RUNNING",
            Self::ActionConflict => "PACKAGE_ACTION_ALREADY_RUNNING",
            Self::RecoveryRequired => "PACKAGE_ACTION_RECOVERY_REQUIRED",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut payload = serializer.serialize_struct("AppError", 2)?;
        payload.serialize_field("code", self.code())?;
        payload.serialize_field("message", &self.to_string())?;
        payload.end()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_code_and_message_separately() {
        let payload = serde_json::to_value(AppError::ScanCancelled).unwrap();
        assert_eq!(payload["code"], "SCAN_CANCELLED");
        assert_eq!(payload["message"], "扫描已取消");

        let payload = serde_json::to_value(AppError::Command("brew 退出码 1".into())).unwrap();
        assert_eq!(payload["code"], "COMMAND");
        assert_eq!(payload["message"], "本机命令执行失败：brew 退出码 1");
    }

    #[test]
    fn conflict_and_recovery_codes_are_stable() {
        assert_eq!(AppError::ScanConflict.code(), "SCAN_ALREADY_RUNNING");
        assert_eq!(
            AppError::ActionConflict.code(),
            "PACKAGE_ACTION_ALREADY_RUNNING"
        );
        assert_eq!(
            AppError::RecoveryRequired.code(),
            "PACKAGE_ACTION_RECOVERY_REQUIRED"
        );
    }
}
