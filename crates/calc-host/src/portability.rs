//! Host provisions shared by availability checks and portability reports.

use std::collections::BTreeMap;

pub use calc_object_model::{descriptor::Need, policy::Provision};

/// Execution environment. A browser client can use a remote engine or a local worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostKind {
    /// Windows desktop.
    Windows,
    /// macOS desktop.
    Macos,
    /// Linux desktop.
    Linux,
    /// Headless CLI or service.
    Server,
    /// Native engine with a remote browser front end.
    WebHosted,
    /// Engine running inside a browser worker.
    WebWasm,
    /// Engine running under a WASI runtime.
    Wasi,
}

impl HostKind {
    /// All report columns in the order used by the provision table.
    pub const ALL: [Self; 7] = [
        Self::Windows,
        Self::Macos,
        Self::Linux,
        Self::Server,
        Self::WebHosted,
        Self::WebWasm,
        Self::Wasi,
    ];

    /// Name used in reports and on the command line.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Macos => "macos",
            Self::Linux => "linux",
            Self::Server => "server",
            Self::WebHosted => "web-hosted",
            Self::WebWasm => "web-wasm",
            Self::Wasi => "wasi",
        }
    }

    /// Default for the local build. A headless or remote embedder supplies its own profile.
    pub const fn local() -> Self {
        if cfg!(target_os = "wasi") {
            Self::Wasi
        } else if cfg!(target_arch = "wasm32") {
            Self::WebWasm
        } else if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Server
        }
    }

    const fn provision(self, need: Need) -> Provision {
        use Provision::{Host, Mediated, Model, Native, Reject, Replace, Shim};
        // Column order follows PORTABLE_EXECUTION.md section 2.2 and HostKind::ALL.
        let row = match need {
            Need::FileTree => [Native, Native, Native, Native, Native, Shim, Native],
            Need::PersonalDisk => [Native, Native, Native, Native, Mediated, Mediated, Native],
            Need::WindowsPath | Need::FileAttributes | Need::Environment | Need::Settings => {
                [Native, Shim, Shim, Shim, Shim, Shim, Shim]
            }
            Need::Process | Need::Dll | Need::Xll | Need::OsCom | Need::Keys => {
                [Native, Reject, Reject, Reject, Reject, Reject, Reject]
            }
            Need::Dylib => [Reject, Native, Reject, Reject, Reject, Reject, Reject],
            Need::PortableClass => [
                Replace, Replace, Replace, Replace, Replace, Replace, Replace,
            ],
            Need::Network => [Native, Native, Native, Native, Native, Mediated, Reject],
            Need::Dialog => [Host, Host, Host, Reject, Host, Reject, Reject],
            Need::Forms => [Native, Replace, Replace, Reject, Replace, Replace, Reject],
            Need::FileDialog => [Host, Host, Host, Reject, Host, Mediated, Reject],
            Need::Clipboard => [Host, Host, Host, Model, Mediated, Mediated, Model],
            Need::Print => [Host, Host, Host, Replace, Host, Host, Reject],
            Need::OpenExternal => [Native, Native, Native, Reject, Mediated, Mediated, Reject],
            Need::Block => [Native, Native, Native, Native, Native, Reject, Native],
            Need::Codegen => [Native, Native, Native, Native, Native, Reject, Reject],
            Need::Locale | Need::Codepage | Need::InternalClipboard | Need::CommandBars => {
                [Model; 7]
            }
            Need::Clock | Need::Identity | Need::Status => [Host; 7],
            Need::EventLoop => [Host, Host, Host, Host, Host, Host, Reject],
        };
        row[self as usize]
    }
}

/// Default provisions for one host, with facilities an embedder can configure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostProfile {
    kind: HostKind,
    overrides: BTreeMap<Need, Provision>,
}

impl HostProfile {
    /// Windows desktop defaults.
    pub const WINDOWS: Self = Self::new(HostKind::Windows);
    /// macOS desktop defaults.
    pub const MACOS: Self = Self::new(HostKind::Macos);
    /// Linux desktop defaults.
    pub const LINUX: Self = Self::new(HostKind::Linux);
    /// Headless CLI or service defaults.
    pub const SERVER: Self = Self::new(HostKind::Server);
    /// Remote browser front-end defaults.
    pub const WEB_HOSTED: Self = Self::new(HostKind::WebHosted);
    /// Browser worker defaults.
    pub const WEB_WASM: Self = Self::new(HostKind::WebWasm);
    /// WASI runtime defaults.
    pub const WASI: Self = Self::new(HostKind::Wasi);

    /// Start with a host's default provisions.
    pub const fn new(kind: HostKind) -> Self {
        Self {
            kind,
            overrides: BTreeMap::new(),
        }
    }

    /// Environment from which the profile takes its defaults.
    pub const fn kind(&self) -> HostKind {
        self.kind
    }

    /// Effective provision for a facility.
    pub fn provision(&self, need: Need) -> Provision {
        self.overrides
            .get(&need)
            .copied()
            .unwrap_or_else(|| self.kind.provision(need))
    }

    /// Record a facility provided by this embedding or disabled in it.
    pub fn with_provision(mut self, need: Need, provision: Provision) -> Self {
        self.overrides.insert(need, provision);
        self
    }
}
