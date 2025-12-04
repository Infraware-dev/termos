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

    // Note: Tests involving actual Child processes require spawning real processes
    // Those are covered in integration tests
}
