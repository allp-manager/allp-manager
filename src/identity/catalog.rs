use crate::domain::SoftwareType;

#[derive(Debug, Clone, Copy)]
pub struct CanonicalIdentity {
    pub id: &'static str,
    pub display_name: &'static str,
    pub software_type: SoftwareType,
    pub aliases: &'static [&'static str],
    pub official_sources: &'static [&'static str],
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
    },
    CanonicalIdentity {
        id: "apt",
        display_name: "APT",
        software_type: SoftwareType::PackageManager,
        aliases: &["apt", "apt-get", "advanced package tool"],
        official_sources: &["https://salsa.debian.org/apt-team/apt"],
    },
    CanonicalIdentity {
        id: "pacman",
        display_name: "Pacman",
        software_type: SoftwareType::PackageManager,
        aliases: &["pacman", "arch pacman"],
        official_sources: &["https://gitlab.archlinux.org/pacman/pacman"],
    },
    CanonicalIdentity {
        id: "dnf",
        display_name: "DNF",
        software_type: SoftwareType::PackageManager,
        aliases: &["dnf", "dnf5", "fedora dnf"],
        official_sources: &["https://github.com/rpm-software-management/dnf5"],
    },
    CanonicalIdentity {
        id: "zypper",
        display_name: "Zypper",
        software_type: SoftwareType::PackageManager,
        aliases: &["zypper", "opensuse zypper"],
        official_sources: &["https://github.com/openSUSE/zypper"],
    },
    CanonicalIdentity {
        id: "apk",
        display_name: "APK",
        software_type: SoftwareType::PackageManager,
        aliases: &["apk", "alpine apk"],
        official_sources: &["https://gitlab.alpinelinux.org/alpine/apk-tools"],
    },
    CanonicalIdentity {
        id: "flatpak",
        display_name: "Flatpak",
        software_type: SoftwareType::UniversalApplication,
        aliases: &["flatpak"],
        official_sources: &["https://flatpak.org/"],
    },
    CanonicalIdentity {
        id: "snap",
        display_name: "Snap",
        software_type: SoftwareType::UniversalApplication,
        aliases: &["snap", "snapcraft"],
        official_sources: &["https://snapcraft.io/"],
    },
    CanonicalIdentity {
        id: "python",
        display_name: "Python",
        software_type: SoftwareType::LanguageRuntime,
        aliases: &["python", "python3", "cpython"],
        official_sources: &["https://www.python.org/"],
    },
    CanonicalIdentity {
        id: "pip",
        display_name: "pip",
        software_type: SoftwareType::RegistryClient,
        aliases: &["pip", "pip3"],
        official_sources: &["https://pip.pypa.io/"],
    },
    CanonicalIdentity {
        id: "pipx",
        display_name: "pipx",
        software_type: SoftwareType::RegistryClient,
        aliases: &["pipx"],
        official_sources: &["https://pipx.pypa.io/"],
    },
    CanonicalIdentity {
        id: "uv",
        display_name: "uv",
        software_type: SoftwareType::RegistryClient,
        aliases: &["uv", "astral uv"],
        official_sources: &["https://docs.astral.sh/uv/"],
    },
    CanonicalIdentity {
        id: "nodejs",
        display_name: "Node.js",
        software_type: SoftwareType::LanguageRuntime,
        aliases: &["node", "nodejs", "node.js"],
        official_sources: &["https://nodejs.org/"],
    },
    CanonicalIdentity {
        id: "npm",
        display_name: "npm",
        software_type: SoftwareType::RegistryClient,
        aliases: &["npm", "node package manager"],
        official_sources: &["https://www.npmjs.com/"],
    },
    CanonicalIdentity {
        id: "pnpm",
        display_name: "pnpm",
        software_type: SoftwareType::RegistryClient,
        aliases: &["pnpm"],
        official_sources: &["https://pnpm.io/"],
    },
    CanonicalIdentity {
        id: "yarn",
        display_name: "Yarn",
        software_type: SoftwareType::RegistryClient,
        aliases: &["yarn", "yarnpkg"],
        official_sources: &["https://yarnpkg.com/"],
    },
];

pub fn all() -> &'static [CanonicalIdentity] {
    IDENTITIES
}

pub fn find_by_id(id: &str) -> Option<&'static CanonicalIdentity> {
    IDENTITIES.iter().find(|identity| identity.id == id)
}
