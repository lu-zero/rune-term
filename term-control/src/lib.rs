//! Versioned control protocol types for rune-term IPC and CLI.
//!
//! This crate defines the shared object model and read/query-oriented
//! request/response/event shapes used by the local control surface.
//! Transport, authentication/policy, and GUI integration live elsewhere.

use serde::{Deserialize, Serialize};

pub mod codec;
pub mod transport;
use std::path::PathBuf;

/// Protocol/API version for the control surface.
pub const PROTOCOL_VERSION: u32 = 1;

/// Default socket name within the runtime/config directory.
pub const SOCKET_NAME: &str = "control.sock";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Hello {
    pub protocol_version: u32,
    /// Client identifier (e.g. "rune-term-ctl").
    pub client: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// Handshake/capability discovery.
    Hello(Hello),
    /// List configured profiles (available without live GUI state).
    ListProfiles,
    /// Show one profile by name.
    GetProfile { name: String },
    /// List windows/tabs/panes from a running instance (IPC).
    ListTabs,
    /// Basic runtime/app info from a running instance (IPC).
    Info,
    /// Subscribe to read-only events (IPC).
    Subscribe { events: Vec<EventKind> },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Hello {
        protocol_version: u32,
        app: Option<AppInfo>,
    },
    Profiles {
        default_profile: String,
        profiles: Vec<ProfileInfo>,
    },
    Profile {
        profile: Option<ProfileInfo>,
    },
    Tabs {
        windows: Vec<WindowInfo>,
    },
    Info {
        app: AppInfo,
    },
    Subscribed {
        events: Vec<EventKind>,
    },
    Error {
        code: String,
        message: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    TabCreated,
    TabClosed,
    TabFocused,
    TitleChanged,
    Bell,
    Exited,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    TabCreated { tab: TabInfo },
    TabClosed { tab_id: String },
    TabFocused { window_id: String, tab_index: usize },
    TitleChanged { tab_id: String, title: String },
    Bell { tab_id: String },
    Exited { tab_id: String, code: Option<i32> },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub pid: Option<u32>,
    pub socket: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowInfo {
    pub id: String,
    pub focused: bool,
    pub tabs: Vec<TabInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TabInfo {
    pub id: String,
    pub index: usize,
    pub title: String,
    pub profile: String,
    pub active: bool,
    pub panes: Vec<PaneInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaneInfo {
    pub id: String,
    pub focused: bool,
    pub cols: u16,
    pub rows: u16,
    pub backend: String,
    /// Backend/session identity when attached to a multiplexer.
    pub session: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProfileInfo {
    pub name: String,
    pub is_default: bool,
    pub font_family: String,
    pub font_size: Option<f32>,
    pub theme: Option<String>,
    pub backend: String,
    pub shell_program: Option<String>,
    pub shell_args: Vec<String>,
    pub cwd: Option<String>,
}

/// Suggested location for the local control socket.
pub fn default_socket_path() -> Option<PathBuf> {
    runtime_dir()
        .or_else(config_dir)
        .map(|d| d.join(SOCKET_NAME))
}

fn runtime_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .map(|p| p.join("rune-term"))
}

fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return Some(
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("rune-term"),
            );
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return Some(PathBuf::from(appdata).join("rune-term"));
        }
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join("rune-term"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("rune-term"))
}
