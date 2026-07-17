use crate::backends::{
    development::{node::NodeBackend, python::PythonBackend},
    homebrew::HomebrewBackend,
    system::{
        apt::AptBackend,
        dnf::DnfBackend,
        family::{SystemFamilyBackend, APK, EOPKG, PORTAGE, SWUPD, XBPS, ZYPPER},
        pacman::PacmanBackend,
    },
    universal::{flatpak::FlatpakBackend, snap::SnapBackend},
    Backend,
};
use std::sync::Arc;

pub fn builtin_backends() -> Vec<Arc<dyn Backend>> {
    vec![
        Arc::new(AptBackend),
        Arc::new(PacmanBackend),
        Arc::new(DnfBackend),
        Arc::new(SystemFamilyBackend::new(&ZYPPER)),
        Arc::new(SystemFamilyBackend::new(&APK)),
        Arc::new(SystemFamilyBackend::new(&XBPS)),
        Arc::new(SystemFamilyBackend::new(&PORTAGE)),
        Arc::new(SystemFamilyBackend::new(&EOPKG)),
        Arc::new(SystemFamilyBackend::new(&SWUPD)),
        Arc::new(FlatpakBackend),
        Arc::new(SnapBackend),
        Arc::new(HomebrewBackend),
        Arc::new(PythonBackend),
        Arc::new(NodeBackend),
    ]
}
