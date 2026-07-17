use crate::domain::{
    AllpError, AllpResult, InstalledPackage, MatchKind, PackageCandidate, PackageDomain,
    ResultSection, SearchScope,
};
use std::io::{self, IsTerminal, Write};

const FALLBACK_PAGE_SIZE: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlternativeNoMatchAction {
    ConfigureFlatpak,
    SearchAnother(String),
    UnrestrictedSearch,
    ShowDiagnostics,
    Cancelled,
}

pub fn select_no_alternative_action(
    can_configure_flatpak: bool,
    no_interactive: bool,
) -> AllpResult<AlternativeNoMatchAction> {
    if no_interactive {
        return Err(AllpError::NonInteractiveSelectionRequired);
    }
    loop {
        println!();
        if can_configure_flatpak {
            println!("[1] Configure a Flatpak remote");
        } else {
            println!("[1] Configure a Flatpak remote (unavailable)");
        }
        println!("[2] Search using another name");
        println!("[3] Return to unrestricted search");
        println!("[4] Show diagnostics");
        println!("[0] Cancel");
        let input = prompt_line(
            "Choose an action [1-4, 0 to cancel]: ",
            "input closed before an alternative action was selected",
        )?;
        match input.as_str() {
            "1" if can_configure_flatpak => return Ok(AlternativeNoMatchAction::ConfigureFlatpak),
            "1" => eprintln!("Flatpak cannot be configured in the current state."),
            "2" => {
                let query = prompt_line(
                    "Search name: ",
                    "input closed before another search name was entered",
                )?;
                if query.is_empty() {
                    eprintln!("Search name cannot be blank.");
                } else {
                    return Ok(AlternativeNoMatchAction::SearchAnother(query));
                }
            }
            "3" => return Ok(AlternativeNoMatchAction::UnrestrictedSearch),
            "4" => return Ok(AlternativeNoMatchAction::ShowDiagnostics),
            "0" => return Ok(AlternativeNoMatchAction::Cancelled),
            _ => eprintln!("Please enter 1, 2, 3, 4, or 0."),
        }
    }
}

pub fn select_candidate(
    candidates: &[PackageCandidate],
    no_interactive: bool,
) -> AllpResult<usize> {
    select_candidate_inner(candidates, no_interactive, false, "Choose a result")
}

pub fn select_package_candidate(
    candidates: &[PackageCandidate],
    no_interactive: bool,
) -> AllpResult<usize> {
    select_candidate_inner(candidates, no_interactive, true, "Select a package")
}

fn select_candidate_inner(
    candidates: &[PackageCandidate],
    no_interactive: bool,
    prompt_single: bool,
    prompt_label: &str,
) -> AllpResult<usize> {
    if candidates.is_empty() {
        return Err(AllpError::InvalidInput(
            "no candidates are available for selection".to_owned(),
        ));
    }
    let registry_non_exact = candidates.len() == 1
        && matches!(
            candidates[0].domain,
            PackageDomain::Python | PackageDomain::Node
        )
        && candidates[0].match_kind != MatchKind::Exact;
    if candidates.len() == 1 && !registry_non_exact && (!prompt_single || no_interactive) {
        return Ok(0);
    }
    if no_interactive {
        if registry_non_exact {
            return Err(AllpError::AmbiguousSelection(
                "registry result is not an exact match; select it explicitly or use an exact package ID"
                    .to_owned(),
            ));
        }
        return Err(AllpError::NonInteractiveSelectionRequired);
    }

    if should_page_candidate_selection(candidates, no_interactive) {
        return select_candidate_paged(candidates);
    }

    prompt_index(candidates.len(), prompt_label)
}

pub fn should_page_candidate_selection(
    candidates: &[PackageCandidate],
    no_interactive: bool,
) -> bool {
    !no_interactive
        && candidates.len() > FALLBACK_PAGE_SIZE
        && io::stdin().is_terminal()
        && io::stdout().is_terminal()
}

