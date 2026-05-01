use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

pub struct Filter {
    matcher: SkimMatcherV2,
}

impl Filter {
    pub fn new() -> Self {
        Self {
            matcher: SkimMatcherV2::default(),
        }
    }

    /// Returns the matched character indices within `text`, or `None` if no match.
    pub fn match_indices(&self, query: &str, text: &str) -> Option<Vec<usize>> {
        self.matcher
            .fuzzy_indices(text, query)
            .map(|(_, indices)| indices)
    }

    /// Returns the match score, or `None` if there is no match.
    pub fn score(&self, query: &str, text: &str) -> Option<i64> {
        self.matcher.fuzzy_match(text, query)
    }
}

impl Default for Filter {
    fn default() -> Self {
        Self::new()
    }
}
