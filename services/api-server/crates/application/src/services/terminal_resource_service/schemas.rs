//! 空间目录（航站楼/登机口/行李转盘）读写协议。

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TerminalCreate {
    /// 业务键，如 T1 / T2。
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TerminalUpdate {
    pub name: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct GateCreate {
    /// 目录行必须立即挂楼 —— 楼作为 create 的一等字段。
    pub terminal_id: String,
    /// 业务键，如 G-A01。
    pub code: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct GateUpdate {
    pub name: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CarouselCreate {
    /// 目录行必须立即挂楼。
    pub terminal_id: String,
    /// 业务键，如 B1。
    pub code: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CarouselUpdate {
    pub name: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TerminalListQuery {
    pub include_inactive: Option<bool>,
}