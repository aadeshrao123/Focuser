/**
 * Minimal canvas chart library for Focuser.
 * No dependencies — draws bar charts and line charts on <canvas>.
 */

const Charts = {
  colors: {
    blue: '#4e8fff',
    red: '#ef4444',
    green: '#22c55e',
    purple: '#a855f7',
    yellow: '#eab308',
    grid: '#2a2d3e',
    text: '#5c5f73',
    textLight: '#8b8fa3',
  },

  /**
   * Draw a bar chart.
   * @param {string} canvasId
   * @param {Object} data - { labels: string[], values: number[], color?: string }
   */
  bar(canvasId, data) {
    const canvas = document.getElementById(canvasId);
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();

    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);

    const w = rect.width;
    const h = rect.height;
    const padding = { top: 16, right: 16, bottom: 32, left: 44 };
    const chartW = w - padding.left - padding.right;
    const chartH = h - padding.top - padding.bottom;

    ctx.clearRect(0, 0, w, h);

    const maxVal = Math.max(...data.values, 1);
    const barColor = data.color || this.colors.blue;
    const barCount = data.labels.length;
    const barGap = Math.max(4, chartW * 0.08 / barCount);
    const barWidth = Math.max(8, (chartW - barGap * (barCount + 1)) / barCount);

    // Grid lines
    const gridLines = 4;
    ctx.strokeStyle = this.colors.grid;
    ctx.lineWidth = 1;
    ctx.font = '11px -apple-system, sans-serif';
    ctx.fillStyle = this.colors.text;
    ctx.textAlign = 'right';

    for (let i = 0; i <= gridLines; i++) {
      const y = padding.top + (chartH / gridLines) * i;
      const val = Math.round(maxVal - (maxVal / gridLines) * i);
      ctx.beginPath();
      ctx.moveTo(padding.left, y);
      ctx.lineTo(w - padding.right, y);
      ctx.stroke();
      ctx.fillText(val.toString(), padding.left - 8, y + 4);
    }

    // Bars
    ctx.textAlign = 'center';
    for (let i = 0; i < barCount; i++) {
      const x = padding.left + barGap + i * (barWidth + barGap);
      const barH = (data.values[i] / maxVal) * chartH;
      const y = padding.top + chartH - barH;

      // Bar fill with rounded top
      const radius = Math.min(4, barWidth / 2);
      ctx.fillStyle = barColor;
      ctx.beginPath();
      ctx.moveTo(x, y + radius);
      ctx.arcTo(x, y, x + radius, y, radius);
      ctx.arcTo(x + barWidth, y, x + barWidth, y + radius, radius);
      ctx.lineTo(x + barWidth, padding.top + chartH);
      ctx.lineTo(x, padding.top + chartH);
      ctx.closePath();
      ctx.fill();

      // Label
      ctx.fillStyle = this.colors.text;
      ctx.fillText(data.labels[i], x + barWidth / 2, h - padding.bottom + 16);
    }
  },

  /**
   * Draw a line chart.
   * @param {string} canvasId
   * @param {Object} data - { labels: string[], values: number[], color?: string }
   */
  line(canvasId, data) {
    const canvas = document.getElementById(canvasId);
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();

    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);

    const w = rect.width;
    const h = rect.height;
    const padding = { top: 16, right: 16, bottom: 32, left: 44 };
    const chartW = w - padding.left - padding.right;
    const chartH = h - padding.top - padding.bottom;

    ctx.clearRect(0, 0, w, h);

    const maxVal = Math.max(...data.values, 1);
    const lineColor = data.color || this.colors.blue;
    const pointCount = data.labels.length;

    // Grid
    const gridLines = 4;
    ctx.strokeStyle = this.colors.grid;
    ctx.lineWidth = 1;
    ctx.font = '11px -apple-system, sans-serif';
    ctx.fillStyle = this.colors.text;
    ctx.textAlign = 'right';

    for (let i = 0; i <= gridLines; i++) {
      const y = padding.top + (chartH / gridLines) * i;
      const val = Math.round(maxVal - (maxVal / gridLines) * i);
      ctx.beginPath();
      ctx.moveTo(padding.left, y);
      ctx.lineTo(w - padding.right, y);
      ctx.stroke();
      ctx.fillText(val.toString(), padding.left - 8, y + 4);
    }

    if (pointCount < 2) return;

    const stepX = chartW / (pointCount - 1);

    // Area fill
    ctx.beginPath();
    for (let i = 0; i < pointCount; i++) {
      const x = padding.left + i * stepX;
      const y = padding.top + chartH - (data.values[i] / maxVal) * chartH;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.lineTo(padding.left + (pointCount - 1) * stepX, padding.top + chartH);
    ctx.lineTo(padding.left, padding.top + chartH);
    ctx.closePath();
    const gradient = ctx.createLinearGradient(0, padding.top, 0, padding.top + chartH);
    gradient.addColorStop(0, lineColor + '30');
    gradient.addColorStop(1, lineColor + '05');
    ctx.fillStyle = gradient;
    ctx.fill();

    // Line
    ctx.beginPath();
    ctx.strokeStyle = lineColor;
    ctx.lineWidth = 2;
    ctx.lineJoin = 'round';
    for (let i = 0; i < pointCount; i++) {
      const x = padding.left + i * stepX;
      const y = padding.top + chartH - (data.values[i] / maxVal) * chartH;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();

    // Points
    for (let i = 0; i < pointCount; i++) {
      const x = padding.left + i * stepX;
      const y = padding.top + chartH - (data.values[i] / maxVal) * chartH;
      ctx.beginPath();
      ctx.arc(x, y, 3, 0, Math.PI * 2);
      ctx.fillStyle = lineColor;
      ctx.fill();
    }

    // X labels
    ctx.fillStyle = this.colors.text;
    ctx.textAlign = 'center';
    for (let i = 0; i < pointCount; i++) {
      const x = padding.left + i * stepX;
      ctx.fillText(data.labels[i], x, h - padding.bottom + 16);
    }
  },

  /**
   * Draw a horizontal bar chart (for top domains).
   * @param {string} canvasId
   * @param {Object} data - { labels: string[], values: number[], color?: string }
   */
  horizontalBar(canvasId, data) {
    const canvas = document.getElementById(canvasId);
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();

    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);

    const w = rect.width;
    const h = rect.height;
    const padding = { top: 8, right: 16, bottom: 8, left: 120 };
    const chartW = w - padding.left - padding.right;
    const chartH = h - padding.top - padding.bottom;

    ctx.clearRect(0, 0, w, h);

    const maxVal = Math.max(...data.values, 1);
    const barCount = data.labels.length;
    if (barCount === 0) return;

    const barH = Math.min(24, (chartH - 8 * barCount) / barCount);
    const barGap = (chartH - barH * barCount) / (barCount + 1);
    const colors = [this.colors.blue, this.colors.purple, this.colors.green, this.colors.yellow, this.colors.red];

    ctx.font = '12px -apple-system, sans-serif';
    ctx.textBaseline = 'middle';

    for (let i = 0; i < barCount; i++) {
      const y = padding.top + barGap + i * (barH + barGap);
      const barW = (data.values[i] / maxVal) * chartW;
      const color = colors[i % colors.length];

      // Label
      ctx.fillStyle = this.colors.textLight;
      ctx.textAlign = 'right';
      ctx.fillText(data.labels[i], padding.left - 10, y + barH / 2);

      // Bar
      const radius = Math.min(4, barH / 2);
      ctx.fillStyle = color;
      ctx.beginPath();
      ctx.moveTo(padding.left, y);
      ctx.lineTo(padding.left + barW - radius, y);
      ctx.arcTo(padding.left + barW, y, padding.left + barW, y + radius, radius);
      ctx.arcTo(padding.left + barW, y + barH, padding.left + barW - radius, y + barH, radius);
      ctx.lineTo(padding.left, y + barH);
      ctx.closePath();
      ctx.fill();

      // Value
      ctx.fillStyle = this.colors.textLight;
      ctx.textAlign = 'left';
      ctx.fillText(data.values[i].toString(), padding.left + barW + 8, y + barH / 2);
    }
  },
};
