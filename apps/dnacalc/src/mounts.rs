//! Native host setup for the Windows namespace used by the VBA file facet.

use crate::{Cli, Failure, fail};
use calc_host_vba::files::{FileSession, HostPolicy, WindowsMountAccess};
use calc_macro_security::MacroSecurity;
use serde::Deserialize;
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    default_directory: Option<String>,
    #[serde(default)]
    mount: Vec<Mount>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Mount {
    windows: String,
    host: PathBuf,
    #[serde(default)]
    mode: Mode,
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Mode {
    Read,
    #[default]
    ReadWrite,
}

impl From<Mode> for WindowsMountAccess {
    fn from(value: Mode) -> Self {
        match value {
            Mode::Read => Self::Read,
            Mode::ReadWrite => Self::ReadWrite,
        }
    }
}

fn home_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

fn home_component(home: Option<&Path>) -> &str {
    home.and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .filter(|name| !name.contains(['\\', '/', ':', '*', '?', '<', '>', '"', '|']))
        .unwrap_or("User")
}

fn config_path(cli: &Cli, home: Option<&Path>) -> Option<PathBuf> {
    cli.mounts.as_deref().map(PathBuf::from).or_else(|| {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| home.map(|path| path.join(".config")))
            .map(|path| path.join("dnacalc/mounts.toml"))
    })
}

fn read_config(cli: &Cli, home: Option<&Path>) -> Result<Option<(PathBuf, Config)>, Failure> {
    let Some(path) = config_path(cli, home) else {
        return Ok(None);
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound && cli.mounts.is_none() => {
            return Ok(None);
        }
        Err(error) => {
            return Err(fail(format!(
                "cannot read mount configuration {}: {error}",
                path.display()
            )));
        }
    };
    let config = toml::from_str(&text).map_err(|error| {
        fail(format!(
            "invalid mount configuration {}: {error}",
            path.display()
        ))
    })?;
    Ok(Some((path, config)))
}

fn add_mount(policy: HostPolicy, mut mount: Mount) -> Result<HostPolicy, Failure> {
    let host = std::fs::canonicalize(&mount.host).map_err(|error| {
        fail(format!(
            "mount directory {} is unavailable: {error}",
            mount.host.display()
        ))
    })?;
    if !host.is_dir() {
        return Err(fail(format!(
            "mount directory {} is unavailable",
            mount.host.display()
        )));
    }
    if mount
        .windows
        .trim_end_matches(['/', '\\'])
        .eq_ignore_ascii_case("C:")
        && host.parent().is_none()
    {
        return Err(fail("the Windows C: root must map to a private directory"));
    }
    mount.host = host;
    let access = mount.mode.into();
    if let Some(share) = mount.windows.strip_prefix(r"\\") {
        let mut parts = share.trim_end_matches('\\').split('\\');
        if let (Some(server), Some(share), None) = (parts.next(), parts.next(), parts.next())
            && [server, share].into_iter().all(|part| {
                !part.is_empty()
                    && !matches!(part, "." | "..")
                    && !part.contains(['/', ':', '*', '?', '<', '>', '"', '|', '\0'])
            })
        {
            return Ok(policy.with_windows_unc_mount(server, share, mount.host, access));
        }
        return Err(fail(format!("invalid UNC mount {:?}", mount.windows)));
    }
    policy
        .with_windows_drive_mount(&mount.windows, mount.host, access)
        .map_err(|error| fail(format!("invalid mount {:?}: {error}", mount.windows)))
}

