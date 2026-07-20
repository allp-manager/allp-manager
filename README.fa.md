# Allp

[English](README.md) | [فارسی](README.fa.md)

> یک CLI شفاف برای Package Managerهایی که همین حالا روی سیستم شما نصب هستند.

Allp Package Manager تازه ای نیست. هسته runtime آن cross-platform است و Backendهای package بیشتر Linux-first هستند. Allp ابزارهایی مثل APT، Pacman، DNF، Flatpak، Snap، Homebrew/Linuxbrew، Python و Node را کشف می کند و قبل از هر تغییر، دستور Native یا درخواست API محلی دقیق را نشان می دهد.

نسخه فعلی: **0.3.5**
سطح بلوغ: **Public Alpha**

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

لایه platform توزیع و خانواده Linux، macOS، Windows، WSL، container، معماری، libc، کاربرها، مالکیت executable و مسیرهای داده را تشخیص می دهد. عملیات package در Linux بالغ تر است. Homebrew روی macOS هنوز Experimental است؛ Windows فعلا compilation، diagnostics، انتخاب release target و self-replacement به روش deferred را پوشش می دهد و Snap/Flatpak لینوکسی را advertise نمی کند.

| Source | وضعیت | Search | Install | Remove | Update | Upgrade | List | Info |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| APT | Stable alpha | بله | بله | بله | بله | بله | بله | بله |
| Pacman | Stable alpha | بله | بله | بله | بله | بله | بله | بله |
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

## Snap، discovery و exact resolution

وقتی `/run/snapd.socket` قابل دسترس باشد، Snap ابتدا از REST API محلی snapd استفاده می کند. discovery گسترده و exact resolution دو درخواست جدا هستند:

```text
GET /v2/find?q=<encoded-query>&scope=wide
GET /v2/find?name=<encoded-canonical-name>
```

نتیجه discovery هیچ وقت مستقیم Plan نصب نیست. بعد از انتخاب، exact resolution این داده ها را بررسی می کند:

- نام canonical package و عنوان نمایشی؛
- publisher و وضعیت verification؛
- confinement؛
- معماری های قابل استفاده؛
- track و channel؛
- وجود stable channel؛
- وضعیت نصب.

پاسخ معتبر `404` با `kind: snap-not-found` یعنی package unavailable است. این transport failure نیست و باعث اجرای fallback یعنی `snap info` نمی شود. Allp قبل از sudo یا install متوقف می شود. گزینه `Try another installer` نتیجه های قبلی را دور می ریزد، Snap را exclude می کند و Backendهای دیگر را واقعا دوباره اجرا می کند.

CLI fallback فقط وقتی مجاز است که socket وجود نداشته باشد یا permission/connect مشکل داشته باشد، endpoint پشتیبانی نشود، یا پاسخ snapd قابل شناسایی نباشد. دلیل دقیق fallback در diagnostics باقی می ماند. معیار موفقیت CLI همان exit status است؛ warning روی stderr با exit code صفر failure محسوب نمی شود.

نصب REST با `POST /v2/snaps/<name>` انجام می شود و برای classic فقط همان موقع `"classic": true` می فرستد. سپس `/v2/changes/<id>` تا وضعیت نهایی poll می شود. در fallback CLI، metadata کلاسیک فلگ `--classic` را اضافه می کند:

```bash
allp install pycharm --from snap --dry-run
# در حالت CLI fallback:
snap install pycharm --classic
```

برای Snapهای strict، فلگ یا فیلد classic اضافه نمی شود. اگر stable channel وجود نداشته باشد، یا چند stable track بدون default امن وجود داشته باشد، Allp silent choice انجام نمی دهد.

## Flatpak و prerequisiteها

Flatpak چهار حالت جدا دارد: نصب نیست، نصب است ولی remote ندارد، remote دارد، یا Backend error. remoteها با این خروجی machine-readable خوانده می شوند:

```bash
flatpak remotes --columns=name,title,url,filter,options
```

نبود remote یعنی catalog قابل جستجو وجود ندارد، نه این که package match نشده است. Allp می تواند Plan جداگانه user-scoped برای Flathub نشان دهد، اما هیچ وقت آن را خودکار اضافه نمی کند. `--yes` به تنهایی اجازه bootstrap executable، service یا remote را نمی دهد؛ حالت unattended به هر دو `--yes --allow-bootstrap` نیاز دارد و Plan دقیق قبل از اجرا چاپ می شود.

