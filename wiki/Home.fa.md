<div dir="rtl" align="right">

# ویکی Allp

**Allp 0.6.2 · آلفای عمومی · [English](Home.md)**

Allp یک هماهنگ‌کننده شفاف برای Package Managerهای نصب‌شده روی سیستم است. یک
رابط فرمان واحد می‌دهد، اما منبع نرم‌افزار، دستور Native و سطح دسترسی لازم را
از کاربر پنهان نمی‌کند.

## نقشه مستندات

| موضوع | راهنما |
|---|---|
| جریان کار روزمره از بررسی اولیه تا اتوماسیون | [راهنمای عملی استفاده](Usage-Guide.fa.md) |
| نصب و اولین اجرای امن | [شروع کار](Getting-Started.fa.md) |
| سیستم‌عامل‌ها و روش‌های نصب | [پلتفرم‌ها و نصب](Platforms-and-Installation.fa.md) |
| تمام فرمان‌ها و گزینه‌های مهم | [مرجع فرمان‌ها](Command-Reference.fa.md) |
| رتبه‌بندی، هویت و انتخاب تعاملی | [جست‌وجو و انتخاب](Search-and-Selection.fa.md) |
| APT، Pacman، DNF، Bazzite، Flatpak، Snap، Homebrew، Python، Node و Cargo | [Backendها](Backends.fa.md) |
| تازه‌سازی، Upgrade و آپدیت تأییدشده Allp | [نگهداری و Self-update](Maintenance-and-Self-Update.fa.md) |
| فهرست‌های قابل‌انتقال TOML | [پروفایل پکیج‌ها](Package-Profiles.fa.md) |
| اسکریپت، CI و خروجی ساختاریافته | [اتوماسیون و JSON](Automation-and-JSON.fa.md) |
| Config، State، Cache و مسیرهای واقعی | [پیکربندی و داده‌ها](Configuration-and-Data.fa.md) |
| Progress زنده و قواعد Fallback | [رابط Terminal](Terminal-UI.fa.md) |
| لایه‌های Runtime و روش توسعه | [معماری](Architecture.fa.md) |
| مرز اعتماد، sudo و Self-update | [امنیت](Security.fa.md) |
| ساخت، تست، انتشار و مشارکت | [توسعه](Development.fa.md) |
| تشخیص خطا و بازیابی | [عیب‌یابی](Troubleshooting.fa.md) |
| جواب کوتاه سؤال‌های رایج | [پرسش‌های پرتکرار](FAQ.fa.md) |

## مدل ذهنی پروژه

```text
درخواست → کشف مدیرهای نصب‌شده → پرس‌وجو از Backendهای توانمند
       → نمایش انتخاب‌ها → ساخت Plan تغییرناپذیر
       → نمایش دستور دقیق → تأیید → اجرای مستقیم
```

Allp Dependency Resolver یا دیتابیس پکیج مستقل نیست. ابزارهای Native همچنان
منبع حقیقت باقی می‌مانند.

## قول امنیتی Allp

- فرمان‌ها به شکل Program و Argv نگه‌داری می‌شوند، نه رشته Shell.
- هر تغییر قبل از اجرا برنامه‌ریزی و نمایش داده می‌شود.
- `--dry-run` هیچ تغییری ایجاد نمی‌کند و sudo را فراخوانی نمی‌کند.
- `--yes` فقط تأیید نهایی Allp را رد می‌کند و Flagهای Native را کورکورانه اضافه نمی‌کند.
- دسترسی Root فقط به Child لازم داده می‌شود؛ ابزارهای User-scoped در Context کاربر اصلی می‌مانند.
- پروژه Telemetry ندارد و Credential ذخیره نمی‌کند.

برای قراردادهای دقیق مهندسی به [معماری](https://github.com/allp-manager/allp-manager/blob/main/ARCHITECTURE.md)،
[رفتار فرمان‌ها](https://github.com/allp-manager/allp-manager/blob/main/docs/COMMANDS.md)، [قرارداد Backend](https://github.com/allp-manager/allp-manager/blob/main/docs/BACKEND_CONTRACT.md)،
[Schema خروجی JSON](https://github.com/allp-manager/allp-manager/blob/main/docs/JSON_SCHEMA.md) و [مدل امنیت](https://github.com/allp-manager/allp-manager/blob/main/docs/SECURITY_MODEL.md) مراجعه کنید.


</div>
