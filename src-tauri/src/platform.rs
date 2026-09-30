// Windows: platform_win.rs. Anywhere else: inert stand-ins with the exact same
// shapes, so the app builds and type-checks on a Linux CI box or dev machine.

#[cfg(windows)]
#[path = "platform_win.rs"]
mod imp;

#[cfg(not(windows))]
mod imp {
    #![allow(dead_code)]
    use std::time::Duration;

    #[derive(Debug, Clone, PartialEq)]
    pub enum Heard {
        Command,
        Dictation,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct VoiceInfo {
        pub name: String,
        pub language: String,
        pub male: bool,
    }

    #[derive(Debug, Clone)]
    pub enum VoiceEvent {
        Heard { text: String, kind: Heard, confident: bool },
        Listening(bool),
        Dictating(bool),
        NothingHeard,
        Spoken { id: u64, wav: Vec<u8> },
        Voices(Vec<VoiceInfo>),
        Problem(String),
    }

    #[derive(Clone)]
    pub struct Voice;

    impl Voice {
        pub fn start<F>(on_event: F) -> Voice
        where
            F: Fn(VoiceEvent) + Send + Sync + 'static,
        {
            on_event(VoiceEvent::Problem("voice is only available on Windows".into()));
            Voice
        }
        pub fn listen(&self, _phrases: Vec<String>) {}
        pub fn stop_listening(&self) {}
        pub fn pause(&self) {}
        pub fn resume(&self) {}
        pub fn dictate(&self) {}
        pub fn speak(&self, _id: u64, _text: String, _voice: String, _rate: f64) {}
        pub fn list_voices(&self) {}
    }

    pub fn make_non_activating(_hwnd: isize) {}
    pub fn set_activating(_hwnd: isize, _activating: bool) {}
    pub fn exclude_from_capture(_hwnd: isize, _hidden: bool) -> bool {
        false
    }
    pub fn cursor_pos() -> Option<(i32, i32)> {
        None
    }
    pub fn mic_users() -> Vec<String> {
        vec![]
    }
    pub fn serve_relay<F>(_handle: F) -> std::io::Result<()>
    where
        F: Fn(String) -> String + Send + Sync + 'static,
    {
        Ok(())
    }
    pub fn relay_self_test() -> std::io::Result<String> {
        let _ = Duration::from_secs(0);
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "the relay pipe is Windows-only"))
    }
    pub fn start_voice<F>(on_event: F) -> Voice
    where
        F: Fn(VoiceEvent) + Send + Sync + 'static,
    {
        Voice::start(on_event)
    }
}

pub use imp::*;
