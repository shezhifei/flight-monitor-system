/**
 * 由能力快照动态派生「文件选择 accept 白名单」。
 *
 * 输入来源：
 *  - model.input modalities（如 ['text','image','audio','file']）
 *  - security.allowed_input_mime_types（如 ['image/png','audio/wav']）
 *
 * 规则：
 *  - text-only 模型（输入模态仅含 text 或为空）禁用图片/音频/文件上传。
 *  - accept 仅保留与启用模态匹配的 MIME 类型；audio 模态额外放行 audio/*，
 *    image 模态放行 image/*，file 模态放行任意已声明的非 text MIME。
 *  - text/plain 不计入上传 accept（文本走 textarea，不需要文件选择）。
 *
 * 纯函数，无副作用，便于单测。
 */

export type InputModality = 'text' | 'image' | 'audio' | 'file' | string;

export interface AcceptDerivation {
  /** 是否允许文件上传（false → 仅 textarea 文本输入）。 */
  uploadEnabled: boolean;
  /** 直接用于 <input accept="..."> 的字符串（逗号分隔）；uploadEnabled=false 时为空串。 */
  accept: string;
  /** 解析出的、去重后的 MIME 类型列表。 */
  mimeTypes: string[];
}

const TEXT_MIME_PREFIX = 'text/';

function modalityForMime(mime: string): InputModality | null {
  const lower = mime.toLowerCase();
  if (lower.startsWith('image/')) return 'image';
  if (lower.startsWith('audio/')) return 'audio';
  if (lower.startsWith('video/')) return 'file';
  if (lower.startsWith(TEXT_MIME_PREFIX)) return 'text';
  return 'file';
}

/**
 * 根据启用的输入模态过滤 allowed MIME 类型，得到上传 accept 白名单。
 */
export function deriveInputAccept(
  inputModalities: InputModality[] | null | undefined,
  allowedMimeTypes: string[] | null | undefined,
): AcceptDerivation {
  const modalities = new Set(
    (inputModalities || [])
      .map((m) => String(m || '').trim().toLowerCase())
      .filter(Boolean),
  );

  // 非文本上传模态：image / audio / file 任一存在才允许上传。
  const uploadModalities = ['image', 'audio', 'file'].filter((m) => modalities.has(m));
  if (uploadModalities.length === 0) {
    return { uploadEnabled: false, accept: '', mimeTypes: [] };
  }

  const seen = new Set<string>();
  const mimeTypes: string[] = [];
  for (const raw of allowedMimeTypes || []) {
    const mime = String(raw || '').trim();
    if (!mime || seen.has(mime)) continue;
    // text/* 不进入上传 accept（文本走 textarea）。
    if (mime.toLowerCase().startsWith(TEXT_MIME_PREFIX)) continue;
    const owningModality = modalityForMime(mime);
    // 仅保留与启用模态匹配的 MIME；'file' 模态作为兜底放行任意非文本 MIME。
    if (
      (owningModality && modalities.has(owningModality)) ||
      modalities.has('file')
    ) {
      seen.add(mime);
      mimeTypes.push(mime);
    }
  }

  // 若声明了模态但没有任何具体 MIME 命中，用通配回退保证最小可用。
  if (mimeTypes.length === 0) {
    if (modalities.has('image')) mimeTypes.push('image/*');
    if (modalities.has('audio')) mimeTypes.push('audio/*');
    // 'file' 单独存在时不放通配，避免无意义的“任意文件”。
  }

  return {
    uploadEnabled: mimeTypes.length > 0,
    accept: mimeTypes.join(','),
    mimeTypes,
  };
}
