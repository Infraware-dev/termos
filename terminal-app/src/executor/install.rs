/// Package installer using Strategy Pattern
///
/// This module provides a high-level interface for installing packages
/// using the appropriate package manager for the system.
use anyhow::Result;

use super::package_manager::{
    AptPackageManager, BrewPackageManager, ChocoPackageManager, DnfPackageManager, PackageManager,
    PacmanPackageManager, WingetPackageManager, YumPackageManager,
};

/// Package installer that automatically selects the best package manager
#[derive(Debug)]
pub struct PackageInstaller {
    managers: Vec<Box<dyn PackageManager>>,
}

impl PackageInstaller {
    /// Create a new package installer with all supported package managers
    #[must_use]
    pub fn new() -> Self {
        let managers: Vec<Box<dyn PackageManager>> = vec![
            Box::new(AptPackageManager),
            Box::new(YumPackageManager),
            Box::new(DnfPackageManager),
            Box::new(PacmanPackageManager),
            Box::new(BrewPackageManager),
            Box::new(ChocoPackageManager),
            Box::new(WingetPackageManager),
        ];

        Self { managers }
    }

    /// Create an installer with custom package managers
    #[allow(
        dead_code,
        reason = "Constructor for custom manager list, used in testing"
    )]
    #[must_use]
    pub fn with_managers(managers: Vec<Box<dyn PackageManager>>) -> Self {
        Self { managers }
    }

    /// Install a package using the best available package manager
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No package manager is available on the system
    /// - The selected package manager fails to install the package
    #[allow(
        dead_code,
        reason = "Auto-install API for M2/M3, called by package manager implementations"
    )]
    pub async fn install_package(&self, package: &str) -> Result<()> {
        let manager = self.select_package_manager()?;
        manager.install(package).await
    }

    /// Select the best available package manager based on availability and priority
    fn select_package_manager(&self) -> Result<&dyn PackageManager> {
        self.managers
            .iter()
            .filter(|m| m.is_available())
            .max_by_key(|m| m.priority())
            .map(std::convert::AsRef::as_ref)
            .ok_or_else(|| anyhow::anyhow!("No supported package manager found"))
    }

    /// Check if any package manager is available
    #[must_use]
    pub fn is_available(&self) -> bool {
        self.managers.iter().any(|m| m.is_available())
    }

    /// Get the name of the selected package manager
    #[must_use]
    pub fn get_package_manager(&self) -> Option<&str> {
        self.select_package_manager()
            .ok()
            .map(super::package_manager::PackageManager::name)
    }

    /// Get all available package managers
    #[allow(
        dead_code,
        reason = "Diagnostic API for package manager discovery, used in M2/M3"
    )]
    #[must_use]
    pub fn get_available_managers(&self) -> Vec<&str> {
        self.managers
            .iter()
            .filter(|m| m.is_available())
            .map(|m| m.name())
            .collect()
    }
}

impl Default for PackageInstaller {
    fn default() -> Self {
        Self::new()
    }
}

// Static methods for backward compatibility
impl PackageInstaller {
    /// Check if any package manager is available (static method for compatibility)
    #[must_use]
    pub fn is_available_static() -> bool {
        Self::new().is_available()
    }

    /// Get the name of the available package manager (static method for compatibility)
    #[allow(
        dead_code,
        reason = "Static API for backward compatibility, used in M2/M3"
    )]
    #[must_use]
    pub fn get_package_manager_static() -> Option<String> {
        Self::new()
            .get_package_manager()
            .map(std::string::ToString::to_string)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_installer_creation() {
        let installer = PackageInstaller::new();
        assert!(!installer.managers.is_empty());
    }

    #[test]
    fn test_is_available() {
        let installer = PackageInstaller::new();
        // Just check that it doesn't panic
        let _ = installer.is_available();
    }

    #[test]
    fn test_get_available_managers() {
        let installer = PackageInstaller::new();
        let managers = installer.get_available_managers();
        // On most systems, at least one package manager should be available
        // But we just test that the method works
        assert!(managers.len() <= 7); // Max 7 supported managers
    }

    #[test]
    fn test_static_methods() {
        // Test backward compatibility
        let _ = PackageInstaller::is_available_static();
        let _ = PackageInstaller::get_package_manager_static();
    }

    #[tokio::test]
    async fn test_custom_managers() {
        // Test that we can create installer with custom managers
        let managers: Vec<Box<dyn PackageManager>> =
            vec![Box::new(BrewPackageManager), Box::new(AptPackageManager)];

        let installer = PackageInstaller::with_managers(managers);
        assert_eq!(installer.managers.len(), 2);
    }
}
