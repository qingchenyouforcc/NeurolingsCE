Param(
    [string[]]$SourceDir = @(),

    [string[]]$SkillName = @(),

    [string]$AppRoot
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ($AppRoot) {
    $SourceDir = @()
    $SkillName = @()
    foreach ($candidate in @("neurolingsce-skill", "neurolingsce-companion")) {
        $candidateSourceDir = Join-Path $AppRoot $candidate
        if (Test-Path -LiteralPath $candidateSourceDir) {
            $SourceDir += $candidateSourceDir
            $SkillName += $candidate
        }
    }
}

if ($SourceDir.Count -eq 0) {
    throw "No skill source directories were provided or discovered."
}

if ($SkillName.Count -gt 0 -and $SkillName.Count -ne $SourceDir.Count) {
    throw "SkillName count must match SourceDir count when SkillName is provided."
}

$agentsHome = Join-Path $env:USERPROFILE ".agents"
$skillsRoot = Join-Path $agentsHome "skills"

New-Item -ItemType Directory -Path $skillsRoot -Force | Out-Null

for ($index = 0; $index -lt $SourceDir.Count; $index++) {
    $currentSourceDir = $SourceDir[$index]
    if (-not (Test-Path -LiteralPath $currentSourceDir)) {
        throw "Skill source directory not found: $currentSourceDir"
    }

    if ($SkillName.Count -gt 0) {
        $currentSkillName = $SkillName[$index]
    } else {
        $currentSkillName = Split-Path -Leaf (Resolve-Path -LiteralPath $currentSourceDir)
    }

    $targetDir = Join-Path $skillsRoot $currentSkillName

    if (Test-Path -LiteralPath $targetDir) {
        Remove-Item -LiteralPath $targetDir -Recurse -Force
    }

    New-Item -ItemType Directory -Path $targetDir -Force | Out-Null

    Get-ChildItem -LiteralPath $currentSourceDir -Force | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination $targetDir -Recurse -Force
    }

    Write-Host "Installed skill to: $targetDir"
}
