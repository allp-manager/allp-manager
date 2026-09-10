# توسعه و انتشار

[← خانه ویکی](Home.fa.md) · [English](Development.md)

## آماده‌سازی محیط

```bash
git clone https://github.com/allp-manager/allp-manager.git
cd allp-manager
rustup show
cargo build
cargo run -- detect
```

حداقل Rust نسخه 1.74 است و `rust-toolchain.toml` کانال Stable را با rustfmt و Clippy دنبال می‌کند.

## Quality Gate

```bash
make fmt-check
make check
make clippy
make test
make architecture
make release
make docs-check
make quality
```

تست‌ها از Executable جعلی و Fixture استفاده می‌کنند و نباید عملیات مخرب Package
Manager واقعی را اجرا کنند. تغییر Parser به Fixture نماینده نیاز دارد. تغییر
رفتار هم باید Changelog بخش Unreleased، Regression Test و Guardrail تازه داشته باشد.

## چک‌لیست Backend

1. Identity، Category، Requirement و Capability دقیق را اعلام کنید.
2. Argv و Parser بومی را داخل ماژول Backend نگه دارید.
3. برای Mutation فقط Plan برگردانید و Process اجرا نکنید.
4. Scope و Privilege را تعریف کنید.
5. Fixture موفق، بدون نتیجه، خروجی خراب و Failure اضافه کنید.
6. فقط یک بار در `src/backends/catalog.rs` ثبت کنید.
7. `make quality` را اجرا کنید.

## CI و Release

CI شامل Quality Gate لینوکس، تست قابل‌حمل Linux/macOS/Windows، Cross-target،
قرارداد Bootstrap خانواده‌ها و Canary هفتگی Backend است. Release آرشیو مخصوص
Target، Checksum و Manifest می‌سازد.

```bash
make hooks-install
make release-prepare BUMP=patch
make release-status
# commit: release: Allp vX.Y.Z
make release-push
```

Prepare فایل‌های نسخه را تغییر می‌دهد و Quality Gate را اجرا می‌کند. Hook فقط
Tag محلی Annotated و فایل Ignored در `dist/` می‌سازد؛ تا `make release-push`
چیزی Push یا Publish نمی‌شود.

پیش از مشارکت [CONTRIBUTING.md](../CONTRIBUTING.md)،
[Regression Guardrails](../docs/REGRESSION_GUARDRAILS.md) و
[راهنمای Release](../release/README.md) را بخوانید.
