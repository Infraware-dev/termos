//! Background job management for async process tracking
//!
//! This module provides a job table to track background processes spawned with `&`.
//! It supports listing jobs, checking completion status, and cleanup.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::process::Child;

/// Status of a background job
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobStatus {
    /// Job is currently running
    Running,
    /// Job completed with exit code
    Done(i32),
    /// Job was terminated by signal
    Terminated,
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobStatus::Running => write!(f, "Running"),
            JobStatus::Done(code) => write!(f, "Done (exit: {})", code),
            JobStatus::Terminated => write!(f, "Terminated"),
        }
    }
}

/// Information about a background job (without the Child handle)
#[derive(Debug, Clone)]
pub struct JobInfo {
    /// Unique job ID (1-based, like bash)
    pub id: usize,
    /// Process ID
    pub pid: u32,
    /// Original command string
    pub command: String,
    /// Current status
    pub status: JobStatus,
    /// When the job was started
    pub start_time: Instant,
}

/// Internal job entry with Child handle for status checking
struct JobEntry {
    info: JobInfo,
    child: Child,
}

impl std::fmt::Debug for JobEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobEntry")
            .field("info", &self.info)
            .field("child", &"<Child>")
            .finish()
    }
}

/// Manages background jobs
///
/// Tracks spawned background processes and provides methods to:
/// - Add new jobs
/// - List current jobs
/// - Check for completed jobs
/// - Clean up finished jobs
#[derive(Debug, Default)]
pub struct JobManager {
    /// Active jobs (id -> entry)
    jobs: HashMap<usize, JobEntry>,
    /// Next job ID to assign
    next_id: usize,
}

impl JobManager {
    /// Create a new job manager
    pub fn new() -> Self {
        Self {
            jobs: HashMap::new(),
            next_id: 1, // Bash-style 1-based job IDs
        }
    }

    /// Add a new background job
    ///
    /// Returns the assigned job ID
    pub fn add_job(&mut self, command: String, pid: u32, child: Child) -> usize {
        let id = self.next_id;
        self.next_id += 1;

        let info = JobInfo {
            id,
            pid,
            command,
            status: JobStatus::Running,
            start_time: Instant::now(),
        };

        self.jobs.insert(id, JobEntry { info, child });
        id
    }

    /// List all jobs (returns cloned info, safe for display)
    pub fn list_jobs(&self) -> Vec<JobInfo> {
        self.jobs.values().map(|e| e.info.clone()).collect()
    }

    /// Get number of active jobs
    pub fn job_count(&self) -> usize {
        self.jobs.len()
    }

    /// Check for completed jobs and return their info
    ///
    /// This method checks each running job's status and updates accordingly.
    /// Completed jobs are removed from the manager and returned.
    pub fn check_completed(&mut self) -> Vec<JobInfo> {
        let mut completed = Vec::new();
        let mut to_remove = Vec::new();

        for (id, entry) in &mut self.jobs {
            // Try to get exit status without blocking
            match entry.child.try_wait() {
                Ok(Some(status)) => {
                    // Process has exited
                    let exit_code = status.code().unwrap_or(-1);
                    entry.info.status = if status.success() {
                        JobStatus::Done(exit_code)
                    } else if exit_code == -1 {
                        JobStatus::Terminated
                    } else {
                        JobStatus::Done(exit_code)
                    };
                    completed.push(entry.info.clone());
                    to_remove.push(*id);
                }
                Ok(None) => {
                    // Still running
                }
                Err(e) => {
                    // Error checking status - log warning and assume terminated
                    log::warn!(
                        "try_wait() failed for job [{}] (PID {}): {}. Assuming terminated.",
                        entry.info.id,
                        entry.info.pid,
                        e
                    );
                    entry.info.status = JobStatus::Terminated;
                    completed.push(entry.info.clone());
                    to_remove.push(*id);
                }
            }
        }

        // Remove completed jobs
        for id in to_remove {
            self.jobs.remove(&id);
        }

        completed
    }

    /// Remove a specific job by ID
    pub fn remove_job(&mut self, id: usize) -> Option<JobInfo> {
        self.jobs.remove(&id).map(|e| e.info)
    }

    /// Check if there are any running jobs
    pub fn has_running_jobs(&self) -> bool {
        !self.jobs.is_empty()
    }
}

/// Thread-safe shared job manager
pub type SharedJobManager = Arc<RwLock<JobManager>>;

