use http::HeaderValue;

#[derive(Default, Debug)]
pub struct AcceptEncoding(pub Option<String>);

impl AcceptEncoding {
    pub fn is_acceptable(&self, encoding: &HeaderValue) -> bool {
        let Ok(encoding) = encoding.to_str() else {
            return false;
        };

        self.0
            .clone()
            .unwrap_or_default()
            .split(',')
            .any(|value| value.split(';').next().unwrap_or_default().trim() == encoding)
    }
}
