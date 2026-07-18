# Flatpak Backend

Flatpak executable detection and remote configuration are independent capabilities. Backend state is one of:

- `Missing`
- `InstalledNoRemotes`
- `InstalledUserScopeReady`
- `InstalledSystemScopeReady`
- `InstalledBothScopesReady`
- `InstalledRefsWithoutUsableRemote`
- `BackendError`

Remote inspection probes user and system scopes separately with `flatpak remotes --user --columns=name,title,url,filter,options` and `flatpak remotes --system --columns=name,title,url,filter,options`. Zero remotes skips catalog search with a configuration reason; it is not a package-level no-match result and is not reported as "Detected and ready".

Search uses `flatpak search <query> --columns=application,name,description,version,branch,remotes`. Results preserve the application ID, display name, description, version, branch, and remote. Installation is user-scoped and uses `flatpak install --user <remote> <application-id>`.

`allp update` is metadata-only, so Flatpak reports `Not applicable`; `flatpak update` updates installed applications and runtimes and is therefore planned only during `allp upgrade`.

`allp upgrade` only plans scopes with configured remotes:

- user remotes produce `flatpak update --user`
- system remotes produce `flatpak update --system`
- no user or system remotes produce a `Not applicable` record and no native command

## Flathub

Flathub setup is a separate mutation:

```bash
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
```

Allp displays scope, remote, URL, command, and privilege before asking for a separate default-No confirmation. `--yes` alone cannot approve it; non-interactive approval requires `--yes --allow-bootstrap`. After execution, Allp reruns remote detection and continues search only when `flathub` is verified.
