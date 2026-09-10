# اتوماسیون و JSON

[← خانه ویکی](Home.fa.md) · [English](Automation-and-JSON.md)

در اجرای بدون تعامل Backend و Scope را صریح تعیین کنید. JSON برای فرمان‌های
Read-only و Dry Run نگهداری پشتیبانی می‌شود و Prompt، رنگ یا Spinner انسانی با
Stdout ساختاریافته مخلوط نمی‌شود.

```bash
allp detect --json
allp search git --from apt --json
allp list --from flatpak --json
allp info git --from apt --json
allp update --dry-run --json
allp upgrade --dry-run --json
```

## Envelope

```json
{
  "schema_version": 2,
  "command": "search",
  "complete": true,
  "results": {"query": "git", "candidates": [], "groups": [], "backends": []},
  "issues": []
}
```

`schema_version` مرز سازگاری و `complete` سیگنال کیفیت داده است. مقدار False
ممکن است همراه Candidate معتبر باشد، چون یک Backend خروجی ناقص یا ناشناخته داده؛
قبل از اقدام `issues` را بررسی کنید.

```bash
report="$(mktemp)"
if allp search git --from apt --json >"$report"; then
  jq -e '.schema_version == 2 and .complete == true' "$report" >/dev/null
  jq '.results.candidates[] | {backend_id, package_id, match_kind}' "$report"
fi
rm -f "$report"
```

برای اتوماسیون تغییر، اول Dry Run را ذخیره کنید و بعد Backend/Package دقیق را
با `--no-interactive --yes` اجرا کنید. `--yes` منبع مبهم یا Bootstrap را تأیید
نمی‌کند؛ Bootstrap به `--allow-bootstrap` هم نیاز دارد. Exit Code شماره 8 یعنی
بخشی از Batch شکست خورده و موفقیت کامل نیست.

تمام فیلدها در [JSON_SCHEMA.md](../docs/JSON_SCHEMA.md) تعریف شده‌اند.
