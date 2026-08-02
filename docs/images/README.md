# Images Directory

Command:

## Exporting SVG to ICO

```bash
magick -background transparent -define 'icon:auto-resize=16,32,48,64,256' input.svg outuput.icon
```

## Exporting with Inkscape

```bash
inkscape input.svg --export-type=png --export-filename=output.png --export-dpi=300
```

```bash
for file in *.svg; do inkscape "$file" --export-dpi=300 --export-type=png --export-filename="${file/.svg/}.png"; done
```
