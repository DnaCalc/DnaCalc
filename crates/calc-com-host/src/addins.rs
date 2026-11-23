//! Discovery policy shared by the native registry adapter and its test fake.

#[cfg(windows)]
pub(crate) mod native;

/// Office COM add-in metadata from the machine and user registration keys.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Registration {
    pub progid: String,
    pub friendly_name: String,
    pub description: String,
    pub load_behavior: u32,
}

impl Registration {
    #[cfg(any(windows, test))]
    pub(crate) fn starts_with_application(&self) -> bool {
        self.load_behavior & 3 == 3
    }
}

/// Platform or registry failure while listing registered COM add-ins.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoveryError {
    WindowsOnly,
    Registry(i32),
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy)]
pub(crate) enum RegistrationScope {
    Machine,
    User,
}

#[cfg(any(windows, test))]
pub(crate) trait AddinRegistry {
    fn registrations(&self, scope: RegistrationScope) -> Result<Vec<Registration>, i32>;
}

#[cfg(any(windows, test))]
fn discover(
    registry: &impl AddinRegistry,
    allowed: bool,
) -> Result<Vec<Registration>, DiscoveryError> {
    if !allowed {
        return Ok(Vec::new());
    }
    let mut by_progid = std::collections::BTreeMap::new();
    for scope in [RegistrationScope::Machine, RegistrationScope::User] {
        for registration in registry
            .registrations(scope)
            .map_err(DiscoveryError::Registry)?
        {
            by_progid.insert(registration.progid.to_ascii_lowercase(), registration);
        }
    }
    Ok(by_progid.into_values().collect())
}

/// List registered native COM add-ins if the host permits them.
pub fn registered_addins(allowed: bool) -> Result<Vec<Registration>, DiscoveryError> {
    #[cfg(windows)]
    return discover(&native::Registry, allowed);
    #[cfg(not(windows))]
    if allowed {
        Err(DiscoveryError::WindowsOnly)
    } else {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct Registry {
        reads: Cell<usize>,
    }

    impl AddinRegistry for Registry {
        fn registrations(&self, scope: RegistrationScope) -> Result<Vec<Registration>, i32> {
            self.reads.set(self.reads.get() + 1);
            Ok(match scope {
                RegistrationScope::Machine => vec![
                    registration("Sample.Ribbon", "Machine copy", 3),
                    registration("Sample.Demand", "Demand", 8),
                ],
                RegistrationScope::User => vec![
                    registration("sample.ribbon", "User copy", 0),
                    registration("Sample.Startup", "Startup", 3),
                ],
            })
        }
    }

    fn registration(progid: &str, friendly_name: &str, load_behavior: u32) -> Registration {
        Registration {
            progid: progid.into(),
            friendly_name: friendly_name.into(),
            description: format!("{friendly_name} description"),
            load_behavior,
        }
    }

    #[test]
    fn machine_and_user_addins_merge_before_startup_is_selected() {
        let registry = Registry {
            reads: Cell::new(0),
        };
        let registrations = discover(&registry, true).unwrap();
        assert_eq!(registry.reads.get(), 2);
        assert_eq!(registrations.len(), 3);
        assert_eq!(registrations[0].friendly_name, "Demand");
        assert_eq!(registrations[1].friendly_name, "User copy");
        assert_eq!(registrations[1].description, "User copy description");
        assert!(!registrations[0].starts_with_application());
        assert!(!registrations[1].starts_with_application());
        assert!(registrations[2].starts_with_application());
    }

    #[test]
    fn disabled_policy_does_not_read_registrations() {
        let registry = Registry {
            reads: Cell::new(0),
        };
        assert!(discover(&registry, false).unwrap().is_empty());
        assert_eq!(registry.reads.get(), 0);
    }

    #[cfg(not(windows))]
    #[test]
    fn enabled_discovery_on_linux_returns_a_typed_windows_only_refusal() {
        assert_eq!(registered_addins(true), Err(DiscoveryError::WindowsOnly));
        assert_eq!(registered_addins(false), Ok(Vec::new()));
    }
}
