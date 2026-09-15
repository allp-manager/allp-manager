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

Termux فعلاً محیط پشتیبانی‌شدهٔ ALLP Manager نیست. دستور `make install` از `sudo` استفاده می‌کند و در `/usr/local/bin` نصب می‌کند؛ این روش با Termux معمولی سازگار نیست. دستور `make install-user` در مرحلهٔ نصب به sudo نیاز ندارد، اما عملیات مدیریت پکیج را با Termux سازگار نمی‌کند: عملیات تغییردهندهٔ APT در ALLP Manager همچنان دسترسی روت می‌خواهند. پشتیبانی بومی Termux به سازگاری جداگانهٔ اجرا، مسیرها و سطح دسترسی نیاز دارد. تا پیاده‌سازی و آزمایش این پشتیبانی، پکیج‌های Termux را با دستور بومی `pkg` مدیریت کنید.

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
