//! AI 媒体模型 — ASR（语音识别）与 TTS（文本转语音）
//!
//! 定义语音转写和声音合成相关的领域数据结构。

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ASR — 自动语音识别 (Automatic Speech Recognition)
// ---------------------------------------------------------------------------

/// ASR 转写请求参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrParams {
    /// 使用的 ASR 模型名称，如 "whisper-1"
    #[serde(default = "default_asr_model")]
    pub model: String,

    /// 输入音频的语言（ISO-639-1），如 "zh"、"en"。为空时自动检测。
    pub language: Option<String>,

    /// 引导转写的可选提示文本（可包含专业术语以提升准确率）
    pub prompt: Option<String>,

    /// 响应格式: "json", "text", "srt", "verbose_json", "vtt"
    #[serde(default = "default_asr_response_format")]
    pub response_format: String,

    /// 采样温度 (0.0 ~ 1.0)，较低值更确定
    pub temperature: Option<f64>,
}

impl Default for AsrParams {
    fn default() -> Self {
        Self {
            model: default_asr_model(),
            language: None,
            prompt: None,
            response_format: default_asr_response_format(),
            temperature: None,
        }
    }
}

/// ASR 转写结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrResult {
    /// 转写出的文本
    pub text: String,

    /// 检测到的语言代码
    pub language: Option<String>,

    /// 音频时长（毫秒）
    pub duration_ms: Option<u64>,

    /// 分段转写结果（verbose_json 模式下可用）
    pub segments: Option<Vec<AsrSegment>>,

    /// 使用的模型
    pub model: String,
}

/// ASR 分段信息（对应 verbose_json 中的 segment）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrSegment {
    pub id: u32,
    pub start: f64,
    pub end: f64,
    pub text: String,
}

// ---------------------------------------------------------------------------
// TTS — 文本转语音 (Text-to-Speech)
// ---------------------------------------------------------------------------

/// TTS 合成请求参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsParams {
    /// 使用的 TTS 模型名称，如 "tts-1", "tts-1-hd"
    #[serde(default = "default_tts_model")]
    pub model: String,

    /// 声音标识，如 "alloy", "echo", "fable", "onyx", "nova", "shimmer"
    #[serde(default = "default_tts_voice")]
    pub voice: String,

    /// 输出音频格式: "mp3", "opus", "aac", "flac", "wav", "pcm"
    #[serde(default = "default_tts_format")]
    pub response_format: String,

    /// 语速倍率 (0.25 ~ 4.0)，默认 1.0
    #[serde(default = "default_tts_speed")]
    pub speed: f64,
}

impl Default for TtsParams {
    fn default() -> Self {
        Self {
            model: default_tts_model(),
            voice: default_tts_voice(),
            response_format: default_tts_format(),
            speed: default_tts_speed(),
        }
    }
}

/// TTS 合成结果
#[derive(Debug, Clone)]
pub struct TtsResult {
    /// 合成的音频二进制数据
    pub audio_data: Vec<u8>,

    /// 输出音频格式
    pub format: String,

    /// 音频 MIME 类型
    pub content_type: String,
}

// ---------------------------------------------------------------------------
// 能力与元数据
// ---------------------------------------------------------------------------

/// AI 媒体能力类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiMediaCapability {
    /// 语音转写
    Asr,
    /// 文本合成语音
    Tts,
}

/// 可用的 TTS 声音信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub preview_available: bool,
}

/// 支持的音频格式信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportedFormats {
    /// ASR 支持的输入格式
    pub asr_input: Vec<String>,
    /// ASR 支持的输出格式
    pub asr_output: Vec<String>,
    /// TTS 支持的输出格式
    pub tts_output: Vec<String>,
}

impl Default for SupportedFormats {
    fn default() -> Self {
        Self {
            asr_input: vec![
                "mp3".into(),
                "mp4".into(),
                "mpeg".into(),
                "mpga".into(),
                "m4a".into(),
                "wav".into(),
                "webm".into(),
                "ogg".into(),
                "flac".into(),
            ],
            asr_output: vec![
                "json".into(),
                "text".into(),
                "srt".into(),
                "verbose_json".into(),
                "vtt".into(),
            ],
            tts_output: vec![
                "mp3".into(),
                "opus".into(),
                "aac".into(),
                "flac".into(),
                "wav".into(),
                "pcm".into(),
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// 默认值函数
// ---------------------------------------------------------------------------

fn default_asr_model() -> String {
    "whisper-1".to_string()
}

fn default_asr_response_format() -> String {
    "json".to_string()
}

fn default_tts_model() -> String {
    "tts-1".to_string()
}

fn default_tts_voice() -> String {
    "alloy".to_string()
}

fn default_tts_format() -> String {
    "mp3".to_string()
}

fn default_tts_speed() -> f64 {
    1.0
}

// ---------------------------------------------------------------------------
// 辅助方法
// ---------------------------------------------------------------------------

impl TtsResult {
    /// 根据格式推断 MIME 类型
    pub fn mime_type_for_format(format: &str) -> &'static str {
        match format {
            "mp3" => "audio/mpeg",
            "opus" => "audio/opus",
            "aac" => "audio/aac",
            "flac" => "audio/flac",
            "wav" => "audio/wav",
            "pcm" => "audio/pcm",
            _ => "application/octet-stream",
        }
    }
}

/// 获取所有内建 TTS 声音列表
pub fn builtin_voices() -> Vec<VoiceInfo> {
    vec![
        VoiceInfo {
            id: "alloy".into(),
            name: "Alloy".into(),
            description: "中性、平衡的声音".into(),
            preview_available: false,
        },
        VoiceInfo {
            id: "echo".into(),
            name: "Echo".into(),
            description: "温暖、清晰的男声".into(),
            preview_available: false,
        },
        VoiceInfo {
            id: "fable".into(),
            name: "Fable".into(),
            description: "富有表现力的英式声音".into(),
            preview_available: false,
        },
        VoiceInfo {
            id: "onyx".into(),
            name: "Onyx".into(),
            description: "深沉、有力的男声".into(),
            preview_available: false,
        },
        VoiceInfo {
            id: "nova".into(),
            name: "Nova".into(),
            description: "年轻、活力的女声".into(),
            preview_available: false,
        },
        VoiceInfo {
            id: "shimmer".into(),
            name: "Shimmer".into(),
            description: "柔和、温暖的女声".into(),
            preview_available: false,
        },
    ]
}
