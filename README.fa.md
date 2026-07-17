# Allp

[English](README.md) | [فارسی](README.fa.md)

> یک CLI شفاف برای Package Managerهایی که همین حالا روی سیستم شما نصب هستند.

Allp Package Manager تازه ای نیست. Allp ابزارهایی مثل APT، Pacman، DNF، Flatpak، Snap، Homebrew/Linuxbrew، ابزارهای Python و ابزارهای Node را کشف می کند، نتیجه ها را قابل انتخاب نشان می دهد، و قبل از هر تغییر، دستور Native دقیق را چاپ می کند.

نسخه فعلی: **0.3.3**
سطح بلوغ: **Public Alpha**
عنوان انتشار: **Allp v0.3.3 - Snap Validation and Repository Stabilization**

## چرا Allp وجود دارد

نرم افزار در لینوکس فقط در یک جا نیست: بخشی در مخزن های سیستم، بخشی در Flatpak یا Snap، بخشی در Homebrew، و بخشی در اکوسیستم های Python و Node قرار دارد. Allp برای این منابع یک سطح فرمان واحد می سازد، اما Native Package Managerها را مخفی یا جایگزین نمی کند.

اصل های پروژه:

- Package Managerهای Native منبع حقیقت باقی می مانند.
- قبل از هر عملیات تغییردهنده، دستور Native دقیق نمایش داده می شود.
- اجرای مخفی با Shell pipeline انجام نمی شود.
- وقتی چند Source معنی دار وجود دارد، انتخاب Source صریح است.
- Backendها بر اساس Capability کار می کنند، نه حدس زدن رفتار.
- مدیریت privilege متمرکز است و فقط برای Child Process اعمال می شود.

## سیستم ها و Backendها

Allp برای لینوکس و محیط های Linux-like طراحی شده است. Homebrew در این نسخه بیشتر با نگاه Linuxbrew اعتبارسنجی شده و macOS هنوز Experimental است.

| Source | وضعیت | Search | Install | Remove | Update | Upgrade | List | Info |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| APT | Stable alpha | بله | بله | بله | بله | بله | بله | بله |
| Pacman | Stable alpha | بله | بله | بله | خیر | بله | بله | بله |
| DNF / DNF5 | Stable alpha | بله | بله | بله | بله | بله | بله | بله |
| Flatpak | Stable alpha | بله | بله | بله | بله | بله | بله | بله |
| Snap | Stable alpha | بله | بله | بله | بله | بله | بله | بله |
| Zypper، APK، XBPS، Portage، eopkg، swupd | Experimental | بله | ترکیبی | ترکیبی | ترکیبی | ترکیبی | ترکیبی | ترکیبی |
| Homebrew / Linuxbrew | Experimental | بله | بله | بله | بله | بله | بله | بله |
| Python: PyPI با pip، pipx و uv | Experimental | بله | بله | بله | بله | بله | بله | بله |
| Node: npm registry با npm، pnpm و Yarn | Experimental | بله | بله | بله | بله | بله | بله | بله |

جزئیات بیشتر در [docs/CAPABILITY_MATRIX.md](docs/CAPABILITY_MATRIX.md) آمده است.

## نصب و ساخت

ساخت از سورس:

```bash
git clone https://github.com/Aliazadi-1776/allp.git
cd allp
cargo build --release
./target/release/allp --version
```

نصب global باینری release:

```bash
make install
allp --version
allp update && allp upgrade
```

`make install` باینری release را می سازد و آن را به
`/usr/local/bin/allp` نصب می کند. برای همین کپی فایل از `sudo install` استفاده
می شود. برای نصب user-local بدون sudo:

```bash
make install-user
```

نیازمندی ها:

- Rust 1.74 یا جدیدتر
- Cargo
- Package Managerهای Native که می خواهید Allp آن ها را پیدا کند
- `sudo` فقط برای Child Processهایی که واقعا Root لازم دارند

استفاده از Binary منتشرشده:

```bash
allp detect
allp search git
```

## شروع سریع

```bash
allp detect
allp search git
allp install git
allp install git --dry-run
allp install pycharm
allp update
allp upgrade
allp update --scope dev
allp search git --json
```

برای انتخاب دقیق Backend از `--from` استفاده کنید:

```bash
allp install git --from apt --dry-run
allp install pycharm --from snap --dry-run
allp install black --from pipx --dry-run
allp install typescript --from pnpm --dry-run
```

## Search و انتخاب تعاملی

اگر برای `search` یا `install` گزینه های `--from` و `--scope` داده نشود، Allp در Terminal تعاملی یکی از سه Scope را می پرسد:

- `apps`: Packageهای سیستم، Universal applicationها و Homebrew
- `dev`: اکوسیستم های Python و Node
- `all`: همه Sourceهای قابل استفاده

نتیجه ها با سه برچسب نمایش داده می شوند: `Exact`، `Related` و `Fuzzy`. Matchهای Exact همیشه نمایش داده می شوند، Related برای هر Backend محدود است، و Fuzzy فقط با `--all` دیده می شود.

