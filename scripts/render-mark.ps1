# 从 logo-mark.svg 几何重绘高清 PNG（坐标已 x2 = 1024 画布）
Add-Type -AssemblyName System.Drawing

$fmt = [System.Drawing.Imaging.PixelFormat]::Format32bppArgb
$white = New-Object System.Drawing.SolidBrush -ArgumentList ([System.Drawing.Color]::FromArgb(255, 0xF4, 0xF3, 0xF8))
$violet = New-Object System.Drawing.SolidBrush -ArgumentList ([System.Drawing.Color]::FromArgb(255, 0x8B, 0x6C, 0xFF))

function Draw-Poly([System.Drawing.Graphics]$g, [System.Drawing.SolidBrush]$b, [System.Drawing.Point[]]$pts) {
    $g.FillPolygon($b, $pts)
}

function Point-List([int[]]$p) {
    $n = [int]($p.Length / 2)
    $pts = New-Object 'System.Drawing.Point[]' $n
    for ($i = 0; $i -lt $n; $i++) {
        $pts[$i] = New-Object System.Drawing.Point -ArgumentList $p[2*$i], $p[2*$i+1]
    }
    return ,$pts
}

# 形状（1024 画布坐标 = viewBox x2）
$rect1 = @(236, 232, 552, 132)   # 上横
$rect2 = @(236, 660, 552, 132)   # 下横
$poly1 = @(656, 364, 788, 364, 674, 444, 542, 444)   # 白斜块
$poly2 = @(498, 476, 628, 476, 526, 548, 394, 548)   # 紫斜块
$poly3 = @(350, 580, 482, 580, 368, 660, 236, 660)   # 白斜块

function Draw-MarkShapes([System.Drawing.Graphics]$g) {
    $g.FillRectangle($white, $rect1[0], $rect1[1], $rect1[2], $rect1[3])
    $g.FillRectangle($white, $rect2[0], $rect2[1], $rect2[2], $rect2[3])
    Draw-Poly $g $white (Point-List $poly1)
    Draw-Poly $g $violet (Point-List $poly2)
    Draw-Poly $g $white (Point-List $poly3)
}

function Round-Path([int]$r) {
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = 2 * $r
    $path.AddArc(0, 0, $d, $d, 180, 90)
    $path.AddArc(1024 - $d, 0, $d, $d, 270, 90)
    $path.AddArc(1024 - $d, 1024 - $d, $d, $d, 0, 90)
    $path.AddArc(0, 1024 - $d, $d, $d, 90, 90)
    $path.CloseFigure()
    return $path
}

# 1) 透明底悬浮版
$float = New-Object System.Drawing.Bitmap -ArgumentList 1024, 1024, $fmt
$g = [System.Drawing.Graphics]::FromImage($float)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
$g.Clear([System.Drawing.Color]::Transparent)
Draw-MarkShapes $g
$g.Dispose()
$float.Save('E:\zander_project\coding-plan-limit\assets\logo-mark-alpha-1024.png', [System.Drawing.Imaging.ImageFormat]::Png)
$float.Dispose()

# 2) 深色圆角底版
$tile = New-Object System.Drawing.Bitmap -ArgumentList 1024, 1024, $fmt
$g = [System.Drawing.Graphics]::FromImage($tile)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
$g.Clear([System.Drawing.Color]::Transparent)
$path = Round-Path 232
$b1 = New-Object System.Drawing.SolidBrush -ArgumentList ([System.Drawing.Color]::FromArgb(255, 0x1B, 0x19, 0x22))
$g.FillPath($b1, $path)
Draw-MarkShapes $g
$path.Dispose(); $b1.Dispose()
$g.Dispose()
$tile.Save('E:\zander_project\coding-plan-limit\assets\logo-mark-tile-1024.png', [System.Drawing.Imaging.ImageFormat]::Png)
$tile.Dispose()

# 3) 缩 256px 透明版 → ui/icons/app-mark.png
$src = [System.Drawing.Image]::FromFile('E:\zander_project\coding-plan-limit\assets\logo-mark-alpha-1024.png')
$out = New-Object System.Drawing.Bitmap -ArgumentList 256, 256, $fmt
$g2 = [System.Drawing.Graphics]::FromImage($out)
$g2.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$g2.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
$g2.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
$g2.DrawImage($src, 0, 0, 256, 256)
$g2.Dispose(); $out.Dispose(); $src.Dispose()

Write-Output 'DONE'
