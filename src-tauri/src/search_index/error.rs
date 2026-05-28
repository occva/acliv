#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchIndexError {
    NotFound(String),
    Internal(String),
}

impl SearchIndexError {
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    pub fn message(&self) -> &str {
        match self {
            Self::NotFound(message) | Self::Internal(message) => message,
        }
    }
}

impl std::fmt::Display for SearchIndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl From<String> for SearchIndexError {
    fn from(message: String) -> Self {
        Self::Internal(message)
    }
}

impl From<&str> for SearchIndexError {
    fn from(message: &str) -> Self {
        Self::Internal(message.to_string())
    }
}
