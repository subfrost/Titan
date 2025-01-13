use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Pagination {
    #[serde(default = "default_skip")]
    pub skip: u64,
    #[serde(default = "default_limit")]
    pub limit: u64,
}

impl Default for Pagination {
    fn default() -> Self {
        Pagination {
            skip: 0,
            limit: u64::MAX,
        }
    }
}

fn default_skip() -> u64 {
    0
}

fn default_limit() -> u64 {
    u64::MAX
}

impl Into<Pagination> for (u64, u64) {
    fn into(self) -> Pagination {
        Pagination {
            skip: self.0,
            limit: self.1,
        }
    }
}

impl Into<(u64, u64)> for Pagination {
    fn into(self) -> (u64, u64) {
        (self.skip, self.limit)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationResponse<T> {
    pub items: Vec<T>,
    pub offset: u64,
}