pub fn select_installed(packages: &[InstalledPackage], no_interactive: bool) -> AllpResult<usize> {
    if packages.is_empty() {
        return Err(AllpError::InvalidInput(
            "no installed packages are available for selection".to_owned(),
        ));
    }
    if packages.len() == 1 {
        return Ok(0);
    }
    if no_interactive {
        return Err(AllpError::NonInteractiveSelectionRequired);
    }

    prompt_index(packages.len(), "Choose installed package")
}

pub fn select_search_scope(no_interactive: bool) -> AllpResult<SearchScope> {
    if no_interactive {
        return Err(AllpError::NonInteractiveSelectionRequired);
    }

    loop {
        println!("Where should Allp search?\n");
        println!("[1] {}", SearchScope::AppsAndTools.label());
        println!("[2] {}", SearchScope::DeveloperEcosystems.label());
        println!("[3] {}", SearchScope::AllSources.label());
        let input = prompt_line(
            "\nChoose a search scope [1-3, 0 to cancel]: ",
            "input closed before a search scope was selected",
        )?;
        match input.as_str() {
            "1" => return Ok(SearchScope::AppsAndTools),
            "2" => return Ok(SearchScope::DeveloperEcosystems),
            "3" => return Ok(SearchScope::AllSources),
            "0" => {
                return Err(AllpError::InvalidInput(
                    "operation cancelled by user".to_owned(),
                ))
            }
            _ => eprintln!("Please enter 1, 2, 3, or 0."),
        }
    }
}

pub fn select_installer(
    candidate: &PackageCandidate,
    no_interactive: bool,
) -> AllpResult<Option<String>> {
    if candidate.installers.len() <= 1 {
        return Ok(None);
    }
    if no_interactive {
        return Err(AllpError::AmbiguousSelection(format!(
            "Multiple installers are available for {}. Use --from with one of: {}",
            candidate.package_id,
            candidate.installers.join(", ")
        )));
    }

    println!("\nSelected package");
    println!(
        "{} · {}",
        candidate
            .source
            .as_deref()
            .unwrap_or(&candidate.backend_name),
        candidate.package_id
    );
    println!("\nChoose an installer:");
    for (index, installer) in candidate.installers.iter().enumerate() {
        println!("[{}] {}", index + 1, installer);
        if let Some(description) = installer_description(installer) {
            println!("    {description}");
        }
    }

    let index = prompt_index(candidate.installers.len(), "Choose")?;
    Ok(Some(candidate.installers[index].clone()))
}

fn prompt_index(count: usize, label: &str) -> AllpResult<usize> {
    loop {
        let input = prompt_line(
            &format!("{label} [1-{count}, 0 to cancel]: "),
            "input closed before a selection was made",
        )?;
        let Ok(value) = input.parse::<usize>() else {
            eprintln!("Please enter a number between 1 and {count}.");
            continue;
        };

        if value == 0 {
            return Err(AllpError::InvalidInput(
                "operation cancelled by user".to_owned(),
            ));
        }
        if (1..=count).contains(&value) {
            return Ok(value - 1);
        }

        eprintln!("Please enter a number between 1 and {count}.");
    }
}

fn prompt_line(prompt: &str, eof_message: &str) -> AllpResult<String> {
    print!("{prompt}");
    io::stdout().flush()?;
    read_complete_line(eof_message)
}

fn read_complete_line(eof_message: &str) -> AllpResult<String> {
    let mut input = String::new();
    input.clear();
    if io::stdin().read_line(&mut input)? == 0 {
        return Err(AllpError::Timeout(eof_message.to_owned()));
    }
    Ok(input.trim().to_owned())
}

pub fn confirm_fuzzy_candidate(no_interactive: bool) -> AllpResult<()> {
    if no_interactive {
        return Err(AllpError::AmbiguousSelection(
            "the only result is a fuzzy match; use an exact package ID or select interactively"
                .to_owned(),
        ));
    }

    loop {
        let input = prompt_line(
            "The result is not an exact name match. Continue? [y/N]: ",
            "input closed before confirmation was provided",
        )?;
        match input.to_ascii_lowercase().as_str() {
            "y" | "yes" => return Ok(()),
            "" | "n" | "no" => {
                return Err(AllpError::InvalidInput(
                    "operation cancelled by user".to_owned(),
                ))
            }
            _ => eprintln!("Please answer yes or no."),
        }
    }
}

