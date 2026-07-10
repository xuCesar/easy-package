use std::{
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};

use crate::{
    error::AppError,
    models::{EnvironmentScan, ScanPhase, ScanProgress},
};

const FIXTURE: &str = include_str!("../e2e/scan.json");

pub fn is_enabled() -> bool {
    std::env::var("EASY_PACKAGE_E2E").as_deref() == Ok("1")
}

pub fn scan(
    cancelled: &AtomicBool,
    scan_id: &str,
    progress: &(dyn Fn(ScanProgress) + Send + Sync),
) -> Result<EnvironmentScan, AppError> {
    for (completed, phase) in [
        (1, ScanPhase::Managers),
        (2, ScanPhase::Projects),
        (3, ScanPhase::Health),
    ] {
        thread::sleep(Duration::from_millis(120));
        if cancelled.load(Ordering::SeqCst) {
            return Err(AppError::ScanCancelled);
        }
        progress(ScanProgress {
            scan_id: scan_id.into(),
            phase,
            completed,
            total: 3,
            manager_id: None,
        });
    }
    let scan = serde_json::from_str(FIXTURE)?;
    progress(ScanProgress {
        scan_id: scan_id.into(),
        phase: ScanPhase::Complete,
        completed: 3,
        total: 3,
        manager_id: None,
    });
    Ok(scan)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::*;

    #[test]
    fn parses_the_static_e2e_fixture_without_local_environment_access() {
        let scan = scan(&AtomicBool::new(false), "fixture", &|_| {}).unwrap();
        assert_eq!(scan.packages[0].name, "typescript");
        assert_eq!(scan.dependency_insights[0].name, "react");
    }
}
