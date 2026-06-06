import sys
from PIL import Image, ImageDraw

def process_images(src_path):
    img = Image.open(src_path).convert("RGBA")
    width, height = img.size
    
    # Apply a standard macOS rounded rectangle mask
    # macOS standard radius is approx 22.5% of the width
    radius = int(width * 0.225)
    
    mask = Image.new("L", (width, height), 0)
    draw = ImageDraw.Draw(mask)
    draw.rounded_rectangle((0, 0, width, height), radius=radius, fill=255)
    
    # Apply mask
    img.putalpha(mask)

    # Save standard png
    img_1024 = img.resize((1024, 1024), Image.Resampling.LANCZOS)
    img_1024.save("Resources/AppIcon.png")
    
    # Save Firefox extension icons
    img_48 = img.resize((48, 48), Image.Resampling.LANCZOS)
    img_48.save("Extensions/Firefox/icons/icon-48.png")
    img_128 = img.resize((128, 128), Image.Resampling.LANCZOS)
    img_128.save("Extensions/Firefox/icons/icon-128.png")
    
    # MenuBarIconTemplate (64x64 monochrome)
    data = img.getdata()
    new_data = []
    
    for item in data:
        r, g, b, a = item
        if r > 100 and r > b * 1.5 and a > 0:
            alpha = min(255, max(0, int((r - 40) * 1.2)))
            new_data.append((0, 0, 0, alpha))
        else:
            new_data.append((0, 0, 0, 0))
            
    menu_bar_full = Image.new("RGBA", img.size)
    menu_bar_full.putdata(new_data)
    
    menu_bar_64 = menu_bar_full.resize((64, 64), Image.Resampling.LANCZOS)
    menu_bar_64.save("Sources/Firelink/Assets.xcassets/MenuBarIcon.imageset/MenuBarIconTemplate.png")
    
    print("Done generating main PNGs")
    
if __name__ == '__main__':
    process_images(sys.argv[1])
