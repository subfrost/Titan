use {
    self::{ImageRendering::*, Media::*},
    brotli::enc::backward_references::BrotliEncoderMode::{
        self, BROTLI_MODE_GENERIC as GENERIC, BROTLI_MODE_TEXT as TEXT,
    },
    std::io::Error,
    std::str::FromStr,
};

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum Media {
    Iframe,
    Image(ImageRendering),
    Unknown,
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum ImageRendering {
    Auto,
    Pixelated,
}

impl Media {
    #[rustfmt::skip]
    const TABLE: &'static [(&'static str, BrotliEncoderMode, Media, &'static [&'static str])] = &[
    ("image/apng",                  GENERIC, Image(Pixelated), &["apng"]),
    ("image/avif",                  GENERIC, Image(Auto),      &["avif"]),
    ("image/gif",                   GENERIC, Image(Pixelated), &["gif"]),
    ("image/jpeg",                  GENERIC, Image(Pixelated), &["jpg", "jpeg"]),
    ("image/jxl",                   GENERIC, Image(Auto),      &[]),
    ("image/png",                   GENERIC, Image(Pixelated), &["png"]),
    ("image/svg+xml",               TEXT,    Iframe,           &["svg"]),
    ("image/webp",                  GENERIC, Image(Pixelated), &["webp"]),
  ];

    pub fn is_unknown(&self) -> bool {
        matches!(self, Media::Unknown)
    }
}

impl FromStr for Media {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for entry in Self::TABLE {
            if entry.0 == s {
                return Ok(entry.2);
            }
        }

        Ok(Media::Unknown)
    }
}