/// Create a new shared job manager
pub fn create_shared_job_manager() -> SharedJobManager {
    Arc::new(RwLock::new(JobManager::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_status_display() {
        assert_eq!(format!("{}", JobStatus::Running), "Running");
        assert_eq!(format!("{}", JobStatus::Done(0)), "Done (exit: 0)");
        assert_eq!(format!("{}", JobStatus::Done(1)), "Done (exit: 1)");
        assert_eq!(format!("{}", JobStatus::Terminated), "Terminated");
    }

    #[test]
    fn test_job_manager_new() {
        let mgr = JobManager::new();
        assert_eq!(mgr.job_count(), 0);
        assert!(!mgr.has_running_jobs());
    }

    #[test]
    fn test_job_manager_list_empty() {
        let mgr = JobManager::new();
        let jobs = mgr.list_jobs();
        assert!(jobs.is_empty());
    }

    #[test]
    fn test_create_shared_job_manager() {
        let shared = create_shared_job_manager();
        let mgr = shared.read().unwrap();
        assert_eq!(mgr.job_count(), 0);
    }

    // ==================== JobStatus Tests ====================

    #[test]
    fn test_job_status_debug() {
        assert!(format!("{:?}", JobStatus::Running).contains("Running"));
        assert!(format!("{:?}", JobStatus::Done(42)).contains("42"));
        assert!(format!("{:?}", JobStatus::Terminated).contains("Terminated"));
    }

    #[test]
    fn test_job_status_clone() {
        let status = JobStatus::Done(0);
        let cloned = status.clone();
        assert_eq!(status, cloned);
    }

    #[test]
    fn test_job_status_eq() {
        assert_eq!(JobStatus::Running, JobStatus::Running);
        assert_eq!(JobStatus::Done(0), JobStatus::Done(0));
        assert_eq!(JobStatus::Terminated, JobStatus::Terminated);
        assert_ne!(JobStatus::Running, JobStatus::Terminated);
        assert_ne!(JobStatus::Done(0), JobStatus::Done(1));
    }

    // ==================== JobInfo Tests ====================

    #[test]
    fn test_job_info_debug() {
        let info = JobInfo {
            id: 1,
            pid: 12345,
            command: "sleep 10".to_string(),
            status: JobStatus::Running,
            start_time: Instant::now(),
        };
        let debug_str = format!("{:?}", info);
        assert!(debug_str.contains("JobInfo"));
        assert!(debug_str.contains("12345"));
        assert!(debug_str.contains("sleep"));
    }

    #[test]
    fn test_job_info_clone() {
        let info = JobInfo {
            id: 1,
            pid: 12345,
            command: "echo test".to_string(),
            status: JobStatus::Done(0),
            start_time: Instant::now(),
        };
        let cloned = info.clone();
        assert_eq!(info.id, cloned.id);
        assert_eq!(info.pid, cloned.pid);
        assert_eq!(info.command, cloned.command);
        assert_eq!(info.status, cloned.status);
    }

    // ==================== JobManager with Child Process Tests ====================

    #[tokio::test]
    #[cfg(unix)]
    async fn test_job_manager_add_job() {
        use tokio::process::Command;

        let mut mgr = JobManager::new();

        // Spawn a real process
        let child = Command::new("sleep")
            .arg("0.1")
            .spawn()
            .expect("Failed to spawn sleep");

        let pid = child.id().expect("No pid");
        let job_id = mgr.add_job("sleep 0.1".to_string(), pid, child);

        assert_eq!(job_id, 1);
        assert_eq!(mgr.job_count(), 1);
        assert!(mgr.has_running_jobs());

        // List jobs
        let jobs = mgr.list_jobs();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, 1);
        assert_eq!(jobs[0].command, "sleep 0.1");
        assert_eq!(jobs[0].status, JobStatus::Running);
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_job_manager_multiple_jobs() {
        use tokio::process::Command;

        let mut mgr = JobManager::new();

        // Spawn multiple processes
        let child1 = Command::new("sleep")
            .arg("0.1")
            .spawn()
            .expect("Failed to spawn sleep 1");
        let pid1 = child1.id().expect("No pid 1");
        let id1 = mgr.add_job("sleep 0.1".to_string(), pid1, child1);

        let child2 = Command::new("sleep")
            .arg("0.2")
            .spawn()
            .expect("Failed to spawn sleep 2");
        let pid2 = child2.id().expect("No pid 2");
        let id2 = mgr.add_job("sleep 0.2".to_string(), pid2, child2);

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(mgr.job_count(), 2);

        let jobs = mgr.list_jobs();
        assert_eq!(jobs.len(), 2);
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_job_manager_check_completed() {
        use tokio::process::Command;

        let mut mgr = JobManager::new();

        // Spawn a fast-completing process
        let child = Command::new("true").spawn().expect("Failed to spawn true");

        let pid = child.id().expect("No pid");
        mgr.add_job("true".to_string(), pid, child);

        // Wait for it to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Check completed
        let completed = mgr.check_completed();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].command, "true");
        assert!(matches!(completed[0].status, JobStatus::Done(0)));

        // Job should be removed
        assert_eq!(mgr.job_count(), 0);
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_job_manager_check_completed_with_failure() {
        use tokio::process::Command;

        let mut mgr = JobManager::new();

        // Spawn a process that exits with error
        let child = Command::new("false")
            .spawn()
            .expect("Failed to spawn false");

        let pid = child.id().expect("No pid");
        mgr.add_job("false".to_string(), pid, child);

        // Wait for it to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Check completed
        let completed = mgr.check_completed();
        assert_eq!(completed.len(), 1);
        assert!(matches!(completed[0].status, JobStatus::Done(1)));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_job_manager_remove_job() {
        use tokio::process::Command;

        let mut mgr = JobManager::new();

        let child = Command::new("sleep")
            .arg("10")
            .spawn()
            .expect("Failed to spawn sleep");

        let pid = child.id().expect("No pid");
        let job_id = mgr.add_job("sleep 10".to_string(), pid, child);

        // Remove the job
        let removed = mgr.remove_job(job_id);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().command, "sleep 10");
        assert_eq!(mgr.job_count(), 0);

        // Try to remove non-existent job
        let not_found = mgr.remove_job(999);
        assert!(not_found.is_none());
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_job_manager_still_running() {
        use tokio::process::Command;

        let mut mgr = JobManager::new();

        // Spawn a long-running process
        let child = Command::new("sleep")
            .arg("10")
            .spawn()
            .expect("Failed to spawn sleep");

        let pid = child.id().expect("No pid");
        mgr.add_job("sleep 10".to_string(), pid, child);

        // Check completed immediately (should be empty)
        let completed = mgr.check_completed();
        assert!(completed.is_empty());

        // Job should still be there
        assert_eq!(mgr.job_count(), 1);

        // Clean up - remove the job to kill the process
        mgr.remove_job(1);
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_job_manager_job_ids_increment() {
        use tokio::process::Command;

        let mut mgr = JobManager::new();

        for i in 1..=5 {
            let child = Command::new("true").spawn().expect("Failed to spawn true");
            let pid = child.id().expect("No pid");
            let id = mgr.add_job(format!("job {}", i), pid, child);
            assert_eq!(id, i);
        }

        // Wait for all to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        mgr.check_completed();

        // Next job should have ID 6
        let child = Command::new("true").spawn().expect("Failed to spawn true");
        let pid = child.id().expect("No pid");
        let id = mgr.add_job("job 6".to_string(), pid, child);
        assert_eq!(id, 6);
    }

    // ==================== JobEntry Tests ====================

    #[tokio::test]
    #[cfg(unix)]
    async fn test_job_entry_debug() {
        use tokio::process::Command;

        let child = Command::new("true").spawn().expect("Failed to spawn true");

        let pid = child.id().expect("No pid");
        let info = JobInfo {
            id: 1,
            pid,
            command: "true".to_string(),
            status: JobStatus::Running,
            start_time: Instant::now(),
        };

        let entry = JobEntry { info, child };
        let debug_str = format!("{:?}", entry);
        assert!(debug_str.contains("JobEntry"));
        assert!(debug_str.contains("true"));
        assert!(debug_str.contains("<Child>"));
    }

    // ==================== Default Trait Tests ====================

    #[test]
    fn test_job_manager_default() {
        let mgr = JobManager::default();
        assert_eq!(mgr.job_count(), 0);
        assert!(!mgr.has_running_jobs());
    }

    // ==================== SharedJobManager Tests ====================

    #[tokio::test]
    #[cfg(unix)]
    async fn test_shared_job_manager_concurrent_access() {
        use tokio::process::Command;

        let shared = create_shared_job_manager();
        let shared_clone = shared.clone();

        // Add job from one reference
        {
            let child = Command::new("sleep")
                .arg("0.1")
                .spawn()
                .expect("Failed to spawn sleep");
            let pid = child.id().expect("No pid");
            let mut mgr = shared.write().unwrap();
            mgr.add_job("sleep 0.1".to_string(), pid, child);
        }

        // Read from another reference
        {
            let mgr = shared_clone.read().unwrap();
            assert_eq!(mgr.job_count(), 1);
        }

        // Wait and check completed
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        {
            let mut mgr = shared.write().unwrap();
            let completed = mgr.check_completed();
            assert_eq!(completed.len(), 1);
        }
    }
}
