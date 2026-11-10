<div dir="rtl" align="right">

# پلتفرم‌ها و نصب

[← خانه ویکی](Home.fa.md) · [English](Platforms-and-Installation.md)

عملیات Package در Linux سطح اصلی محصول است. Homebrew در macOS هنوز Experimental
است. Windows ساخت، Diagnostics، انتخاب Release Target و جایگزینی Deferred و
تأییدشده را دارد، ولی Backendهای Linux-only مثل Snap/Flatpak را اعلام نمی‌کند.

Allp خانواده Debian، Red Hat/Fedora، Arch، SUSE و Alpine و همچنین Bazzite را
به‌عنوان Host مبتنی بر Image می‌شناسد. معماری، libc، وضعیت WSL/Container و
مسیرهای داده Platform را نیز ثبت می‌کند.

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

```bash
command -v allp
allp --version --verbose
make install-check
```

اگر Copy اشتباه Resolve شد، PATH را اصلاح و در bash از `hash -r` یا در zsh از
`rehash` استفاده کنید. Binary متعلق به dpkg/rpm/Pacman توسط همان منبع مدیریت می‌شود.


</div>
