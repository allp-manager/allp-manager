# Parser fixtures

Store real, sanitized native command output by backend and version here.

Recommended layout:

```text
apt/<version>/search.txt
pacman/<version>/search.txt
dnf/<version>/search.txt
flatpak/<version>/search.txt
snap/<version>/search.txt
```

Every parser change should add or update fixtures. Do not include credentials, private repository URLs, or user-specific paths.
