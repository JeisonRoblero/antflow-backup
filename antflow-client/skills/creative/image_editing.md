---
id: "creative_image_editing"
friendly_name: "Image Editing & Drawing"
description: "Use to draw, paint, edit, crop, or create an image, picture, drawing, heart, rose, or canvas in mspaint."
---

# ROLE
You draw and edit images. Read the system prompt OS first. Always prefer programmatic python drawing over manual mouse simulation.

# DECISION TABLE — pick ONE row, execute it
# FORMAT:
OS | Python? | Input | → Action
any      | yes | pointer | → script A (Python Pillow draw)
Windows  | no  | pointer | → script B (PowerShell GDI draw)
macOS/Linux| no | pointer | → script C (sips / ImageMagick)
Android/iOS| any | touch   | → tell user: not supported

# SCRIPTS

## A — Python Pillow draw
```python
import sys
from PIL import Image, ImageDraw
# Create or open image
img = Image.new('RGB', (600, 600), color='white')
draw = ImageDraw.Draw(img)
# Example drawing coordinates: Red heart/flower shape
draw.polygon([(300,500), (100,250), (200,100), (300,200), (400,100), (500,250)], fill='red')
img.save("PATH")
print("saved")
```

## B — PowerShell GDI draw
```powershell
Add-Type -AssemblyName System.Drawing
$bmp = New-Object System.Drawing.Bitmap(600,600)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.Clear([System.Drawing.Color]::White)
$pen = New-Object System.Drawing.Pen([System.Drawing.Color]::Red, 5)
$g.DrawEllipse($pen, 150, 150, 300, 300)
$bmp.Save("PATH")
$g.Dispose();$bmp.Dispose()
Write-Host "saved"
```

## C — sips / ImageMagick
```bash
# Draw basic shape using macOS sips or ImageMagick if available
convert -size 600x600 xc:white -fill red -draw "circle 300,300 300,450" "PATH"
echo "saved"
```

# SUBSTITUTION
- PATH → target file destination (e.g. "$env:USERPROFILE\Pictures\drawing.png" or "~/Pictures/drawing.png")

# AFTER RUNNING
Check exit_code == 0. If yes: tell user "Image saved at PATH".
If no: report exact stderr.
