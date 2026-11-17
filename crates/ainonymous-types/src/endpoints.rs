//! Résolution de l'URL HTTP du daemon `ainonymous-daemon`, partagée par
//! le proxy et la CLI.
//!
//! Variable principale : `AINON_DAEMON_URL` (ex. `http://127.0.0.1:18890`).
//! Compatibilité : si elle est absente mais que l'ancienne variable
//! `AINON_HOLOCHAIN_APP_PORT` est définie, elle est interprétée comme le port
//! HTTP du daemon (c'était son usage réel côté proxy), avec un avertissement.

/// Port HTTP par défaut du daemon (`daemon_port` dans la config du daemon).
pub const DEFAULT_DAEMON_PORT: u16 = 8890;
/// Variable d'environnement donnant l'URL de base du daemon.
pub const DAEMON_URL_ENV: &str = "AINON_DAEMON_URL";
/// Ancienne variable (dépréciée) : port HTTP du daemon sur 127.0.0.1.
pub const LEGACY_DAEMON_PORT_ENV: &str = "AINON_HOLOCHAIN_APP_PORT";

/// Origine de l'URL retenue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonUrlSource {
    /// `AINON_DAEMON_URL`
    DaemonUrlEnv,
    /// `AINON_HOLOCHAIN_APP_PORT` (déprécié)
    LegacyPortEnv,
    /// Valeur par défaut `http://127.0.0.1:8890`
    Default,
}

/// URL de base du daemon résolue (sans `/` final).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonUrl {
    pub url: String,
    pub source: DaemonUrlSource,
}

/// Résolution pure (testable) à partir des valeurs brutes des variables.
pub fn resolve_daemon_url(daemon_url: Option<&str>, legacy_port: Option<&str>) -> DaemonUrl {
    if let Some(u) = daemon_url.map(str::trim).filter(|u| !u.is_empty()) {
        return DaemonUrl {
            url: u.trim_end_matches('/').to_string(),
            source: DaemonUrlSource::DaemonUrlEnv,
        };
    }
    if let Some(port) = legacy_port.and_then(|p| p.trim().parse::<u16>().ok()) {
        return DaemonUrl {
            url: format!("http://127.0.0.1:{port}"),
            source: DaemonUrlSource::LegacyPortEnv,
        };
    }
    DaemonUrl {
        url: format!("http://127.0.0.1:{DEFAULT_DAEMON_PORT}"),
        source: DaemonUrlSource::Default,
    }
}

/// Résolution depuis l'environnement du processus.
pub fn daemon_url_from_env() -> DaemonUrl {
    let daemon_url = std::env::var(DAEMON_URL_ENV).ok();
    let legacy = std::env::var(LEGACY_DAEMON_PORT_ENV).ok();
    resolve_daemon_url(daemon_url.as_deref(), legacy.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_daemon_port_8890() {
        let r = resolve_daemon_url(None, None);
        assert_eq!(r.url, "http://127.0.0.1:8890");
        assert_eq!(r.source, DaemonUrlSource::Default);
    }

    #[test]
    fn daemon_url_env_wins_and_is_normalized() {
        let r = resolve_daemon_url(Some("http://10.0.0.2:18890/"), Some("1234"));
        assert_eq!(r.url, "http://10.0.0.2:18890");
        assert_eq!(r.source, DaemonUrlSource::DaemonUrlEnv);
    }

    #[test]
    fn legacy_port_is_still_honoured() {
        let r = resolve_daemon_url(None, Some("18890"));
        assert_eq!(r.url, "http://127.0.0.1:18890");
        assert_eq!(r.source, DaemonUrlSource::LegacyPortEnv);
    }

    #[test]
    fn empty_or_invalid_values_fall_back_to_default() {
        let r = resolve_daemon_url(Some("  "), Some("pas-un-port"));
        assert_eq!(r.source, DaemonUrlSource::Default);
    }
}
