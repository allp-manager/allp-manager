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
const SOCKET_TIMEOUT: Duration = Duration::from_millis(300);

pub(super) fn watch_refresh() -> Option<Receiver<Option<String>>> {
    let client = SnapdClient::with_timeout(snapd_socket_path(), SOCKET_TIMEOUT);
    let baseline = client.get(ACTIVE_CHANGES).ok()?;
    let existing = refreshes(baseline.result.as_array()?)
        .map(|(id, _)| id.to_owned())
        .collect::<BTreeSet<_>>();

    let (sender, receiver) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("snap-refresh-progress".to_owned())
        .spawn(move || {
            let mut tracked_id: Option<String> = None;
            loop {
                thread::sleep(POLL_INTERVAL);
                let Ok(response) = client.get(ACTIVE_CHANGES) else {
                    break;
                };
                let Some(changes) = response.result.as_array() else {
                    break;
                };
                let mut new_refreshes =
                    refreshes(changes).filter(|(id, _)| !existing.contains(*id));
                let change = new_refreshes.next();
                // The CLI doesn't publish its change ID while waiting. Avoid
                // attributing progress when several new refreshes are active.
                if new_refreshes.next().is_some() {
                    break;
                }
                let detail = if let Some((id, change)) = change {
                    if tracked_id.as_deref().is_some_and(|tracked| tracked != id) {
                        break;
                    }
                    tracked_id.get_or_insert_with(|| id.to_owned());
                    task_detail(change)
                } else if tracked_id.is_some() {
                    break;
                } else {
                    None
                };
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
            !matches!(
                task["kind"].as_str(),
                Some("check-rerefresh" | "process-delayed-security-backend-effects")
            )
        })
        .or_else(|| active.next())?;
    let summary = task["summary"].as_str()?;
    Some(match task["status"].as_str() {
        Some("Wait") => format!("Waiting: {summary}"),
        Some("Undoing" | "Abort") => format!("Reverting: {summary}"),
        _ => summary.to_owned(),
    })
}

fn download_detail(task: &Value) -> Option<String> {
    let progress = &task["progress"];
    let name = task["summary"]
        .as_str()
        .and_then(|summary| summary.split('"').nth(1))
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
