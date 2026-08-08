use super::checksum::sha256_bytes;
use super::trusted_helper::resolve_self_update_helper;
use super::{
    BuildSource, ContinuousBuildManifest, GitHubRepository, ReleaseDescriptor, ReleaseSource,
    UpdateChannel, CONTINUOUS_MANIFEST_NAME, CONTINUOUS_WORKFLOW_NAME, CONTINUOUS_WORKFLOW_PATH,
    OFFICIAL_REPOSITORY,
};
use crate::{
    domain::{AllpError, AllpResult, NativeCommand},
    execution::{render_native_command, ProcessRunner, StdProcessRunner},
    release::{ReleaseManifest, Version},
};
use serde_json::Value;
use std::{
    cmp::Reverse,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

const MAX_METADATA_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub etag: Option<String>,
    pub rate_limit: Option<RateLimitInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitInfo {
    pub remaining: Option<u64>,
    pub reset_at_unix: Option<u64>,
    pub retry_after_seconds: Option<u64>,
}

pub trait HttpClient: Send + Sync {
    fn get(&self, url: &str, etag: Option<&str>) -> AllpResult<HttpResponse>;
}

pub struct CurlHttpClient {
    runner: Box<dyn ProcessRunner>,
}

impl Default for CurlHttpClient {
    fn default() -> Self {
        Self {
            runner: Box::new(StdProcessRunner),
        }
    }
}

impl CurlHttpClient {
    #[cfg(test)]
    pub fn with_runner(runner: Box<dyn ProcessRunner>) -> Self {
        Self { runner }
    }
}

impl HttpClient for CurlHttpClient {
    fn get(&self, url: &str, etag: Option<&str>) -> AllpResult<HttpResponse> {
        validate_https_url(url)?;
        let curl = resolve_self_update_helper("curl")?;
        let headers = SecureHeadersFile::create()?;
        let mut command = NativeCommand::new(curl).args([
            "--silent",
            "--show-error",
            "--location",
            "--max-redirs",
            "5",
            "--connect-timeout",
            "10",
            "--max-time",
            "30",
            "--max-filesize",
            "4194304",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--header",
            "Accept: application/vnd.github+json",
            "--header",
            "X-GitHub-Api-Version: 2026-03-10",
            "--header",
            "User-Agent: allp-self-update",
            "--dump-header",
        ]);
        command = command.arg(headers.path().as_os_str());
        if let Some(etag) = etag {
            command = command.args(["--header", &format!("If-None-Match: {etag}")]);
        }
        command = command.arg(url).timeout(Duration::from_secs(35));
        let rendered = render_native_command(&command);
        let output = self.runner.capture(&command);
        let header_text = fs::read_to_string(headers.path()).unwrap_or_default();
        let output = output?;
        if !output.success {
            return Err(AllpError::CommandFailed {
                backend: "GitHub release source".to_owned(),
                command: rendered,
                code: output.code,
                stderr: output.stderr,
            });
        }
        if output.stdout.len() > MAX_METADATA_BYTES {
            return Err(AllpError::InvalidInput(
                "GitHub release metadata exceeded the 4 MiB safety limit".to_owned(),
            ));
        }
        let (status, response_etag, rate_limit) = parse_response_headers(&header_text)?;
        Ok(HttpResponse {
            status,
            body: output.stdout.into_bytes(),
            etag: response_etag,
            rate_limit,
        })
    }
}

pub struct GitHubReleaseSource<'a> {
    repository: GitHubRepository,
    client: &'a dyn HttpClient,
    response_etag: Mutex<Option<String>>,
}

/// Channel-aware source boundary. Stable/prerelease checks delegate to the tag provider while
/// continuous checks require the trusted Actions run, artifact, manifest, and transport mirror.
pub struct GitHubActionsBuildSource<'a> {
    releases: GitHubReleaseSource<'a>,
}

impl<'a> GitHubActionsBuildSource<'a> {
    pub fn official(client: &'a dyn HttpClient) -> Self {
        Self {
            releases: GitHubReleaseSource::official(client),
        }
    }

    pub fn official_with_etag(client: &'a dyn HttpClient, etag: Option<&str>) -> Self {
        Self {
            releases: GitHubReleaseSource::official_with_etag(client, etag),
        }
    }

    #[cfg(test)]
    fn new(repository: GitHubRepository, client: &'a dyn HttpClient) -> Self {
        Self {
            releases: GitHubReleaseSource::new(repository, client),
        }
    }
}

impl BuildSource for GitHubActionsBuildSource<'_> {
    fn latest_build(
        &self,
        channel: UpdateChannel,
        current: &Version,
        target: Option<&str>,
    ) -> AllpResult<Option<ReleaseDescriptor>> {
        if channel == UpdateChannel::Continuous {
            let target = target.ok_or_else(|| AllpError::UnsupportedOperation {
                backend: "GitHub Actions build source".to_owned(),
                operation: "continuous update for an unsupported target".to_owned(),
            })?;
            self.releases.latest_continuous_build(target)
        } else {
            self.releases.latest_release(channel, current)
        }
    }

    fn response_etag(&self) -> Option<String> {
        ReleaseSource::response_etag(&self.releases)
    }
}

impl<'a> GitHubReleaseSource<'a> {
    pub fn official(client: &'a dyn HttpClient) -> Self {
        Self {
            repository: OFFICIAL_REPOSITORY,
            client,
            response_etag: Mutex::new(None),
        }
    }

    pub fn official_with_etag(client: &'a dyn HttpClient, _etag: Option<&str>) -> Self {
        Self {
            repository: OFFICIAL_REPOSITORY,
            client,
            response_etag: Mutex::new(None),
        }
    }

    #[cfg(test)]
    pub fn new(repository: GitHubRepository, client: &'a dyn HttpClient) -> Self {
        Self {
            repository,
            client,
            response_etag: Mutex::new(None),
        }
    }
}

