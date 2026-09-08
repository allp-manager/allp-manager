use crate::domain::SoftwareType;

#[derive(Debug, Clone, Copy)]
pub struct CanonicalIdentity {
    pub id: &'static str,
    pub display_name: &'static str,
    pub software_type: SoftwareType,
    pub aliases: &'static [&'static str],
    pub official_sources: &'static [&'static str],
    pub verified_packages: &'static [VerifiedPackageIdentity],
}

#[derive(Debug, Clone, Copy)]
pub struct VerifiedPackageIdentity {
    pub backend_id: &'static str,
    pub package_id: &'static str,
    pub evidence: &'static str,
}

pub const HOMEBREW_ID: &str = "homebrew";

const IDENTITIES: &[CanonicalIdentity] = &[
    CanonicalIdentity {
        id: HOMEBREW_ID,
        display_name: "Homebrew",
        software_type: SoftwareType::PackageManager,
        aliases: &["homebrew", "home brew", "brew", "linuxbrew"],
        official_sources: &[
            "https://brew.sh/",
            "https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh",
        ],
        verified_packages: &[],
    },
    CanonicalIdentity {
        id: "apt",
        display_name: "APT",
        software_type: SoftwareType::PackageManager,
        aliases: &["apt", "apt-get", "advanced package tool"],
        official_sources: &["https://salsa.debian.org/apt-team/apt"],
        verified_packages: &[],
    },
    CanonicalIdentity {
        id: "pacman",
        display_name: "Pacman",
        software_type: SoftwareType::PackageManager,
        aliases: &["pacman", "arch pacman"],
        official_sources: &["https://gitlab.archlinux.org/pacman/pacman"],
        verified_packages: &[],
    },
    CanonicalIdentity {
        id: "dnf",
        display_name: "DNF",
        software_type: SoftwareType::PackageManager,
        aliases: &["dnf", "dnf5", "fedora dnf"],
        official_sources: &["https://github.com/rpm-software-management/dnf5"],
        verified_packages: &[],
    },
    CanonicalIdentity {
        id: "zypper",
        display_name: "Zypper",
        software_type: SoftwareType::PackageManager,
        aliases: &["zypper", "opensuse zypper"],
        official_sources: &["https://github.com/openSUSE/zypper"],
        verified_packages: &[],
    },
    CanonicalIdentity {
        id: "apk",
        display_name: "APK",
        software_type: SoftwareType::PackageManager,
        aliases: &["apk", "alpine apk"],
        official_sources: &["https://gitlab.alpinelinux.org/alpine/apk-tools"],
        verified_packages: &[],
    },
    CanonicalIdentity {
        id: "flatpak",
        display_name: "Flatpak",
        software_type: SoftwareType::UniversalApplication,
        aliases: &["flatpak"],
        official_sources: &["https://flatpak.org/"],
        verified_packages: &[],
    },
    CanonicalIdentity {
        id: "snap",
        display_name: "Snap",
        software_type: SoftwareType::UniversalApplication,
        aliases: &["snap", "snapcraft"],
        official_sources: &["https://snapcraft.io/"],
        verified_packages: &[],
    },
    CanonicalIdentity {
        id: "python",
        display_name: "Python",
        software_type: SoftwareType::LanguageRuntime,
        aliases: &["python", "python3", "cpython"],
        official_sources: &["https://www.python.org/"],
        verified_packages: &[],
    },
    CanonicalIdentity {
        id: "pip",
        display_name: "pip",
        software_type: SoftwareType::RegistryClient,
        aliases: &["pip", "pip3"],
        official_sources: &["https://pip.pypa.io/"],
        verified_packages: &[],
    },
    CanonicalIdentity {
        id: "pipx",
        display_name: "pipx",
        software_type: SoftwareType::RegistryClient,
        aliases: &["pipx"],
        official_sources: &["https://pipx.pypa.io/"],
        verified_packages: &[],
    },
    CanonicalIdentity {
        id: "uv",
        display_name: "uv",
        software_type: SoftwareType::RegistryClient,
        aliases: &["uv", "astral uv"],
        official_sources: &["https://docs.astral.sh/uv/"],
        verified_packages: &[],
    },
    CanonicalIdentity {
        id: "nodejs",
        display_name: "Node.js",
        software_type: SoftwareType::LanguageRuntime,
        aliases: &["node", "nodejs", "node.js"],
        official_sources: &["https://nodejs.org/"],
        verified_packages: &[],
    },
    CanonicalIdentity {
        id: "npm",
        display_name: "npm",
        software_type: SoftwareType::RegistryClient,
        aliases: &["npm", "node package manager"],
        official_sources: &["https://www.npmjs.com/"],
        verified_packages: &[],
    },
    CanonicalIdentity {
        id: "pnpm",
        display_name: "pnpm",
        software_type: SoftwareType::RegistryClient,
        aliases: &["pnpm"],
        official_sources: &["https://pnpm.io/"],
        verified_packages: &[],
    },
    CanonicalIdentity {
        id: "yarn",
        display_name: "Yarn",
        software_type: SoftwareType::RegistryClient,
        aliases: &["yarn", "yarnpkg"],
        official_sources: &["https://yarnpkg.com/"],
        verified_packages: &[],
    },
    CanonicalIdentity {
        id: "rust",
        display_name: "Rust",
        software_type: SoftwareType::LanguageRuntime,
        aliases: &["rust", "rustlang", "rust language"],
        official_sources: &["https://www.rust-lang.org/"],
        verified_packages: &[],
    },
    CanonicalIdentity {
        id: "cargo",
        display_name: "Cargo",
        software_type: SoftwareType::RegistryClient,
        aliases: &["cargo", "rust package manager", "crates.io"],
        official_sources: &["https://doc.rust-lang.org/cargo/"],
        verified_packages: &[],
    },
    CanonicalIdentity {
        id: "firefox",
        display_name: "Firefox",
        software_type: SoftwareType::DesktopApplication,
        aliases: &["firefox", "mozilla firefox", "firefox browser"],
        official_sources: &["https://www.mozilla.org/firefox/"],
        verified_packages: &[
            VerifiedPackageIdentity {
                backend_id: "apt",
                package_id: "firefox",
                evidence: "built-in Firefox package mapping for APT",
            },
            VerifiedPackageIdentity {
                backend_id: "dnf",
                package_id: "firefox",
                evidence: "built-in Firefox package mapping for DNF",
            },
            VerifiedPackageIdentity {
                backend_id: "pacman",
                package_id: "firefox",
                evidence: "built-in Firefox package mapping for Pacman",
            },
            VerifiedPackageIdentity {
                backend_id: "flatpak",
                package_id: "org.mozilla.firefox",
                evidence: "built-in Firefox application-ID mapping for Flatpak",
            },
            VerifiedPackageIdentity {
                backend_id: "snap",
                package_id: "firefox",
                evidence: "built-in Firefox package mapping for Snap",
            },
        ],
    },
];

pub fn all() -> &'static [CanonicalIdentity] {
    IDENTITIES
}

pub fn find_by_id(id: &str) -> Option<&'static CanonicalIdentity> {
    IDENTITIES.iter().find(|identity| identity.id == id)
}
