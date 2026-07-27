//! Detect and drain leftover stdin after a multi-line paste.
//!
//! Terminals treat newlines in a paste as Enter, so the first prompt only sees
//! the first line. Remaining lines must be consumed before the next prompt.

use std::io::{self, Write};
use std::thread;
use std::time::Duration;

/// Returns true when stdin has data that can be read without waiting for the user.
pub fn stdin_has_pending_input() -> bool {
    #[cfg(windows)]
    {
        stdin_has_pending_input_windows()
    }
    #[cfg(unix)]
    {
        stdin_has_pending_input_unix()
    }
    #[cfg(not(any(windows, unix)))]
    {
        false
    }
}

/// Reads all currently available stdin bytes without blocking when empty.
pub fn read_available_stdin_bytes() -> Vec<u8> {
    #[cfg(windows)]
    {
        read_available_stdin_bytes_windows()
    }
    #[cfg(unix)]
    {
        read_available_stdin_bytes_unix()
    }
    #[cfg(not(any(windows, unix)))]
    {
        Vec::new()
    }
}

/// Splits available stdin data into non-empty logical lines.
pub fn drain_available_stdin_lines() -> Vec<String> {
    let bytes = read_available_stdin_bytes();
    if bytes.is_empty() {
        return Vec::new();
    }

    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<String> = text
        .split('\n')
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect();

    if lines.last().map(|line| line.is_empty()).unwrap_or(false) {
        lines.pop();
    }

    lines.into_iter().filter(|line| !line.is_empty()).collect()
}

/// Reads a task name from stdin, absorbing multi-line paste into one string.
///
/// Unlike `dialoguer::Input`, this keeps reading while stdin still has pending
/// paste data (with short retries), then collapses whitespace/newlines.
pub fn read_pastable_line(prompt: &str) -> io::Result<String> {
    print!("{} › ", prompt);
    io::stdout().flush()?;

    let mut lines = Vec::new();
    let stdin = io::stdin();

    let mut first = String::new();
    stdin.read_line(&mut first)?;
    let first = first.trim_end_matches(['\r', '\n']).to_string();
    if !first.is_empty() {
        lines.push(first);
    }

    // Pull remaining paste lines that are already (or soon) buffered.
    for _ in 0..8 {
        thread::sleep(Duration::from_millis(25));
        if !stdin_has_pending_input() {
            // One more short wait — mintty/ConPTY sometimes delivers late.
            thread::sleep(Duration::from_millis(40));
            if !stdin_has_pending_input() {
                break;
            }
        }

        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']).to_string();
        if trimmed.is_empty() {
            break;
        }
        lines.push(trimmed);
    }

    // Byte-level drain as a last resort (pipes where read_line already returned).
    for extra in drain_available_stdin_lines() {
        lines.push(extra);
    }

    Ok(lines.join("\n"))
}

#[cfg(windows)]
fn stdin_has_pending_input_windows() -> bool {
    use winapi::shared::minwindef::{DWORD, FALSE, TRUE};
    use winapi::um::consoleapi::{GetNumberOfConsoleInputEvents, PeekConsoleInputA};
    use winapi::um::handleapi::INVALID_HANDLE_VALUE;
    use winapi::um::namedpipeapi::PeekNamedPipe;
    use winapi::um::processenv::GetStdHandle;
    use winapi::um::winbase::STD_INPUT_HANDLE;
    use winapi::um::wincontypes::{INPUT_RECORD, KEY_EVENT};
    use winapi::um::winnt::HANDLE;

    unsafe {
        let handle: HANDLE = GetStdHandle(STD_INPUT_HANDLE);
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return false;
        }

        // Pipe / pty (Git Bash, many terminals): byte backlog is authoritative.
        let mut available: DWORD = 0;
        if PeekNamedPipe(
            handle,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut available,
            std::ptr::null_mut(),
        ) != FALSE
            && available > 0
        {
            return true;
        }

        // Native console: only treat pending KEY_EVENT characters as paste remainder.
        let mut events: DWORD = 0;
        if GetNumberOfConsoleInputEvents(handle, &mut events) == FALSE || events == 0 {
            return false;
        }

        let mut records = vec![std::mem::zeroed::<INPUT_RECORD>(); events as usize];
        let mut read: DWORD = 0;
        if PeekConsoleInputA(handle, records.as_mut_ptr(), events, &mut read) == FALSE {
            return false;
        }

        for record in records.iter().take(read as usize) {
            if record.EventType == KEY_EVENT {
                let key = record.Event.KeyEvent();
                if key.bKeyDown == TRUE {
                    let unicode = *key.uChar.UnicodeChar();
                    let ascii = *key.uChar.AsciiChar() as u8;
                    if unicode != 0 || ascii != 0 {
                        return true;
                    }
                }
            }
        }

        false
    }
}

#[cfg(windows)]
fn read_available_stdin_bytes_windows() -> Vec<u8> {
    use std::ptr;
    use winapi::shared::minwindef::{DWORD, FALSE};
    use winapi::um::fileapi::ReadFile;
    use winapi::um::handleapi::INVALID_HANDLE_VALUE;
    use winapi::um::namedpipeapi::PeekNamedPipe;
    use winapi::um::processenv::GetStdHandle;
    use winapi::um::winbase::STD_INPUT_HANDLE;

    unsafe {
        let handle = GetStdHandle(STD_INPUT_HANDLE);
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Vec::new();
        }

        let mut available: DWORD = 0;
        let peeked = PeekNamedPipe(handle, ptr::null_mut(), 0, ptr::null_mut(), &mut available, ptr::null_mut());
        if peeked == FALSE || available == 0 {
            return Vec::new();
        }

        let mut buf = vec![0u8; available as usize];
        let mut read: DWORD = 0;
        let ok = ReadFile(
            handle,
            buf.as_mut_ptr() as *mut _,
            available,
            &mut read,
            ptr::null_mut(),
        );
        if ok == FALSE {
            return Vec::new();
        }
        buf.truncate(read as usize);
        buf
    }
}

#[cfg(unix)]
fn stdin_has_pending_input_unix() -> bool {
    use nix::poll::{poll, PollFd, PollFlags};
    use std::os::fd::AsFd;

    let stdin = std::io::stdin();
    let mut fds = [PollFd::new(stdin.as_fd(), PollFlags::POLLIN)];
    matches!(poll(&mut fds, 0u16), Ok(n) if n > 0)
}

#[cfg(unix)]
fn read_available_stdin_bytes_unix() -> Vec<u8> {
    use nix::poll::{poll, PollFd, PollFlags};
    use std::io::Read;
    use std::os::fd::{AsRawFd, BorrowedFd};

    let stdin = std::io::stdin();
    if !stdin_has_pending_input_unix() {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut stdin_lock = stdin.lock();
    loop {
        let mut fds = [PollFd::new(
            unsafe { BorrowedFd::borrow_raw(stdin_lock.as_raw_fd()) },
            PollFlags::POLLIN,
        )];
        match poll(&mut fds, 0u16) {
            Ok(n) if n > 0 => match stdin_lock.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => out.extend_from_slice(&chunk[..n]),
                Err(_) => break,
            },
            _ => break,
        }
        if out.len() > 64 * 1024 {
            break;
        }
    }
    out
}