pub fn confirm_conflicting_identity(
    candidate: &PackageCandidate,
    no_interactive: bool,
) -> AllpResult<()> {
    let canonical = candidate
        .identity
        .canonical_name
        .as_deref()
        .unwrap_or("the requested software");
    let message = format!(
        "{} from {} is only an exact package-name match and conflicts with {canonical}.",
        candidate.package_id, candidate.backend_name
    );
    if no_interactive {
        return Err(AllpError::AmbiguousSelection(format!(
            "{message}\n\nInteractive confirmation is required before installing a conflicting identity."
        )));
    }

    println!("\nIdentity warning");
    println!("{message}");
    if let Some(warning) = &candidate.identity.warning {
        println!("{warning}");
    }
    loop {
        let input = prompt_line(
            "Install this conflicting package anyway? [y/N]: ",
            "input closed before confirmation was provided",
        )?;
        match input.to_ascii_lowercase().as_str() {
            "y" | "yes" => return Ok(()),
            "" | "n" | "no" | "q" => {
                return Err(AllpError::InvalidInput(
                    "operation cancelled by user".to_owned(),
                ))
            }
            _ => eprintln!("Please answer yes or no."),
        }
    }
}

pub struct ConfirmationRequest {
    pub prompt: String,
    pub default_yes: bool,
    pub non_interactive_hint: String,
}

pub fn confirm_execution(
    no_interactive: bool,
    yes: bool,
    request: ConfirmationRequest,
) -> AllpResult<bool> {
    if yes {
        return Ok(true);
    }
    if no_interactive {
        return Err(AllpError::InvalidInput(
            format!(
                "confirmation required before executing a mutating command; no interactive terminal is available.\n\n{}",
                request.non_interactive_hint
            ),
        ));
    }

    loop {
        let suffix = if request.default_yes {
            "[Y/n]"
        } else {
            "[y/N]"
        };
        let input = prompt_line(
            &format!("{} {suffix}: ", request.prompt),
            "input closed before confirmation was provided",
        )?;
        match input.to_ascii_lowercase().as_str() {
            "" if request.default_yes => return Ok(true),
            "" => return Ok(false),
            "y" | "yes" => return Ok(true),
            "n" | "no" | "q" => return Ok(false),
            "\u{1b}" => return Ok(false),
            _ => eprintln!("Please answer yes or no."),
        }
    }
}

fn select_candidate_paged(candidates: &[PackageCandidate]) -> AllpResult<usize> {
    let mut filter = String::new();
    let mut page = 0usize;
    let page_size = terminal_page_size();

    loop {
        let visible = visible_candidate_indices(candidates, &filter);
        if visible.is_empty() {
            println!("\nNo results match the current filter.");
            print!("Type / to change the filter or q to cancel: ");
            io::stdout().flush()?;
            let input = read_trimmed_line()?;
            if is_cancel(&input) {
                return Err(cancelled());
            }
            if input.starts_with('/') {
                filter = read_filter_from_command(&input)?;
                page = 0;
            }
            continue;
        }

        let page_count = visible.len().div_ceil(page_size);
        if page >= page_count {
            page = page_count.saturating_sub(1);
        }
        render_candidate_page(candidates, &visible, page, page_size, &filter);

        print!("Select result: ");
        io::stdout().flush()?;
        let input = read_raw_line()?;
        let command = parse_paged_command(&input);
        match command {
            PagedCommand::Next => {
                if page + 1 < page_count {
                    page += 1;
                }
            }
            PagedCommand::Previous => {
                page = page.saturating_sub(1);
            }
            PagedCommand::Filter(value) => {
                filter = value;
                page = 0;
            }
            PagedCommand::Cancel => return Err(cancelled()),
            PagedCommand::Help => print_pager_help(),
            PagedCommand::SelectHighlighted => {
                let start = page * page_size;
                return Ok(visible[start]);
            }
            PagedCommand::Select(number) => {
                if (1..=candidates.len()).contains(&number) {
                    let index = number - 1;
                    if visible.contains(&index) {
                        return Ok(index);
                    }
                    eprintln!("Result {number} is hidden by the current filter.");
                } else {
                    eprintln!("Please enter a number between 1 and {}.", candidates.len());
                }
            }
            PagedCommand::Invalid => eprintln!("Enter a result number, Space, b, /, ?, or q."),
        }
    }
}