در انتخاب های بزرگ، شماره ها ثابت می مانند. Space صفحه بعد، `b` صفحه قبل، `/` فیلتر، عدد انتخاب مستقیم، Enter انتخاب نتیجه Highlight شده یا اولین نتیجه قابل مشاهده، و `q` یا Escape لغو است.

## رفتار sudo و Root

روش پیشنهادی:

```bash
allp update
```

Allp معمولا باید با کاربر عادی اجرا شود. اگر یک Child Command دسترسی Root بخواهد، Allp بعد از نمایش Plan و گرفتن تایید، فقط همان Child را با `sudo --` اجرا می کند. Dry run هیچ وقت sudo را اجرا نمی کند.

اگر عمدا اجرا کنید:

```bash
sudo allp update
```

Allp دوباره sudo اضافه نمی کند. عملیات Root مستقیم اجرا می شوند و عملیات user-scoped مثل Homebrew، Python، Node و Flatpak-user در صورت وجود `SUDO_USER` با کاربر اصلی اجرا می شوند.

گزینه `--yes` فقط تایید نهایی خود Allp را رد می کند. این گزینه هیچ وقت فلگ هایی مثل `-y` یا `--assumeyes` را به ابزار Native اضافه نمی کند.

## Snap، validation و classic confinement

در نسخه 0.3.3 نتیجه خام `snap find` دیگر مستقیم به Plan نصب تبدیل نمی شود. بعد از انتخاب نتیجه Snap، Allp دستور `snap info <candidate>` را اجرا می کند و این موارد را resolve می کند:

- نام canonical package و عنوان نمایشی؛
- publisher و وضعیت verification؛
- confinement؛
- معماری های قابل استفاده؛
- track و channel؛
- وجود stable channel؛
- وضعیت نصب با `snap list <canonical-name>`.

اگر metadata بگوید confinement از نوع classic است، Plan شامل `--classic` می شود:

```bash
allp install pycharm --from snap --dry-run
# native plan:
snap install pycharm --classic
```

برای Snapهای strict، فلگ `--classic` اضافه نمی شود. اگر stable channel وجود نداشته باشد، یا چند stable track بدون default امن وجود داشته باشد، Allp نصب را متوقف می کند و silent choice انجام نمی دهد. انتخاب گر تعاملی channel هنوز از محدودیت های Alpha است.

## Python و Node

در Python، Source برابر PyPI است و pip، pipx و uv نقش Installer دارند. در Node، Source برابر npm registry است و npm، pnpm و Yarn نقش Installer دارند. صرفا مشابه بودن نام، یک package رجیستری را official نمی کند و Fuzzy matchهای Python/Node به صورت خودکار نصب نمی شوند.

```bash
allp search openai --from python
allp install black --from pipx --dry-run
allp search typescript --from node
allp install typescript --from pnpm --dry-run
allp update --scope dev --target all --dry-run
```

## Dry Run و JSON

Dry run همچنان discovery، search، انتخاب، validation metadata و ساخت execution plan را انجام می دهد. فقط اجرای دستور Native تغییردهنده را رد می کند.

```bash
allp install git --dry-run
allp install pycharm --from snap --dry-run
allp update --dry-run
```

نمونه JSON:

```bash
allp detect --json
allp search git --json
allp list --json
allp info git --json
allp update --dry-run --json
```

خروجی انسانی با JSON stdout مخلوط نمی شود.

## Makefile

Makefile ریشه پروژه workflowهای توسعه، نصب و release محلی را با دستورهای شفاف
اجرا می کند:

```bash
make help
make fmt
make fmt-check
make check
make clippy
make test
make architecture
make build
make release
make quality
make run ARGS="search git"
make version
make git-status
make docs-check
make install
make reinstall
make uninstall
make install-user
make install-check
```

هدف های `make install`، `make reinstall` و `make uninstall` فقط برای مدیریت
`/usr/local/bin/allp` از sudo استفاده می کنند. این هدف ها Package بومی نصب
نمی کنند، عملیات package-manager را اجرا نمی کنند، commit/push/tag/publish
انجام نمی دهند، و failureها را مخفی نمی کنند.

## workflow انتشار محلی

workflow انتشار فقط محلی است. چیزی push نمی شود، GitHub Release ساخته نمی شود،
و assetی upload نمی شود.

یک بار در هر clone:

```bash
make hooks-install
```

آماده سازی نسخه بعدی به صورت صریح:

```bash
make release-prepare BUMP=patch
# یا:
make release-prepare VERSION=0.3.4
```

`release-prepare` نسخه package، فایل Cargo.lock از مسیر Cargo، CHANGELOG،
اشاره های نسخه در READMEها، و draft قابل track مثل
`release/RELEASE_NOTES_v0.3.4.md` را به روز می کند و بعد `make quality` را
اجرا می کند. فقط اگر quality gate موفق باشد marker محلی و ignored نوشته می شود.

فایل های آماده شده را مثل همیشه commit کنید، مثلا از VS Code Source Control:

```text
release: Allp v0.3.4
```

