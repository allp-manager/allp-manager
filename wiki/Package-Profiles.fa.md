<div dir="rtl" align="right">

# پروفایل پکیج‌ها

[← خانه ویکی](Home.fa.md) · [English](Package-Profiles.md)

Profile یک Inventory نسخه‌دار و Experimental در قالب TOML است که هم Package ID
و هم Backend مالک را حفظ می‌کند.

```bash
allp profile save workstation
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

Version فقط Metadata مشاهده‌شده است و Pin نیست. Allp پکیج APT را به DNF ترجمه
نمی‌کند. پیش از اجرا همه Backendها یک‌جا بررسی می‌شوند تا نبود یک Backend بعد
از تغییرهای قبلی Host را نیمه‌کاره نگذارد؛ سپس مسیر عادی Search، Plan و تأیید
برای هر Package اجرا می‌شود.

Inventory سیستمی ممکن است Dependencyها را هم داشته باشد؛ فایل Export را قبل
از انتقال بازبینی کنید. Import فیلد ناشناخته، Entry تکراری، نام ناامن، نسخه
پشتیبانی‌نشده و اندازه غیرعادی را رد می‌کند. Writeها Atomic و دارای Rollback هستند.

در Linux معمولاً فایل‌ها در `~/.config/allp/profiles/` هستند؛ مسیر واقعی را با
`allp doctor` ببینید.


</div>
