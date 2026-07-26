use std::sync::{Arc, Mutex};

use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OperationKind {
    Scan,
    PackageAction,
}

#[derive(Debug, Clone)]
struct ActiveOperation {
    id: String,
    kind: OperationKind,
}

#[derive(Debug, Clone, Default)]
pub struct OperationCoordinator {
    active: Arc<Mutex<Option<ActiveOperation>>>,
}

impl OperationCoordinator {
    pub fn begin_scan(&self, id: &str) -> Result<(), AppError> {
        self.begin(id, OperationKind::Scan)
    }

    pub fn begin_package_action(&self, id: &str) -> Result<(), AppError> {
        self.begin(id, OperationKind::PackageAction)
    }

    pub fn finish(&self, id: &str) {
        if let Ok(mut active) = self.active.lock() {
            if active.as_ref().is_some_and(|operation| operation.id == id) {
                *active = None;
            }
        }
    }

    fn begin(&self, id: &str, kind: OperationKind) -> Result<(), AppError> {
        let mut active = self
            .active
            .lock()
            .map_err(|error| AppError::Command(error.to_string()))?;
        if let Some(operation) = active.as_ref() {
            return Err(match operation.kind {
                OperationKind::Scan => AppError::ScanConflict,
                OperationKind::PackageAction => AppError::ActionConflict,
            });
        }
        *active = Some(ActiveOperation {
            id: id.into(),
            kind,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_scans_and_package_actions() {
        let coordinator = OperationCoordinator::default();
        coordinator.begin_scan("scan-1").unwrap();
        assert_eq!(
            coordinator.begin_package_action("action-1").unwrap_err().code(),
            "SCAN_ALREADY_RUNNING"
        );
        coordinator.finish("scan-1");
        coordinator.begin_package_action("action-1").unwrap();
        assert_eq!(
            coordinator.begin_scan("scan-2").unwrap_err().code(),
            "PACKAGE_ACTION_ALREADY_RUNNING"
        );
        coordinator.finish("action-1");
        assert!(coordinator.begin_scan("scan-2").is_ok());
    }
}
