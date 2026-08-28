// 从各家官网抓取最新 logo（apple-touch-icon / favicon，优先大尺寸 PNG）
import fs from 'node:fs';
import path from 'node:path';

const TARGETS = [
  { id: 'zhipu',          urls: ['https://open.bigmodel.cn', 'https://bigmodel.cn'] },
  { id: 'kimi',           urls: ['https://www.kimi.com', 'https://kimi.moonshot.cn'] },
  { id: 'deepseek',       urls: ['https://www.deepseek.com', 'https://api-docs.deepseek.com'] },
  { id: 'minimax',        urls: ['https://platform.minimaxi.com', 'https://www.minimax.io'] },
  { id: 'stepfun',        urls: ['https://platform.stepfun.com', 'https://www.stepfun.com'] },
  { id: 'siliconflow',    urls: ['https://cloud.siliconflow.cn', 'https://www.siliconflow.cn'] },
  { id: 'xiaomi',         urls: ['https://platform.xiaomimimo.com'] },
  { id: 'alibaba',        urls: ['https://www.aliyun.com', 'https://bailian.console.aliyun.com'] },
  { id: 'claude-official',urls: ['https://claude.ai'] },
  { id: 'codex',          urls: ['https://chatgpt.com', 'https://openai.com'] },
  { id: 'packycode',      urls: ['https://www.packyapi.ai'] },
];

const OUT = path.join(process.cwd(), 'ui', 'icons');
const UA = { headers: { 'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/126 Safari/537.36' } };

function abs(base, href) {
  try { return new URL(href, base).href; } catch { return null; }
}

async function fetchBuffer(url) {
  const res = await fetch(url, { ...UA, redirect: 'follow', signal: AbortSignal.timeout(15000) });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return Buffer.from(await res.arrayBuffer());
}

async function findIcons(pageUrl) {
  let html;
  try {
    const res = await fetch(pageUrl, { ...UA, redirect: 'follow', signal: AbortSignal.timeout(15000) });
    if (!res.ok) return [];
    html = await res.text();
  } catch { return []; }

  const found = [];
  const linkRe = /<link[^>]+rel=["'][^"']*(apple-touch-icon|icon|shortcut)[^"']*["'][^>]*>/gi;
  for (const m of html.matchAll(linkRe)) {
    const tag = m[0];
    const href = tag.match(/href=["']([^"']+)["']/i)?.[1];
    if (!href) continue;
    const sizes = tag.match(/sizes=["'](\d+)x\d+["']/i)?.[1];
    const isApple = /apple-touch-icon/i.test(tag);
    const isSvg = /\.svg(\?|$)/i.test(href);
    found.push({
      url: abs(pageUrl, href),
      score: (sizes ? parseInt(sizes) : 0) + (isApple ? 180 : 0) + (isSvg ? 500 : 0),
    });
  }
  // meta og:image 也常是正方形 logo
  const og = html.match(/<meta[^>]+property=["']og:image["'][^>]+content=["']([^"']+)["']/i)
          || html.match(/<meta[^>]+content=["']([^"']+)["'][^>]+property=["']og:image["']/i);
  if (og) found.push({ url: abs(pageUrl, og[1]), score: 100 });
  // 兜底：/favicon.ico
  found.push({ url: abs(pageUrl, '/favicon.ico'), score: 1 });
  return found.sort((a, b) => b.score - a.score);
}

function sniff(buf) {
  if (buf.length < 8) return null;
  if (buf[0] === 0x89 && buf[1] === 0x50) return 'png';
  if (buf[0] === 0xff && buf[1] === 0xd8) return 'jpg';
  if (buf.slice(0, 4).toString() === '<svg' || buf.includes('<svg')) return 'svg';
  if (buf.slice(0, 3).toString() === 'GIF') return 'gif';
  return null;
}

(async () => {
  for (const t of TARGETS) {
    let done = false;
    for (const pageUrl of t.urls) {
      if (done) break;
      try {
        const candidates = await findIcons(pageUrl);
        for (const c of candidates) {
          if (!c.url || done) continue;
          try {
            const buf = await fetchBuffer(c.url);
            const kind = sniff(buf);
            if (!kind || buf.length < 500) continue;
            if (kind === 'gif') continue;
            const ext = kind === 'svg' ? 'svg' : kind === 'jpg' ? 'jpg' : 'png';
            fs.writeFileSync(path.join(OUT, `${t.id}.${ext}`), buf);
            // 清理该 id 的旧格式文件，避免多份并存
            for (const old of ['png', 'jpg', 'svg']) {
              if (old !== ext) fs.rmSync(path.join(OUT, `${t.id}.${old}`), { force: true });
            }
            console.log(`${t.id}: ${ext} ${buf.length}B  <- ${c.url}`);
            done = true;
            break;
          } catch { /* 试下一个候选 */ }
        }
      } catch { /* 试下一个站点 */ }
    }
    if (!done) console.log(`${t.id}: !! 全部来源失败，保留现有图标`);
  }
})();
