# معماری

[← خانه ویکی](Home.fa.md) · [English](Architecture.md)

Allp یک برنامه Rust 2021 بر پایه مدل‌های Domain، Capabilityهای Backend و
Execution Planهای تغییرناپذیر است.

```text
CLI
 └─ App bootstrap
    ├─ PlatformContext + RuntimePrivilegeContext
    ├─ CapabilityRegistry + RequirementSet
    ├─ Detector → DetectedBackendSet
    └─ Operation → Query/Plan → Renderer → ProcessRunner

Self-update
 └─ منبع GitHub ثابت → Manifest → اعتبارسنجی Stage
    → جایگزینی مخصوص Platform → بررسی/Rollback → ادامه محافظت‌شده
```

## مسئولیت ماژول‌ها

| ماژول | مسئولیت |
|---|---|
| `domain` | مدل‌های خالص پکیج، گزارش، Capability، خطا و اجرا |
| `platform` | OS، خانواده توزیع، معماری/libc، WSL/Container، کاربر و مسیر داده |
| `capabilities`, `requirements` | Resolve ابزار و پیش‌نیاز ساختاریافته |
| `discovery` | کشف تازه Backend و Stateهای صریح آمادگی |
| `backends` | Argv Native، Parser، Capability و ساخت Plan |
| `operations` | Use case مستقل از منبع و جریان انتخاب |
| `execution` | اجرای مستقیم Process، Timeout، Stream و مرز Privilege |
| `cli` | آرگومان Clap، Prompt، JSON، Pager و UI نگهداری |
| `identity` | ارتباط نرم‌افزار میان Backendها بدون انتخاب خودکار منبع |
| `profiles` | ذخیره Atomic و اعتبارسنجی‌شده TOML |
| `self_update`, `release`, `state` | کشف Release معتبر، تأیید و جایگزینی |

## قانون‌های تغییرناپذیر

1. Discovery در هر اجرا تازه است.
2. Operation عمومی با Capability کار می‌کند، نه Backend ID ثابت.
3. Flag و Parser بومی داخل ماژول Backend می‌ماند.
4. Backend تغییردهنده Plan برمی‌گرداند و Process اجرا نمی‌کند.
5. Runner از `std::process::Command` استفاده می‌کند، نه `sh -c`.
6. چند منبع معنادار به تصمیم کاربر نیاز دارند.
7. خروجی ناشناخته Parser هرگز «بدون نتیجه» فرض نمی‌شود.

## جریان Search

Backendها با Concurrency محدود Query می‌شوند. نتیجه به Exact، Related و Fuzzy
نرمال می‌شود. Exact همیشه دیده می‌شود، Related به‌شکل Round-robin میان Backendها
انتخاب می‌شود و Fuzzy به `--all` نیاز دارد. رابطه تأییدشده، رابطه احتمالی و صرفاً
هم‌نام جدا نمایش داده می‌شوند.

## افزودن Backend

Backend باید قرارداد `src/backends/contract.rs` را پیاده کند، Requirement و
Capability را اعلام کند، Parser/Plan خود را نگه دارد، یک بار در Catalog ثبت شود
و Fixture و Test داشته باشد.

```bash
make quality
bash scripts/check-architecture.sh
```

قواعد مرجع در [ARCHITECTURE.md](../ARCHITECTURE.md) و
[ADDING_BACKEND.md](../docs/ADDING_BACKEND.md) هستند.
