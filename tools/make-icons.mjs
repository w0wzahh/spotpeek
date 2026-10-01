// Generates all app icons from src-tauri/icons/icon.svg.
// Usage: node tools/make-icons.mjs
import { readFileSync, writeFileSync } from "node:fs";
import sharp from "sharp";

const SRC = "src-tauri/icons/icon.svg";
const svg = readFileSync(SRC);

const render = (size) =>
  sharp(svg, { density: 384 }).resize(size, size).png().toBuffer();

// Individual PNGs used by the bundle config / tray
const targets = [
  ["src-tauri/icons/32x32.png", 32],
  ["src-tauri/icons/128x128.png", 128],
  ["src-tauri/icons/128x128@2x.png", 256],
  ["src-tauri/icons/icon.png", 512],
  ["src-tauri/icons/tray.png", 64],
  // Windows Store / MSIX tile assets
  ["src-tauri/icons/StoreLogo.png", 50],
  ["src-tauri/icons/Square30x30Logo.png", 30],
  ["src-tauri/icons/Square44x44Logo.png", 44],
  ["src-tauri/icons/Square71x71Logo.png", 71],
  ["src-tauri/icons/Square89x89Logo.png", 89],
  ["src-tauri/icons/Square107x107Logo.png", 107],
  ["src-tauri/icons/Square142x142Logo.png", 142],
  ["src-tauri/icons/Square150x150Logo.png", 150],
  ["src-tauri/icons/Square284x284Logo.png", 284],
  ["src-tauri/icons/Square310x310Logo.png", 310],
];

for (const [path, size] of targets) {
  writeFileSync(path, await render(size));
  console.log("wrote", path, `${size}px`);
}

// Windows .ico — PNG-compressed frames (valid since Vista)
const icoSizes = [16, 24, 32, 48, 64, 128, 256];
const frames = await Promise.all(icoSizes.map(render));

const header = Buffer.alloc(6);
header.writeUInt16LE(0, 0); // reserved
header.writeUInt16LE(1, 2); // type: icon
header.writeUInt16LE(frames.length, 4);

const dir = Buffer.alloc(16 * frames.length);
let offset = 6 + dir.length;
frames.forEach((png, i) => {
  const size = icoSizes[i];
  dir.writeUInt8(size >= 256 ? 0 : size, i * 16); // width (0 = 256)
  dir.writeUInt8(size >= 256 ? 0 : size, i * 16 + 1); // height
  dir.writeUInt8(0, i * 16 + 2); // palette
  dir.writeUInt8(0, i * 16 + 3); // reserved
  dir.writeUInt16LE(1, i * 16 + 4); // planes
  dir.writeUInt16LE(32, i * 16 + 6); // bpp
  dir.writeUInt32LE(png.length, i * 16 + 8);
  dir.writeUInt32LE(offset, i * 16 + 12);
  offset += png.length;
});

writeFileSync("src-tauri/icons/icon.ico", Buffer.concat([header, dir, ...frames]));
console.log("wrote src-tauri/icons/icon.ico", `(${icoSizes.join(", ")}px)`);

// Raw RGBA for embedding in the binary (avoids tauri's image-png dep).
const raw = await sharp(svg, { density: 384 })
  .resize(64, 64)
  .ensureAlpha()
  .raw()
  .toBuffer();
writeFileSync("src-tauri/icons/tray.rgba", raw);
console.log("wrote src-tauri/icons/tray.rgba (64x64 raw RGBA)");
