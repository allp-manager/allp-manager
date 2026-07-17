use super::{
    GitHubRepository, ReleaseDescriptor, ReleaseSource, UpdateChannel, OFFICIAL_REPOSITORY,
};
use crate::{
    discovery::path::find_executable,
    domain::{AllpError, AllpResult, NativeCommand},
    execution::{render_native_command, ProcessRunner, StdProcessRunner},
    release::{ReleaseManifest, Version},
};
use serde_json::Value;
use std::{fs, path::PathBuf, sync::Mutex, time::Duration};

const MAX_METADATA_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub etag: Option<String>,
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
        let curl = find_executable("curl").ok_or_else(|| {
            AllpError::BackendNotDetected(
                "curl HTTPS client is required for self-update".to_owned(),
            )
        })?;
        let headers = temporary_headers_path();
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
            "X-GitHub-Api-Version: 2022-11-28",
            "--header",
            "User-Agent: allp-self-update",
            "--dump-header",
        ]);
        command = command.arg(headers.as_os_str());
        if let Some(etag) = etag {
            command = command.args(["--header", &format!("If-None-Match: {etag}")]);
        }
        command = command.arg(url).timeout(Duration::from_secs(35));
        let rendered = render_native_command(&command);
        let output = self.runner.capture(&command);
        let header_text = fs::read_to_string(&headers).unwrap_or_default();
        let _ = fs::remove_file(&headers);
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
        let (status, response_etag) = parse_response_headers(&header_text)?;
        Ok(HttpResponse {
            status,
            body: output.stdout.into_bytes(),
            etag: response_etag,
        })
    }
}

pub struct GitHubReleaseSource<'a> {
    repository: GitHubRepository,
    client: &'a dyn HttpClient,
    etag: Option<String>,
    response_etag: Mutex<Option<String>>,
}

impl<'a> GitHubReleaseSource<'a> {
    pub fn official(client: &'a dyn HttpClient) -> Self {
        Self {
            repository: OFFICIAL_REPOSITORY,
            client,
            etag: None,
            response_etag: Mutex::new(None),
        }
    }

    pub fn official_with_etag(client: &'a dyn HttpClient, etag: Option<&str>) -> Self {
        Self {
            repository: OFFICIAL_REPOSITORY,
            client,
            etag: etag.map(str::to_owned),
            response_etag: Mutex::new(None),
        }
    }

    #[cfg(test)]
    pub fn new(repository: GitHubRepository, client: &'a dyn HttpClient) -> Self {
        Self {
            repository,
            client,
            etag: None,
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
        if self.repository != OFFICIAL_REPOSITORY {
            return Err(AllpError::InvalidInput(
                "self-update repository does not match Allp's trusted repository".to_owned(),
            ));
        }
        let api_url = format!(
            "https://api.github.com/repos/{}/{}/releases?per_page=20",
            self.repository.owner, self.repository.name
        );
        let mut response = self.client.get(&api_url, self.etag.as_deref())?;
        if response.status == 304 {
            response = self.client.get(&api_url, None)?;
        }
        if response.status != 200 {
            return Err(AllpError::CommandFailed {
                backend: "GitHub release source".to_owned(),
                command: format!("GET {api_url}"),
                code: Some(i32::from(response.status)),
                stderr: normalized_http_error(&response.body),
            });
        }
        *self.response_etag.lock().expect("response ETag lock") = response.etag.clone();
        let releases: Value =
            serde_json::from_slice(&response.body).map_err(|error| AllpError::Parse {
                backend: "GitHub release source".to_owned(),
                message: error.to_string(),
            })?;
        let releases = releases.as_array().ok_or_else(|| AllpError::Parse {
            backend: "GitHub release source".to_owned(),
            message: "GitHub releases response was not an array".to_owned(),
        })?;

        let mut selected: Option<(Version, &Value)> = None;
        for release in releases {
            if release
                .get("draft")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                continue;
            }
            let prerelease = release
                .get("prerelease")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if channel == UpdateChannel::Stable && prerelease {
                continue;
            }
            let Some(tag) = release.get("tag_name").and_then(Value::as_str) else {
                continue;
            };
            let Ok(version) = tag.parse::<Version>() else {
                continue;
            };
            if version <= *current {
                continue;
            }
            if selected.as_ref().map_or(true, |(best, _)| version > *best) {
                selected = Some((version, release));
            }
        }
        let Some((version, release)) = selected else {
            return Ok(None);
        };
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
            return Err(AllpError::CommandFailed {
                backend: "GitHub release source".to_owned(),
                command: format!("GET {manifest_url}"),
                code: Some(i32::from(manifest_response.status)),
                stderr: normalized_http_error(&manifest_response.body),
            });
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
        Ok(Some(ReleaseDescriptor {
            version,
            tag,
            channel: release_channel,
            published_at: release
                .get("published_at")
                .and_then(Value::as_str)
                .map(str::to_owned),
            manifest,
            etag: response.etag,
        }))
    }

