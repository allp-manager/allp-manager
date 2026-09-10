# راهنمای Backendها

[← خانه ویکی](Home.fa.md) · [English](Backends.md)

Allp در هر اجرا Backendها را دوباره کشف می‌کند و فقط Capability واقعاً موجود
را اعلام می‌کند. Experimental یعنی پیاده‌سازی و با Fake PATH تست شده، ولی هنوز
به اعتبارسنجی گسترده روی سیستم واقعی نیاز دارد.

| خانواده | Backendها | وضعیت | توضیح |
|---|---|---|---|
| سیستمی | APT، Pacman، DNF/DNF5 | Stable alpha | پکیج‌های Native توزیع |
| Image-based | rpm-ostree روی Bazzite/Fedora Atomic | Experimental | Deployment و Layering تراکنشی |
| سایر Linux | Zypper، APK، XBPS، Portage، eopkg، swupd | Experimental | قابلیت‌ها بسته به ابزار متفاوت است |
| اپ عمومی | Flatpak، Snap | Stable alpha | آماده‌بودن Remote/Socket بررسی می‌شود |
| چندسکویی | Homebrew/Linuxbrew | Experimental | Owner و Prefix اعتبارسنجی می‌شود |
| توسعه | PyPI با pip/pipx/uv | Experimental | Scope محیط، کاربر یا Tool |
| توسعه | npm با npm/pnpm/Yarn | Experimental | Scope پروژه، Workspace یا Global |
| توسعه | crates.io با Cargo | Experimental | Binary crate؛ بدون تغییر Dependency پروژه |

## Backendهای سیستمی

APT Metadata را با `apt-get update` تازه می‌کند. Pacman عمداً Update مستقل
ندارد، چون Partial Upgrade امن نیست و از Sync+Upgrade کامل استفاده می‌کند. DNF
هر دو شکل خروجی DNF4 و DNF5 را پوشش می‌دهد.

روی Bazzite تغییر Host از طریق DNF غیرفعال است و `rpm-ostree` Refresh، ارتقای
Image و Layering صریح را انجام می‌دهد. Layering معمولاً Reboot می‌خواهد و بعد
از پیشنهاد Flatpak، Homebrew یا Container به‌عنوان آخرین راه نشان داده می‌شود.

## Flatpak

وضعیت‌های «ابزار نصب نیست»، «بدون Remote»، «آماده» و «Probe شکست‌خورده» جدا
هستند. افزودن Flathub یک Plan مستقل و User-scoped است.

```bash
allp doctor flatpak
allp search firefox --from flatpak
allp install org.mozilla.firefox --from flatpak --dry-run
```

## Snap

Snap ابتدا از Socket محلی snapd برای جست‌وجوی گسترده، Resolve دقیق، نصب و
پیگیری Change استفاده می‌کند. نام Canonical، Publisher، Confinement، معماری،
Channel و وضعیت نصب قبل از Plan بررسی می‌شوند. پاسخ قطعی `snap-not-found` به
CLI قدیمی Fallback نمی‌کند؛ Fallback فقط برای خطای Transport/Compatibility است.

```bash
allp doctor snap
allp install pycharm --from snap --dry-run
```

`--classic` فقط وقتی Metadata واقعاً Classic بودن را اعلام کند اضافه می‌شود.

## Homebrew

Detect، Doctor و عملیات از یک Locator اعتبارسنجی‌شده استفاده می‌کنند. مسیر
تنظیم‌شده، State قبلی، مسیر کاربر اصلی و Prefix رسمی بررسی می‌شوند. در اجرای
`sudo allp` نیز Homebrew با Owner تأییدشده اجرا می‌شود.

```bash
allp doctor homebrew --verbose --no-color
sudo allp update --from homebrew --dry-run --skip-self-update
```

## Python، Node و Rust

هم‌نام بودن یک پکیج Registry به معنی رسمی‌بودن نیست و نتیجه Fuzzy خودکار نصب
نمی‌شود. Scope کاربر/پروژه حفظ می‌شود و این ابزارها مخفیانه Root نمی‌شوند.

```bash
allp install black --from pipx --dry-run
allp install typescript --from pnpm --dry-run
allp install ripgrep --from cargo --dry-run
```

نگهداری Cargo هیچ‌وقت `cargo add` یا `cargo update` پروژه را اجرا نمی‌کند.
ارتقای Binaryهای Global به ابزار اختیاری `cargo-update` نیاز دارد. جدول دقیق
در [Capability Matrix](../docs/CAPABILITY_MATRIX.md) نگه‌داری می‌شود.
