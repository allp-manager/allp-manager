<div dir="rtl" align="right">

# راهنمای عملی استفاده

[← خانه ویکی](Home.fa.md) · [English](Usage-Guide.md)

این راهنما مطابق استفاده واقعی پیش می‌رود: شناخت سیستم، Search، انتخاب منبع
معتبر، دیدن Plan، اجرای تغییر و بررسی نتیجه.

## ۱. سیستم را بشناسید

```bash
allp --version --verbose
allp doctor
allp detect --verbose
```

`doctor` سیستم‌عامل، خانواده توزیع، معماری/libc، Context کاربر و sudo، مالکیت
نصب Allp، مسیرهای داده، Release Target و آمادگی Backendها را توضیح می‌دهد.
`detect` وضعیت و Capability همه Backendها را نشان می‌دهد. ابتدا علت
`FoundButUnavailable` یا `FoundButUnconfigured` را برطرف کنید.

## ۲. در Scope درست جست‌وجو کنید

```bash
allp search firefox --scope apps
allp search black --scope dev
allp search git --scope all
allp search pycharm --from snap
```

- `apps`: پکیج سیستمی، اپ عمومی و Homebrew.
- `dev`: اکوسیستم Python، Node و Rust/Cargo.
- `all`: تمام منابع مجاز.
- `--from`: یک Backend یا Installer دقیق مثل `apt`، `flatpak`، `snap`،
  `homebrew`، `pipx`، `pnpm` یا `cargo`.

نتیجه‌ها Exact، Related و Fuzzy هستند. `--exact` فقط تطبیق دقیق و `--all` نتیجه
Fuzzy ضعیف را هم نشان می‌دهد. هم‌نامی، برابری پکیج‌ها را اثبات نمی‌کند.

## ۳. قبل از نصب بررسی کنید

```bash
allp info firefox
allp info firefox --from flatpak --full
allp info git --from apt --raw
allp install git --from apt --dry-run
```

خروجی عادی `info` خلاصه و Curated است؛ `--full` Metadata نرمال بیشتر و `--raw`
خروجی Native می‌دهد. Dry Run تمام Discovery، Resolve و Plan را انجام می‌دهد،
ولی هیچ تغییری ایجاد نمی‌کند و sudo را اجرا نمی‌کند.

## ۴. نصب یا حذف کنید

```bash
allp install git --from apt
allp install org.mozilla.firefox --from flatpak
allp install black --from pipx
allp install typescript --from pnpm
allp install ripgrep --from cargo

allp remove git --from apt --dry-run
allp remove git --from apt
```

Package ID، Source، Scope، Argv دقیق Native و سطح دسترسی را بخوانید. اگر منبع
مبهم است آن را صریح انتخاب کنید؛ `--yes` برای شما منبع انتخاب نمی‌کند. نصب
پیش‌نیاز یا افزودن Remote یک Plan جداگانه است.

## ۵. نرم‌افزارهای نصب‌شده را ببینید

```bash
allp list
allp list --from apt --filter git
allp list --from flatpak --limit 50 --no-pager
allp list --json
```

Filter قبل از Limit اعمال می‌شود. خروجی بلند با Pager مستقیم نمایش داده می‌شود
و Pipeline مخفی ندارد.

## ۶. سیستم را نگهداری کنید

```bash
allp update --dry-run
allp upgrade --dry-run
allp update
allp upgrade
```

Update معمولاً Metadata را تازه می‌کند؛ Upgrade نرم‌افزار نصب‌شده را تغییر
می‌دهد. Plan کل Batch قبل از تأیید ساخته می‌شود، سپس عملیات ترتیبی اجرا و بعد
از Failure یک Backend ادامه پیدا می‌کند. Exit Code شماره ۸ یعنی شکست جزئی.

```bash
allp update --from apt --skip-self-update
allp upgrade --from flatpak
allp update --scope dev --target tools --dry-run
allp upgrade --scope dev --target global --dry-run
allp update --no-tui
```

## ۷. مجموعه ابزار را بازسازی کنید

```bash
allp profile save workstation
allp profile export workstation workstation.toml
# فایل TOML را در مقصد بازبینی کنید
allp profile import workstation.toml --name new-machine
allp profile install new-machine --dry-run
allp profile install new-machine
```

Profile هویت Backend را حفظ می‌کند. Version فقط مشاهده Inventory است و Pin
نیست؛ Inventory سیستمی ممکن است Dependencyها را هم داشته باشد.

## ۸. در CI یا Script استفاده کنید

```bash
allp search git --from apt --json
allp update --dry-run --json
allp install git --from apt --no-interactive --yes
```

`schema_version` مورد انتظار، `complete` و `issues` را بررسی و Exit Codeها را
مدیریت کنید. Mutation واقعی باید بعد از Dry Run بازبینی‌شده باشد. Bootstrap
بدون تعامل علاوه بر `--yes` به `--allow-bootstrap` نیاز دارد.

## عادت‌های پیشنهادی

1. Allp را با کاربر عادی اجرا کنید، نه `sudo allp` کلی.
2. برای نصب حساس و تکرارپذیر از `--from` استفاده کنید.
3. Backend ناآشنا و Batch نگهداری را Dry Run کنید.
4. Lockهای Package Manager را حذف نکنید.
5. مالک Registry را بررسی کنید و فقط به نام آشنا اعتماد نکنید.


</div>