fn built_in(
    mut policy: HostPolicy,
    home: Option<&Path>,
    user: &str,
    unguarded: bool,
) -> Result<(HostPolicy, String), Failure> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| home.map(|path| path.join(".local/share")))
        .ok_or_else(|| fail("mount configuration needs XDG_DATA_HOME, HOME or --mounts"))?;
    let private = data.join("dnacalc/drive_c");
    let user_prefix = format!(r"C:\Users\{user}");
    let default_directory = format!(r"{user_prefix}\Documents");
    let private_documents = private.join("Users").join(user).join("Documents");
    std::fs::create_dir_all(&private_documents).map_err(|error| {
        fail(format!(
            "cannot create private workbook tree {}: {error}",
            private.display()
        ))
    })?;
    policy = policy.with_windows_drive_root('C', &private);

    if let Some(home) = home.filter(|_| unguarded) {
        let documents = home.join("Documents");
        let desktop = home.join("Desktop");
        let temp = home.join("tmp");
        let documents = if documents.is_dir() {
            documents
        } else {
            private_documents
        };
        for (windows, host) in [
            (user_prefix.clone(), home.to_owned()),
            (default_directory.clone(), documents),
            (format!(r"{user_prefix}\Desktop"), desktop),
            (r"C:\Temp".to_owned(), temp),
        ] {
            if host.is_dir() {
                policy = add_mount(
                    policy,
                    Mount {
                        windows,
                        host,
                        mode: Mode::ReadWrite,
                    },
                )?;
            }
        }
    }
    Ok((policy, default_directory))
}

fn host_path(path: &Path, config: &Path, home: Option<&Path>) -> Result<PathBuf, Failure> {
    if let Ok(suffix) = path.strip_prefix("~") {
        return home
            .map(|home| home.join(suffix))
            .ok_or_else(|| fail("a mount using ~ needs HOME"));
    }
    if path.is_absolute() {
        return Ok(path.to_owned());
    }
    Ok(config.parent().unwrap_or_else(|| Path::new("")).join(path))
}

pub(crate) fn configure(cli: &Cli) -> Result<(FileSession, String), Failure> {
    let home = home_path();
    let user = home_component(home.as_deref());
    let unguarded = cli.level == MacroSecurity::Unguarded;
    let mut policy = HostPolicy {
        allow_process_spawn: false,
        allow_dynamic_link: false,
        allow_com_activation: false,
        allow_interaction: false,
        windows_faithful: true,
        ..HostPolicy::native()
    };
    let default_directory = if let Some((config_path, config)) = read_config(cli, home.as_deref())?
    {
        for mut mount in config.mount {
            mount.host = host_path(&mount.host, &config_path, home.as_deref())?;
            policy = add_mount(policy, mount)?;
        }
        config.default_directory
    } else {
        let (defaults, directory) = built_in(policy, home.as_deref(), user, unguarded)?;
        policy = defaults;
        Some(directory)
    };

    let absolute = std::path::absolute(&cli.book)
        .map_err(|error| fail(format!("cannot resolve {}: {error}", cli.book)))?;
    let folder = absolute
        .parent()
        .ok_or_else(|| fail("the workbook has no parent directory"))?;
    let folder = std::fs::canonicalize(folder).map_err(|error| {
        fail(format!(
            "cannot resolve workbook directory {}: {error}",
            folder.display()
        ))
    })?;
    let name = absolute
        .file_name()
        .ok_or_else(|| fail("the workbook has no file name"))?;
    let workbook = folder.join(name);
    let mut files = FileSession::new(policy.clone());
    if files.windows_path(&workbook).is_none() {
        let stem = workbook
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Workbook");
        let stem: String = stem
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect();
        let mount = Mount {
            windows: format!(r"C:\Users\{user}\Workbooks\{stem}"),
            host: folder,
            mode: if unguarded {
                Mode::ReadWrite
            } else {
                Mode::Read
            },
        };
        files = FileSession::new(add_mount(policy, mount)?);
    }
    let full_name = files.windows_path(&workbook).ok_or_else(|| {
        fail(format!(
            "the workbook has no mounted Windows path: {}",
            cli.book
        ))
    })?;
    if let Some(directory) = default_directory.or_else(|| {
        full_name
            .rsplit_once('\\')
            .filter(|(folder, _)| folder.contains(':'))
            .map(|(folder, _)| {
                if folder.ends_with(':') {
                    format!("{folder}\\")
                } else {
                    folder.to_owned()
                }
            })
    }) {
        files.set_default_directory(directory).map_err(|error| {
            fail(format!(
                "cannot set the Windows starting directory: {error}"
            ))
        })?;
    }
    Ok((files, full_name))
}
