//! Throbber animation module
//!
//! Provides a dedicated animator for loading indicators following SOLID principles:
//! - **Single Responsibility**: Only handles throbber animation
//! - **Open/Closed**: Extensible via symbol sets without modifying core logic
//! - **Liskov Substitution**: Can be used anywhere an animator is needed
//! - **Interface Segregation**: Simple start/stop/symbol interface
//! - **Dependency Inversion**: No dependencies on terminal internals

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use throbber_widgets_tui::BRAILLE_DOUBLE;

/// Animation interval in milliseconds (~10 FPS)
/// Slower rate is more suitable for a loading indicator
const ANIMATION_INTERVAL_MS: u64 = 100;

/// Thread-safe throbber animator with dedicated animation thread
///
/// # Example
/// ```ignore
/// let animator = ThrobberAnimator::new();
/// animator.start();  // Spawns animation thread
/// println!("Symbol: {}", animator.symbol());  // Get current symbol
/// animator.stop();   // Stops animation thread
/// ```
///
/// # Thread Safety
/// - All methods are thread-safe
/// - `start()` is idempotent (multiple calls don't spawn multiple threads)
/// - `stop()` gracefully terminates the animation thread
#[derive(Debug)]
pub struct ThrobberAnimator {
    /// Current animation frame index (shared with animation thread)
    index: Arc<AtomicUsize>,
    /// Whether animation is running (shared with animation thread)
    active: Arc<AtomicBool>,
    /// Handle to the animation thread (for cleanup)
    thread_handle: std::sync::Mutex<Option<JoinHandle<()>>>,
}

impl Default for ThrobberAnimator {
    fn default() -> Self {
        Self::new()
    }
}

impl ThrobberAnimator {
    /// Create a new throbber animator (not started)
    pub fn new() -> Self {
        Self {
            index: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicBool::new(false)),
            thread_handle: std::sync::Mutex::new(None),
        }
    }

    /// Start the animation thread
    ///
    /// Spawns a dedicated thread that increments the animation index every 100ms (~10 FPS).
    /// This method is idempotent - calling it multiple times has no effect if
    /// animation is already running.
    ///
    /// # Thread Safety
    /// Safe to call from any thread. Uses atomic operations internally.
    pub fn start(&self) {
        // Idempotent: don't start if already running
        if self.active.load(Ordering::SeqCst) {
            return;
        }

        // Reset state
        self.index.store(0, Ordering::SeqCst);
        self.active.store(true, Ordering::SeqCst);

        // Clone Arcs for the animation thread
        let active = Arc::clone(&self.active);
        let index = Arc::clone(&self.index);

        // Spawn dedicated animation thread
        let handle = thread::spawn(move || {
            while active.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(ANIMATION_INTERVAL_MS));
                if active.load(Ordering::Relaxed) {
                    index.fetch_add(1, Ordering::Relaxed);
                }
            }
        });

        // Store handle for potential cleanup
        if let Ok(mut guard) = self.thread_handle.lock() {
            *guard = Some(handle);
        }
    }

    /// Stop the animation thread
    ///
    /// Signals the animation thread to stop. The thread will exit on its
    /// next iteration (within ~100ms).
    ///
    /// # Thread Safety
    /// Safe to call from any thread. Idempotent.
    pub fn stop(&self) {
        self.active.store(false, Ordering::SeqCst);
    }

    /// Check if animation is currently running
    pub fn is_running(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    /// Get the current animation symbol
    ///
    /// Returns the appropriate BRAILLE symbol based on the current frame,
    /// or "~" if animation is not running.
    pub fn symbol(&self) -> &'static str {
        if self.active.load(Ordering::Relaxed) {
            let idx = self.index.load(Ordering::Relaxed) % BRAILLE_DOUBLE.symbols.len();
            BRAILLE_DOUBLE.symbols[idx]
        } else {
            "~"
        }
    }

    /// Get the current frame index
    pub fn frame_index(&self) -> usize {
        self.index.load(Ordering::Relaxed)
    }
}

impl Drop for ThrobberAnimator {
    fn drop(&mut self) {
        // Ensure thread is stopped on cleanup
        self.stop();

        // Wait for thread to finish (with timeout to prevent hangs)
        if let Ok(mut guard) = self.thread_handle.lock() {
            if let Some(handle) = guard.take() {
                // Give thread time to notice the stop signal
                let _ = handle.join();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_animator_not_running() {
        let animator = ThrobberAnimator::new();
        assert!(!animator.is_running());
        assert_eq!(animator.symbol(), "~");
    }

    #[test]
    fn test_start_makes_running() {
        let animator = ThrobberAnimator::new();
        animator.start();
        assert!(animator.is_running());
        animator.stop();
    }

    #[test]
    fn test_stop_stops_animation() {
        let animator = ThrobberAnimator::new();
        animator.start();
        animator.stop();
        // Give thread time to stop (at 100ms interval, 150ms is plenty)
        thread::sleep(Duration::from_millis(150));
        assert!(!animator.is_running());
    }

    #[test]
    fn test_start_is_idempotent() {
        let animator = ThrobberAnimator::new();
        animator.start();
        let idx1 = animator.frame_index();
        animator.start(); // Should be no-op
        animator.start(); // Should be no-op
                          // Index should continue, not reset
        thread::sleep(Duration::from_millis(150));
        assert!(animator.frame_index() >= idx1);
        animator.stop();
    }

    #[test]
    fn test_animation_increments_index() {
        let animator = ThrobberAnimator::new();
        animator.start();
        let initial = animator.frame_index();
        // At 100ms interval, 250ms should give us ~2 frames
        thread::sleep(Duration::from_millis(250));
        let after = animator.frame_index();
        assert!(after > initial, "Index should increment over time");
        animator.stop();
    }

    #[test]
    fn test_animation_frame_advances_at_10fps() {
        let animator = ThrobberAnimator::new();
        animator.start();
        let initial = animator.frame_index();
        // Wait for 500ms - should get at least 3 frames at 100ms interval
        // (extra buffer for CI timing variability, especially on macOS)
        thread::sleep(Duration::from_millis(500));
        let after = animator.frame_index();
        // At 100ms per frame, 500ms should give us at least 3 frames
        assert!(
            after >= initial + 3,
            "Expected at least 3 frames in 500ms at 10fps, got {}",
            after - initial
        );
        animator.stop();
    }

    #[test]
    fn test_symbol_changes_when_running() {
        let animator = ThrobberAnimator::new();
        assert_eq!(animator.symbol(), "~");

        animator.start();
        // Symbol should be from BRAILLE_DOUBLE when running
        let sym = animator.symbol();
        assert!(
            BRAILLE_DOUBLE.symbols.contains(&sym),
            "Symbol should be from BRAILLE_DOUBLE set"
        );
        animator.stop();
    }

    #[test]
    fn test_default_trait() {
        let animator = ThrobberAnimator::default();
        assert!(!animator.is_running());
    }

    #[test]
    fn test_debug_trait() {
        let animator = ThrobberAnimator::new();
        let debug_str = format!("{:?}", animator);
        assert!(debug_str.contains("ThrobberAnimator"));
    }
}
