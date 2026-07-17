# Allp

[English](README.md)

> یک CLI برای Package Managerهای Native لینوکس؛ بدون عملیات مخفی.

Allp یک Package Manager جدید نیست. Allp ابزارهای موجود مثل APT، Pacman، DNF، Flatpak، Snap، Homebrew، ابزارهای Python و ابزارهای Node را در هر اجرا تشخیص می‌دهد، نتیجه‌ها را کوچک و قابل انتخاب نشان می‌دهد، دستور Native را قبل از اجرا چاپ می‌کند و همان دستور را مستقیم اجرا می‌کند.

## وضعیت

این نسخه یک کاندید Alpha عمومی برای v0.3.3 است.

عنوان انتشار: **Allp v0.3.3 — Official Software Resolution**.

Backendهای هدف این Alpha:

- APT
- Pacman
- DNF / DNF5
- Zypper، APK، XBPS، Portage، eopkg و swupd به صورت Experimental
- Flatpak
- Snap
- Homebrew / Linuxbrew به صورت Experimental
- Python: PyPI همراه pip، pipx و uv به صورت Experimental
- Node.js: npm registry همراه npm، pnpm و Yarn به صورت Experimental

Cargo، Composer، Go و اکوسیستم‌های دیگر هنوز خارج از Scope این فاز هستند.

## نمونه سریع

```bash
allp detect
allp search git
allp install git --dry-run
allp update --dry-run
```

## شکل CLI

شکل رسمی دستورها Command-first است:

```text
allp <command> [arguments] [options]
```

نمونه‌ها:

```bash
allp search git --limit 10
allp search git --scope apps
allp search openai --scope dev
allp search git --exact
allp install git --from apt --dry-run
allp install git --scope all --dry-run
allp remove git --from apt --dry-run
allp update --scope dev --target all --dry-run
allp update --from npm --target project --dry-run
allp update --from pipx --target tools --dry-run
allp install black --from pipx --dry-run
allp install typescript --from pnpm --dry-run
allp list --from apt --json
allp info git --from apt --json
allp info git --from apt --full
```

## Search

نتایج جست‌وجو سه سطح دارند:

- `Exact`
- `Related`
- `Fuzzy`

به صورت پیش‌فرض همه Matchهای Exact نمایش داده می‌شوند، برای هر Backend حداکثر پنج Match مرتبط نشان داده می‌شود، و خروجی قابل مشاهده حداکثر ۲۵ مورد است. Matchهای ضعیف Fuzzy فقط با `--all` نمایش داده می‌شوند.

Allp هیچ وقت Related را معادل همان نرم‌افزار فرض نمی‌کند.

اگر یک نام در چند اکوسیستم پیدا شود، مثلا APT، Homebrew، Python و Node، Allp هیچ‌کدام را بی‌صدا انتخاب نمی‌کند.

`--scope` یک انتخاب گسترده است:

- `--scope apps`: اپلیکیشن‌ها و ابزارها؛ شامل Packageهای سیستمی، Universal applicationها و Homebrew
- `--scope dev`: اکوسیستم‌های توسعه؛ شامل Python / PyPI و Node.js / npm registry
- `--scope all`: همه Sourceها با ترتیب System Packages، Universal Applications و Developer Ecosystems

`--from` دقیق‌تر است و Backend، Source، Ecosystem یا Installer را مشخص می‌کند. در Terminal تعاملی، اگر `--from` و `--scope` داده نشود، Allp دقیقا سه انتخاب نشان می‌دهد: Apps and tools، Developer ecosystems و All sources.

برای Install، همه نتیجه‌ها شماره Global و ثابت دارند. وقتی نتیجه زیاد باشد، Selector تعاملی استفاده می‌شود: Space برای صفحه بعد، `b` برای صفحه قبل، عدد برای انتخاب مستقیم، `/` برای Filter، Enter برای نتیجه Highlight شده یا اولین نتیجه قابل مشاهده، و `q` یا Escape برای لغو. در خروجی non-TTY و JSON این Selector فعال نمی‌شود.

## Dry Run و sudo

```bash
allp install git --dry-run
allp update --dry-run
```

در Dry Run، Detection و Planning انجام می‌شود اما Command تغییردهنده اجرا نمی‌شود.

## تایید نهایی و `--yes`

