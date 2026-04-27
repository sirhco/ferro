use serde::{Deserialize, Serialize};

use crate::{
    content::Status,
    id::{ContentTypeId, SiteId},
    locale::Locale,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContentQuery {
    pub site_id: Option<SiteId>,
    pub type_id: Option<ContentTypeId>,
    pub type_slug: Option<String>,
    pub slug: Option<String>,
    pub status: Option<Status>,
    pub locale: Option<Locale>,
    pub search: Option<String>,
    #[serde(default)]
    pub order: Vec<Order>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub field: String,
    pub dir: SortDir,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SortDir {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub per_page: u32,
}
