use focuser_ipc::{AgentRequest, WindowObservation};
#[cfg(windows)]
pub fn ours(pid: u32) -> bool {
    let Ok((sid, session)) = focuser_ipc::windows::current_identity() else {
        return false;
    };
    focuser_ipc::windows::process_identity(pid).is_ok_and(|p| p.sid == sid && p.session == session)
}
#[cfg(not(windows))]
pub fn ours(_: u32) -> bool {
    false
}

#[cfg(windows)]
pub fn sample() -> AgentRequest {
    use windows::Win32::{
        Foundation::*,
        System::SystemInformation::GetTickCount,
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    };
    unsafe fn window(hwnd: HWND) -> Option<WindowObservation> {
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 || !ours(pid) {
                return None;
            }
            let mut title = [0u16; 2048];
            let len = GetWindowTextW(hwnd, &mut title);
            Some(WindowObservation {
                pid,
                title: String::from_utf16_lossy(&title[..len.max(0) as usize]),
            })
        }
    }
    unsafe extern "system" fn each(hwnd: HWND, parameter: LPARAM) -> BOOL {
        unsafe {
            let result = &mut *(parameter.0 as *mut Vec<WindowObservation>);
            if result.len() >= 256 {
                return false.into();
            }
            if IsWindowVisible(hwnd).as_bool()
                && let Some(window) = window(hwnd)
            {
                result.push(window);
            }
            true.into()
        }
    }
    unsafe {
        let mut windows = Vec::new();
        let _ = EnumWindows(
            Some(each),
            LPARAM((&mut windows as *mut Vec<WindowObservation>) as isize),
        );
        let foreground = window(GetForegroundWindow());
        let mut input = LASTINPUTINFO {
            cbSize: size_of::<LASTINPUTINFO>() as u32,
            ..Default::default()
        };
        let idle = !GetLastInputInfo(&mut input).as_bool()
            || GetTickCount().wrapping_sub(input.dwTime) >= 60_000;
        AgentRequest::Observe {
            foreground,
            windows,
            idle,
        }
    }
}
#[cfg(not(windows))]
pub fn sample() -> AgentRequest {
    AgentRequest::Observe {
        foreground: None,
        windows: vec![],
        idle: true,
    }
}

/// Resolve the actual kernel TCP endpoint owner. Headers/Origin are additional
/// filters; neither is accepted as a Windows identity.
#[cfg(windows)]
pub fn http_peer(address: std::net::SocketAddr, local_port: u16) -> bool {
    use windows::Win32::NetworkManagement::IpHelper::*;
    use windows::Win32::Networking::WinSock::AF_INET;
    if !address.ip().is_loopback() {
        return false;
    }
    unsafe {
        let mut size = 0;
        let _ = GetExtendedTcpTable(
            None,
            &mut size,
            false,
            AF_INET.0 as u32,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        );
        if size == 0 || size > 16 * 1024 * 1024 {
            return false;
        }
        let mut buffer = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
        if GetExtendedTcpTable(
            Some(buffer.as_mut_ptr().cast()),
            &mut size,
            false,
            AF_INET.0 as u32,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        ) != 0
        {
            return false;
        }
        let table = &*buffer.as_ptr().cast::<MIB_TCPTABLE_OWNER_PID>();
        let max = (buffer.len() * size_of::<usize>() - size_of::<u32>())
            / size_of::<MIB_TCPROW_OWNER_PID>();
        if table.dwNumEntries as usize > max {
            return false;
        }
        std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize)
            .iter()
            .any(|row| {
                u16::from_be(row.dwLocalPort as u16) == address.port()
                    && u16::from_be(row.dwRemotePort as u16) == local_port
                    && row.dwLocalAddr == u32::from_ne_bytes([127, 0, 0, 1])
                    && row.dwRemoteAddr == u32::from_ne_bytes([127, 0, 0, 1])
                    && ours(row.dwOwningPid)
            })
    }
}
#[cfg(not(windows))]
pub fn http_peer(_: std::net::SocketAddr, _: u16) -> bool {
    false
}
