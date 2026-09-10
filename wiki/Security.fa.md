# مدل امنیت

[← خانه ویکی](Home.fa.md) · [English](Security.md)

Allp ابزارهایی را هماهنگ می‌کند که ممکن است سیستم‌عامل را تغییر دهند. ویژگی
امنیتی اصلی آن Plan قابل‌مشاهده و مرز اجرای محدود است؛ نه ادعای قابل‌اعتمادبودن
تمام پکیج‌های ثالث.

## مرز فرمان و دسترسی

- Program و Argument جدا هستند و بدون Shell اجرا می‌شوند.
- Package ID که با `-` شروع شود قبل از تغییر رد می‌شود.
- Executable نیازمند Root و Parentهای آن Canonical و از نظر مالکیت/Permission بررسی می‌شوند.
- اجرای عادی با کاربر شروع می‌شود و فقط Child لازم `sudo --` می‌گیرد.
- Maintenance یک بار `sudo -v` می‌گیرد و بعد `sudo -n --` اجرا می‌کند تا Prompt وارد UI نشود.
- Homebrew، Python، Node، Cargo و Flatpak کاربری به Original User تأییدشده برمی‌گردند.
- عملیات User-scoped در Root مستقیم، وقتی کاربر اصلی معلوم نیست، رد می‌شود.

## مدل تأیید

```bash
allp install git --from apt --dry-run  # بدون تغییر و بدون sudo
allp install git --from apt            # Plan → تأیید → اجرا
allp install git --from apt --yes      # فقط ردکردن تأیید نهایی Allp
```

Bootstrap یک تغییر مستقل است. اجرای بدون تعامل آن بعد از نمایش Plan دقیق، به
هر دو گزینه `--yes --allow-bootstrap` نیاز دارد.

## اعتماد به Registry و خروجی

خروجی Package Manager داده غیرقابل‌اعتماد است. اسم‌های Registryهای Python،
Node و Rust رسمی فرض نمی‌شوند و Fuzzy Match خودکار نصب نمی‌شود. Build Script یا
Installer Hook ممکن است در نصب Native واقعی اجرا شود، اما در Dry Run هرگز.

## زنجیره اعتماد Self-update

فقط هویت Repository رسمی Compile‌شده پذیرفته می‌شود. درخواست HTTPS محدود است؛
هویت Manifest، Target، نام آرشیو، Size و SHA-256 بررسی می‌شوند و Path/Link ناامن
رد می‌شود. هویت Build و Byteهای فایل Stage در تمام Helperها دوباره بررسی می‌شوند.
Backup تا Post-check موفق نگه داشته می‌شود. Binary متعلق به dpkg/rpm/Pacman
بازنویسی نمی‌شود و Update به همان منبع Native سپرده می‌شود.

## حریم خصوصی و گزارش

Allp Telemetry، Daemon، ذخیره رمز sudo یا Credential Store ندارد. Injection،
Privilege Escalation، Resolve ناامن، افشای Credential یا JSON گمراه‌کننده را از
Private Security Advisory گزارش کنید، نه Issue عمومی.

نسخه Alpha ممیزی امنیتی نشده است. [SECURITY.md](../SECURITY.md) و
[مدل مرجع امنیت](../docs/SECURITY_MODEL.md) را بخوانید.
