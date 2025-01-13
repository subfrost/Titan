use {
    super::accept_encoding::AcceptEncoding,
    crate::models::Inscription,
    brotli::Decompressor,
    http::{header, HeaderMap, HeaderValue},
    std::io::Read,
    thiserror::Error,
};

#[derive(Debug, Error)]
pub enum ContentError {
    #[error("invalid CSP origin: {0}")]
    InvalidCspOrigin(String),
    #[error("brotli error: {0}")]
    BrotliError(String),
    #[error("not acceptable")]
    NotAcceptable {
        accept_encoding: AcceptEncoding,
        content_encoding: HeaderValue,
    },
}

pub fn content_response(
    inscription: Inscription,
    accept_encoding: AcceptEncoding,
    csp_origin: Option<String>,
    decompress: bool,
) -> Result<Option<(HeaderMap, Vec<u8>)>, ContentError> {
    let mut headers = HeaderMap::new();

    match csp_origin {
        None => {
            headers.insert(
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_static(
                    "default-src 'self' 'unsafe-eval' 'unsafe-inline' data: blob:",
                ),
            );
            headers.append(
          header::CONTENT_SECURITY_POLICY,
          HeaderValue::from_static("default-src *:*/content/ *:*/blockheight *:*/blockhash *:*/blockhash/ *:*/blocktime *:*/r/ 'unsafe-eval' 'unsafe-inline' data: blob:"),
        );
        }
        Some(origin) => {
            let csp = format!("default-src {origin}/content/ {origin}/blockheight {origin}/blockhash {origin}/blockhash/ {origin}/blocktime {origin}/r/ 'unsafe-eval' 'unsafe-inline' data: blob:");
            headers.insert(
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_str(&csp)
                    .map_err(|err| ContentError::InvalidCspOrigin(err.to_string()))?,
            );
        }
    }

    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=1209600, immutable"),
    );

    headers.insert(
        header::CONTENT_TYPE,
        inscription
            .content_type()
            .and_then(|content_type| content_type.parse().ok())
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );

    if let Some(content_encoding) = inscription.content_encoding() {
        if accept_encoding.is_acceptable(&content_encoding) {
            headers.insert(header::CONTENT_ENCODING, content_encoding);
        } else if decompress && content_encoding == "br" {
            let Some(body) = inscription.into_body() else {
                return Ok(None);
            };

            let mut decompressed = Vec::new();

            Decompressor::new(body.as_slice(), 4096)
                .read_to_end(&mut decompressed)
                .map_err(|err| ContentError::BrotliError(err.to_string()))?;

            return Ok(Some((headers, decompressed)));
        } else {
            return Err(ContentError::NotAcceptable {
                accept_encoding,
                content_encoding,
            });
        }
    }

    let Some(body) = inscription.into_body() else {
        return Ok(None);
    };

    Ok(Some((headers, body)))
}
