<div dir="rtl" align="right">

# نگهداری و Self-update

[← خانه ویکی](Home.fa.md) · [English](Maintenance-and-Self-Update.md)

Allp پیش از اجرای Batch نگهداری، Plan تمام Backendهای انتخابی را می‌سازد. Refresh
شدن Metadata در APT پیش‌نیاز Upgrade همان Backend است؛ شکست آن Upgrade را عقب
می‌اندازد، مگر کاربر صریحاً Metadata قدیمی را بپذیرد.

```bash
allp update --dry-run
allp upgrade --dry-run
allp upgrade --allow-stale-metadata  # فقط بازیابی
```

معنا را Backend تعیین می‌کند: APT/DNF Metadata را تازه می‌کنند، Pacman Sync و
Upgrade را ترکیب می‌کند، Flatpak/Snap اپ‌های نصب‌شده را Refresh می‌کنند، Homebrew
Metadata و Package Upgrade را جدا می‌کند و rpm-ostree تغییر تراکنشی Stage می‌کند.

## کنترل‌های Self-update

```bash
allp self-update --check-only
allp self-update --update-channel stable
allp self-update --update-channel continuous
allp self-update --update-channel prerelease
allp update --skip-self-update
allp update --self-only
allp update --offline
```

نصب تازه Stable است و انتخاب صریح Channel ذخیره می‌شود. Offline نه GitHub و نه
Source پکیج را صدا می‌زند. Build محلی جدیدتر Downgrade نمی‌شود و `LocalAhead`
گزارش می‌گردد.

Asset رسمی با OS، معماری، libc و Target از Manifest و شواهد SHA-256 انتخاب می‌شود.
Backup تا تأیید Binary جدید حفظ می‌شود. جایگزینی موفق در `allp update` فقط یک بار
برنامه جدید را اجرا و نگهداری Backendها را بدون Loop ادامه می‌دهد.


</div>