impl ReleaseSource for GitHubReleaseSource<'_> {
    fn latest_release(
        &self,
        channel: UpdateChannel,
        current: &Version,
    ) -> AllpResult<Option<ReleaseDescriptor>> {
        if channel == UpdateChannel::Continuous {
            return Err(AllpError::InvalidInput(
                "continuous builds must use GitHubActionsBuildSource".to_owned(),
            ));
        }
        if self.repository != OFFICIAL_REPOSITORY {
            return Err(AllpError::InvalidInput(
                "self-update repository does not match Allp's trusted repository".to_owned(),
            ));
        }
        match channel {
            UpdateChannel::Stable => self.latest_stable_release(current),
            UpdateChannel::Prerelease => self.latest_semantic_prerelease(current),
            UpdateChannel::Continuous => unreachable!("continuous channel rejected above"),
        }
    }

    fn response_etag(&self) -> Option<String> {
        self.response_etag
            .lock()
            .expect("response ETag lock")
            .clone()
    }
}

impl GitHubReleaseSource<'_> {
    fn latest_stable_release(&self, current: &Version) -> AllpResult<Option<ReleaseDescriptor>> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            self.repository.owner, self.repository.name
        );
        let response = self.client.get(&url, None)?;
        if response.status == 404 {
            return Ok(None);
        }
        require_http_success("GitHub release source", &url, &response)?;
        *self.response_etag.lock().expect("response ETag lock") = response.etag.clone();
        let release: Value =
            serde_json::from_slice(&response.body).map_err(|error| AllpError::Parse {
                backend: "GitHub release source".to_owned(),
                message: error.to_string(),
            })?;
        let version = release
            .get("tag_name")
            .and_then(Value::as_str)
            .ok_or_else(|| AllpError::InvalidInput("latest stable release has no tag".to_owned()))?
            .parse::<Version>()
            .map_err(AllpError::InvalidInput)?;
        if version <= *current {
            return Ok(None);
        }
        self.load_semantic_release(&release, version, response.etag)
            .map(Some)
    }

    fn latest_semantic_prerelease(
        &self,
        current: &Version,
    ) -> AllpResult<Option<ReleaseDescriptor>> {
        // Query only refs beginning with `v`; per-push `continuous-v...` tags cannot starve this
        // channel even when they dominate the ordinary releases feed.
        let refs_url = format!(
            "https://api.github.com/repos/{}/{}/git/matching-refs/tags/v",
            self.repository.owner, self.repository.name
        );
        let response = self.client.get(&refs_url, None)?;
        if response.status == 404 {
            return Ok(None);
        }
        require_http_success("GitHub prerelease source", &refs_url, &response)?;
        *self.response_etag.lock().expect("response ETag lock") = response.etag.clone();
        let refs: Value =
            serde_json::from_slice(&response.body).map_err(|error| AllpError::Parse {
                backend: "GitHub prerelease source".to_owned(),
                message: error.to_string(),
            })?;
        let refs = refs.as_array().ok_or_else(|| AllpError::Parse {
            backend: "GitHub prerelease source".to_owned(),
            message: "GitHub matching refs response was not an array".to_owned(),
        })?;
        let mut versions = refs
            .iter()
            .filter_map(|reference| {
                let name = reference.get("ref")?.as_str()?.strip_prefix("refs/tags/")?;
                let version = name.parse::<Version>().ok()?;
                (version > *current).then_some(version)
            })
            .collect::<Vec<_>>();
        versions.sort_unstable_by_key(|version| Reverse(*version));
        versions.dedup();
        for version in versions {
            let tag = format!("v{version}");
            let url = format!(
                "https://api.github.com/repos/{}/{}/releases/tags/{tag}",
                self.repository.owner, self.repository.name
            );
            let release_response = self.client.get(&url, None)?;
            if release_response.status == 404 {
                continue;
            }
            require_http_success("GitHub prerelease source", &url, &release_response)?;
            let release: Value =
                serde_json::from_slice(&release_response.body).map_err(|error| {
                    AllpError::Parse {
                        backend: "GitHub prerelease source".to_owned(),
                        message: error.to_string(),
                    }
                })?;
            if release
                .get("draft")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                continue;
            }
            return self
                .load_semantic_release(&release, version, response.etag.clone())
                .map(Some);
        }
        Ok(None)
    }

    fn load_semantic_release(
        &self,
        release: &Value,
        version: Version,
        etag: Option<String>,
    ) -> AllpResult<ReleaseDescriptor> {
        let tag = release
            .get("tag_name")
            .and_then(Value::as_str)
            .expect("selected release has a tag")
            .to_owned();
        let manifest_url = release
            .get("assets")
            .and_then(Value::as_array)
            .and_then(|assets| {
                assets.iter().find_map(|asset| {
                    (asset.get("name").and_then(Value::as_str)
                        == Some("allp-release-manifest.json"))
                    .then(|| asset.get("browser_download_url").and_then(Value::as_str))
                    .flatten()
                })
            })
            .ok_or_else(|| {
                AllpError::InvalidInput(format!(
                    "GitHub release {tag} does not include allp-release-manifest.json"
                ))
            })?;
        validate_release_asset_url(self.repository, &tag, manifest_url)?;
        let manifest_response = self.client.get(manifest_url, None)?;
        if manifest_response.status != 200 {
            return Err(http_response_error(
                "GitHub release source",
                manifest_url,
                &manifest_response,
            ));
        }
        let manifest: ReleaseManifest =
            serde_json::from_slice(&manifest_response.body).map_err(|error| AllpError::Parse {
                backend: "Allp release manifest".to_owned(),
                message: error.to_string(),
            })?;
        manifest.validate().map_err(|message| AllpError::Parse {
            backend: "Allp release manifest".to_owned(),
            message,
        })?;
        if manifest.version != version || manifest.tag != tag {
            return Err(AllpError::InvalidInput(
                "release manifest identity does not match the selected GitHub release".to_owned(),
            ));
        }
        let release_channel = if release
            .get("prerelease")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            UpdateChannel::Prerelease
        } else {
            UpdateChannel::Stable
        };
        if manifest.channel != release_channel.as_str() {
            return Err(AllpError::InvalidInput(
                "release manifest channel does not match the selected GitHub release".to_owned(),
            ));
        }
        Ok(ReleaseDescriptor {
            version,
            tag,
            channel: release_channel,
            published_at: release
                .get("published_at")
                .and_then(Value::as_str)
                .map(str::to_owned),
            manifest,
            build_identity: None,
            etag,
        })
    }
}

