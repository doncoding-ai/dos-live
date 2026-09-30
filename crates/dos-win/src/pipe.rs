//! The relay: `\\.\pipe\dos-live-<SID>`, one JSON line in, one JSON line out.
//!
//! Three locks on the door: the name carries the user's SID, the pipe's DACL
//! only admits that user (and SYSTEM), and every connection is checked for a
//! same-user peer before a byte is read. Remote clients are rejected outright.

use std::io;
use std::sync::Arc;
use std::time::Duration;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    CloseHandle, LocalFree, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE, HANDLE, HLOCAL,
    INVALID_HANDLE_VALUE,
};
use windows::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FlushFileBuffers, ReadFile, WriteFile, FILE_FLAGS_AND_ATTRIBUTES, FILE_FLAG_FIRST_PIPE_INSTANCE,
    FILE_SHARE_NONE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, WaitNamedPipeW, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};

use crate::sid::{current_user_sid, peer_is_same_user};
use crate::wide;

pub const MAX_LINE: usize = 64 * 1024;

/// `\\.\pipe\dos-live-<sid>` — the same string on both ends.
pub fn pipe_name() -> String {
    let who = current_user_sid().unwrap_or_else(|| std::env::var("USERNAME").unwrap_or_else(|_| "user".into()));
    format!(r"\\.\pipe\dos-live-{who}")
}

/// Owns a HANDLE and closes it on drop.
struct Owned(HANDLE);

// A pipe handle may be used from the thread that serves the connection.
unsafe impl Send for Owned {}

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

/// A security descriptor admitting only this user and SYSTEM.
struct Dacl {
    sd: PSECURITY_DESCRIPTOR,
}

// The descriptor is immutable once built and only read by CreateNamedPipeW.
unsafe impl Send for Dacl {}
unsafe impl Sync for Dacl {}

impl Dacl {
    fn for_current_user() -> Option<Dacl> {
        let sid = current_user_sid()?;
        let sddl = wide(&format!("D:P(A;;GA;;;{sid})(A;;GA;;;SY)"));
        let mut sd = PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(PCWSTR(sddl.as_ptr()), SDDL_REVISION_1, &mut sd, None).ok()?;
        }
        Some(Dacl { sd })
    }

    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.sd.0,
            bInheritHandle: false.into(),
        }
    }
}

impl Drop for Dacl {
    fn drop(&mut self) {
        unsafe {
            let _ = LocalFree(Some(HLOCAL(self.sd.0)));
        }
    }
}

fn last_error() -> io::Error {
    io::Error::last_os_error()
}

fn create_instance(name: &[u16], first: bool, dacl: Option<&Dacl>) -> io::Result<Owned> {
    let mut mode: FILE_FLAGS_AND_ATTRIBUTES = PIPE_ACCESS_DUPLEX;
    if first {
        // Refuse to serve on a name somebody else already created.
        mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    let attrs = dacl.map(|d| d.attributes());
    let handle = unsafe {
        CreateNamedPipeW(
            PCWSTR(name.as_ptr()),
            mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            MAX_LINE as u32,
            MAX_LINE as u32,
            0,
            attrs.as_ref().map(|a| a as *const SECURITY_ATTRIBUTES),
        )
    };
    if handle.is_invalid() || handle == INVALID_HANDLE_VALUE {
        return Err(last_error());
    }
    Ok(Owned(handle))
}

fn read_line(h: HANDLE) -> io::Result<Vec<u8>> {
    let mut data = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let mut n = 0u32;
        let ok = unsafe { ReadFile(h, Some(&mut chunk), Some(&mut n), None) };
        if ok.is_err() || n == 0 {
            break;
        }
        data.extend_from_slice(&chunk[..n as usize]);
        if let Some(i) = data.iter().position(|b| *b == b'\n') {
            data.truncate(i);
            return Ok(data);
        }
        if data.len() > MAX_LINE {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "line too long"));
        }
    }
    if data.is_empty() {
        Err(io::Error::new(io::ErrorKind::UnexpectedEof, "nothing received"))
    } else {
        Ok(data)
    }
}

fn write_line(h: HANDLE, line: &str) -> io::Result<()> {
    let mut bytes = line.as_bytes().to_vec();
    bytes.push(b'\n');
    let mut off = 0;
    while off < bytes.len() {
        let mut n = 0u32;
        unsafe { WriteFile(h, Some(&bytes[off..]), Some(&mut n), None) }.map_err(|_| last_error())?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "pipe closed"));
        }
        off += n as usize;
    }
    unsafe {
        let _ = FlushFileBuffers(h);
    }
    Ok(())
}

/// Runs the relay server on background threads. `handle` gets each request
/// line and returns the reply line; it may block (an Ask waits for a human).
pub fn serve<F>(handle: F) -> io::Result<()>
where
    F: Fn(String) -> String + Send + Sync + 'static,
{
    let name = wide(&pipe_name());
    let dacl = Dacl::for_current_user();
    // Fail fast if another process owns the name.
    let first = create_instance(&name, true, dacl.as_ref())?;
    let handle = Arc::new(handle);
    std::thread::Builder::new().name("dos-relay".into()).spawn(move || {
        let mut next = Some(first);
        loop {
            let inst = match next.take() {
                Some(i) => i,
                None => match create_instance(&name, false, dacl.as_ref()) {
                    Ok(i) => i,
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                },
            };
            let connected = unsafe { ConnectNamedPipe(inst.0, None) };
            let ok = match connected {
                Ok(()) => true,
                Err(e) => e.code() == ERROR_PIPE_CONNECTED.to_hresult(),
            };
            if !ok {
                continue;
            }
            let h = handle.clone();
            let _ = std::thread::Builder::new().name("dos-relay-conn".into()).spawn(move || {
                let pipe = inst;
                if peer_is_same_user(pipe.0, true) {
                    if let Ok(bytes) = read_line(pipe.0) {
                        let reply = h(String::from_utf8_lossy(&bytes).into_owned());
                        let _ = write_line(pipe.0, &reply);
                    }
                }
                unsafe {
                    let _ = DisconnectNamedPipe(pipe.0);
                }
            });
        }
    })?;
    Ok(())
}

/// Client side (dosctl): send one request line, wait for the reply line.
/// `connect_timeout` bounds how long we wait for a free pipe instance.
pub fn request(line: &str, connect_timeout: Duration) -> io::Result<String> {
    let name = wide(&pipe_name());
    let deadline = std::time::Instant::now() + connect_timeout;
    let pipe = loop {
        let h = unsafe {
            CreateFileW(
                PCWSTR(name.as_ptr()),
                GENERIC_READ.0 | GENERIC_WRITE.0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        };
        match h {
            Ok(h) => break Owned(h),
            Err(e) if e.code() == ERROR_PIPE_BUSY.to_hresult() && std::time::Instant::now() < deadline => unsafe {
                let _ = WaitNamedPipeW(PCWSTR(name.as_ptr()), 250);
            },
            Err(e) => {
                return Err(io::Error::new(io::ErrorKind::NotConnected, format!("Dos Live is not running ({e})")));
            }
        }
    };
    if !peer_is_same_user(pipe.0, false) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the pipe is not served by your own Dos Live"));
    }
    write_line(pipe.0, line)?;
    let reply = read_line(pipe.0)?;
    Ok(String::from_utf8_lossy(&reply).into_owned())
}
