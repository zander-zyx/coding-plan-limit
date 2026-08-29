# Center-crop a source image to a 1024x1024 PNG (default app icon source).
# Usage: powershell -File scripts/make-icon.ps1 -SourcePath <path-to-image>
param(
    [Parameter(Mandatory = $true)][string]$SourcePath,
    [string]$OutPath = (Join-Path $PSScriptRoot '..\assets\icon-source.png')
)
Add-Type -AssemblyName System.Drawing
if (-not (Test-Path $SourcePath)) { Write-Error "Source image not found: $SourcePath"; exit 1 }

$src = [System.Drawing.Image]::FromFile($SourcePath)
$bmp = $null
try {
    $side = [Math]::Min($src.Width, $src.Height)
    $bmp = New-Object System.Drawing.Bitmap -ArgumentList 1024, 1024
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    try {
        $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
        $srcRect = New-Object System.Drawing.Rectangle -ArgumentList ([int](($src.Width - $side) / 2)), ([int](($src.Height - $side) / 2)), $side, $side
        $dstRect = New-Object System.Drawing.Rectangle -ArgumentList 0, 0, 1024, 1024
        $g.DrawImage($src, $dstRect, $srcRect, [System.Drawing.GraphicsUnit]::Pixel)
    } finally { $g.Dispose() }

    $dir = Split-Path $OutPath
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
    $bmp.Save($OutPath, [System.Drawing.Imaging.ImageFormat]::Png)
    Write-Output "OK -> $OutPath"
} finally {
    $src.Dispose()
    if ($bmp) { $bmp.Dispose() }
}