impl GitHubReleaseSource<'_> {
    fn latest_continuous_build(&self, target: &str) -> AllpResult<Option<ReleaseDescriptor>> {
        if self.repository != OFFICIAL_REPOSITORY {
            return Err(AllpError::InvalidInput(
                "self-update repository does not match Allp's trusted repository".to_owned(),
            ));
        }
        let api_url = format!(
            "https://api.github.com/repos/{}/{}/releases?per_page=20",
            self.repository.owner, self.repository.name
        );
        let response = self.client.get(&api_url, None)?;
        if response.status == 404 {
            return Ok(None);
        }
        require_http_success("GitHub Actions build source", &api_url, &response)?;
        *self.response_etag.lock().expect("response ETag lock") = response.etag.clone();
        let releases: Value =
            serde_json::from_slice(&response.body).map_err(|error| AllpError::Parse {
                backend: "GitHub Actions build source".to_owned(),
                message: error.to_string(),
            })?;
        let releases = releases.as_array().ok_or_else(|| AllpError::Parse {
            backend: "GitHub Actions build source".to_owned(),
            message: "GitHub releases response was not an array".to_owned(),
        })?;
        self.latest_continuous_release(releases, response.etag, target)
    }

    fn latest_continuous_release(
        &self,
        releases: &[Value],
        etag: Option<String>,
        target: &str,
    ) -> AllpResult<Option<ReleaseDescriptor>> {
        let mut candidates = releases
            .iter()
            .filter(|release| {
                !release
                    .get("draft")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                    && release
                        .get("prerelease")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
            })
            .filter_map(|release| {
                let tag = release.get("tag_name")?.as_str()?;
                let (version, revision) = parse_continuous_tag(tag)?;
                Some((version, revision, release))
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|(version, revision, _)| Reverse((*version, *revision)));
        for (tag_version, tag_revision, release) in candidates {
            if let Some(descriptor) = self.evaluate_continuous_candidate(
                tag_version,
                tag_revision,
                release,
                etag.clone(),
                target,
            )? {
                return Ok(Some(descriptor));
            }
        }
        Ok(None)
    }

    fn evaluate_continuous_candidate(
        &self,
        tag_version: Version,
        tag_revision: u64,
        release: &Value,
        etag: Option<String>,
        target: &str,
    ) -> AllpResult<Option<ReleaseDescriptor>> {
        let tag = release
            .get("tag_name")
            .and_then(Value::as_str)
            .expect("selected continuous release has a tag");
        let manifest_url = release
            .get("assets")
            .and_then(Value::as_array)
            .and_then(|assets| {
                assets.iter().find_map(|asset| {
                    (asset.get("name").and_then(Value::as_str) == Some(CONTINUOUS_MANIFEST_NAME))
                        .then(|| asset.get("browser_download_url").and_then(Value::as_str))
                        .flatten()
                })
            })
            .ok_or_else(|| {
                AllpError::InvalidInput(format!(
                    "continuous build {tag} is missing {CONTINUOUS_MANIFEST_NAME}"
                ))
            })?;
        validate_release_asset_url(self.repository, tag, manifest_url)?;
        let manifest_response = self.client.get(manifest_url, None)?;
        require_http_success("Allp continuous manifest", manifest_url, &manifest_response)?;
        let manifest: ContinuousBuildManifest = serde_json::from_slice(&manifest_response.body)
            .map_err(|error| AllpError::Parse {
                backend: "Allp continuous manifest".to_owned(),
                message: error.to_string(),
            })?;
        manifest.validate().map_err(|message| AllpError::Parse {
            backend: "Allp continuous manifest".to_owned(),
            message,
        })?;
        let manifest_sha256 = sha256_bytes(&manifest_response.body);
        if manifest.base_version != tag_version
            || manifest.build_revision != tag_revision
            || manifest.expected_tag() != tag
        {
            return Err(AllpError::InvalidInput(
                "continuous manifest identity does not match its publication tag".to_owned(),
            ));
        }
        validate_continuous_asset_publication(self.repository, tag, release, &manifest)?;

        let run_url = format!(
            "https://api.github.com/repos/{}/{}/actions/runs/{}",
            self.repository.owner, self.repository.name, manifest.workflow_run_id
        );
        let run_response = self.client.get(&run_url, None)?;
        require_http_success("GitHub Actions build source", &run_url, &run_response)?;
        let run: Value =
            serde_json::from_slice(&run_response.body).map_err(|error| AllpError::Parse {
                backend: "GitHub Actions build source".to_owned(),
                message: error.to_string(),
            })?;
        if !successful_expected_workflow(&run, &manifest) {
            // Failed, cancelled, and still-running workflow runs are never update candidates.
            return Ok(None);
        }
        let artifacts_url = format!(
            "https://api.github.com/repos/{}/{}/actions/runs/{}/artifacts",
            self.repository.owner, self.repository.name, manifest.workflow_run_id
        );
        let artifacts_response = self.client.get(&artifacts_url, None)?;
        require_http_success(
            "GitHub Actions artifact source",
            &artifacts_url,
            &artifacts_response,
        )?;
        let artifacts: Value =
            serde_json::from_slice(&artifacts_response.body).map_err(|error| AllpError::Parse {
                backend: "GitHub Actions artifact source".to_owned(),
                message: error.to_string(),
            })?;
        if !has_expected_actions_artifact(&artifacts, &manifest, &manifest_sha256) {
            return Err(AllpError::InvalidInput(format!(
                "successful workflow run {} has no unexpired authoritative artifact for {}",
                manifest.workflow_run_id, manifest.display_version
            )));
        }

        let identity = manifest
            .identity_for_target(target)
            .map_err(AllpError::InvalidInput)?;
        Ok(Some(ReleaseDescriptor {
            version: manifest.base_version,
            tag: tag.to_owned(),
            channel: UpdateChannel::Continuous,
            published_at: release
                .get("published_at")
                .and_then(Value::as_str)
                .map(str::to_owned),
            manifest: manifest.as_release_manifest(),
            build_identity: Some(identity),
            etag,
        }))
    }
}