Providerهای ساختاریافته APT، DNF، Pacman، Zypper و APK می توانند در mappingهای شناخته شده نصب Flatpak یا Snap را Plan کنند. بعد از تایید و اجرا، capability و Backend دوباره detect می شوند و فقط پس از verification عملیات اصلی ادامه پیدا می کند. نتیجه Flatpak، application ID، branch، remote، version، name و description را نگه می دارد و نصب با remote و application ID انجام می شود.

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

## Update، Self-Update و Doctor

`allp update` در حالت پیش فرض ابتدا repository قابل اعتماد `Aliazadi-1776/allp` را برای نسخه جدید بررسی می کند و بعد سراغ Backendها می رود. phaseها شامل self-update، refresh platform/capability، planning، confirmation، execution و summary هستند.

```bash
allp doctor
allp self-update --check-only
allp self-update --offline
allp update --check-only
allp update --skip-self-update
allp update --self-only
allp update --offline
allp update --update-channel prerelease
```

channel پیش فرض `stable` است و prerelease فقط صریح انتخاب و persist می شود. Release باید `allp-release-manifest.json` معتبر داشته باشد. نسخه ها با SemVer عددی مقایسه می شوند و asset بر اساس OS، معماری، libc، فرمت executable و target انتخاب می شود.

Download فقط HTTPS، با timeout، redirect و size limit و فقط برای repository، tag و asset دقیق انجام می شود. SHA-256، مسیرهای archive و نسخه binary staged قبل از نصب بررسی می شوند. در Linux/macOS جایگزینی با staging هم فایل سیستم، backup rollback و verification نهایی انجام می شود؛ برای مسیر non-writable فقط helper کوچک elevate می شود. Windows از helper deferred استفاده می کند. re-execution محافظت شده باعث می شود `allp update` فقط یک بار ادامه یابد و loop نسازد. حالت offline با GitHub یا remote sourceها تماس نمی گیرد.

`allp doctor` اطلاعات platform، user، path و ownership/writability باینری، executableهای resolved، Backendها، socket مربوط به Snap، remoteهای Flatpak، update source، release target و مسیرهای cache/state/config را بدون token یا environment خصوصی گزارش می کند.

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
make doctor
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

workflow انتشار صریح است. مرحله آماده سازی محلی چیزی push نمی کند، GitHub
Release نمی سازد و assetی upload نمی کند. GitHub Release فقط وقتی ساخته می شود
که tag نسخه ای مثل `v0.3.5` push شود.

یک بار در هر clone:

```bash
make hooks-install
```

آماده سازی نسخه بعدی به صورت صریح:

```bash
make release-prepare BUMP=patch
# یا:
make release-prepare VERSION=0.3.5
```

`release-prepare` نسخه package، فایل Cargo.lock از مسیر Cargo، CHANGELOG،
اشاره های نسخه در READMEها، title قابل track مثل
`release/RELEASE_TITLE_v0.3.5.txt`، و draft قابل track مثل
`release/RELEASE_NOTES_v0.3.5.md` را به روز می کند و بعد `make quality` را
اجرا می کند. فقط اگر quality gate موفق باشد marker محلی و ignored نوشته می شود.

فایل های آماده شده را مثل همیشه commit کنید، مثلا از VS Code Source Control:

```text
release: Allp v0.3.5
```

فقط commitی که subject آن با `release:` شروع شود و با marker آماده شده همخوان
باشد finalize می شود. hook بعد از commit این خروجی های محلی را می سازد:

- tag محلی annotated با نام `v0.3.5`
- `dist/allp-v0.3.5-source.tar.gz`
- `dist/allp-v0.3.5-source.tar.gz.sha256`
- `dist/RELEASE_NOTES_v0.3.5.md`

آرشیو سورس از همان tag commit شده با `git archive` ساخته می شود. commitهای
معمولی مثل `fix: improve Snap parsing` نسخه را تغییر نمی دهند، tag نمی سازند،
و خروجی `dist/` تولید نمی کنند. برای بررسی وضعیت از `make release-status` و
برای تست automation در repositoryهای موقت از `make release-workflow-test`
استفاده کنید. نمونه taskهای VS Code در `contrib/vscode/tasks.json` قرار دارد،
چون `.vscode/` حالت editor-local دارد و ignored است.

