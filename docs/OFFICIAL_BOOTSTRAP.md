# Official Bootstrap

Official bootstrap candidates are not registry packages and are not detected package-manager backends. They are catalog-backed installer plans for software Allp can identify canonically.

v0.3.3 includes an official Homebrew bootstrap candidate. It is shown before registry package-name collisions and is marked `Official installer`.

Bootstrap dry-runs do not download or execute anything. Real bootstrap execution still requires the normal final Allp confirmation. `--yes` bypasses only that final Allp confirmation; it does not bypass identity ambiguity or native installer prompts.

Bootstrap plans must not use a hidden `curl | bash` pipeline. The Homebrew plan downloads the official script to a temporary file and then runs that file explicitly with `/bin/bash`.
