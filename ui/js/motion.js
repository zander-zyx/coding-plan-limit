// ============================================================
// Plan Limit — GSAP 动效层（主窗口 + 弹窗共享）
// 规范：动效须有语义（数据到达 / 视图切换 / 换色响应 / 弹层开关），
// prefers-reduced-motion 时全部瞬时；gsap 未加载时降级为直接设值。
// ============================================================
(function () {
  const hasGsap = typeof window.gsap === 'object' && !!window.gsap;
  const reduced = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  // count-up 文本格式（与 rowHtml 服务端渲染保持一致）
  const FMT = {
    pct0: (v) => `${Math.round(v)}%`,
    pct1: (v) => `${v.toFixed(1)}%`,
    cny: (v) => `¥${v.toFixed(2)}`,
    usd: (v) => `$${v.toFixed(2)}`,
  };

  let hueTween = null;

  window.Motion = {
    /** 行入场瀑布：内容逐行到达（transform 不参与布局，popup 测高安全） */
    rowsIn(container) {
      if (!hasGsap || reduced()) return;
      const rows = container.querySelectorAll('.row');
      if (!rows.length) return;
      gsap.fromTo(rows,
        { autoAlpha: 0, y: 6 },
        { autoAlpha: 1, y: 0, duration: 0.26, ease: 'power3.out', stagger: 0.03, overwrite: 'auto' });
    },

    /** 数据到达：进度条填充与数字滚动同步驱动 */
    dataIn(container) {
      const bars = container.querySelectorAll('.bar > i[data-w]');
      if (!hasGsap || reduced()) {
        bars.forEach((el) => { el.style.width = el.dataset.w + '%'; el.removeAttribute('data-w'); });
        return; // 数字已是服务端渲染的最终值
      }
      bars.forEach((el) => {
        gsap.fromTo(el, { width: '0%' },
          { width: el.dataset.w + '%', duration: 0.7, ease: 'power3.out' });
        el.removeAttribute('data-w');
      });
      container.querySelectorAll('[data-count]').forEach((el) => {
        const target = parseFloat(el.dataset.count);
        const fmt = FMT[el.dataset.fmt];
        if (!Number.isFinite(target) || !fmt) return;
        el.textContent = fmt(0); // 防"最终值闪一帧再跳 0"
        const o = { v: 0 };
        gsap.to(o, {
          v: target, duration: 0.9, ease: 'power2.out',
          onUpdate: () => { el.textContent = fmt(o.v); },
        });
      });
    },

    /** 视图切换入场 */
    viewIn(section) {
      if (!hasGsap || reduced() || !section || section.hidden) return;
      gsap.fromTo(section,
        { autoAlpha: 0, y: 4 },
        { autoAlpha: 1, y: 0, duration: 0.22, ease: 'power2.out', overwrite: 'auto' });
    },

    /** 弹层打开：微弹入场（back.out 低强度，克制不夸张） */
    modalIn(modal) {
      if (!modal || !hasGsap || reduced()) return;
      gsap.killTweensOf(modal);
      gsap.fromTo(modal,
        { y: 10, scale: 0.98 },
        { y: 0, scale: 1, duration: 0.32, ease: 'back.out(1.4)', overwrite: 'auto' });
    },

    /** 弹层关闭：快退后回调清表单；返回 false 表示未接管（调用方同步清理） */
    modalOut(modal, onDone) {
      if (!modal || !hasGsap || reduced()) return false;
      gsap.killTweensOf(modal);
      gsap.to(modal, {
        y: 8, scale: 0.985, duration: 0.16, ease: 'power2.in',
        onComplete: () => onDone && onDone(),
      });
      return true;
    },

    /** 主题色切换：--brand-hue 走最短环径平滑过渡，全站派生色同步渐变 */
    hueTo(hue) {
      const root = document.documentElement;
      if (!hasGsap || reduced()) {
        root.style.setProperty('--brand-hue', String(hue));
        return;
      }
      const cur = parseFloat(root.style.getPropertyValue('--brand-hue'));
      const from = Number.isFinite(cur) ? cur : 252;
      const delta = ((hue - from + 540) % 360) - 180;
      if (hueTween) hueTween.kill();
      const o = { h: from };
      hueTween = gsap.to(o, {
        h: from + delta, duration: 0.5, ease: 'power2.inOut',
        onUpdate: () => root.style.setProperty('--brand-hue', String(Math.round(o.h))),
      });
    },
  };
})();
