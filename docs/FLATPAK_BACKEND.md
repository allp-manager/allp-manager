# Flatpak Backend

Flatpak executable detection and remote configuration are independent capabilities. Backend state is one of:

- `NotInstalled`
- `InstalledWithoutRemotes`
- `InstalledWithRemotes`
- `BackendError`

Remote inspection uses `flatpak remotes --columns=name,title,url,filter,options`. Zero remotes skips catalog search with a configuration reason; it is not a package-level no-match result.

Search uses `flatpak search <query> --columns=application,name,description,version,branch,remotes`. Results preserve the application ID, display name, description, version, branch, and remote. Installation is user-scoped and uses `flatpak install --user <remote> <application-id>`.

## Flathub

Flathub setup is a separate mutation:

```bash
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
```

Allp displays scope, remote, URL, command, and privilege before asking for a separate default-No confirmation. `--yes` alone cannot approve it; non-interactive approval requires `--yes --allow-bootstrap`. After execution, Allp reruns remote detection and continues search only when `flathub` is verified.