وقتی tag محلی release آماده شد، `make release-push` را به صورت صریح اجرا کنید.
این target commit انتشار، annotated tag، و اشاره tag به همان commit را بررسی
می کند و بعد branch فعلی و tag همخوان را push می کند. workflow GitHub Actions
فقط با tag نسخه ای اجرا می شود و GitHub Release را از title و notes آماده شده
می سازد. Binaryهای Linux x86_64/aarch64، macOS x86_64/aarch64 و Windows
x86_64 build و test می شوند؛ archive و checksum آنها، source archive دقیق tag
و `allp-release-manifest.json` تولید و verify و upload می شوند. Release موجود
هیچ وقت بی صدا overwrite نمی شود.

## عیب یابی

| مشکل | راهنمایی |
|---|---|
| قفل APT | صبر کنید Package Manager فعلی تمام شود. Lock fileهای dpkg را حذف نکنید. |
| مشکل DNF/RPM database | Permission یا سلامت rpmdb را بررسی و اصلاح کنید. |
| نبودن pip، pipx یا uv | `allp detect --verbose` را اجرا کنید و ابزار مورد نیاز را آگاهانه نصب یا تنظیم کنید. |
| Permission برای npm global | prefix مربوط به npm را user-owned کنید یا از Node manager کاربری استفاده کنید؛ Allp برای npm global sudo اضافه نمی کند. |
| Flatpak بدون remote | `allp doctor` را اجرا کنید و فقط در صورت نیاز Plan جداگانه Flathub را تایید کنید. |
| Snap exact unavailable | diagnostics و `allp doctor` را ببینید؛ REST `snap-not-found` معتبر authoritative است. |
| Snap CLI fallback | diagnostics دلیل fallback و argv/stdout/stderr دقیق را نشان می دهد. |
| Self-update unavailable | `allp self-update --check-only -v`؛ target ناسازگار باینری فعلی را تغییر نمی دهد. |

## مدل امنیتی

Allp دستورها را به صورت executable path و argument vector نگه می دارد و Package Managerها را از طریق `sh -c` اجرا نمی کند. خروجی ابزارهای Native داده است، نه کد. Bootstrapها Planهای جدا هستند. Self-update repository خارجی، asset name ناامن، manifest خراب، checksum اشتباه، archive traversal و staged version اشتباه را رد می کند. state شامل credential نیست. Allp sudo password ذخیره نمی کند، telemetry ندارد، و confirmation flagهای Native اضافه نمی کند.

برای گزارش مشکل امنیتی [SECURITY.md](SECURITY.md) را ببینید.

## معماری

```text
CLI -> platform/capabilities -> requirements -> discovery -> operation -> backend -> execution
                                      |             |             |
                                  bootstrap     alternatives   diagnostics
CLI -> self_update -> release manifest -> verified replacement -> guarded re-execution
```

Backendها syntax، transport REST/CLI و parser خود را نگه می دارند. Operationهای عمومی capability، alternative، انتخاب، confirmation و Plan immutable را هماهنگ می کنند. Providerهای bootstrap از Backend نیازمند جدا هستند. Runner اجرای مستقیم process، streaming خروجی، sudo و de-escalation را مدیریت می کند.

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

کارهای نزدیک شامل validation روی distroهای واقعی، fixtureهای بیشتر، انتخاب گر تعاملی channel در Snap و تست عمیق تر signal/trusted-path است. اکوسیستم هایی مثل Cargo، Composer، Go، RubyGems، Maven/Gradle و GUI/TUI در نسخه 0.3.5 پیاده سازی نشده اند.

[ROADMAP.md](ROADMAP.md) و [TODO.md](TODO.md) را ببینید.

## Changelog

نسخه `0.3.5` برای Pacman در `allp update` یک Plan صریح با دستور `pacman -Sy` و یادداشت policy درباره پرهیز از partial upgrade اضافه می کند. جزئیات در [CHANGELOG.md](CHANGELOG.md) است.

## محدودیت های شناخته شده

- Allp هنوز Public Alpha است و audit امنیتی کامل نشده است.
- انتخاب چند track/channel در Snap محافظه کارانه است و ممکن است به دستور Native `snap` نیاز داشته باشد.
- Release قدیمی GitHub بدون manifest و binary سازگار نمی تواند خودکار self-update شود.
- Backendهای Experimental باید روی سیستم های واقعی بیشتری اعتبارسنجی شوند.
- سیاست های پروژه ای Python و Node عمدا محتاطانه هستند.
- signal forwarding و trusted-path validation عمیق تر هنوز کار آینده است.

## مجوز

MIT. فایل [LICENSE](LICENSE) را ببینید.


### 💚 Donate

https://daramet.com/wrench
