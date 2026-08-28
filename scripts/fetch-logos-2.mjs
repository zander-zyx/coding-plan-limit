// 第二轮：定向抓取失败/抓错的 logo，PNG/JPG 做尺寸方形校验（拒收横幅图）
import fs from 'node:fs';
import path from 'node:path';

const OUT = path.join(process.cwd(), 'ui', 'icons');
const UA = { headers: { 'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/126 Safari/537.36' } };

const TARGETS = [
  { id: 'deepseek', urls: ['https://www.deepseek.com/favicon.ico', 'https://api-docs.deepseek.com/favicon.ico', 'https://www.google.com/s2/favicons?domain=deepseek.com&sz=128'] },
  { id: 'xiaomi', urls: ['https://platform.xiaomimimo.com/favicon.ico', 'https://www.google.com/s2/favicons?domain=xiaomimimo.com&sz=128'] },
  { id: 'claude-official', urls: ['https://claude.ai/images/claude_app_icon.png', 'https://www.google.com/s2/favicons?domain=claude.ai&sz=128'] },
  { id: 'codex', urls: ['https://chatgpt.com/favicon.ico', 'https://www.google.com/s2/favicons?domain=chatgpt.com&sz=128', 'https://www.google.com/s2/favicons?domain=openai.com&sz=128'] },
  { id: 'siliconflow', urls: ['https://cloud.siliconflow.cn/favicon.ico', 'https://www.google.com/s2/favicons?domain=siliconflow.cn&sz=128'] },
  { id: 'zhipu', urls: ['https://www.google.com/s2/favicons?domain=bigmodel.cn&sz=128'] },
];

// PNG: IHDR 宽高在 16..24；JPG: 扫 SOF0/2 段
function pngSize(b) { return b.length > 24 && b[0] === 0x89 ? { w: b.readUInt32BE(16), h: b.readUInt32BE(20) } : null; }
function jpgSize(b) {
  let i = 2;
  while (i + 9 < b.length) {
    if (b[i] !== 0xff) { i++; continue; }
    const marker = b[i + 1];
    if (marker >= 0xc0 && marker <= 0xcf && marker !== 0xc4 && marker !== 0xc8 && marker !== 0xcc) {
      return { h: b.readUInt16BE(i + 5), w: b.readUInt16BE(i + 7) };
    }
    i += 2 + b.readUInt16BE(i + 2);
  }
  return null;
}
function squareEnough(size) {
  if (!size) return true; // 无法判尺寸（如 ico/svg）则放行
  const { w, h } = size;
  if (w < 48 || h < 48) return false;
  const r = Math.min(w, h) / Math.max(w, h);
  return r > 0.7; // 拒收横幅
}

async function grab(url) {
  const res = await fetch(url, { ...UA, redirect: 'follow', signal: AbortSignal.timeout(15000) });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const buf = Buffer.from(await res.arrayBuffer());
  if (buf.length < 300) throw new Error('too small');
  const kind = buf[0] === 0x89 ? 'png' : buf[0] === 0xff ? 'jpg' : /<svg/.test(buf.slice(0, 200).toString()) ? 'svg' : buf.slice(0, 4).toString() === 'RIFF' ? 'ico' : buf.slice(0, 4).includes(0x00) ? 'ico' : null;
  if (!kind || kind === 'ico') { if (kind === 'ico') { fs.writeFileSync(path.join(OUT, tmpName), buf); return { ext: 'ico-skip' }; } throw new Error('unknown format'); }
  const size = kind === 'png' ? pngSize(buf) : jpgSize(buf);
  if (!squareEnough(size)) throw new Error(`非方形 ${size?.w}x${size?.h}`);
  return { buf, ext: kind === 'svg' ? 'svg' : kind === 'jpg' ? 'jpg' : 'png', size };
}
let tmpName = '_tmp';

for (const t of TARGETS) {
  let ok = false;
  for (const url of t.urls) {
    if (ok) break;
    try {
      const r = await grab(url);
      if (r.ext === 'ico-skip') { console.log(`${t.id}: ico 格式暂存 ${url}（可用但非首选）`); fs.copyFileSync(path.join(OUT, '_tmp'), path.join(OUT, `${t.id}.ico`)); fs.rmSync(path.join(OUT, '_tmp'), { force: true }); ok = true; continue; }
      for (const old of ['png', 'jpg', 'svg']) fs.rmSync(path.join(OUT, `${t.id}.${old}`), { force: true });
      fs.writeFileSync(path.join(OUT, `${t.id}.${r.ext}`), r.buf);
      console.log(`${t.id}: ${r.ext} ${r.buf.length}B ${JSON.stringify(r.size)} <- ${url}`);
      ok = true;
    } catch (e) { console.log(`${t.id}: skip ${url} (${e.message})`); }
  }
  if (!ok) console.log(`${t.id}: !! 保留现有`);
}
