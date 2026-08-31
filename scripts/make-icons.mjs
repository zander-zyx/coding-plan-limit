// 从满铺 Logo Mark 生成全套应用默认图标（PNG 全尺寸 + ICO）
// 用法：node scripts/make-icons.mjs
// 说明：读取 assets/logo-mark-tile-1024.png（本仓库自产 PNG，filter=0 无滤波，可直解），
//       箱式 area-average 下采样，保证小尺寸边缘干净。
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';

const ROOT = path.resolve(new URL('..', import.meta.url).pathname);
const SRC = path.join(ROOT, 'assets/logo-mark-tile-1024.png');
const ICONS = path.join(ROOT, 'src-tauri/icons');

// ── PNG 解码（仅支持本仓库自产的 filter=0 RGBA PNG）──
function decodePNG(buf) {
  if (buf.readUInt32BE(0) !== 0x89504e47) throw new Error('not a PNG');
  let off = 8, w = 0, h = 0, idat = [];
  while (off < buf.length) {
    const len = buf.readUInt32BE(off);
    const type = buf.toString('ascii', off + 4, off + 8);
    const data = buf.subarray(off + 8, off + 8 + len);
    if (type === 'IHDR') { w = data.readUInt32BE(0); h = data.readUInt32BE(4); }
    if (type === 'IDAT') idat.push(data);
    off += 12 + len;
    if (type === 'IEND') break;
  }
  const raw = zlib.inflateSync(Buffer.concat(idat));
  const rgba = Buffer.alloc(w * h * 4);
  for (let y = 0; y < h; y++) {
    const p = y * (w * 4 + 1);
    if (raw[p] !== 0) throw new Error(`unsupported filter ${raw[p]}`);
    raw.copy(rgba, y * w * 4, p + 1, p + 1 + w * 4);
  }
  return { rgba, w, h };
}

// ── PNG 编码（filter none, RGBA）──
const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; t[n] = c >>> 0; }
  return t;
})();
function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type), data]);
  const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}
function encodePNG(rgba, w, h) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8; ihdr[9] = 6;
  const raw = Buffer.alloc(h * (w * 4 + 1));
  for (let y = 0; y < h; y++) {
    raw[y * (w * 4 + 1)] = 0; // filter none
    rgba.copy(raw, y * (w * 4 + 1) + 1, y * w * 4, (y + 1) * w * 4);
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr), chunk('IDAT', zlib.deflateSync(raw, { level: 9 })), chunk('IEND', Buffer.alloc(0)),
  ]);
}

// ── 箱式 area-average 下采样（抗锯齿，边缘干净）──
function resize(src, dw) {
  const { rgba, w, h } = src;
  const out = Buffer.alloc(dw * dw * 4);
  const sx = w / dw, sy = h / dw;
  for (let y = 0; y < dw; y++) {
    const y0 = y * sy, y1 = (y + 1) * sy;
    for (let x = 0; x < dw; x++) {
      const x0 = x * sx, x1 = (x + 1) * sx;
      let r = 0, g = 0, b = 0, a = 0, area = 0;
      for (let yy = Math.floor(y0); yy < Math.min(h, Math.ceil(y1)); yy++) {
        const wy = Math.min(y1, yy + 1) - Math.max(y0, yy);
        if (wy <= 0) continue;
        for (let xx = Math.floor(x0); xx < Math.min(w, Math.ceil(x1)); xx++) {
          const wx = Math.min(x1, xx + 1) - Math.max(x0, xx);
          if (wx <= 0) continue;
          const wgt = wx * wy, o = (yy * w + xx) * 4;
          r += rgba[o] * wgt; g += rgba[o + 1] * wgt; b += rgba[o + 2] * wgt; a += rgba[o + 3] * wgt;
          area += wgt;
        }
      }
      const o = (y * dw + x) * 4;
      out[o] = Math.round(r / area); out[o + 1] = Math.round(g / area);
      out[o + 2] = Math.round(b / area); out[o + 3] = Math.round(a / area);
    }
  }
  return out;
}

// ── ICO（PNG 内嵌，Vista+）──
function makeICO(pngs) {
  const count = pngs.length;
  const head = Buffer.alloc(6);
  head.writeUInt16LE(0, 0); head.writeUInt16LE(1, 2); head.writeUInt16LE(count, 4);
  const entries = []; let offset = 6 + 16 * count;
  for (const { size, data } of pngs) {
    const e = Buffer.alloc(16);
    e[0] = size >= 256 ? 0 : size; e[1] = size >= 256 ? 0 : size;
    e.writeUInt16LE(1, 4); e.writeUInt16LE(32, 6);
    e.writeUInt32LE(data.length, 8); e.writeUInt32LE(offset, 12);
    entries.push(e); offset += data.length;
  }
  return Buffer.concat([head, ...entries, ...pngs.map(p => p.data)]);
}

// ── 生成 ──
const src = decodePNG(fs.readFileSync(SRC));
const png = (n) => encodePNG(resize(src, n), n, n);

const plain = [16, 24, 30, 32, 44, 48, 50, 64, 71, 89, 107, 128, 142, 150, 256, 284, 310, 512];
const made = {};
for (const n of plain) made[n] = png(n);

const files = {
  '32x32.png': 32, '64x64.png': 64, '128x128.png': 128, '128x128@2x.png': 256,
  'icon.png': 512,
  'Square30x30Logo.png': 30, 'Square44x44Logo.png': 44, 'Square71x71Logo.png': 71,
  'Square89x89Logo.png': 89, 'Square107x107Logo.png': 107, 'Square142x142Logo.png': 142,
  'Square150x150Logo.png': 150, 'Square284x284Logo.png': 284, 'Square310x310Logo.png': 310,
  'StoreLogo.png': 50,
};
for (const [name, n] of Object.entries(files)) fs.writeFileSync(path.join(ICONS, name), made[n]);

// UI 缩略图（设置页「原色」选项）
fs.writeFileSync(path.join(ROOT, 'ui/icons/app.png'), made[128]);

// Windows ICO：16/24/32/48/64/128/256
const icoSizes = [16, 24, 32, 48, 64, 128, 256].map(n => ({ size: n, data: made[n] }));
fs.writeFileSync(path.join(ICONS, 'icon.ico'), makeICO(icoSizes));

console.log('OK: PNG ×' + (Object.keys(files).length + 1) + ' + icon.ico (7 sizes)');
console.log('icns 请用 iconutil 生成（见 build 脚本）');