    fn response_etag(&self) -> Option<String> {
        self.response_etag
            .lock()
            .expect("response ETag lock")
            .clone()
    }
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

fn temporary_headers_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "allp-github-headers-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("request")
    ))
}

fn parse_response_headers(headers: &str) -> AllpResult<(u16, Option<String>)> {
    let mut status = None;
    let mut etag = None;
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
                etag = lines.find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("etag")
                        .then(|| value.trim().to_owned())
                });
            }
        }
    }
    status
        .map(|status| (status, etag))
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
    }

    #[test]
    fn prerelease_is_ignored_in_stable_mode() {
        let http = MockHttp {
            responses: Mutex::new(vec![HttpResponse {
                status: 200,
                body: br#"[{"tag_name":"v0.3.4","draft":false,"prerelease":true,"assets":[]}]"#
                    .to_vec(),
                etag: None,
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
        let release = br#"[{"tag_name":"v0.3.4","draft":false,"prerelease":false,"published_at":"2026-07-17T00:00:00Z","assets":[{"name":"allp-release-manifest.json","browser_download_url":"https://github.com/Aliazadi-1776/allp/releases/download/v0.3.4/allp-release-manifest.json"}]}]"#;
        let manifest = br#"{"schema_version":1,"version":"0.3.4","tag":"v0.3.4","channel":"stable","published_at":"2026-07-17T00:00:00Z","minimum_updater_version":"0.3.3","assets":[{"target":"x86_64-unknown-linux-gnu","os":"linux","architecture":"x86_64","libc":"glibc","archive":"allp-v0.3.4-x86_64-unknown-linux-gnu.tar.gz","binary":"allp","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","size":42}]}"#;
        let http = MockHttp {
            responses: Mutex::new(vec![
                HttpResponse {
                    status: 200,
                    body: release.to_vec(),
                    etag: Some("etag-release-list".to_owned()),
                },
                HttpResponse {
                    status: 200,
                    body: manifest.to_vec(),
                    etag: None,
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
    fn etag_is_sent_and_not_modified_response_is_refreshed() {
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
            responses: Mutex::new(vec![
                HttpResponse {
                    status: 304,
                    body: Vec::new(),
                    etag: None,
                },
                HttpResponse {
                    status: 200,
                    body: b"[]".to_vec(),
                    etag: Some("etag-fresh".to_owned()),
                },
            ]),
            etags: Mutex::new(Vec::new()),
        };
        let source = GitHubReleaseSource::official_with_etag(&http, Some("etag-old"));
        assert!(source
            .latest_release(UpdateChannel::Stable, &Version::new(0, 3, 3))
            .expect("conditional refresh should work")
            .is_none());
        assert_eq!(
            *http.etags.lock().unwrap(),
            vec![Some("etag-old".to_owned()), None]
        );
        assert_eq!(source.response_etag().as_deref(), Some("etag-fresh"));
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
}
