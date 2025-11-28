use crate::repository::ToolRepository;
use crate::tools::{Tool, ToolSearchStrategy};
use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use std::collections::HashSet;
use std::sync::Arc;

/// Simple tag/description based search that rewards tag matches and keyword overlap.
pub struct TagSearchStrategy {
    tool_repository: Arc<dyn ToolRepository>,
    description_weight: f64,
    word_regex: Regex,
}

impl TagSearchStrategy {
    /// Build a new tag search strategy with configurable description weight.
    pub fn new(repo: Arc<dyn ToolRepository>, description_weight: f64) -> Self {
        Self {
            tool_repository: repo,
            description_weight,
            word_regex: Regex::new(r"\w+").unwrap(),
        }
    }
}

struct ScoredTool {
    tool: Tool,
    score: f64,
}

#[async_trait]
impl ToolSearchStrategy for TagSearchStrategy {
    /// Score tools by tags and description keywords and return the best matches.
    async fn search_tools(&self, query: &str, limit: usize) -> Result<Vec<Tool>> {
        let query_lower = query.trim().to_lowercase();
        let words: Vec<String> = self
            .word_regex
            .find_iter(&query_lower)
            .map(|m| m.as_str().to_string())
            .collect();

        let query_word_set: HashSet<String> = words.into_iter().collect();

        let tools = self.tool_repository.get_tools().await?;
        let mut scored: Vec<ScoredTool> = Vec::new();

        for t in tools {
            let mut score = 0.0;

            // Match against tags
            for tag in &t.tags {
                let tag_lower = tag.to_lowercase();

                // Direct substring match
                if query_lower.contains(&tag_lower) {
                    score += 1.0;
                }

                // Word-level overlap
                for m in self.word_regex.find_iter(&tag_lower) {
                    if query_word_set.contains(m.as_str()) {
                        score += self.description_weight;
                    }
                }
            }

            // Match against description
            let desc_lower = t.description.to_lowercase();
            for m in self.word_regex.find_iter(&desc_lower) {
                let w = m.as_str();
                if w.len() > 2 && query_word_set.contains(w) {
                    score += self.description_weight;
                }
            }

            scored.push(ScoredTool { tool: t, score });
        }

        // Sort descending by score
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut result = Vec::new();
        for st in scored.iter() {
            if st.score > 0.0 {
                result.push(st.tool.clone());
                if limit > 0 && result.len() >= limit {
                    break;
                }
            }
        }

        // If no matches, fallback to top N (for discoverability)
        if result.is_empty() && !scored.is_empty() {
            for (i, st) in scored.iter().enumerate() {
                if limit > 0 && i >= limit {
                    break;
                }
                result.push(st.tool.clone());
            }
        }

        Ok(result)
    }
}
