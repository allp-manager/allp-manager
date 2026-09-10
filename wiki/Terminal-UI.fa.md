<div dir="rtl" align="right">

# رابط Terminal و تعامل

[← خانه ویکی](Home.fa.md) · [English](Terminal-UI.md)

اجرای واقعی و Interactive فرمان‌های `update` و `upgrade` می‌تواند Footer زنده
شبیه APT داشته باشد. Stdout/Stderr Native در Scrollback عادی می‌ماند و Footer
درصد، Backend/Action فعال، زمان و وضعیت Queue را نشان می‌دهد. Renderer فقط
Observer است و Argv یا Privilege برنامه‌ریزی‌شده را تغییر نمی‌دهد.

```bash
allp update
allp upgrade
allp update --no-tui
allp update --no-color
```

در JSON، Dry Run، Redirect/Non-TTY، `TERM=dumb` و `--no-interactive` نمای زنده
خاموش است. Control Sequence غیرقابل‌اعتماد Native پاک‌سازی و Footer متناسب با
عرض Terminal کوتاه می‌شود.

اگر Child نیازمند Root باشد، احراز sudo قبل از Renderer تمام می‌شود. انقضای
Credential، Footer را تعلیق و بیرون آن دوباره اعتبارسنجی می‌کند؛ شکست به‌عنوان
Operation مسدود طبقه‌بندی می‌شود.


</div>
