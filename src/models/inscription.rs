use {
    super::Media,
    crate::db::Entry,
    borsh::{BorshDeserialize, BorshSerialize},
    core::str,
    http::HeaderValue,
};

#[derive(Debug, PartialEq, Clone, Eq, Default, BorshSerialize, BorshDeserialize)]
pub struct Inscription {
    pub body: Option<Vec<u8>>,
    pub content_encoding: Option<Vec<u8>>,
    pub content_type: Option<Vec<u8>>,
}

impl Inscription {
    pub fn content_type(&self) -> Option<&str> {
        str::from_utf8(self.content_type.as_ref()?).ok()
    }

    pub fn content_encoding(&self) -> Option<HeaderValue> {
        HeaderValue::from_str(str::from_utf8(self.content_encoding.as_ref()?).unwrap_or_default())
            .ok()
    }

    pub fn media(&self) -> Media {
        if self.body.is_none() {
            return Media::Unknown;
        }

        let Some(content_type) = self.content_type() else {
            return Media::Unknown;
        };

        content_type.parse().unwrap_or(Media::Unknown)
    }

    pub fn into_body(self) -> Option<Vec<u8>> {
        self.body
    }
}

impl Entry for Inscription {}
