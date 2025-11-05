/// Auto-install missing commands
use anyhow::Result;
use crate::executor::command::CommandExecutor;

/// Package installer for missing commands
pub struct PackageInstaller;

impl PackageInstaller {
    /// Install a package using the system package manager
    pub async fn install_package(package: &str) -> Result<()> {
        let package_manager = Self::detect_package_manager()?;

        match package_manager.as_str() {
            "apt-get" => Self::install_with_apt(package).await,
            "yum" => Self::install_with_yum(package).await,
            "dnf" => Self::install_with_dnf(package).await,
            "pacman" => Self::install_with_pacman(package).await,
            "brew" => Self::install_with_brew(package).await,
            "choco" => Self::install_with_choco(package).await,
            "winget" => Self::install_with_winget(package).await,
            _ => anyhow::bail!("No supported package manager found"),
        }
    }

    /// Detect the available package manager
    fn detect_package_manager() -> Result<String> {
        let managers = vec![
            ("apt-get", "apt-get"),
            ("yum", "yum"),
            ("dnf", "dnf"),
            ("pacman", "pacman"),
            ("brew", "brew"),
            ("choco", "choco"),
            ("winget", "winget"),
        ];

        for (name, cmd) in managers {
            if CommandExecutor::command_exists(cmd) {
                return Ok(name.to_string());
            }
        }

        anyhow::bail!("No supported package manager found")
    }

    /// Install using apt-get (Debian/Ubuntu)
    async fn install_with_apt(package: &str) -> Result<()> {
        CommandExecutor::execute_sudo("apt-get", &[
            "install".to_string(),
            "-y".to_string(),
            package.to_string(),
        ])
        .await?;
        Ok(())
    }

    /// Install using yum (RedHat/CentOS)
    async fn install_with_yum(package: &str) -> Result<()> {
        CommandExecutor::execute_sudo("yum", &[
            "install".to_string(),
            "-y".to_string(),
            package.to_string(),
        ])
        .await?;
        Ok(())
    }

    /// Install using dnf (Fedora)
    async fn install_with_dnf(package: &str) -> Result<()> {
        CommandExecutor::execute_sudo("dnf", &[
            "install".to_string(),
            "-y".to_string(),
            package.to_string(),
        ])
        .await?;
        Ok(())
    }

    /// Install using pacman (Arch)
    async fn install_with_pacman(package: &str) -> Result<()> {
        CommandExecutor::execute_sudo("pacman", &[
            "-S".to_string(),
            "--noconfirm".to_string(),
            package.to_string(),
        ])
        .await?;
        Ok(())
    }

    /// Install using brew (macOS)
    async fn install_with_brew(package: &str) -> Result<()> {
        CommandExecutor::execute("brew", &["install".to_string(), package.to_string()])
            .await?;
        Ok(())
    }

    /// Install using chocolatey (Windows)
    async fn install_with_choco(package: &str) -> Result<()> {
        CommandExecutor::execute("choco", &[
            "install".to_string(),
            "-y".to_string(),
            package.to_string(),
        ])
        .await?;
        Ok(())
    }

    /// Install using winget (Windows)
    async fn install_with_winget(package: &str) -> Result<()> {
        CommandExecutor::execute("winget", &["install".to_string(), package.to_string()])
            .await?;
        Ok(())
    }

    /// Check if a package manager is available
    pub fn is_available() -> bool {
        Self::detect_package_manager().is_ok()
    }

    /// Get the name of the available package manager
    pub fn get_package_manager() -> Option<String> {
        Self::detect_package_manager().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_package_manager() {
        // This test will succeed if any package manager is available
        let _ = PackageInstaller::detect_package_manager();
    }

    #[test]
    fn test_is_available() {
        // Just check that the function doesn't panic
        let _ = PackageInstaller::is_available();
    }
}
