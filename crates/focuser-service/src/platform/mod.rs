#[cfg(target_os = "linux")]
#[allow(clippy::collapsible_if)]
pub mod linux;
#[cfg(target_os = "macos")]
#[allow(clippy::collapsible_if)]
pub mod macos;
#[cfg(windows)]
#[allow(clippy::collapsible_if)]
pub mod windows;

use focuser_common::platform::PlatformBlocker;

/// Create the platform-specific blocker for the current OS.
pub fn create_blocker() -> Box<dyn PlatformBlocker> {
    #[cfg(windows)]
    {
        Box::new(windows::WindowsBlocker::new())
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxBlocker::new())
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacOsBlocker::new())
    }
}
