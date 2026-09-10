<div dir="rtl" align="right">

# پیکربندی و داده‌ها

[← خانه ویکی](Home.fa.md) · [English](Configuration-and-Data.md)

Allp از قرارداد Data Directory هر Platform پیروی می‌کند. در Linux مسیرها معمولاً
زیر `~/.config/allp`، `~/.local/state/allp` و `~/.cache/allp` هستند. در Script
مسیر را حدس نزنید؛ `allp doctor` مقدار واقعی Environment را نشان می‌دهد.

| داده | کاربرد | Credential؟ |
|---|---|---|
| Config | Profile و تنظیم صریح ابزار | خیر |
| State | Channel/Provenance آپدیت، ETag و Locator معتبر | خیر |
| Cache | Stage محدود Self-update و فایل موقت | خیر |

Profileها در `profiles/*.toml` زیر Config هستند و State به‌شکل Atomic نوشته
می‌شود. مسیر ذخیره‌شده Homebrew پیش از استفاده دوباره اعتبارسنجی می‌شود.

`allp doctor` مسیر واقعی، مالکیت/Writable بودن Executable، کاربر فعلی و اصلی و
Release Target را بدون چاپ Token یا Environment نامرتبط گزارش می‌کند. متغیرهای
`ALLP_TEST_*` و متغیرهای ادامه داخلی API عمومی تنظیمات نیستند؛ از Flagهای مستند
و Profile استفاده کنید.


</div>
