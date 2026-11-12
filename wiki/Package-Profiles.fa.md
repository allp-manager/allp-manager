<div dir="rtl" align="right">

# پروفایل پکیج‌ها

[← خانه ویکی](Home.fa.md) · [English](Package-Profiles.md)

Profile یک Inventory نسخه‌دار و Experimental در قالب TOML است که هم Package ID
و هم Backend مالک را حفظ می‌کند.

```bash
allp profile save workstation
allp profile verify workstation
allp profile list
allp profile show workstation
allp profile export workstation workstation.toml
allp profile import workstation.toml --name laptop
allp profile install laptop --dry-run
allp profile install laptop
```

```toml
version = 1
name = "developer-tools"

[[packages]]
backend = "apt"
package = "git"

[[packages]]
backend = "rust"
package = "ripgrep"
version = "14.1.1"
```

Version فقط Metadata مشاهده‌شده است و Pin نیست. تبدیل صریح بین Debian/Ubuntu،
Arch و Fedora با `allp profile convert workstation --to arch` انجام می‌شود.
پروفایل مبدأ حفظ می‌شود و هر بسته در گزارش JSON وضعیت مستقل دارد. گزینهٔ
`--verify` موجودبودن شناسهٔ دقیق را در مقصد بررسی می‌کند و `--map FILE` برای
نگاشت‌های تأییدشدهٔ خودتان است. موارد نامشخص در گزارش می‌مانند.
[راهنمای مهاجرت](../docs/PROFILE_MIGRATION.md) را ببینید.
پیش از اجرا همه Backendها یک‌جا بررسی می‌شوند تا نبود یک Backend بعد
از تغییرهای قبلی Host را نیمه‌کاره نگذارد؛ سپس مسیر عادی Search، Plan و تأیید
برای هر Package اجرا می‌شود.

برای برنامه‌های Snap/Flatpak، نصب پیش‌نیازها پیش از برنامه‌ها پیشنهاد می‌شود.
در حالت غیرتعاملی از `--yes --allow-bootstrap` استفاده کنید؛ برای ردکردن این
برنامه‌ها گزینهٔ `profile install NAME --without snap,flatpak` وجود دارد.

Inventory سیستمی ممکن است Dependencyها را هم داشته باشد؛ فایل Export را قبل
از انتقال بازبینی کنید. Import فیلد ناشناخته، Entry تکراری، نام ناامن، نسخه
پشتیبانی‌نشده و اندازه غیرعادی را رد می‌کند. Writeها Atomic و دارای Rollback هستند.

در Linux معمولاً فایل‌ها در `~/.config/allp/profiles/` هستند؛ مسیر واقعی را با
`allp doctor` ببینید.


</div>