fn render_candidate_page(
    candidates: &[PackageCandidate],
    visible: &[usize],
    page: usize,
    page_size: usize,
    filter: &str,
) {
    let start = page * page_size;
    let end = ((page + 1) * page_size).min(visible.len());
    let page_count = visible.len().div_ceil(page_size);

    println!("\nSearch Results · Page {}/{}", page + 1, page_count);
    if !filter.is_empty() {
        println!("Filter: {filter}");
    }

    let page_indices = &visible[start..end];
    for section in ResultSection::ordered_for_scope(SearchScope::AllSources) {
        let section_items = page_indices
            .iter()
            .copied()
            .filter(|index| candidates[*index].result_section() == *section)
            .collect::<Vec<_>>();
        if section_items.is_empty() {
            continue;
        }
        println!("\n{}", section.label());
        for index in section_items {
            print_candidate_line(index, &candidates[index]);
        }
    }

    println!("\nShowing {}-{} of {}", start + 1, end, visible.len());
    println!("Space: next page · b: previous page · number: select · /: filter · q: cancel");
}

fn print_candidate_line(index: usize, candidate: &PackageCandidate) {
    println!(
        "[{}] {:<12} {:<32} {:<18} {}",
        index + 1,
        candidate.backend_name,
        candidate.package_id,
        prompt_candidate_label(candidate),
        candidate.version.as_deref().unwrap_or("unknown")
    );
    let source = candidate.source.as_deref().unwrap_or("unknown source");
    println!(
        "    source: {source} · type: {} · scope: {}",
        candidate.artifact_kind,
        candidate.scope.as_deref().unwrap_or("unknown")
    );
    if let Some(description) = &candidate.description {
        println!("    {description}");
    }
    if candidate.backend_id == "snap"
        && candidate
            .metadata
            .get("snap.availability")
            .is_some_and(|value| value == "discovered")
    {
        println!("    availability: not yet verified");
    }
}

fn prompt_candidate_label(candidate: &PackageCandidate) -> &str {
    if candidate.backend_id == "snap"
        && candidate
            .metadata
            .get("snap.availability")
            .is_some_and(|value| value == "discovered")
    {
        if candidate.match_kind == MatchKind::Exact {
            "Exact search match"
        } else {
            "Search match"
        }
    } else {
        candidate.identity.label()
    }
}

