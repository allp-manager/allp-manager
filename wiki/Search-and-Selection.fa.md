<div dir="rtl" align="right">

# جست‌وجو و انتخاب

[← خانه ویکی](Home.fa.md) · [English](Search-and-Selection.md)

Allp Backendهای دارای Capability را با Concurrency محدود جست‌وجو می‌کند. هر
Parser علاوه بر Candidate، Issue ساختاریافته برمی‌گرداند؛ خروجی Native ناشناخته
و غیرخالی `unrecognized_output` است، نه «بدون نتیجه» ساختگی.

## رتبه‌بندی

1. تطبیق Exact با Package ID یا Display Name.
2. نتیجه Related با سقف هر Backend و انتخاب Round-robin.
3. نتیجه Fuzzy که فقط با `--all` دیده می‌شود.

```bash
allp search git --exact
allp search editor --limit 10
allp search editor --all --limit 50
```

Identity میان Backendها فقط اطلاعات می‌دهد: Mapping تأییدشده می‌تواند Group
بسازد، رابطه احتمالی با عدم قطعیت نشان داده می‌شود و هم‌نام ساده جدا می‌ماند.
Allp هیچ منبع معناداری را خودکار انتخاب نمی‌کند.

## کنترل‌های تعاملی

در نتیجه بلند، Space/`b` صفحه جلو/عقب، `/` Filter، عدد انتخاب Global، Enter
انتخاب Highlight/اول و `q` یا Escape لغو است. Filter شماره اصلی را تغییر نمی‌دهد.

## اجرای بدون تعامل

Script نمی‌تواند به سؤال Scope یا Source جواب دهد؛ آن‌ها را صریح بدهید:

```bash
allp search git --scope apps --json
allp install git --from apt --dry-run --no-interactive
allp install git --from apt --no-interactive --yes
```

اگر ابهام باقی بماند Exit Code شماره ۴ همراه راه بازیابی برمی‌گردد.


</div>
