//! macOS microphone permission (TCC): asks for it explicitly before opening the microphone, and
//! names the app the permission belongs to. In the installed app that is Muse Sing; started with
//! `pnpm tauri dev` the binary is not an app bundle, so macOS charges the permission to the app it
//! was launched from (Terminal, iTerm, VS Code…) and shows the request, and the Settings entry, for it.

/// The microphone permission as macOS reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Granted,
    Denied,
    /// blocked by a profile or parental controls
    Restricted,
    /// never asked
    Undetermined,
}

#[cfg(target_os = "macos")]
mod imp {
    use super::Access;
    use block2::RcBlock;
    use objc2::runtime::{AnyClass, AnyObject, Bool};
    use objc2::{class, msg_send};
    use std::time::Duration;

    #[link(name = "AVFoundation", kind = "framework")]
    unsafe extern "C" {
        static AVMediaTypeAudio: &'static AnyObject;
    }

    unsafe extern "C" {
        fn proc_pidpath(pid: i32, buf: *mut u8, size: u32) -> i32;
        fn dlsym(handle: *mut std::ffi::c_void, symbol: *const std::ffi::c_char) -> *mut std::ffi::c_void;
    }
    const RTLD_DEFAULT: *mut std::ffi::c_void = -2isize as *mut std::ffi::c_void;

    /// `responsibility_get_pid_responsible_for_pid` (libsystem, private): the process macOS charges
    /// privacy permissions to. Looked up at run time, so that a macOS without it still starts the app.
    fn responsible_pid(pid: i32) -> Option<i32> {
        let f = unsafe { dlsym(RTLD_DEFAULT, c"responsibility_get_pid_responsible_for_pid".as_ptr()) };
        if f.is_null() {
            return None;
        }
        let f: extern "C" fn(i32) -> i32 = unsafe { std::mem::transmute(f) };
        Some(f(pid)).filter(|&p| p > 0)
    }

    fn device() -> &'static AnyClass {
        class!(AVCaptureDevice)
    }

    pub fn status() -> Access {
        // AVAuthorizationStatus: 0 not determined, 1 restricted, 2 denied, 3 authorized
        let s: isize = unsafe { msg_send![device(), authorizationStatusForMediaType: AVMediaTypeAudio] };
        match s {
            3 => Access::Granted,
            2 => Access::Denied,
            1 => Access::Restricted,
            _ => Access::Undetermined,
        }
    }

    pub fn request() -> Access {
        let (tx, rx) = std::sync::mpsc::channel::<bool>();
        let block = RcBlock::new(move |granted: Bool| {
            let _ = tx.send(granted.as_bool());
        });
        unsafe {
            let _: () = msg_send![device(), requestAccessForMediaType: AVMediaTypeAudio, completionHandler: &*block];
        }
        // the user has time to read the dialog; if it never answers, the stream opening says the rest
        match rx.recv_timeout(Duration::from_secs(120)) {
            Ok(true) => Access::Granted,
            Ok(false) => Access::Denied,
            Err(_) => status(),
        }
    }

    pub fn responsible_path() -> Option<String> {
        let pid = responsible_pid(std::process::id() as i32)?;
        unsafe {
            let mut buf = vec![0u8; 4096];
            let n = proc_pidpath(pid, buf.as_mut_ptr(), buf.len() as u32);
            if n <= 0 {
                return None;
            }
            buf.truncate(n as usize);
            String::from_utf8(buf).ok()
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::Access;
    pub fn status() -> Access {
        Access::Granted
    }
    pub fn request() -> Access {
        Access::Granted
    }
    pub fn responsible_path() -> Option<String> {
        None
    }
}

/// The name of the app shown in System Settings for an executable path: the outermost `.app`
/// (`/Applications/iTerm.app/Contents/MacOS/iTerm2` → "iTerm"), else the file name.
pub fn app_name(path: &str) -> String {
    if let Some(app) = path.split('/').find(|c| c.ends_with(".app")) {
        return app.trim_end_matches(".app").to_string();
    }
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// The app the microphone permission belongs to ("Muse Sing" in the installed app).
pub fn owner() -> String {
    imp::responsible_path().map(|p| app_name(&p)).unwrap_or_else(|| "Muse Sing".into())
}

/// Makes sure the microphone may be used: asks macOS (showing its dialog) if it never did.
/// The error names the app to switch on in System Settings.
pub fn ensure() -> crate::error::Result<()> {
    let before = imp::status();
    let s = if before == Access::Undetermined { imp::request() } else { before };
    let path = imp::responsible_path();
    // in the terminal of `pnpm tauri dev`: what macOS said and whom it charges
    eprintln!("microfono: permesso {before:?} → {s:?}, attribuito a {}", path.as_deref().unwrap_or("?"));
    match s {
        Access::Granted => Ok(()),
        Access::Undetermined => Err(crate::error::Error::Other(format!(
            "Microfono: macOS non ha mostrato la richiesta del permesso per «{}». Chiudi e riapri Muse Sing e riprova; se si ripete, riferisci questo messaggio. [permesso non chiesto: {}]",
            owner(),
            path.as_deref().unwrap_or("?")
        ))),
        Access::Denied => Err(crate::error::Error::Other(format!(
            "Microfono: macOS non dà il permesso a «{0}». Apri Impostazioni di Sistema → Privacy e sicurezza → Microfono, attiva «{0}» e riaccendi il microfono (a volte serve riaprire «{0}»). [permesso negato: {1}]",
            owner(),
            path.as_deref().unwrap_or("?")
        ))),
        Access::Restricted => Err(crate::error::Error::Other(format!(
            "Microfono: il microfono è bloccato per «{}» da un profilo o dalle restrizioni di questo Mac. [permesso limitato]",
            owner()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_names() {
        assert_eq!(app_name("/Applications/iTerm.app/Contents/MacOS/iTerm2"), "iTerm");
        assert_eq!(app_name("/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal"), "Terminal");
        assert_eq!(app_name("/Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper.app/Contents/MacOS/Code Helper"), "Visual Studio Code");
        assert_eq!(app_name("/Applications/Muse Sing.app/Contents/MacOS/muse-sing"), "Muse Sing");
        assert_eq!(app_name("/usr/local/bin/tool"), "tool");
    }

    #[test]
    fn granted_off_macos() {
        if !cfg!(target_os = "macos") {
            assert!(ensure().is_ok());
        }
    }
}
