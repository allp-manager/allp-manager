# عیب‌یابی

[← خانه ویکی](Home.fa.md) · [English](Troubleshooting.md)

با شواهد Read-only شروع کنید:

```bash
allp --version --verbose
allp doctor
allp detect --verbose
allp detect --json >allp-detect.json
```

| نشانه | واکنش امن |
|---|---|
| Backend پیدا نمی‌شود | Executable بومی و `detect --verbose` را ببینید؛ Bootstrap را فقط بعد از Plan جداگانه اجرا کنید. |
| Backend یا Lock مشغول است | منتظر Process مالک بمانید؛ فایل Lock dpkg/rpm را حذف نکنید. |
| Search ناقص است | Issues را بخوانید؛ Parser ناشناخته به معنی «بدون نتیجه» نیست. |
| چند نتیجه در Non-interactive | Package ID دقیق و `--from <backend>` بدهید. |
| Flatpak بدون Remote | `doctor flatpak` و Plan صریح Flathub کاربری را بررسی کنید. |
| Resolve دقیق Snap شکست می‌خورد | `doctor snap`؛ REST not-found قطعی است ولی خطای Transport ممکن است CLI Fallback بدهد. |
| خطای Permission در npm | مالکیت Prefix یا Node Manager کاربری را اصلاح کنید؛ npm Global را با sudo اجرا نکنید. |
| Cargo Upgrade موجود نیست | `cargo-update` را آگاهانه نصب کنید یا Binary crate را دستی مدیریت کنید. |
| تغییر Host در Bazzite | Flatpak، Homebrew یا Container را ترجیح دهید؛ Layering و نیاز Reboot را بررسی کنید. |
| Self-update در دسترس نیست | `allp self-update --check-only -v`؛ خطا Binary فعلی را دست‌نخورده می‌گذارد. |
| Binary قدیمی اجرا می‌شود | `command -v allp` و `make install-check`؛ سپس `hash -r` یا `rehash`. |
| Live UI مناسب نیست | `--no-tui`؛ JSON، Redirect و Non-interactive خودکار Fallback دارند. |

بعد از شکست Metadata در APT علت اصلی را برطرف کنید. `--allow-stale-metadata`
فقط راه بازیابی صریح است، نه رفتار معمول.

در Bug Report فرمان دقیق، نسخه توزیع و ابزار، انتظار، خروجی Native و نسخه پاک‌شده
`allp detect --json` را بفرستید. Credential یا جزئیات آسیب‌پذیری را عمومی نکنید.
