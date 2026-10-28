param(
    [int[]]$OptimumOrigin = @(18, 18),
    [int[]]$TerminalOrigin = @(0, 0),
    [int[]]$Size = @(900, 420),
    [int]$Zoom = 8,
    [int[]]$ZoomOrigin = @(0, 0),
    [int[]]$ZoomSize = @(160, 60),
    [string]$Out = "$PSScriptRoot\captures"
)

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class Capture {
  [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  public struct RECT { public int L, T, R, B; }
}
"@
[Capture]::SetProcessDpiAwarenessContext([IntPtr]-4) | Out-Null
New-Item -ItemType Directory -Force -Path $Out | Out-Null

function Grab([string]$process, [int[]]$origin, [string]$name) {
    $h = (Get-Process $process -ErrorAction Stop | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1).MainWindowHandle
    $r = New-Object Capture+RECT
    [Capture]::GetClientRect($h, [ref]$r) | Out-Null
    $full = New-Object System.Drawing.Bitmap($r.R, $r.B, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($full)
    $dc = $g.GetHdc()
    [Capture]::PrintWindow($h, $dc, 3) | Out-Null
    $g.ReleaseHdc($dc)
    $g.Dispose()
    $crop = $full.Clone((New-Object System.Drawing.Rectangle($origin[0], $origin[1], $Size[0], $Size[1])), $full.PixelFormat)
    $crop.Save("$Out\$name.png")
    $full.Dispose()
    "$name dpi=$([Capture]::GetDpiForWindow($h)) client=$($r.R)x$($r.B)"
    return $crop
}

function Zoomed([System.Drawing.Bitmap]$source) {
    $z = New-Object System.Drawing.Bitmap(($ZoomSize[0] * $Zoom), ($ZoomSize[1] * $Zoom))
    $g = [System.Drawing.Graphics]::FromImage($z)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::NearestNeighbor
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::Half
    $src = New-Object System.Drawing.Rectangle($ZoomOrigin[0], $ZoomOrigin[1], $ZoomSize[0], $ZoomSize[1])
    $dst = New-Object System.Drawing.Rectangle(0, 0, $z.Width, $z.Height)
    $g.DrawImage($source, $dst, $src, [System.Drawing.GraphicsUnit]::Pixel)
    $g.Dispose()
    return $z
}

$optimum = Grab "optimum" $OptimumOrigin "optimum"
$terminal = Grab "WindowsTerminal" $TerminalOrigin "terminal"
$optimum = $optimum[-1]
$terminal = $terminal[-1]

$a = Zoomed $optimum
$b = Zoomed $terminal
$side = New-Object System.Drawing.Bitmap(($a.Width * 2 + 16), $a.Height)
$g = [System.Drawing.Graphics]::FromImage($side)
$g.Clear([System.Drawing.Color]::Magenta)
$g.DrawImage($a, 0, 0)
$g.DrawImage($b, $a.Width + 16, 0)
$g.Dispose()
$side.Save("$Out\zoom-optimum-vs-terminal.png")

$diff = New-Object System.Drawing.Bitmap($Size[0], $Size[1])
$total = 0
for ($y = 0; $y -lt $Size[1]; $y++) {
    for ($x = 0; $x -lt $Size[0]; $x++) {
        $p = $optimum.GetPixel($x, $y)
        $q = $terminal.GetPixel($x, $y)
        $d = [Math]::Max([Math]::Abs($p.R - $q.R), [Math]::Max([Math]::Abs($p.G - $q.G), [Math]::Abs($p.B - $q.B)))
        $total += $d
        $v = [Math]::Min(255, $d * 4)
        $diff.SetPixel($x, $y, [System.Drawing.Color]::FromArgb($v, $v, $v))
    }
}
$diff.Save("$Out\diff.png")
"mean channel difference: {0:N3}" -f ($total / ($Size[0] * $Size[1]))
"saved to $Out"
