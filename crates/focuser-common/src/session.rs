//! What the desktop session will tell us about itself.
//!
//! Only one question so far, and it has an awkward answer on one platform.

/// Can this machine tell us which application is in front?
///
/// App allowances count down by sampling the focused window, so where the
/// answer is no, an app allowance can never move and never trigger a block.
///
/// Wayland is the no. It has no protocol for "which window is focused" — that
/// was left out deliberately, so one app cannot watch another. GNOME needs a
/// shell extension and KDE exposes nothing public, so there is no portable
/// implementation to write. Windows, macOS and X11 all answer fine.
///
/// The app asks this so it can say so on the Allowances page rather than
/// leaving someone staring at a timer that never moves.
pub fn app_usage_measurable() -> bool {
    !is_wayland()
}

fn is_wayland() -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    session.eq_ignore_ascii_case("wayland") || std::env::var_os("WAYLAND_DISPLAY").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_platform_but_wayland_can_measure() {
        // On CI this runs under Windows, macOS and X11-less Linux containers
        // alike; only a real Wayland session should answer false.
        assert_eq!(app_usage_measurable(), !is_wayland());
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn wayland_is_a_linux_only_concern() {
        assert!(!is_wayland());
        assert!(app_usage_measurable());
    }
}