fn parse_continuous_tag(tag: &str) -> Option<(Version, u64)> {
    let value = tag.strip_prefix("continuous-v")?;
    let (base, revision) = value.rsplit_once('.')?;
    Some((base.parse().ok()?, revision.parse().ok()?))
}

fn require_http_success(backend: &str, url: &str, response: &HttpResponse) -> AllpResult<()> {
    if response.status == 200 {
        return Ok(());
    }
    Err(http_response_error(backend, url, response))
}

fn http_response_error(backend: &str, url: &str, response: &HttpResponse) -> AllpError {
    let mut message = normalized_http_error(&response.body);
    if matches!(response.status, 403 | 429) {
        if let Some(limit) = &response.rate_limit {
            let remaining = limit
                .remaining
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_owned());
            let reset = limit
                .reset_at_unix
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_owned());
            let retry = limit
                .retry_after_seconds
                .map(|value| format!("; retry after {value} seconds"))
                .unwrap_or_default();
            message.push_str(&format!(
                "; GitHub API rate limit remaining={remaining}, reset Unix timestamp={reset}{retry}"
            ));
        } else {
            message.push_str("; GitHub may be rate limiting anonymous API requests");
        }
    }
    AllpError::CommandFailed {
        backend: backend.to_owned(),
        command: format!("GET {url}"),
        code: Some(i32::from(response.status)),
        stderr: message,
    }
}

fn validate_continuous_asset_publication(
    repository: GitHubRepository,
    tag: &str,
    release: &Value,
    manifest: &ContinuousBuildManifest,
) -> AllpResult<()> {
    let published = release
        .get("assets")
        .and_then(Value::as_array)
        .ok_or_else(|| AllpError::InvalidInput("continuous release has no assets".to_owned()))?;
    for expected in &manifest.assets {
        let url = published.iter().find_map(|asset| {
            (asset.get("name").and_then(Value::as_str) == Some(expected.archive.as_str()))
                .then(|| asset.get("browser_download_url").and_then(Value::as_str))
                .flatten()
        });
        let Some(url) = url else {
            return Err(AllpError::InvalidInput(format!(
                "continuous manifest asset {} was not published",
                expected.archive
            )));
        };
        validate_release_asset_url(repository, tag, url)?;
    }
    Ok(())
}

fn successful_expected_workflow(run: &Value, manifest: &ContinuousBuildManifest) -> bool {
    run.get("id")
        .and_then(Value::as_u64)
        .map(|id| id.to_string())
        == Some(manifest.workflow_run_id.clone())
        && run.get("run_number").and_then(Value::as_u64) == Some(manifest.workflow_run_number)
        && run
            .get("run_attempt")
            .and_then(Value::as_u64)
            .is_some_and(|attempt| {
                manifest.build_id == format!("{}.{}", manifest.workflow_run_id, attempt)
            })
        && run.get("name").and_then(Value::as_str) == Some(CONTINUOUS_WORKFLOW_NAME)
        && run
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|path| path == CONTINUOUS_WORKFLOW_PATH)
        && run.get("status").and_then(Value::as_str) == Some("completed")
        && run.get("conclusion").and_then(Value::as_str) == Some("success")
        && run.get("head_sha").and_then(Value::as_str) == Some(manifest.git_commit.as_str())
        && run.get("head_branch").and_then(Value::as_str) == Some("main")
        && run
            .get("event")
            .and_then(Value::as_str)
            .is_some_and(|event| matches!(event, "push" | "workflow_dispatch"))
}

fn has_expected_actions_artifact(
    response: &Value,
    manifest: &ContinuousBuildManifest,
    manifest_sha256: &str,
) -> bool {
    let expected_name = format!(
        "allp-continuous-{}-{manifest_sha256}",
        manifest.display_version
    );
    response
        .get("artifacts")
        .and_then(Value::as_array)
        .is_some_and(|artifacts| {
            artifacts.iter().any(|artifact| {
                artifact.get("name").and_then(Value::as_str) == Some(expected_name.as_str())
                    && artifact.get("expired").and_then(Value::as_bool) == Some(false)
                    && artifact.get("size_in_bytes").and_then(Value::as_u64) > Some(0)
                    && artifact
                        .get("digest")
                        .and_then(Value::as_str)
                        .is_some_and(valid_actions_artifact_digest)
                    && artifact
                        .get("workflow_run")
                        .and_then(|run| run.get("id"))
                        .and_then(Value::as_u64)
                        .map(|id| id.to_string())
                        == Some(manifest.workflow_run_id.clone())
            })
        })
}