fn visible_candidate_indices(candidates: &[PackageCandidate], filter: &str) -> Vec<usize> {
    let filter = filter.trim().to_ascii_lowercase();
    candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            if filter.is_empty()
                || candidate.package_id.to_ascii_lowercase().contains(&filter)
                || candidate
                    .display_name
                    .to_ascii_lowercase()
                    .contains(&filter)
                || candidate
                    .description
                    .as_deref()
                    .map(|description| description.to_ascii_lowercase().contains(&filter))
                    .unwrap_or(false)
            {
                Some(index)
            } else {
                None
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PagedCommand {
    Next,
    Previous,
    Filter(String),
    Select(usize),
    SelectHighlighted,
    Cancel,
    Help,
    Invalid,
}

fn parse_paged_command(input: &str) -> PagedCommand {
    let raw = input.trim_end_matches(['\r', '\n']);
    let trimmed = raw.trim();
    if raw == " " || trimmed.eq_ignore_ascii_case("space") {
        return PagedCommand::Next;
    }
    if trimmed.is_empty() {
        return PagedCommand::SelectHighlighted;
    }
    if trimmed == "\u{1b}" || trimmed.eq_ignore_ascii_case("q") {
        return PagedCommand::Cancel;
    }
    if trimmed.eq_ignore_ascii_case("b") {
        return PagedCommand::Previous;
    }
    if trimmed == "?" {
        return PagedCommand::Help;
    }
    if trimmed.starts_with('/') {
        return match read_filter_from_command(trimmed) {
            Ok(filter) => PagedCommand::Filter(filter),
            Err(_) => PagedCommand::Cancel,
        };
    }
    match trimmed.parse::<usize>() {
        Ok(value) => PagedCommand::Select(value),
        Err(_) => PagedCommand::Invalid,
    }
}

fn read_filter_from_command(input: &str) -> AllpResult<String> {
    let value = input.trim_start_matches('/').trim();
    if !value.is_empty() {
        return Ok(value.to_owned());
    }
    print!("Filter results: ");
    io::stdout().flush()?;
    read_trimmed_line()
}

fn read_trimmed_line() -> AllpResult<String> {
    Ok(read_raw_line()?.trim().to_owned())
}

fn read_raw_line() -> AllpResult<String> {
    let mut input = String::new();
    if io::stdin().read_line(&mut input)? == 0 {
        return Err(AllpError::Timeout(
            "input closed before a selection was made".to_owned(),
        ));
    }
    Ok(input)
}

fn print_pager_help() {
    println!("Space: next page");
    println!("b: previous page");
    println!("<number>: select a stable result number");
    println!("/: filter results");
    println!("Enter: select the first visible result on this page");
    println!("q or Escape: cancel");
}

fn terminal_page_size() -> usize {
    std::env::var("LINES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .and_then(|lines| lines.checked_sub(8))
        .filter(|lines| *lines >= 4)
        .unwrap_or(FALLBACK_PAGE_SIZE)
        .min(20)
}

fn installer_description(installer: &str) -> Option<&'static str> {
    match installer.to_ascii_lowercase().as_str() {
        "pipx" => Some("Isolated user CLI environment"),
        "uv" => Some("Isolated user tool environment"),
        "pip" => Some("Current Python environment"),
        "npm" => Some("Global user tool through npm"),
        "pnpm" => Some("Global user tool through pnpm"),
        "yarn" => Some("Global user tool through Yarn"),
        _ => None,
    }
}

fn is_cancel(input: &str) -> bool {
    input == "\u{1b}" || input.eq_ignore_ascii_case("q")
}

fn cancelled() -> AllpError {
    AllpError::InvalidInput("operation cancelled by user".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{parse_paged_command, visible_candidate_indices, PagedCommand};
    use crate::domain::{BackendCategory, MatchKind, PackageCandidate, PackageDomain};

    #[test]
    fn paged_command_parses_required_controls() {
        assert_eq!(parse_paged_command(" \n"), PagedCommand::Next);
        assert_eq!(parse_paged_command("b\n"), PagedCommand::Previous);
        assert_eq!(parse_paged_command("17\n"), PagedCommand::Select(17));
        assert_eq!(
            parse_paged_command("/git\n"),
            PagedCommand::Filter("git".to_owned())
        );
        assert_eq!(parse_paged_command("q\n"), PagedCommand::Cancel);
        assert_eq!(parse_paged_command("\u{1b}\n"), PagedCommand::Cancel);
        assert_eq!(parse_paged_command("\n"), PagedCommand::SelectHighlighted);
    }

    #[test]
    fn filtered_results_keep_original_global_numbers() {
        let candidates = vec![
            candidate("alpha"),
            candidate("git"),
            candidate("git-lfs"),
            candidate("other"),
        ];

        let visible = visible_candidate_indices(&candidates, "git");

        assert_eq!(visible, vec![1, 2]);
    }

    fn candidate(package_id: &str) -> PackageCandidate {
        PackageCandidate {
            backend_id: "example".to_owned(),
            backend_name: "Example".to_owned(),
            category: BackendCategory::System,
            domain: PackageDomain::System,
            package_id: package_id.to_owned(),
            display_name: package_id.to_owned(),
            version: None,
            description: None,
            source: None,
            installers: Vec::new(),
            artifact_kind: "test".to_owned(),
            scope: None,
            match_kind: MatchKind::Related,
            identity: PackageCandidate::infer_identity(
                MatchKind::Related,
                PackageDomain::System,
                "test",
            ),
            metadata: Default::default(),
        }
    }
}
