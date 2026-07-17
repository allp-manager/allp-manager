use crate::platform::{Architecture, LibcFamily, OperatingSystem, PlatformContext};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::{cmp::Ordering, fmt, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl FromStr for Version {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let value = raw.strip_prefix('v').unwrap_or(raw);
        if value.is_empty()
            || value.contains('-')
            || value.contains('+')
            || value.split('.').count() != 3
        {
            return Err(format!("invalid semantic version: {raw}"));
        }
        let mut parts = value.split('.');
        let parse = |part: Option<&str>| -> Result<u64, String> {
            let part = part.ok_or_else(|| format!("invalid semantic version: {raw}"))?;
            if part.is_empty()
                || (part.len() > 1 && part.starts_with('0'))
                || !part.bytes().all(|byte| byte.is_ascii_digit())
            {
                return Err(format!("invalid semantic version: {raw}"));
            }
            part.parse()
                .map_err(|_| format!("invalid semantic version: {raw}"))
        };
        Ok(Self::new(
            parse(parts.next())?,
            parse(parts.next())?,
            parse(parts.next())?,
        ))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Serialize for Version {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseManifest {
    pub schema_version: u32,
    pub version: Version,
    pub tag: String,
    pub channel: String,
    pub published_at: String,
    pub minimum_updater_version: Version,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub target: String,
    pub os: String,
    pub architecture: String,
    pub libc: Option<String>,
    pub archive: String,
    pub binary: String,
    pub sha256: String,
    pub size: u64,
}

impl ReleaseManifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err(format!(
                "unsupported release manifest schema {}",
                self.schema_version
            ));
        }
        if self.tag != format!("v{}", self.version) {
            return Err("release manifest tag does not match its version".to_owned());
        }
        if !matches!(self.channel.as_str(), "stable" | "prerelease") {
            return Err("release manifest channel is invalid".to_owned());
        }
        let mut targets = std::collections::HashSet::new();
        for asset in &self.assets {
            if !targets.insert(asset.target.as_str()) {
                return Err(format!("duplicate release target: {}", asset.target));
            }
            if !safe_asset_name(&asset.archive) || !safe_asset_name(&asset.binary) {
                return Err(format!("unsafe release asset path: {}", asset.archive));
            }
            if asset.sha256.len() != 64
                || !asset.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(format!("invalid SHA-256 for {}", asset.archive));
            }
            if asset.size == 0 {
                return Err(format!(
                    "release asset {} has an invalid size",
                    asset.archive
                ));
            }
        }
        Ok(())
    }

    pub fn asset_for(&self, platform: &PlatformContext) -> Option<&ReleaseAsset> {
        let target = platform.target_triple()?;
        self.assets.iter().find(|asset| {
            asset.target == target
                && asset.os == os_manifest_name(platform.os)
                && asset.architecture == architecture_manifest_name(platform.architecture)
                && libc_matches(asset.libc.as_deref(), platform.libc)
        })
    }
}

fn safe_asset_name(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('.')
        && !value.contains('/')
        && !value.contains('\\')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

fn os_manifest_name(os: OperatingSystem) -> &'static str {
    match os {
        OperatingSystem::Linux => "linux",
        OperatingSystem::MacOs => "macos",
        OperatingSystem::Windows => "windows",
        OperatingSystem::Other => "other",
    }
}

fn architecture_manifest_name(architecture: Architecture) -> &'static str {
    architecture.as_str()
}

fn libc_matches(asset: Option<&str>, platform: Option<LibcFamily>) -> bool {
    match platform {
        Some(libc) => asset == Some(libc.as_str()),
        None => asset.is_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::Version;

    #[test]
    fn semantic_versions_are_compared_numerically() {
        assert!("0.3.10".parse::<Version>().unwrap() > "0.3.9".parse().unwrap());
        assert!("0.4.0".parse::<Version>().unwrap() > "0.3.99".parse().unwrap());
        assert!("1.0.0".parse::<Version>().unwrap() > "0.99.0".parse().unwrap());
    }

    #[test]
    fn malformed_and_four_part_versions_are_rejected() {
        assert!("0.3.3.1".parse::<Version>().is_err());
        assert!("0.03.4".parse::<Version>().is_err());
        assert!("next".parse::<Version>().is_err());
    }
}
