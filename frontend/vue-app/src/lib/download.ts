export interface DownloadTextFileOptions {
  content: BlobPart | BlobPart[];
  filename: string;
  mimeType: string;
}

export function downloadTextFile(options: DownloadTextFileOptions): void {
  if (typeof document === 'undefined') {
    throw new Error('当前环境不支持文件下载');
  }

  const contentParts = Array.isArray(options.content) ? options.content : [options.content];
  const blob = new Blob(contentParts, { type: options.mimeType });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');

  try {
    anchor.href = url;
    anchor.download = options.filename;
    anchor.style.display = 'none';
    document.body.appendChild(anchor);
    anchor.click();
  } finally {
    if (anchor.parentNode) {
      anchor.parentNode.removeChild(anchor);
    }
    URL.revokeObjectURL(url);
  }
}
