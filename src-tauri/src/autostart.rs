use std::path::PathBuf;

use tauri::AppHandle;
use thiserror::Error;

pub(crate) const AUTOSTART_ARGUMENT: &str = "--autostart";

#[derive(Debug, Error)]
pub(crate) enum AutostartError {
    #[error("无法读取当前应用路径：{0}")]
    Executable(#[source] std::io::Error),
    #[error("无法访问用户目录：{0}")]
    HomeDirectory(String),
    #[error("无法写入开机自启动配置：{0}")]
    Io(#[source] std::io::Error),
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    #[error("当前平台不支持开机自启动")]
    Unsupported,
}

fn current_executable() -> Result<PathBuf, AutostartError> {
    std::env::current_exe().map_err(AutostartError::Executable)
}

pub(crate) fn is_enabled(app: &AppHandle) -> Result<bool, AutostartError> {
    platform::is_enabled(app)
}

pub(crate) fn set_enabled(app: &AppHandle, enabled: bool) -> Result<(), AutostartError> {
    platform::set_enabled(app, enabled)
}

#[cfg(target_os = "macos")]
mod platform {
    use std::{
        fs::{self, OpenOptions},
        io::Write,
        path::{Path, PathBuf},
    };

    use tauri::{AppHandle, Manager};

    use super::{current_executable, AutostartError, AUTOSTART_ARGUMENT};

    const LAUNCH_AGENT_LABEL: &str = "com.electronicjourney.app";

    fn launch_agent_path(app: &AppHandle) -> Result<PathBuf, AutostartError> {
        let home = app
            .path()
            .home_dir()
            .map_err(|error| AutostartError::HomeDirectory(error.to_string()))?;
        Ok(home
            .join("Library")
            .join("LaunchAgents")
            .join(format!("{LAUNCH_AGENT_LABEL}.plist")))
    }

    fn xml_escape(value: &str) -> String {
        value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }

    fn launch_agent_contents(executable: &Path) -> String {
        let executable = xml_escape(&executable.to_string_lossy());
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n\
<dict>\n\
    <key>Label</key>\n\
    <string>{LAUNCH_AGENT_LABEL}</string>\n\
    <key>ProgramArguments</key>\n\
    <array>\n\
        <string>{executable}</string>\n\
        <string>{AUTOSTART_ARGUMENT}</string>\n\
    </array>\n\
    <key>RunAtLoad</key>\n\
    <true/>\n\
    <key>LimitLoadToSessionType</key>\n\
    <string>Aqua</string>\n\
</dict>\n\
</plist>\n"
        )
    }

    fn write_atomic(path: &Path, contents: &str) -> Result<(), AutostartError> {
        let parent = path
            .parent()
            .ok_or_else(|| AutostartError::HomeDirectory("无法确定 LaunchAgents 目录".into()))?;
        fs::create_dir_all(parent).map_err(AutostartError::Io)?;

        let temporary_path = parent.join(format!(
            ".{LAUNCH_AGENT_LABEL}.{}.plist.tmp",
            std::process::id()
        ));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary_path)
                .map_err(AutostartError::Io)?;
            file.write_all(contents.as_bytes())
                .map_err(AutostartError::Io)?;
            file.sync_all().map_err(AutostartError::Io)?;
            drop(file);
            fs::rename(&temporary_path, path).map_err(AutostartError::Io)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }

    pub(super) fn is_enabled(app: &AppHandle) -> Result<bool, AutostartError> {
        let path = launch_agent_path(app)?;
        let executable = current_executable()?;
        match fs::read_to_string(path) {
            Ok(contents) => Ok(contents == launch_agent_contents(&executable)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(AutostartError::Io(error)),
        }
    }

    pub(super) fn set_enabled(app: &AppHandle, enabled: bool) -> Result<(), AutostartError> {
        let path = launch_agent_path(app)?;
        if enabled {
            let executable = current_executable()?;
            write_atomic(&path, &launch_agent_contents(&executable))
        } else {
            match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(AutostartError::Io(error)),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::launch_agent_contents;
        use std::path::Path;

        #[test]
        fn launch_agent_starts_the_current_binary_in_the_user_session() {
            let contents = launch_agent_contents(Path::new(
                "/Applications/Electronic Journey.app/Contents/MacOS/electronic-journey-desktop",
            ));

            assert!(contents.contains("<key>RunAtLoad</key>"));
            assert!(contents.contains("<string>--autostart</string>"));
            assert!(contents.contains("Electronic Journey.app"));
            assert!(contents.contains("<string>Aqua</string>"));
        }

        #[test]
        fn launch_agent_escapes_xml_characters_in_paths() {
            let contents = launch_agent_contents(Path::new("/Users/me/EJ & Archive/<release>.app"));

            assert!(contents.contains("EJ &amp; Archive/&lt;release&gt;.app"));
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use std::io::ErrorKind;

    use tauri::AppHandle;
    use winreg::{
        enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE},
        RegKey,
    };

    use super::{current_executable, AutostartError, AUTOSTART_ARGUMENT};

    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const VALUE_NAME: &str = "Electronic Journey";

    fn run_command() -> Result<String, AutostartError> {
        let executable = current_executable()?;
        Ok(format!(
            "\"{}\" {AUTOSTART_ARGUMENT}",
            executable.to_string_lossy()
        ))
    }

    pub(super) fn is_enabled(_app: &AppHandle) -> Result<bool, AutostartError> {
        let run_key = RegKey::predef(HKEY_CURRENT_USER);
        let key = match run_key.open_subkey_with_flags(RUN_KEY, KEY_READ) {
            Ok(key) => key,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(AutostartError::Io(error)),
        };

        match key.get_value::<String, _>(VALUE_NAME) {
            Ok(value) => Ok(value == run_command()?),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
            Err(error) => Err(AutostartError::Io(error)),
        }
    }

    pub(super) fn set_enabled(_app: &AppHandle, enabled: bool) -> Result<(), AutostartError> {
        let run_key = RegKey::predef(HKEY_CURRENT_USER);
        if enabled {
            let (key, _) = run_key.create_subkey(RUN_KEY).map_err(AutostartError::Io)?;
            let command = run_command()?;
            key.set_value(VALUE_NAME, &command)
                .map_err(AutostartError::Io)
        } else {
            let key = match run_key.open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE) {
                Ok(key) => key,
                Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(AutostartError::Io(error)),
            };
            match key.delete_value(VALUE_NAME) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
                Err(error) => Err(AutostartError::Io(error)),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn run_command_quotes_the_executable_before_the_startup_argument() {
            let executable =
                r#"C:\Program Files\Electronic Journey\electronic-journey-desktop.exe"#;
            let command = format!("\"{executable}\" --autostart");

            assert_eq!(
                command,
                r#""C:\Program Files\Electronic Journey\electronic-journey-desktop.exe" --autostart"#
            );
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use tauri::AppHandle;

    use super::AutostartError;

    pub(super) fn is_enabled(_app: &AppHandle) -> Result<bool, AutostartError> {
        Ok(false)
    }

    pub(super) fn set_enabled(_app: &AppHandle, _enabled: bool) -> Result<(), AutostartError> {
        Err(AutostartError::Unsupported)
    }
}