فقط commitی که subject آن با `release:` شروع شود و با marker آماده شده همخوان
باشد finalize می شود. hook بعد از commit این خروجی های محلی را می سازد:

- tag محلی annotated با نام `v0.3.4`
- `dist/allp-v0.3.4-source.tar.gz`
- `dist/allp-v0.3.4-source.tar.gz.sha256`
- `dist/RELEASE_NOTES_v0.3.4.md`

آرشیو سورس از همان tag commit شده با `git archive` ساخته می شود. commitهای
معمولی مثل `fix: improve Snap parsing` نسخه را تغییر نمی دهند، tag نمی سازند،
و خروجی `dist/` تولید نمی کنند. برای بررسی وضعیت از `make release-status` و
برای تست automation در repositoryهای موقت از `make release-workflow-test`
استفاده کنید. نمونه taskهای VS Code در `contrib/vscode/tasks.json` قرار دارد،
چون `.vscode/` حالت editor-local دارد و ignored است.

## عیب یابی

| مشکل | راهنمایی |
|---|---|
| قفل APT | صبر کنید Package Manager فعلی تمام شود. Lock fileهای dpkg را حذف نکنید. |
| مشکل DNF/RPM database | Permission یا سلامت rpmdb را بررسی و اصلاح کنید. |
| نبودن pip، pipx یا uv | `allp detect --verbose` را اجرا کنید و ابزار مورد نیاز را آگاهانه نصب یا تنظیم کنید. |
| Permission برای npm global | prefix مربوط به npm را user-owned کنید یا از Node manager کاربری استفاده کنید؛ Allp برای npm global sudo اضافه نمی کند. |
| Scope در Flatpak | با `allp list --from flatpak` نصب user/system را مشخص کنید. |
| خطای metadata در Snap | `snap info <name>` و `allp search <name> --from snap --all` را اجرا کنید؛ نتیجه stale قبل از نصب block می شود. |
| Snap classic | Plan معتبر Allp بعد از `snap info` در صورت نیاز `--classic` را اضافه می کند. |

## مدل امنیتی

Allp دستورها را به صورت executable path و argument vector نگه می دارد و Package Managerها را از طریق `sh -c` اجرا نمی کند. خروجی ابزارهای Native داده محسوب می شود، نه کد قابل اعتماد. Dry run installerها را اجرا نمی کند. Allp sudo password ذخیره نمی کند، telemetry ندارد، و confirmation flagهای Native اضافه نمی کند.

برای گزارش مشکل امنیتی [SECURITY.md](SECURITY.md) را ببینید.

## معماری

```text
CLI -> discovery -> operation -> backend parser/planner -> renderer -> process runner
```

Backendها syntax و parser مخصوص ابزار Native را نگه می دارند. Operationهای عمومی انتخاب، confirmation و plan را هماهنگ می کنند. Runner اجرای مستقیم process، streaming خروجی، sudo و de-escalation به کاربر اصلی را مدیریت می کند.

برای جزئیات بیشتر: [ARCHITECTURE.md](ARCHITECTURE.md)، [docs/BACKEND_CONTRACT.md](docs/BACKEND_CONTRACT.md)، [docs/PRIVILEGE_MODEL.md](docs/PRIVILEGE_MODEL.md).

## توسعه

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
bash scripts/check-architecture.sh
cargo build --release
make quality
```

برای رفتار Package Managerها از fake executable و fixture استفاده کنید. تست ها نباید عملیات destructive واقعی روی Package Managerها انجام دهند.

## مشارکت

Parser و flagهای مخصوص هر Backend باید داخل همان Backend بمانند. برای تغییر parser fixture اضافه کنید. قرارداد CLI، JSON، privilege، dry-run و UI ترمینال را حفظ کنید. [CONTRIBUTING.md](CONTRIBUTING.md) را ببینید.

## Roadmap

کارهای نزدیک شامل validation روی distroهای واقعی، fixtureهای بیشتر، doctor/diagnostics، و UX امن تر برای انتخاب channel در Snap است. اکوسیستم هایی مثل Cargo، Composer، Go، RubyGems، Maven/Gradle و حالت های GUI/TUI در نسخه 0.3.3 پیاده سازی نشده اند.

[ROADMAP.md](ROADMAP.md) و [TODO.md](TODO.md) را ببینید.

## Changelog

نسخه 0.3.3 برنامه ریزی نصب Snap، hygiene مخزن، مستندات انتشار و Makefile را پایدارتر می کند. جزئیات در [CHANGELOG.md](CHANGELOG.md) است.

## محدودیت های شناخته شده

- Allp هنوز Public Alpha است و audit امنیتی کامل نشده است.
- انتخاب چند track/channel در Snap محافظه کارانه است و ممکن است به دستور Native `snap` نیاز داشته باشد.
- Backendهای Experimental باید روی سیستم های واقعی بیشتری اعتبارسنجی شوند.
- سیاست های پروژه ای Python و Node عمدا محتاطانه هستند.
- signal forwarding و trusted-path validation عمیق تر هنوز کار آینده است.

## مجوز

MIT. فایل [LICENSE](LICENSE) را ببینید.
