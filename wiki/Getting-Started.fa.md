<div dir="rtl" align="right">

# شروع کار

[← خانه ویکی](Home.fa.md) · [English](Getting-Started.md)

## پیش‌نیازها

- Linux برای عملیات بالغ‌تر؛ Homebrew در macOS هنوز Experimental است.
- حداقل یک Package Manager پشتیبانی‌شده روی سیستم.
- Rust 1.74 یا جدیدتر، فقط برای Build از سورس.
- `sudo` فقط وقتی Child Process انتخاب‌شده واقعاً Root می‌خواهد.

## نصب Release تأییدشده

```bash
curl --fail --location --output install-allp.sh \
  https://github.com/allp-manager/allp-manager/releases/latest/download/install-allp.sh
less install-allp.sh
sh install-allp.sh
```

نصاب SHA-256 و محتوای آرشیو را بررسی می‌کند و برنامه را در `~/.local/bin`
می‌گذارد. برای نسخه یا مقصد دلخواه:

```bash
ALLP_INSTALL_DIR="$HOME/bin" sh install-allp.sh 0.6.2
```

سپس هویت Build را بررسی کنید:

```bash
allp --version
allp --version --verbose
```

## ساخت از سورس

```bash
git clone https://github.com/allp-manager/allp-manager.git
cd allp-manager
cargo build --release
./target/release/allp --version --verbose
```

برای نصب کاربری `make install-user` و برای `/usr/local/bin` فرمان `make install` را اجرا کنید.

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
