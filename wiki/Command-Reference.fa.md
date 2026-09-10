# مرجع فرمان‌ها

[← خانه ویکی](Home.fa.md) · [English](Command-Reference.md)

ساختار اصلی `allp <command> [arguments] [options]` است. برای جزئیات بیشتر `-v`
را یک یا چند بار اضافه کنید. گزینه‌های عمومی خروجی `--json`، `--no-color` و `--no-tui` هستند.

| فرمان | کاربرد | مثال |
|---|---|---|
| `detect` | وضعیت همه Backendهای داخلی | `allp detect --json` |
| `search <query>` | جست‌وجو در منابع مجاز | `allp search firefox --scope apps` |
| `install <package>` | Resolve، Plan، تأیید و نصب | `allp install git --from apt --dry-run` |
| `remove <package>` | پیدا کردن نسخه نصب‌شده و حذف | `allp remove git --from apt` |
| `update` | تازه‌کردن Metadata و در صورت لزوم خود Allp | `allp update --dry-run` |
| `upgrade` | ارتقای نرم‌افزارهای نصب‌شده | `allp upgrade --from flatpak` |
| `list` | فهرست نصب‌شده‌ها بر اساس Backend | `allp list --from apt --filter git` |
| `info <package>` | Metadata نرمال یا Native | `allp info git --full` |
| `doctor [backend]` | عیب‌یابی کل سیستم یا یک Backend | `allp doctor homebrew` |
| `profile` | ذخیره، Import/Export و اجرای Inventory | `allp profile save dev` |
| `self-update` | دریافت Build رسمی و تأییدشده | `allp self-update --check-only` |

## جست‌وجو و انتخاب منبع

```bash
allp search ripgrep --exact
allp search editor --all --limit 50
allp search black --scope dev
allp search pycharm --from snap
```

خروجی عادی شامل نتیجه‌های Exact و تعداد محدودی Related است؛ `--all` نتیجه‌های
Fuzzy را هم نشان می‌دهد. اجرای JSON یا Non-interactive بدون Scope صریح به‌صورت
پیش‌فرض محدوده Apps را جست‌وجو می‌کند.

## عملیات تغییردهنده

```bash
allp install org.mozilla.firefox --from flatpak --dry-run
allp install black --from pipx --no-interactive --yes
allp remove ripgrep --from cargo --dry-run
```

- `--dry-run`: اعتبارسنجی و ساخت Plan بدون اجرا.
- `--no-interactive`: هیچ Selector یا Prompt نمایش داده نشود.
- `--yes`: فقط تأیید نهایی Allp را بعد از حل همه انتخاب‌ها رد می‌کند.
- `--allow-bootstrap`: همراه `--yes` اجازه اجرای پیش‌نیاز جداگانه را در اتوماسیون می‌دهد.

## نگهداری

```bash
allp update --skip-self-update
allp update --self-only
allp update --check-only
allp update --offline
allp update --scope dev --target tools --dry-run
allp upgrade --scope dev --target all --dry-run
allp upgrade --allow-stale-metadata  # فقط بازیابی صریح
```

Targetهای توسعه `project`، `workspace`، `global`، `environment`، `tools` و `all`
هستند. ترکیب پشتیبانی‌نشده گزارش می‌شود و Allp چیزی را حدس نمی‌زند.

## Inventory و اطلاعات

```bash
allp list --from apt --filter openssl --limit 20 --no-pager
allp info firefox --from flatpak
allp info git --full
allp info git --from apt --raw
```

`--raw` خروجی Native Backend و `--full` فیلدهای نرمال بیشتر را نشان می‌دهد.

## Exit Codeهای پایدار

| کد | معنی |
|---:|---|
| 0 | موفق |
| 2 | CLI یا ورودی نامعتبر |
| 3 | پکیج پیدا نشد |
| 4 | انتخاب مبهم یا نیازمند تعامل |
| 5 | Backend پیدا یا پیکربندی نشد |
| 6 | عملیات پشتیبانی نمی‌شود |
| 7 | فرمان Native یا Validation شکست خورد |
| 8 | شکست جزئی چند Backend |
| 9 | Timeout یا لغو |
| 10 | خطای داخلی، Parse یا I/O |
| 11 | Backend مشغول یا Lock در اختیار Process دیگر |

جزئیات دقیق در [قرارداد فرمان‌ها](../docs/COMMANDS.md) قرار دارد.
