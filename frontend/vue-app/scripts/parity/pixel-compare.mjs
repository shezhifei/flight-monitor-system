/**
 * Browser-free-ish pixel comparison helpers for parity visual gates.
 * Uses Chromium (via Playwright) canvas so no pngjs/pixelmatch dependency is required.
 */

/**
 * @param {import('playwright').Page} page
 * @param {Buffer} leftPng
 * @param {Buffer} rightPng
 * @returns {Promise<{ ratio: number, differing: number, total: number, width: number, height: number, sizeMismatch: boolean }>}
 */
export async function comparePngBuffers(page, leftPng, rightPng) {
  return page.evaluate(async ({ left, right }) => {
    async function decode(bytes) {
      const blob = new Blob([new Uint8Array(bytes)], { type: 'image/png' });
      const bitmap = await createImageBitmap(blob);
      const canvas = document.createElement('canvas');
      canvas.width = bitmap.width;
      canvas.height = bitmap.height;
      const ctx = canvas.getContext('2d', { willReadFrequently: true });
      ctx.drawImage(bitmap, 0, 0);
      const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
      bitmap.close();
      return imageData;
    }

    const a = await decode(left);
    const b = await decode(right);
    if (a.width !== b.width || a.height !== b.height) {
      return {
        ratio: 1,
        differing: a.width * a.height,
        total: a.width * a.height,
        width: a.width,
        height: a.height,
        sizeMismatch: true,
      };
    }

    const total = a.width * a.height;
    let differing = 0;
    const da = a.data;
    const db = b.data;
    // Treat alpha-aware RGB distance with a small anti-aliasing threshold.
    for (let i = 0; i < da.length; i += 4) {
      const dr = Math.abs(da[i] - db[i]);
      const dg = Math.abs(da[i + 1] - db[i + 1]);
      const dbv = Math.abs(da[i + 2] - db[i + 2]);
      const daa = Math.abs(da[i + 3] - db[i + 3]);
      if (dr > 8 || dg > 8 || dbv > 8 || daa > 8) {
        differing += 1;
      }
    }
    return {
      ratio: total === 0 ? 0 : differing / total,
      differing,
      total,
      width: a.width,
      height: a.height,
      sizeMismatch: false,
    };
  }, {
    left: [...leftPng],
    right: [...rightPng],
  });
}

/**
 * Compare differently-sized full-page captures on their maximum shared canvas.
 * Pixels outside either source image count as different, so a page cannot hide
 * structural debt by becoming shorter. This is a progress metric only; final
 * parity still requires equal dimensions through comparePngBuffers().
 *
 * @param {import('playwright').Page} page
 * @param {Buffer} leftPng
 * @param {Buffer} rightPng
 * @returns {Promise<{ ratio: number, differing: number, total: number, width: number, height: number, sizeMismatch: boolean }>}
 */
export async function comparePngBuffersOnSharedCanvas(page, leftPng, rightPng) {
  return page.evaluate(async ({ left, right }) => {
    async function decode(bytes) {
      const blob = new Blob([new Uint8Array(bytes)], { type: 'image/png' });
      const bitmap = await createImageBitmap(blob);
      const canvas = document.createElement('canvas');
      canvas.width = bitmap.width;
      canvas.height = bitmap.height;
      const ctx = canvas.getContext('2d', { willReadFrequently: true });
      ctx.drawImage(bitmap, 0, 0);
      const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
      bitmap.close();
      return imageData;
    }

    const a = await decode(left);
    const b = await decode(right);
    const width = Math.max(a.width, b.width);
    const height = Math.max(a.height, b.height);
    const total = width * height;
    let differing = 0;

    for (let y = 0; y < height; y += 1) {
      for (let x = 0; x < width; x += 1) {
        if (x >= a.width || y >= a.height || x >= b.width || y >= b.height) {
          differing += 1;
          continue;
        }
        const aIndex = (y * a.width + x) * 4;
        const bIndex = (y * b.width + x) * 4;
        const dr = Math.abs(a.data[aIndex] - b.data[bIndex]);
        const dg = Math.abs(a.data[aIndex + 1] - b.data[bIndex + 1]);
        const db = Math.abs(a.data[aIndex + 2] - b.data[bIndex + 2]);
        const da = Math.abs(a.data[aIndex + 3] - b.data[bIndex + 3]);
        if (dr > 8 || dg > 8 || db > 8 || da > 8) differing += 1;
      }
    }

    return {
      ratio: total === 0 ? 0 : differing / total,
      differing,
      total,
      width,
      height,
      sizeMismatch: a.width !== b.width || a.height !== b.height,
    };
  }, {
    left: [...leftPng],
    right: [...rightPng],
  });
}

export const VISUAL_THRESHOLDS = Object.freeze({
  /** Desktop critical-region threshold from the parity plan. */
  region: 0.003,
  /** Full-page threshold from the parity plan. */
  fullPage: 0.01,
});
