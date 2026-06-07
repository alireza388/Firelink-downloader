import sys
from PIL import Image, ImageDraw

def generate_gradient_preview(src_path, dest_path):
    img = Image.open(src_path).convert("RGBA")
    width, height = img.size
    pixels = img.load()
    
    # Background color from an inner point
    bg_color = pixels[100, 100]
    
    # Define gradient colors
    # Top color: slightly lighter/richer blue (e.g. #1E2541)
    # Bottom color: darker navy (e.g. #0A0D1A)
    top_color = (30, 37, 65, 255)
    bottom_color = (10, 13, 26, 255)
    
    # Create new image
    new_img = Image.new("RGBA", (width, height))
    new_pixels = new_img.load()
    
    for y in range(height):
        # Interpolate background color for this row
        ratio = y / float(height - 1)
        grad_r = int(top_color[0] * (1 - ratio) + bottom_color[0] * ratio)
        grad_g = int(top_color[1] * (1 - ratio) + bottom_color[1] * ratio)
        grad_b = int(top_color[2] * (1 - ratio) + bottom_color[2] * ratio)
        grad_color = (grad_r, grad_g, grad_b, 255)
        
        for x in range(width):
            p = pixels[x, y]
            
            # distance from original background color
            dist = max(abs(p[0]-bg_color[0]), abs(p[1]-bg_color[1]), abs(p[2]-bg_color[2]))
            
            if dist < 15:
                # purely background
                new_pixels[x, y] = grad_color
            elif dist < 60:
                # anti-aliased edge, blend
                alpha = (dist - 15) / 45.0
                r = int(p[0] * alpha + grad_color[0] * (1 - alpha))
                g = int(p[1] * alpha + grad_color[1] * (1 - alpha))
                b = int(p[2] * alpha + grad_color[2] * (1 - alpha))
                new_pixels[x, y] = (r, g, b, 255)
            else:
                # purely logo
                new_pixels[x, y] = p
                
    # Apply rounded rectangle mask
    radius = int(width * 0.225)
    mask = Image.new("L", (width, height), 0)
    draw = ImageDraw.Draw(mask)
    draw.rounded_rectangle((0, 0, width, height), radius=radius, fill=255)
    new_img.putalpha(mask)
    
    # Resize to something reasonable for preview
    preview = new_img.resize((512, 512), Image.Resampling.LANCZOS)
    preview.save(dest_path)
    print("Preview saved to", dest_path)

if __name__ == '__main__':
    generate_gradient_preview(sys.argv[1], sys.argv[2])
