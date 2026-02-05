use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Meta {
    pub paging: Paging,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paging {
    pub page: i64,
    pub size: i64,
    pub total_items: i64,
    pub total_pages: i64,
}

#[derive(Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub meta: Meta,
    pub data: Vec<T>,
}

#[derive(Serialize, Deserialize)]
pub struct Response<T> {
    pub data: T,
}
