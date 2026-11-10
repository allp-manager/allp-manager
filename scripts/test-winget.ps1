#Requires -Version 7.2

param(
    [Parameter(Mandatory)] [string] $AllpPath,
    [Parameter(Mandatory)] [string] $OutputDirectory,
    [switch] $LiveLifecycle
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$allp = (Resolve-Path -LiteralPath $AllpPath).Path
$winget = (Get-Command winget.exe -CommandType Application).Source
$results = (New-Item -ItemType Directory -Path $OutputDirectory -Force).FullName
$package = 'sharkdp.fd'
$noApplicationsFound = -1978335212
$upgradeNotApplicable = -1978335189

function Invoke-Logged {
    param(
        [string] $Name,
        [string] $Program,
        [string[]] $CliArgs,
        [switch] $Json,
        [int[]] $ExpectedExitCodes = @(0)
    )

    $stderrPath = Join-Path $results "$Name.stderr.log"
    $output = & $Program @CliArgs 2> $stderrPath
    $exitCode = $LASTEXITCODE
    $text = $output -join [Environment]::NewLine
    $text | Set-Content -LiteralPath (Join-Path $results "$Name.stdout.log") -Encoding utf8
    if ($exitCode -notin $ExpectedExitCodes) {
        $details = Get-Content -LiteralPath $stderrPath -Raw
        throw "$Name exited with code $exitCode.`n$text`n$details"
    }
    if ($Json) {
        return (ConvertFrom-Json -InputObject $text)
    }
    if ($text) { Write-Host $text }
}

function Assert-Result([bool] $Passed, [string] $Message) {
    if (-not $Passed) { throw $Message }
    Write-Host "Passed: $Message"
    if ($env:GITHUB_STEP_SUMMARY) {
        "- $Message" | Add-Content -LiteralPath $env:GITHUB_STEP_SUMMARY -Encoding utf8
    }
}

function Get-AllpInventory([string] $Name) {
    $report = Invoke-Logged $Name $allp @('list', '--from', 'winget', '--json') -Json
    Assert-Result ($report.complete -eq $true) 'Allp parsed the installed WinGet inventory completely.'
    return @($report.results | Where-Object { $_.package_id -eq $package -and $_.backend_id -eq 'winget' })
}

if ($env:GITHUB_STEP_SUMMARY) {
    '### Native Windows Package Manager' | Add-Content -LiteralPath $env:GITHUB_STEP_SUMMARY -Encoding utf8
}
Invoke-Logged 'winget-version' $winget @('--version')
Invoke-Logged 'allp-version' $allp @('--version')
if ($LiveLifecycle) {
    # Accept the public community source agreement only on the explicitly selected test runner.
    Invoke-Logged 'native-source-search' $winget @('search', '--id', $package, '--exact', '--source', 'winget', '--accept-source-agreements', '--disable-interactivity')
    # Keep native output to diagnose parser failures on a new App Installer version.
    Invoke-Logged 'native-query-search' $winget @('search', '--query', $package, '--count', '30', '--source', 'winget', '--disable-interactivity')
    Invoke-Logged 'native-show' $winget @('show', '--id', $package, '--exact', '--source', 'winget', '--disable-interactivity')
    Invoke-Logged 'native-list-before' $winget @('list', '--source', 'winget', '--disable-interactivity') -ExpectedExitCodes @(0, $noApplicationsFound)
    Invoke-Logged 'native-upgrade-preview' $winget @('upgrade', '--source', 'winget', '--disable-interactivity') -ExpectedExitCodes @(0, $noApplicationsFound, $upgradeNotApplicable)
}

$detected = Invoke-Logged 'allp-detect' $allp @('detect', '--json') -Json
$manager = $detected.results | Where-Object { $_.backend_id -eq 'winget' }
Assert-Result ($detected.complete -and $manager.state -eq 'ready') 'Allp detected native WinGet and reported it ready.'
$missing = @('search', 'info', 'list', 'install', 'remove', 'update', 'upgrade') |
    Where-Object { $_ -notin $manager.capabilities }
Assert-Result (-not $missing) 'WinGet advertises search, info, list, install, remove, update and upgrade.'

$search = Invoke-Logged 'allp-search' $allp @('search', $package, '--exact', '--from', 'winget', '--json') -Json
$match = @($search.results.candidates | Where-Object {
    $_.package_id -eq $package -and $_.backend_id -eq 'winget' -and $_.source -eq 'winget'
})
Assert-Result ($search.complete -and $match.Count -eq 1) 'An exact Allp search resolved the community source and package ID.'
$notFound = Invoke-Logged 'allp-no-match' $allp @('search', 'allp-smoke.Nonexistent-089d4bdc', '--exact', '--from', 'winget', '--json') -Json
Assert-Result ($notFound.complete -and @($notFound.results.candidates).Count -eq 0) 'A native WinGet no-match response remained an empty successful search.'
$info = Invoke-Logged 'allp-info' $allp @('info', $package, '--from', 'winget', '--json') -Json
Assert-Result ($info.complete -and $info.results.package_id -eq $package -and [bool]$info.results.version) 'Allp info reported the exact WinGet package and version.'

$update = Invoke-Logged 'allp-update-preview' $allp @('update', '--from', 'winget', '--skip-self-update', '--dry-run', '--no-interactive', '--json') -Json
Assert-Result ($update.complete -and [bool]($update.plans | Where-Object { $_.command -match 'source update --name winget' })) 'Allp update planned a WinGet community source refresh.'
$before = @(Get-AllpInventory 'allp-list-before')
$install = Invoke-Logged 'allp-install-preview' $allp @('install', $package, '--from', 'winget', '--dry-run', '--no-interactive', '--json') -Json
Assert-Result ($install.backend_id -eq 'winget' -and $install.package_id -eq $package -and $install.native_command -match 'install .*--id sharkdp\.fd .*--exact' -and $install.native_command -match '--source winget' -and $install.native_command -notmatch '(^| )sudo( |$)') 'Install preview used an exact WinGet ID and community source with no sudo.'
$afterPreview = @(Get-AllpInventory 'allp-list-after-preview')
Assert-Result ($before.Count -eq $afterPreview.Count) 'Install preview left the package installation state unchanged.'

$upgrade = Invoke-Logged 'allp-upgrade-preview' $allp @('upgrade', '--from', 'winget', '--dry-run', '--no-interactive', '--json') -Json
$upgradePlan = $upgrade.plans | Where-Object { $_.command -match 'upgrade .*--all.*--source winget' }
$current = $upgrade.results | Where-Object { $_.backend_id -eq 'winget' -and $_.status -eq 'up_to_date' }
Assert-Result ($upgrade.complete -and ($upgradePlan -or $current)) 'Upgrade preview showed the native WinGet plan or a verified up-to-date inventory.'

if ($LiveLifecycle) {
    Assert-Result ($before.Count -eq 0) 'The disposable test machine had no preexisting copy of the test package.'
    $cleanupNeeded = $true
    try {
        $updated = Invoke-Logged 'allp-update' $allp @('update', '--from', 'winget', '--skip-self-update', '--yes', '--no-interactive', '--json') -Json
        Assert-Result ($updated.complete -and [bool]($updated.results | Where-Object { $_.command -match 'source update --name winget' -and $_.status -in 'completed','updated','up_to_date' })) 'Allp completed a live WinGet community source refresh.'
        Invoke-Logged 'allp-install' $allp @('install', $package, '--from', 'winget', '--yes', '--no-interactive', '--no-tui', '--no-color')
        $installed = @(Get-AllpInventory 'allp-list-installed')
        Assert-Result ($installed.Count -eq 1 -and [bool]$installed[0].version) 'Allp installed the real portable package and listed its version.'
        Invoke-Logged 'native-list-installed' $winget @('list', '--id', $package, '--exact', '--source', 'winget', '--disable-interactivity')

        $remove = Invoke-Logged 'allp-remove-preview' $allp @('remove', $package, '--from', 'winget', '--dry-run', '--no-interactive', '--json') -Json
        Assert-Result ($remove.backend_id -eq 'winget' -and $remove.package_id -eq $package -and $remove.native_command -match 'uninstall .*--id sharkdp\.fd .*--exact') 'Remove preview used the exact installed WinGet ID.'
        Invoke-Logged 'allp-remove' $allp @('remove', $package, '--from', 'winget', '--yes', '--no-interactive', '--no-tui', '--no-color')
        $removed = @(Get-AllpInventory 'allp-list-removed')
        Assert-Result ($removed.Count -eq 0) 'Allp uninstalled the real portable package and verified its absence.'
        $cleanupNeeded = $false
    } finally {
        if ($cleanupNeeded) {
            try {
                Invoke-Logged 'native-cleanup' $winget @('uninstall', '--id', $package, '--exact', '--source', 'winget', '--disable-interactivity')
            } catch {
                Write-Warning "Cleanup of the disposable runner's test package did not succeed: $_"
            }
        }
    }
}