fn valid_actions_artifact_digest(digest: &str) -> bool {
    digest
        .strip_prefix("sha256:")
        .is_some_and(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn validate_https_url(url: &str) -> AllpResult<()> {
    if !url.starts_with("https://") || url.contains('@') || url.contains('#') {
        return Err(AllpError::InvalidInput(format!(
            "self-update refused an unsafe URL: {url}"
        )));
    }
    Ok(())
}

pub fn validate_release_asset_url(
    repository: GitHubRepository,
    tag: &str,
    url: &str,
) -> AllpResult<()> {
    validate_https_url(url)?;
    let prefix = format!(
        "https://github.com/{}/{}/releases/download/{tag}/",
        repository.owner, repository.name
    );
    if !url.starts_with(&prefix) {
        return Err(AllpError::InvalidInput(
            "release asset URL does not belong to Allp's trusted GitHub release".to_owned(),
        ));
    }
    let asset = &url[prefix.len()..];
    if asset.is_empty() || asset.contains('/') || asset.contains("..") {
        return Err(AllpError::InvalidInput(
            "release asset URL contains an unsafe asset name".to_owned(),
        ));
    }
    Ok(())
}

static HEADER_FILE_NONCE: AtomicU64 = AtomicU64::new(0);

struct SecureHeadersFile {
    path: PathBuf,
}

impl SecureHeadersFile {
    fn create() -> std::io::Result<Self> {
        let root = std::env::temp_dir();
        for _ in 0..100 {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            let sequence = HEADER_FILE_NONCE.fetch_add(1, Ordering::Relaxed);
            let path = root.join(format!(
                ".allp-github-headers-{}-{timestamp:x}-{sequence:x}",
                std::process::id()
            ));
            let mut options = OpenOptions::new();
            options.create_new(true).read(true).write(true);
            #[cfg(unix)]
            options.mode(0o600);
            match options.open(&path) {
                Ok(_) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not create an exclusive GitHub header file",
        ))
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for SecureHeadersFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn parse_response_headers(
    headers: &str,
) -> AllpResult<(u16, Option<String>, Option<RateLimitInfo>)> {
    let mut status = None;
    let mut etag = None;
    let mut remaining = None;
    let mut reset_at_unix = None;
    let mut retry_after_seconds = None;
    for block in headers
        .split("\r\n\r\n")
        .filter(|block| !block.trim().is_empty())
    {
        let mut lines = block.lines();
        if let Some(line) = lines.next() {
            if line.starts_with("HTTP/") {
                status = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|value| value.parse().ok());
                etag = None;
                remaining = None;
                reset_at_unix = None;
                retry_after_seconds = None;
                for line in lines {
                    let Some((name, value)) = line.split_once(':') else {
                        continue;
                    };
                    let value = value.trim();
                    if name.eq_ignore_ascii_case("etag") {
                        etag = Some(value.to_owned());
                    } else if name.eq_ignore_ascii_case("x-ratelimit-remaining") {
                        remaining = value.parse().ok();
                    } else if name.eq_ignore_ascii_case("x-ratelimit-reset") {
                        reset_at_unix = value.parse().ok();
                    } else if name.eq_ignore_ascii_case("retry-after") {
                        retry_after_seconds = value.parse().ok();
                    }
                }
            }
        }
    }
    status
        .map(|status| {
            let rate_limit =
                (remaining.is_some() || reset_at_unix.is_some() || retry_after_seconds.is_some())
                    .then_some(RateLimitInfo {
                        remaining,
                        reset_at_unix,
                        retry_after_seconds,
                    });
            (status, etag, rate_limit)
        })
        .ok_or_else(|| AllpError::Parse {
            backend: "GitHub release source".to_owned(),
            message: "HTTPS response did not include a status line".to_owned(),
        })
}

fn normalized_http_error(body: &[u8]) -> String {
    let body = String::from_utf8_lossy(body);
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("GitHub request failed without a response message")
        .chars()
        .take(500)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MockHttp {
        responses: Mutex<Vec<HttpResponse>>,
    }

    impl HttpClient for MockHttp {
        fn get(&self, _url: &str, _etag: Option<&str>) -> AllpResult<HttpResponse> {
            Ok(self.responses.lock().unwrap().remove(0))
        }
    }

    #[test]
    fn wrong_repository_is_rejected_before_network_access() {
        let http = MockHttp {
            responses: Mutex::new(Vec::new()),
        };
        let source = GitHubReleaseSource::new(
            GitHubRepository {
                owner: "attacker",
                name: "allp",
            },
            &http,
        );
        let error = source
            .latest_release(UpdateChannel::Stable, &Version::new(0, 3, 3))
            .expect_err("untrusted repository must fail");
        assert!(error.to_string().contains("trusted repository"));

        let source = GitHubActionsBuildSource::new(
            GitHubRepository {
                owner: "attacker",
                name: "allp",
            },
            &http,
        );
        let error = source
            .latest_build(
                UpdateChannel::Continuous,
                &Version::new(0, 3, 5),
                Some("x86_64-unknown-linux-gnu"),
            )
            .expect_err("untrusted continuous repository must fail");
        assert!(error.to_string().contains("trusted repository"));
    }

    #[test]
    fn missing_latest_stable_release_is_cleanly_ignored() {
        let http = MockHttp {
            responses: Mutex::new(vec![HttpResponse {
                status: 404,
                body: br#"{"message":"Not Found"}"#.to_vec(),
                etag: None,
                rate_limit: None,
            }]),
        };
        let source = GitHubReleaseSource::official(&http);
        assert!(source
            .latest_release(UpdateChannel::Stable, &Version::new(0, 3, 3))
            .unwrap()
            .is_none());
    }

    #[test]
    fn newer_stable_release_loads_its_exact_manifest() {
        let release = br#"{"tag_name":"v0.3.4","draft":false,"prerelease":false,"published_at":"2026-07-17T00:00:00Z","assets":[{"name":"allp-release-manifest.json","browser_download_url":"https://github.com/allp-manager/allp-manager/releases/download/v0.3.4/allp-release-manifest.json"}]}"#;
        let manifest = br#"{"schema_version":1,"version":"0.3.4","tag":"v0.3.4","channel":"stable","published_at":"2026-07-17T00:00:00Z","minimum_updater_version":"0.3.3","assets":[{"target":"x86_64-unknown-linux-gnu","os":"linux","architecture":"x86_64","libc":"glibc","archive":"allp-v0.3.4-x86_64-unknown-linux-gnu.tar.gz","binary":"allp","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","size":42}]}"#;
        let http = MockHttp {
            responses: Mutex::new(vec![
                HttpResponse {
                    status: 200,
                    body: release.to_vec(),
                    etag: Some("etag-release-list".to_owned()),
                    rate_limit: None,
                },
                HttpResponse {
                    status: 200,
                    body: manifest.to_vec(),
                    etag: None,
                    rate_limit: None,
                },
            ]),
        };
        let selected = GitHubReleaseSource::official(&http)
            .latest_release(UpdateChannel::Stable, &Version::new(0, 3, 3))
            .expect("release lookup should work")
            .expect("newer release should be selected");
        assert_eq!(selected.version, Version::new(0, 3, 4));
        assert_eq!(selected.tag, "v0.3.4");
        assert_eq!(selected.etag.as_deref(), Some("etag-release-list"));
    }

    #[test]
    fn stable_lookup_uses_latest_endpoint_not_continuous_release_feed() {
        struct UrlHttp {
            responses: Mutex<Vec<HttpResponse>>,
            urls: Mutex<Vec<String>>,
        }
        impl HttpClient for UrlHttp {
            fn get(&self, url: &str, _etag: Option<&str>) -> AllpResult<HttpResponse> {
                self.urls.lock().unwrap().push(url.to_owned());
                Ok(self.responses.lock().unwrap().remove(0))
            }
        }
        let release = serde_json::json!({
            "tag_name": "v0.3.4",
            "draft": false,
            "prerelease": false,
            "published_at": "2026-07-17T00:00:00Z",
            "assets": [{
                "name": "allp-release-manifest.json",
                "browser_download_url": "https://github.com/allp-manager/allp-manager/releases/download/v0.3.4/allp-release-manifest.json"
            }]
        });
        let manifest = serde_json::json!({
            "schema_version": 1,
            "version": "0.3.4",
            "tag": "v0.3.4",
            "channel": "stable",
            "published_at": "2026-07-17T00:00:00Z",
            "minimum_updater_version": "0.3.3",
            "assets": []
        });
        let http = UrlHttp {
            responses: Mutex::new(vec![response(release), response(manifest)]),
            urls: Mutex::new(Vec::new()),
        };
        GitHubReleaseSource::official(&http)
            .latest_release(UpdateChannel::Stable, &Version::new(0, 3, 3))
            .expect("stable lookup should succeed")
            .expect("stable candidate should exist");
        let urls = http.urls.lock().unwrap();
        assert!(urls[0].ends_with("/releases/latest"));
        assert!(!urls.iter().any(|url| url.contains("releases?per_page")));
    }

    #[test]
    fn semantic_prerelease_lookup_isolated_from_continuous_tags() {
        let refs = serde_json::json!([
            {"ref": "refs/tags/continuous-v9.9.9.999"},
            {"ref": "refs/tags/v0.3.4"}
        ]);
        let release = serde_json::json!({
            "tag_name": "v0.3.4",
            "draft": false,
            "prerelease": true,
            "published_at": "2026-07-17T00:00:00Z",
            "assets": [{
                "name": "allp-release-manifest.json",
                "browser_download_url": "https://github.com/allp-manager/allp-manager/releases/download/v0.3.4/allp-release-manifest.json"
            }]
        });
        let manifest = serde_json::json!({
            "schema_version": 1,
            "version": "0.3.4",
            "tag": "v0.3.4",
            "channel": "prerelease",
            "published_at": "2026-07-17T00:00:00Z",
            "minimum_updater_version": "0.3.3",
            "assets": []
        });
        let http = MockHttp {
            responses: Mutex::new(vec![response(refs), response(release), response(manifest)]),
        };
        let selected = GitHubReleaseSource::official(&http)
            .latest_release(UpdateChannel::Prerelease, &Version::new(0, 3, 3))
            .expect("semantic prerelease lookup should succeed")
            .expect("semantic prerelease should be selected");
        assert_eq!(selected.version, Version::new(0, 3, 4));
        assert_eq!(selected.channel, UpdateChannel::Prerelease);
    }

    #[test]
    fn legacy_etag_is_not_sent_without_a_cached_verified_descriptor() {
        struct RecordingHttp {
            responses: Mutex<Vec<HttpResponse>>,
            etags: Mutex<Vec<Option<String>>>,
        }
        impl HttpClient for RecordingHttp {
            fn get(&self, _url: &str, etag: Option<&str>) -> AllpResult<HttpResponse> {
                self.etags.lock().unwrap().push(etag.map(str::to_owned));
                Ok(self.responses.lock().unwrap().remove(0))
            }
        }
        let http = RecordingHttp {
            responses: Mutex::new(vec![HttpResponse {
                status: 404,
                body: br#"{"message":"Not Found"}"#.to_vec(),
                etag: None,
                rate_limit: None,
            }]),
            etags: Mutex::new(Vec::new()),
        };
        let source = GitHubReleaseSource::official_with_etag(&http, Some("etag-old"));
        assert!(source
            .latest_release(UpdateChannel::Stable, &Version::new(0, 3, 3))
            .expect("uncached refresh should work")
            .is_none());
        assert_eq!(*http.etags.lock().unwrap(), vec![None]);
        assert_eq!(ReleaseSource::response_etag(&source), None);
    }

    #[test]
    fn release_asset_must_belong_to_exact_repository_and_tag() {
        let error = validate_release_asset_url(
            OFFICIAL_REPOSITORY,
            "v0.3.4",
            "https://github.com/other/allp/releases/download/v0.3.4/allp.tar.gz",
        )
        .expect_err("foreign repository should fail");
        assert!(error.to_string().contains("trusted GitHub release"));
    }

    #[cfg(unix)]
    #[test]
    fn header_capture_file_is_exclusive_private_and_removed_on_drop() {
        use std::os::unix::fs::PermissionsExt;

        let headers = SecureHeadersFile::create().expect("secure header file should be created");
        let path = headers.path().to_owned();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .expect_err("exclusive file must already exist")
                .kind(),
            std::io::ErrorKind::AlreadyExists
        );
        drop(headers);
        assert!(!path.exists());
    }

    #[test]
    fn rate_limit_headers_produce_actionable_error() {
        let headers = "HTTP/2 429\r\nx-ratelimit-remaining: 0\r\nx-ratelimit-reset: 1800000000\r\nretry-after: 60\r\n\r\n";
        let (status, _, rate_limit) = parse_response_headers(headers).unwrap();
        let response = HttpResponse {
            status,
            body: br#"{"message":"API rate limit exceeded"}"#.to_vec(),
            etag: None,
            rate_limit,
        };
        let error = require_http_success(
            "GitHub Actions build source",
            "https://api.github.com/test",
            &response,
        )
        .expect_err("rate limiting must fail");
        let rendered = error.to_string();
        assert!(rendered.contains("rate limit remaining=0"));
        assert!(rendered.contains("reset Unix timestamp=1800000000"));
        assert!(rendered.contains("retry after 60 seconds"));
    }

    #[test]
    fn successful_continuous_workflow_produces_build_candidate() {
        let http = MockHttp {
            responses: Mutex::new(vec![
                response(continuous_release()),
                response(continuous_manifest()),
                response(continuous_run("completed", "success")),
                response(continuous_artifacts(false)),
            ]),
        };
        let candidate = GitHubActionsBuildSource::official(&http)
            .latest_build(
                UpdateChannel::Continuous,
                &Version::new(0, 3, 5),
                Some("x86_64-unknown-linux-gnu"),
            )
            .expect("continuous lookup should succeed")
            .expect("successful workflow should be eligible");
        let identity = candidate
            .build_identity
            .expect("continuous candidate has an identity");
        assert_eq!(identity.display_version(), "0.3.5.2");
        assert_eq!(identity.git_commit, "a".repeat(40));
        assert_eq!(identity.target, "x86_64-unknown-linux-gnu");
        assert_eq!(candidate.channel, UpdateChannel::Continuous);
    }

    #[test]
    fn continuous_feed_404_is_a_clean_no_candidate_result() {
        let http = MockHttp {
            responses: Mutex::new(vec![HttpResponse {
                status: 404,
                body: br#"{"message":"Not Found"}"#.to_vec(),
                etag: None,
                rate_limit: None,
            }]),
        };
        assert!(GitHubActionsBuildSource::official(&http)
            .latest_build(
                UpdateChannel::Continuous,
                &Version::new(0, 3, 5),
                Some("x86_64-unknown-linux-gnu"),
            )
            .expect("an unpublished continuous feed should be a clean result")
            .is_none());
    }

    #[test]
    fn missing_requested_continuous_asset_is_left_for_structured_classification() {
        let http = MockHttp {
            responses: Mutex::new(vec![
                response(continuous_release()),
                response(continuous_manifest()),
                response(continuous_run("completed", "success")),
                response(continuous_artifacts(false)),
            ]),
        };
        let requested = "aarch64-apple-darwin";
        let candidate = GitHubActionsBuildSource::official(&http)
            .latest_build(
                UpdateChannel::Continuous,
                &Version::new(0, 3, 5),
                Some(requested),
            )
            .expect("source should return metadata for structured target classification")
            .expect("successful workflow metadata should remain available");
        assert_eq!(candidate.build_identity.unwrap().target, requested);
        assert!(candidate
            .manifest
            .assets
            .iter()
            .all(|asset| asset.target != requested));
    }

    #[test]
    fn failed_newest_continuous_candidate_falls_back_deterministically() {
        let http = MockHttp {
            responses: Mutex::new(vec![
                response(serde_json::json!([
                    continuous_release_candidate(3, 124),
                    continuous_release_candidate(2, 123)
                ])),
                response(continuous_manifest_candidate(3, 124, "b")),
                response(continuous_run_candidate(3, 124, "b", "failure")),
                response(continuous_manifest()),
                response(continuous_run_candidate(2, 123, "a", "success")),
                response(continuous_artifacts(false)),
            ]),
        };
        let candidate = GitHubActionsBuildSource::official(&http)
            .latest_build(
                UpdateChannel::Continuous,
                &Version::new(0, 3, 5),
                Some("x86_64-unknown-linux-gnu"),
            )
            .expect("fallback lookup should succeed")
            .expect("older successful candidate should be selected");
        assert_eq!(
            candidate.build_identity.unwrap().display_version(),
            "0.3.5.2"
        );
    }

    #[test]
    fn failed_or_running_continuous_workflow_is_ignored() {
        for (status, conclusion) in [("completed", "failure"), ("in_progress", "")] {
            let http = MockHttp {
                responses: Mutex::new(vec![
                    response(continuous_release()),
                    response(continuous_manifest()),
                    response(continuous_run(status, conclusion)),
                ]),
            };
            assert!(GitHubActionsBuildSource::official(&http)
                .latest_build(
                    UpdateChannel::Continuous,
                    &Version::new(0, 3, 5),
                    Some("x86_64-unknown-linux-gnu"),
                )
                .expect("ineligible workflow should be ignored")
                .is_none());
        }
    }

    #[test]
    fn continuous_release_without_manifest_is_invalid() {
        let mut release = continuous_release();
        release[0]["assets"] = serde_json::json!([]);
        let http = MockHttp {
            responses: Mutex::new(vec![response(release)]),
        };
        let error = GitHubActionsBuildSource::official(&http)
            .latest_build(
                UpdateChannel::Continuous,
                &Version::new(0, 3, 5),
                Some("x86_64-unknown-linux-gnu"),
            )
            .expect_err("missing manifest must invalidate the candidate");
        assert!(error
            .to_string()
            .contains("missing allp-continuous-manifest"));
    }

    #[test]
    fn expired_actions_artifact_is_rejected() {
        let http = MockHttp {
            responses: Mutex::new(vec![
                response(continuous_release()),
                response(continuous_manifest()),
                response(continuous_run("completed", "success")),
                response(continuous_artifacts(true)),
            ]),
        };
        let error = GitHubActionsBuildSource::official(&http)
            .latest_build(
                UpdateChannel::Continuous,
                &Version::new(0, 3, 5),
                Some("x86_64-unknown-linux-gnu"),
            )
            .expect_err("expired artifact must be ineligible");
        assert!(error
            .to_string()
            .contains("unexpired authoritative artifact"));
    }

    #[test]
    fn actions_artifact_without_github_digest_is_rejected() {
        let mut artifacts = continuous_artifacts(false);
        artifacts["artifacts"][0]
            .as_object_mut()
            .expect("artifact object")
            .remove("digest");
        let http = MockHttp {
            responses: Mutex::new(vec![
                response(continuous_release()),
                response(continuous_manifest()),
                response(continuous_run("completed", "success")),
                response(artifacts),
            ]),
        };
        let error = GitHubActionsBuildSource::official(&http)
            .latest_build(
                UpdateChannel::Continuous,
                &Version::new(0, 3, 5),
                Some("x86_64-unknown-linux-gnu"),
            )
            .expect_err("an artifact without GitHub's digest must be ineligible");
        assert!(error
            .to_string()
            .contains("unexpired authoritative artifact"));
    }

    #[test]
    fn actions_artifact_name_must_bind_exact_manifest_hash() {
        let mut artifacts = continuous_artifacts(false);
        artifacts["artifacts"][0]["name"] =
            Value::String("allp-continuous-0.3.5.2-deadbeef".to_owned());
        let http = MockHttp {
            responses: Mutex::new(vec![
                response(continuous_release()),
                response(continuous_manifest()),
                response(continuous_run("completed", "success")),
                response(artifacts),
            ]),
        };
        let error = GitHubActionsBuildSource::official(&http)
            .latest_build(
                UpdateChannel::Continuous,
                &Version::new(0, 3, 5),
                Some("x86_64-unknown-linux-gnu"),
            )
            .expect_err("artifact name must bind the downloaded manifest bytes");
        assert!(error
            .to_string()
            .contains("unexpired authoritative artifact"));
    }

    fn response(value: Value) -> HttpResponse {
        HttpResponse {
            status: 200,
            body: serde_json::to_vec(&value).unwrap(),
            etag: None,
            rate_limit: None,
        }
    }

    fn continuous_release() -> Value {
        serde_json::json!([{
            "tag_name": "continuous-v0.3.5.2",
            "draft": false,
            "prerelease": true,
            "published_at": "2026-08-08T00:00:00Z",
            "assets": [
                {
                    "name": CONTINUOUS_MANIFEST_NAME,
                    "browser_download_url": "https://github.com/allp-manager/allp-manager/releases/download/continuous-v0.3.5.2/allp-continuous-manifest.json"
                },
                {
                    "name": "allp-0.3.5.2-x86_64-unknown-linux-gnu.tar.gz",
                    "browser_download_url": "https://github.com/allp-manager/allp-manager/releases/download/continuous-v0.3.5.2/allp-0.3.5.2-x86_64-unknown-linux-gnu.tar.gz"
                }
            ]
        }])
    }

    fn continuous_manifest() -> Value {
        serde_json::json!({
            "schema_version": 1,
            "channel": "continuous",
            "base_version": "0.3.5",
            "build_revision": 2,
            "display_version": "0.3.5.2",
            "git_commit": "a".repeat(40),
            "build_id": "123.1",
            "workflow_run_id": "123",
            "workflow_run_number": 2,
            "workflow_name": CONTINUOUS_WORKFLOW_NAME,
            "workflow_file": CONTINUOUS_WORKFLOW_PATH,
            "built_at": "2026-08-08T00:00:00Z",
            "minimum_updater_version": "0.3.5",
            "assets": [{
                "target": "x86_64-unknown-linux-gnu",
                "os": "linux",
                "architecture": "x86_64",
                "libc": "glibc",
                "archive": "allp-0.3.5.2-x86_64-unknown-linux-gnu.tar.gz",
                "binary": "allp",
                "sha256": "b".repeat(64),
                "size": 42
            }]
        })
    }

    fn continuous_run(status: &str, conclusion: &str) -> Value {
        serde_json::json!({
            "id": 123,
            "run_number": 2,
            "run_attempt": 1,
            "name": CONTINUOUS_WORKFLOW_NAME,
            "path": CONTINUOUS_WORKFLOW_PATH,
            "status": status,
            "conclusion": conclusion,
            "head_sha": "a".repeat(40),
            "head_branch": "main",
            "event": "push"
        })
    }

    fn continuous_artifacts(expired: bool) -> Value {
        let manifest_bytes = serde_json::to_vec(&continuous_manifest()).unwrap();
        let manifest_sha256 = sha256_bytes(&manifest_bytes);
        serde_json::json!({
            "total_count": 1,
            "artifacts": [{
                "id": 456,
                "name": format!("allp-continuous-0.3.5.2-{manifest_sha256}"),
                "expired": expired,
                "size_in_bytes": 42,
                "digest": format!("sha256:{}", "c".repeat(64)),
                "workflow_run": {"id": 123}
            }]
        })
    }

    fn continuous_release_candidate(revision: u64, run_id: u64) -> Value {
        let display = format!("0.3.5.{revision}");
        let tag = format!("continuous-v{display}");
        let archive = format!("allp-{display}-x86_64-unknown-linux-gnu.tar.gz");
        serde_json::json!({
            "tag_name": tag,
            "draft": false,
            "prerelease": true,
            "published_at": "2026-08-08T00:00:00Z",
            "assets": [
                {
                    "name": CONTINUOUS_MANIFEST_NAME,
                    "browser_download_url": format!("https://github.com/allp-manager/allp-manager/releases/download/{tag}/{CONTINUOUS_MANIFEST_NAME}")
                },
                {
                    "name": archive,
                    "browser_download_url": format!("https://github.com/allp-manager/allp-manager/releases/download/{tag}/{archive}")
                }
            ],
            "test_run_id": run_id
        })
    }

    fn continuous_manifest_candidate(revision: u64, run_id: u64, commit: &str) -> Value {
        let display = format!("0.3.5.{revision}");
        serde_json::json!({
            "schema_version": 1,
            "channel": "continuous",
            "base_version": "0.3.5",
            "build_revision": revision,
            "display_version": display,
            "git_commit": commit.repeat(40),
            "build_id": format!("{run_id}.1"),
            "workflow_run_id": run_id.to_string(),
            "workflow_run_number": revision,
            "workflow_name": CONTINUOUS_WORKFLOW_NAME,
            "workflow_file": CONTINUOUS_WORKFLOW_PATH,
            "built_at": "2026-08-08T00:00:00Z",
            "minimum_updater_version": "0.3.5",
            "assets": [{
                "target": "x86_64-unknown-linux-gnu",
                "os": "linux",
                "architecture": "x86_64",
                "libc": "glibc",
                "archive": format!("allp-{display}-x86_64-unknown-linux-gnu.tar.gz"),
                "binary": "allp",
                "sha256": "b".repeat(64),
                "size": 42
            }]
        })
    }

    fn continuous_run_candidate(
        revision: u64,
        run_id: u64,
        commit: &str,
        conclusion: &str,
    ) -> Value {
        serde_json::json!({
            "id": run_id,
            "run_number": revision,
            "run_attempt": 1,
            "name": CONTINUOUS_WORKFLOW_NAME,
            "path": CONTINUOUS_WORKFLOW_PATH,
            "status": "completed",
            "conclusion": conclusion,
            "head_sha": commit.repeat(40),
            "head_branch": "main",
            "event": "push"
        })
    }
}
