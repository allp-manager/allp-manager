<div dir="rtl" align="right">

# شروع کار

[← خانه ویکی](Home.fa.md) · [English](Getting-Started.md)

## پیش‌نیازها

- Linux برای عملیات بالغ‌تر؛ Homebrew در macOS هنوز Experimental است.
- حداقل یک Package Manager پشتیبانی‌شده روی سیستم.
- Rust 1.74 یا جدیدتر، فقط برای Build از سورس.
- `sudo` فقط وقتی Child Process انتخاب‌شده واقعاً Root می‌خواهد.

**⚠️ اگر Rust و Cargo ندارید، ابتدا آن‌ها را نصب کنید.** Git، Make و کامپایلر و لینکِر C هم لازم‌اند. از [راهنمای رسمی نصب Rust](https://www.rust-lang.org/tools/install) استفاده کنید، ترمینال را دوباره باز کنید و سپس بررسی کنید:

```bash
rustc --version
cargo --version
```

## نصب از سورس

```bash
git clone https://github.com/allp-manager/allp-manager.git
cd allp-manager
make install
```

دستور `make install` برنامه را می‌سازد و با `sudo` در `/usr/local/bin/allp` نصب می‌کند.

## نصب کاربری بدون sudo

برای نصب فقط برای کاربر خودتان، در همان پوشهٔ مخزن به‌جای `make install` این دستورها را اجرا کنید:

```bash
make install-user
export PATH="$HOME/.local/bin:$PATH"
allp --version
```

مقصد نصب `~/.local/bin/allp` است. دستور PATH بالا برای ترمینال فعلی است؛ برای ترمینال‌های بعدی هم این خط را در فایل تنظیمات Shell خود اضافه کنید.

## Termux در اندروید

در Android، Allp را با کاربر معمولی برنامهٔ Termux اجرا کنید. از آرشیو Android برای ARM64 یا x86-64 استفاده کنید، یا با `pkg install rust clang make git` ابزارهای ساخت را بگیرید و در پوشهٔ سورس `make install-termux` بزنید. نصب‌کنندهٔ ریلیز با `sh install-allp.sh` اجرا می‌شود و مسیر پیش‌فرض آن `$PREFIX/bin` است.

مدیر `termux` (با نام `--from pkg` هم قابل انتخاب است) جست‌وجو، فهرست، اطلاعات بسته، نصب، حذف و ارتقا را در محدودهٔ Termux با APT یا Pacman همان محیط انجام می‌دهد. Termux اجرای مدیر بسته با شناسهٔ مؤثر کاربر صفر را رد می‌کند. روی گوشی روت‌شده هم از شل روت خارج شوید و با کاربر عادی Termux ادامه دهید. برای پیش‌نمایش ارتقا `allp upgrade --from termux --dry-run` را بزنید.

## اولین اجرای امن

```bash
allp doctor
allp detect --verbose
allp search git
allp install git --from apt --dry-run
allp install git --from apt
```

قبل از فرمان آخر، Backend، Argv و سطح دسترسی Plan را بخوانید. با `--scope apps`،
`--scope dev` یا `--scope all` محدوده را صریح کنید؛ وقتی منبع مهم است همیشه
`--from <backend>` بدهید.

## تفاوت Update و Upgrade

`update` معمولاً Metadata را تازه می‌کند و `upgrade` نرم‌افزارهای نصب‌شده را
ارتقا می‌دهد. معنای دقیق را هر Backend تعیین می‌کند و Allp آن را قبل از اجرا نشان می‌دهد.

```bash
allp update --dry-run
allp upgrade --dry-run
allp update
allp upgrade
```

در خطای Lock هیچ‌وقت فایل Lock مدیر Native را حذف نکنید؛ منتظر Process مالک
بمانید یا آن را به شکل امن بررسی کنید.


</div>
