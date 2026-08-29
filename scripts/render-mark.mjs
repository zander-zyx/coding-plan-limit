// Logo Mark 光栅化：从 SVG 几何直接渲染高清透明 PNG（无外部依赖）
// 2x 超采样抗锯齿 → 1024 输出；透明底悬浮版 + 深色圆角底版 + 256px 应用版
import fs from 'node:fs';
import zlib from 'node:zlib';

const OUT = 1024;
const SS = 2;                 // 超采样
const DIM = OUT * SS;         // 2048

// ── 形状（1024 坐标系，SVG x2）──
const SHAPES_FLOAT = [
  { type: 'rect', x: 236, y: 232, w: 552, h: 132, c: [0xF4, 0xF3, 0xF8] },          // 上横
  { type: 'rect', x: 236, y: 660, w: 552, h: 132, c: [0xF4, 0xF3, 0xF8] },          // 下横
  { type: 'poly', pts: [656, 364, 788, 364, 674, 444, 542, 444], c: [0xF4, 0xF3, 0xF8] },
  { type: 'poly', pts: [498, 476, 628, 476, 526, 548, 394, 548], c: [0x8B, 0x6C, 0xFF] }, // 紫
  { type: 'poly', pts: [350, 580, 482, 580, 368, 660, 236, 660], c: [0xF4, 0xF3, 0xF8] },
];
const TILE_BG = { type: 'round', x: 0, y: 0, w: 1024, h: 1024, r: 232, c: [0x1B, 0x19, 0x22] };
const SHAPES_TILE = [TILE_BG, ...SHAPES_FLOAT];

// 凸多边形/矩形 内部测试（叉积同号，边界算内）
function inside(shape, x, y) {
  if (shape.type === 'rect') {
    return x >= shape.x && x <= shape.x + shape.w && y >= shape.y && y <= shape.y + shape.h;
  }
  if (shape.type === 'round') {
    // 圆角矩形 SDF
    const r = shape.r;
    const cx = shape.x + shape.w / 2, cy = shape.y + shape.h / 2;
    const hw = shape.w / 2 - r, hh = shape.h / 2 - r;
    const dx = Math.abs(x - cx) - hw;
    const dy = Math.abs(y - cy) - hh;
    const ox = Math.max(dx, 0), oy = Math.max(dy, 0);
    return Math.hypot(ox, oy) + Math.min(Math.max(dx, dy), 0) <= 0;
  }
  const p = shape.pts;
  let sign = 0;
  for (let i = 0; i < 4; i++) {
    const x1 = p[i * 2], y1 = p[i * 2 + 1];
    const x2 = p[((i + 1) % 4) * 2], y2 = p[((i + 1) % 4) * 2 + 1];
    const cross = (x2 - x1) * (y - y1) - (y2 - y1) * (x - x1);
    if (cross !== 0) {
      const s = Math.sign(cross);
      if (sign === 0) sign = s;
      else if (s !== sign) return false;
    }
  }
  return true;
}

// 渲染一版（返回 RGBA Buffer，1024×1024）
function render(shapes) {
  const px = new Uint8Array(OUT * OUT * 4);
  // 每输出像素取 SS×SS 子样本，形状按顺序覆盖（后画盖前画）
  for (let py = 0; py < OUT; py++) {
    for (let pxx = 0; pxx < OUT; pxx++) {
      let r = 0, g = 0, b = 0, a = 0;
      for (let sy = 0; sy < SS; sy++) {
        for (let sx = 0; sx < SS; sx++) {
          const x = pxx + (sx + 0.5) / SS;
          const y = py + (sy + 0.5) / SS;
          // 从最上层往下找第一个命中的形状
          let cr = 0, cg = 0, cb = 0, ca = 0;
          for (let i = shapes.length - 1; i >= 0; i--) {
            if (inside(shapes[i], x, y)) {
              const c = shapes[i].c;
              cr = c[0]; cg = c[1]; cb = c[2]; ca = 255;
              break;
            }
          }
          r += cr; g += cg; b += cb; a += ca;
        }
      }
      const n = SS * SS;
      const o = (py * OUT + pxx) * 4;
      px[o] = Math.round(r / n);
      px[o + 1] = Math.round(g / n);
      px[o + 2] = Math.round(b / n);
      px[o + 3] = Math.round(a / n);
    }
  }
  return px;
}

// ── PNG 编码（colorType 6, 8bit）──
const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();
function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}
function encodePNG(rgba, w, h) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8; ihdr[9] = 6; ihdr[10] = 0; ihdr[11] = 0; ihdr[12] = 0;
  const raw = Buffer.alloc(h * (w * 4 + 1));
  for (let y = 0; y < h; y++) {
    raw[y * (w * 4 + 1)] = 0; // filter none
    rgba.copy ? rgba.copy(raw, y * (w * 4 + 1) + 1, y * w * 4, (y + 1) * w * 4)
             : Buffer.from(rgba.buffer, y * w * 4, w * 4).copy(raw, y * (w * 4 + 1) + 1);
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', zlib.deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

// ── 生成 ──
console.log('渲染悬浮透明版…');
const alpha = render(SHAPES_FLOAT);
fs.writeFileSync('assets/logo-mark-alpha-1024.png', encodePNG(alpha, OUT, OUT));

console.log('渲染圆角底版…');
const tile = render(SHAPES_TILE);
fs.writeFileSync('assets/logo-mark-tile-1024.png', encodePNG(tile, OUT, OUT));

// 256px 应用版（箱式下采样）
console.log('生成 256px 应用版…');
const small = new Uint8Array(256 * 256 * 4);
const f = 4;
for (let y = 0; y < 256; y++) {
  for (let x = 0; x < 256; x++) {
    let r = 0, g = 0, b = 0, a = 0;
    for (let sy = 0; sy < f; sy++) {
      for (let sx = 0; sx < f; sx++) {
        const o = ((y * f + sy) * OUT + (x * f + sx)) * 4;
        r += alpha[o]; g += alpha[o + 1]; b += alpha[o + 2]; a += alpha[o + 3];
      }
    }
    const n = f * f, o2 = (y * 256 + x) * 4;
    small[o2] = Math.round(r / n);
    small[o2 + 1] = Math.round(g / n);
    small[o2 + 2] = Math.round(b / n);
    small[o2 + 3] = Math.round(a / n);
  }
}
fs.writeFileSync('ui/icons/app-mark.png', encodePNG(small, 256, 256));

// 校验：中心紫块采样
const cx = 524, cy = 514;
const o = (cy * OUT + cx) * 4;
console.log('采样紫块像素:', alpha[o], alpha[o + 1], alpha[o + 2], alpha[o + 3]);
console.log('DONE: alpha-1024 / tile-1024 / app-mark-256');
