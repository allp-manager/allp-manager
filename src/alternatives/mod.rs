use crate::domain::IdentityMetadata;
use std::collections::HashSet;

#[derive(Debug, Clone, Default)]
pub struct AlternativeSearchRequest {
    pub query: String,
    pub software_identity: Option<IdentityMetadata>,
    pub excluded_backends: HashSet<String>,
}

impl AlternativeSearchRequest {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            software_identity: None,
            excluded_backends: HashSet::new(),
        }
    }

    pub fn exclude(&mut self, backend_id: impl Into<String>) {
        self.excluded_backends.insert(backend_id.into());
    }

    pub fn is_excluded(&self, backend_id: &str) -> bool {
        self.excluded_backends
            .iter()
            .any(|excluded| excluded.eq_ignore_ascii_case(backend_id))
    }

    pub fn unrestricted(&mut self) {
        self.excluded_backends.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::AlternativeSearchRequest;

    #[test]
    fn exclusions_are_case_insensitive_and_resettable() {
        let mut request = AlternativeSearchRequest::new("editor");
        request.exclude("snap");
        assert!(request.is_excluded("Snap"));
        request.unrestricted();
        assert!(!request.is_excluded("snap"));
    }
}
