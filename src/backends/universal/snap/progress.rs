use super::{snapd_socket_path, SnapdClient};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    sync::mpsc::{self, Receiver, TrySendError},
    thread,
    time::Duration,
};

const ACTIVE_CHANGES: &str = "/v2/changes?select=in-progress";
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const SOCKET_TIMEOUT: Duration = Duration::from_secs(1);

pub(super) fn watch_refresh() -> Option<Receiver<Option<String>>> {
    let client = SnapdClient::with_timeout(snapd_socket_path(), SOCKET_TIMEOUT);
    let existing = client.get(ACTIVE_CHANGES).ok().and_then(|baseline| {
        Some(
            refreshes(baseline.result.as_array()?)
                .map(|(id, _)| id.to_owned())
                .collect::<BTreeSet<_>>(),
        )
    });

    let (sender, receiver) = mpsc::sync_channel(1);
    let _ = sender.try_send(Some("Waiting for Snap task details".to_owned()));
    thread::Builder::new()
        .name("snap-refresh-progress".to_owned())
        .spawn(move || {
            let mut tracked_id: Option<String> = None;
            loop {
                thread::sleep(POLL_INTERVAL);
                let response = client.get(ACTIVE_CHANGES);
                let detail = match response.as_ref().ok().and_then(|r| r.result.as_array()) {
                    None => Some("Snap progress unavailable; retrying".to_owned()),
                    Some(changes) => {
                        if let Some(tracked) = tracked_id.as_deref() {
                            let Some((_, change)) =
                                refreshes(changes).find(|(id, _)| *id == tracked)
                            else {
                                break;
                            };
                            task_detail(change)
                        } else if let Some(existing) = &existing {
                            let mut new_refreshes =
                                refreshes(changes).filter(|(id, _)| !existing.contains(*id));
                            match (new_refreshes.next(), new_refreshes.next()) {
                                (Some((id, change)), None) => {
                                    tracked_id = Some(id.to_owned());
                                    task_detail(change)
                                }
                                (None, _) => Some("Waiting for Snap task details".to_owned()),
                                _ => Some(
                                    "Multiple Snap refreshes active; inspect snap changes"
                                        .to_owned(),
                                ),
                            }
                        } else {
                            // Without a baseline, the CLI's change cannot be
                            // identified safely. Label this as system activity.
                            let details = refreshes(changes)
                                .filter_map(|(_, change)| task_detail(change))
                                .take(2)
                                .collect::<Vec<_>>();
                            Some(if details.is_empty() {
                                "Waiting for Snap task details".to_owned()
                            } else {
                                format!("System activity: {}", details.join("; "))
                            })
                        }
                    }
                };
                let detail =
                    Some(detail.unwrap_or_else(|| "Waiting for Snap task details".to_owned()));
                if let Err(TrySendError::Disconnected(_)) = sender.try_send(detail) {
                    break;
                }
            }
        })
        .ok()?;
    Some(receiver)
}

fn refreshes(changes: &[Value]) -> impl Iterator<Item = (&str, &Value)> {
    changes
        .iter()
        .filter(|change| change["kind"] == "refresh-snap" && change["ready"] == false)
        .filter_map(|change| Some((change["id"].as_str()?, change)))
}

fn task_detail(change: &Value) -> Option<String> {
    let mut active = change["tasks"].as_array()?.iter().filter(|task| {
        matches!(
            task["status"].as_str(),
            Some("Doing" | "Wait" | "Undoing" | "Abort")
        )
    });
    let downloads = active
        .clone()
        .filter(|task| task["kind"] == "download-snap" && task["status"] == "Doing")
        .filter_map(download_detail)
        .take(2)
        .collect::<Vec<_>>();
    if !downloads.is_empty() {
        return Some(format!("Downloading {}", downloads.join(", ")));
    }

    let task = active
        .clone()
        .find(|task| {
            task["status"] == "Doing"
                && !matches!(
                    task["kind"].as_str(),
                    Some("check-rerefresh" | "process-delayed-security-backend-effects")
                )
        })
        .or_else(|| {
            active.clone().find(|task| {
                !matches!(
                    task["kind"].as_str(),
                    Some("check-rerefresh" | "process-delayed-security-backend-effects")
                )
            })
        })
        .or_else(|| active.next())?;
    let summary = task["summary"].as_str()?;
    let summary = task_name(task)
        .map(|name| format!("{name} · {summary}"))
        .unwrap_or_else(|| summary.to_owned());
    Some(match task["status"].as_str() {
        Some("Wait") => format!("Waiting: {summary}"),
        Some("Undoing" | "Abort") => format!("Reverting: {summary}"),
        _ => summary,
    })
}

fn download_detail(task: &Value) -> Option<String> {
    let progress = &task["progress"];
    let name = task_name(task)
        .or_else(|| progress["label"].as_str())
        .filter(|name| !name.is_empty())?;
    match (progress["done"].as_u64(), progress["total"].as_u64()) {
        (Some(done), Some(total)) if total > 0 => {
            let percent = u128::from(done.min(total)) * 100 / u128::from(total);
            Some(format!("{name} {percent}%"))
        }
        _ => Some(name.to_owned()),
    }
}

fn task_name(task: &Value) -> Option<&str> {
    task["data"]["affected-snaps"]
        .as_array()
        .and_then(|names| names.first())
        .and_then(Value::as_str)
        .or_else(|| {
            task["summary"]
                .as_str()
                .and_then(|summary| summary.split('"').nth(1))
        })
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests {
    use super::task_detail;
    use serde_json::{json, Value};

    #[test]
    fn live_download_fixture_reports_the_package_and_real_percentage() {
        let change: Value = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/snap/refresh-progress.json"
        ))
        .unwrap();
        assert_eq!(
            task_detail(&change).as_deref(),
            Some("Downloading code 91%")
        );
    }

    #[test]
    fn package_name_precedes_long_phase_text_and_work_takes_priority_over_waiting() {
        let change = json!({"tasks": [
            {"kind":"run-hook", "status":"Wait", "summary":"Waiting for an unrelated hook"},
            {"kind":"link-snap", "status":"Doing", "summary":"Make current revision of a snap available to the system", "data":{"affected-snaps":["code"]}},
            {"kind":"check-rerefresh", "status":"Doing", "summary":"Monitoring snaps"}
        ]});
        assert_eq!(
            task_detail(&change).as_deref(),
            Some("code · Make current revision of a snap available to the system")
        );
    }

    #[test]
    fn waiting_and_rollback_keep_package_identity_without_fabricating_percentages() {
        for (status, expected) in [
            ("Wait", "Waiting: code · Wait for code"),
            ("Undoing", "Reverting: code · Wait for code"),
        ] {
            let change = json!({"tasks":[{"kind":"link-snap", "status":status, "summary":"Wait for code", "data":{"affected-snaps":["code"]}}]});
            assert_eq!(task_detail(&change).as_deref(), Some(expected));
        }
        let change = json!({"tasks":[{"kind":"download-snap", "status":"Doing", "summary":"Download snap \"code\"", "progress":{"done":0,"total":0}}]});
        assert_eq!(task_detail(&change).as_deref(), Some("Downloading code"));
    }
}