هر عملیات واقعی تغییردهنده بعد از نمایش Execution Plan به تایید نهایی Allp نیاز دارد؛ حتی اگر فقط یک نتیجه Exact پیدا شده باشد. Remove به صورت پیش‌فرض No است و Upgradeهای پرریسک هم پیش‌فرض No دارند.

گزینه `--yes` یا `-y` فقط تایید نهایی خود Allp را رد می‌کند:

```bash
allp install git --from apt --yes
allp update --from npm --target global --yes
```

این گزینه هیچ وقت فلگ‌هایی مثل `-y` یا `--assumeyes` را به Package Managerهای Native اضافه نمی‌کند و ابهام Package، انتخاب Installer/Target، حفاظت Homebrew، PEP 668 یا بررسی Ownership را دور نمی‌زند.

## Update برای Python و Node

`allp update` و `allp upgrade` هدف‌های Python و Node را هم کشف می‌کنند و برای هر هدف یا Plan عملیاتی می‌سازند یا دلیل دقیق Skip را نشان می‌دهند.

نمونه‌ها:

```bash
allp update --scope dev --target all --dry-run
allp update --from npm --target project --dry-run
allp update --from pnpm --target workspace --dry-run
allp update --from yarn --target project --dry-run
allp update --from pip --target environment --dry-run
allp update --from pipx --target tools --dry-run
allp update --from uv --target tools --dry-run
```

Node از دستورهای Native مثل `npm update`، `npm update --global`، `pnpm update`، `pnpm update --latest` و دستور مناسب نسخه Yarn استفاده می‌کند و هیچ وقت `npx update` تولید نمی‌کند. Python خروجی JSON مربوط به pip outdated را inspect می‌کند، برای محیط فعال از `python -m pip install --upgrade ...` استفاده می‌کند، و `pipx upgrade-all` و `uv tool upgrade --all` را پشتیبانی می‌کند.

روش پیشنهادی:

```bash
allp update
```

خود Allp را با `sudo` اجرا نکنید. Allp فقط Child Processهایی را که Root لازم دارند Elevate می‌کند.

قبل از اجرای Child Processهایی که Root لازم دارند، Allp دستورهای Native را نشان می‌دهد، نیاز به Administrator Access را توضیح می‌دهد، و قبل از اجرای واقعی دستورهای تغییردهنده تایید نهایی می‌گیرد. Dry Run هیچ وقت sudo را اجرا نمی‌کند.

اگر Allp از قبل با sudo اجرا شده باشد، برای عملیات Root دوباره sudo اضافه نمی‌کند و عملیات User-scoped مثل Homebrew، Python و Node را با کاربر اصلی sudo اجرا می‌کند، اگر آن کاربر قابل تشخیص باشد.

برای خروجی بزرگ `list` از Pager استفاده می‌شود. گزینه‌های `--filter`، `--limit` و `--no-pager` برای کنترل خروجی وجود دارند. خروجی پیش‌فرض `info` خلاصه و curated است؛ `--full` و `--raw` جزئیات بیشتر را نشان می‌دهند.

## مستندات

- [معماری](ARCHITECTURE.md)
- [قرارداد CLI](docs/CLI_CONTRACT.md)
- [معنای دستورها](docs/COMMANDS.md)
- [قرارداد Backend](docs/BACKEND_CONTRACT.md)
- [ماتریس قابلیت‌ها](docs/CAPABILITY_MATRIX.md)
- [مدل تایید](docs/CONFIRMATION_MODEL.md)
- [هویت نرم‌افزار](docs/SOFTWARE_IDENTITY.md)
- [Bootstrap رسمی](docs/OFFICIAL_BOOTSTRAP.md)
- [برخورد نام‌ها](docs/NAME_COLLISIONS.md)
- [Update برای اکوسیستم‌های توسعه](docs/DEVELOPER_UPDATES.md)
- [برنامه تست v0.3.2](docs/V0_3_2_TEST_PLAN.md)
- [برنامه تست v0.3.3](docs/V0_3_3_TEST_PLAN.md)
- [Bootstrap برای Homebrew](docs/HOMEBREW_BOOTSTRAP.md)
- [Roadmap](ROADMAP.md)
- [راهنمای افزودن Backend](docs/ADDING_BACKEND.md)
