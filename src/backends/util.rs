use crate::{
    backends::Backend,
    domain::{AllpError, AllpResult, MatchKind, NativeCommand},
    execution::{CommandOutput, ProcessRunner},
};
use std::{collections::BTreeMap, path::Path};

pub fn capture_checked(
    backend: &dyn Backend,
    runner: &dyn ProcessRunner,
    command: NativeCommand,
) -> AllpResult<String> {
    let rendered = crate::execution::render_native_command(&command);
    let output = runner.capture(&command)?;
    ensure_success(backend, rendered, output)
}

fn ensure_success(
    backend: &dyn Backend,
    rendered: String,
    output: CommandOutput,
) -> AllpResult<String> {
    if output.success {
        Ok(output.stdout)
    } else if backend.id() == "dnf"
        && output
            .stderr
            .to_ascii_lowercase()
            .contains("rpmdb open failed")
    {
        Err(AllpError::InvalidInput(
            "DNF could not open the RPM database. Check RPM database permissions or repair the rpmdb before retrying."
                .to_owned(),
        ))
    } else {
        Err(AllpError::CommandFailed {
            backend: backend.display_name().to_owned(),
            command: rendered,
            code: output.code,
            stderr: output.stderr,
        })
    }
}

pub fn match_kind(value: &str, query: &str) -> MatchKind {
    if value.eq_ignore_ascii_case(query) {
        MatchKind::Exact
    } else if value
        .to_ascii_lowercase()
        .starts_with(&query.to_ascii_lowercase())
        && value
            .chars()
            .nth(query.chars().count())
            .is_some_and(|character| matches!(character, '-' | '_' | '.' | '+'))
    {
        MatchKind::Related
    } else {
        MatchKind::Fuzzy
    }
}

pub fn parse_key_value_lines(input: &str) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    let mut current_key: Option<String> = None;

    for line in input.lines() {
        if line.trim().is_empty() {
            if !fields.is_empty() {
                break;
            }
            continue;
        }

        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(key) = current_key.as_ref() {
                let entry = fields.entry(key.clone()).or_insert_with(String::new);
                if !entry.is_empty() {
                    entry.push(' ');
                }
                entry.push_str(line.trim());
            }
            continue;
        }

        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_owned();
            fields.insert(key.clone(), value.trim().to_owned());
            current_key = Some(key);
        }
    }

    fields
}

pub fn split_tab_or_whitespace(line: &str) -> Vec<String> {
    let tabs: Vec<String> = line
        .split('\t')
        .map(|value| value.trim().to_owned())
        .collect();
    if tabs.len() > 1 {
        return tabs;
    }

    line.split_whitespace().map(str::to_owned).collect()
}

pub fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
