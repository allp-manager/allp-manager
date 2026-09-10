<div dir="rtl" align="right">

# پلتفرم‌ها و نصب

[← خانه ویکی](Home.fa.md) · [English](Platforms-and-Installation.md)

عملیات Package در Linux سطح اصلی محصول است. Homebrew در macOS هنوز Experimental
است. Windows ساخت، Diagnostics، انتخاب Release Target و جایگزینی Deferred و
تأییدشده را دارد، ولی Backendهای Linux-only مثل Snap/Flatpak را اعلام نمی‌کند.

Allp خانواده Debian، Red Hat/Fedora، Arch، SUSE و Alpine و همچنین Bazzite را
به‌عنوان Host مبتنی بر Image می‌شناسد. معماری، libc، وضعیت WSL/Container و
مسیرهای داده Platform را نیز ثبت می‌کند.

## روش‌های نصب

| روش | مقصد | مناسب برای |
|---|---|---|
| Installer تأییدشده | پیش‌فرض `~/.local/bin/allp` | بیشتر کاربران |
| `make install-user` | `~/.local/bin/allp` | Build سورس بدون sudo |
| `make install` | `/usr/local/bin/allp` | Build سراسری سورس |
| `cargo build --release` | `target/release/allp` | توسعه و تست |

```bash
curl --fail --location --output install-allp.sh \
  https://github.com/allp-manager/allp-manager/releases/latest/download/install-allp.sh
less install-allp.sh
sh install-allp.sh
```

نصاب Platform/Architecture، آرشیو دقیق، SHA-256 کنار آن و اعضای آرشیو را بررسی
می‌کند و عمداً `curl | sh` پیشنهاد نمی‌شود.

```bash
command -v allp
allp --version --verbose
make install-check
```

اگر Copy اشتباه Resolve شد، PATH را اصلاح و در bash از `hash -r` یا در zsh از
`rehash` استفاده کنید. Binary متعلق به dpkg/rpm/Pacman توسط همان منبع مدیریت می‌شود.


</div>
