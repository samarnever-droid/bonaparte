// Generates a minimal 32x32 32bpp icon.ico (Bonaparte blue square with an
// accent triangle) so tauri-build can embed the Windows resource.
import { writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const outDir = join(here, "icons");
const out = join(outDir, "icon.ico");
mkdirSync(outDir, { recursive: true });

const S = 32;
const px = new Uint8Array(S * S * 4); // BGRA, bottom-up
const set = (x, y, [r, g, b]) => {
  const row = S - 1 - y;
  const i = (row * S + x) * 4;
  px[i] = b;
  px[i + 1] = g;
  px[i + 2] = r;
  px[i + 3] = 255;
};
for (let y = 0; y < S; y++) {
  for (let x = 0; x < S; x++) {
    // dark navy field, accent "flag" triangle in the upper-left
    const inFlag = y >= 8 && y <= 22 && x >= 6 && x - 6 <= ((22 - y) * 10) / 7;
    set(x, y, inFlag ? [0x6b, 0x8a, 0xfd] : [0x14, 0x16, 0x1c]);
  }
}

const bmpInfo = Buffer.alloc(40);
bmpInfo.writeUInt32LE(40, 0); // biSize
bmpInfo.writeInt32LE(S, 4);
bmpInfo.writeInt32LE(S * 2, 8); // XOR + AND heights
bmpInfo.writeUInt16LE(1, 12); // planes
bmpInfo.writeUInt16LE(32, 14); // bpp
bmpInfo.writeUInt32LE(0, 16); // BI_RGB
bmpInfo.writeUInt32LE(px.length + S * 4, 20); // image size incl. AND mask

const andMask = Buffer.alloc(S * 4); // all-opaque mask: zeros

const image = Buffer.concat([bmpInfo, Buffer.from(px), andMask]);

const dir = Buffer.alloc(6);
dir.writeUInt16LE(0, 0);
dir.writeUInt16LE(1, 2); // type: icon
dir.writeUInt16LE(1, 4); // count

const entry = Buffer.alloc(16);
entry[0] = S;
entry[1] = S;
entry.writeUInt16LE(1, 4); // planes
entry.writeUInt16LE(32, 6); // bpp
entry.writeUInt32LE(image.length, 8);
entry.writeUInt32LE(22, 12); // offset = 6 + 16

writeFileSync(out, Buffer.concat([dir, entry, image]));
console.log("wrote", out);
