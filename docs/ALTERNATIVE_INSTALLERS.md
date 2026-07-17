# Alternative Installer Routing

Alternative search is represented by `AlternativeSearchRequest`: query, optional software identity, and an excluded-backend set.

When Snap exact resolution fails and the user chooses another installer, Allp:

1. discards the previous report and candidate list;
2. excludes Snap before launching workers;
3. executes every eligible remaining backend;
4. reports excluded, unavailable, failed, no-remote, and no-match states;
5. builds a fresh result list without the failed Snap candidate.

`Search again` is different: it clears exclusions and performs a fresh unrestricted search including Snap. No-result recovery can configure a Flatpak remote, accept another query, return to unrestricted search, show diagnostics, or cancel. Prompts are iterative and EOF cancels; they do not recurse or loop automatically.
