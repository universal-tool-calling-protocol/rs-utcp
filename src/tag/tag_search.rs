use crate::repository::ToolRepository;
use crate::tools::{Tool, ToolSearchStrategy};
use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use std::cmp::Ordering;
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

#[derive(Clone)]
struct ScoredTool {
    tool: Tool,
    score: f64,
}

#[async_trait]
impl ToolSearchStrategy for TagSearchStrategy {
    /// Score tools by tags and description keywords and return the best matches.
    async fn search_tools(&self, query: &str, limit: usize) -> Result<Vec<Tool>> {
        let query_lower = query.trim().to_lowercase();
        let query_word_set: HashSet<String> = self
            .word_regex
            .find_iter(&query_lower)
            .map(|m| m.as_str().to_string())
            .collect();

        let tools = self.tool_repository.get_tools().await?;
        if tools.is_empty() {
            return Ok(Vec::new());
        }

        let mut positives = Vec::new();
        let mut nonpositives = Vec::new();

        for tool in tools {
            let score = self.score_tool(&tool, &query_lower, &query_word_set);
            let entry = ScoredTool { tool, score };
            if score > 0.0 {
                positives.push(entry);
            } else {
                nonpositives.push(entry);
            }
        }

        // Unbounded result set keeps full sort for correctness but skips zero-score tools unless necessary.
        if limit == 0 {
            if !positives.is_empty() {
                positives.sort_unstable_by(compare_scored);
                return Ok(positives.into_iter().map(|st| st.tool).collect());
            }

            nonpositives.sort_unstable_by(compare_scored);
            return Ok(nonpositives.into_iter().map(|st| st.tool).collect());
        }

        if !positives.is_empty() {
            take_top_n(&mut positives, limit);
            return Ok(positives.into_iter().map(|st| st.tool).collect());
        }

        if nonpositives.is_empty() {
            return Ok(Vec::new());
        }

        take_top_n(&mut nonpositives, limit);
        Ok(nonpositives.into_iter().map(|st| st.tool).collect())
    }
}

impl TagSearchStrategy {
    fn score_tool(&self, tool: &Tool, query_lower: &str, query_word_set: &HashSet<String>) -> f64 {
        let mut score = 0.0;

        for tag in &tool.tags {
            let tag_lower = tag.to_ascii_lowercase();

            if query_lower.contains(&tag_lower) {
                score += 1.0;
            }

            for m in self.word_regex.find_iter(&tag_lower) {
                if query_word_set.contains(m.as_str()) {
                    score += self.description_weight;
                }
            }
        }

        for m in self.word_regex.find_iter(&tool.description) {
            let word = m.as_str().to_ascii_lowercase();
            if word.len() > 2 && query_word_set.contains(&word) {
                score += self.description_weight;
            }
        }

        score
    }
}

fn compare_scored(a: &ScoredTool, b: &ScoredTool) -> Ordering {
    b.score
        .total_cmp(&a.score)
        .then_with(|| a.tool.name.cmp(&b.tool.name))
}

fn take_top_n(scored: &mut Vec<ScoredTool>, limit: usize) {
    if limit == 0 {
        scored.sort_unstable_by(compare_scored);
        return;
    }

    if scored.len() > limit {
        let pivot = limit - 1;
        scored.select_nth_unstable_by(pivot, compare_scored);
        scored.truncate(limit);
    }

    scored.sort_unstable_by(compare_scored);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::base::{BaseProvider, ProviderType};
    use crate::repository::in_memory::InMemoryToolRepository;
    use crate::tools::ToolInputOutputSchema;
    use std::sync::Arc;

    fn schema() -> ToolInputOutputSchema {
        ToolInputOutputSchema {
            type_: "object".to_string(),
            properties: None,
            required: None,
            description: None,
            title: None,
            items: None,
            enum_: None,
            minimum: None,
            maximum: None,
            format: None,
        }
    }

    fn make_tool(name: &str, description: &str, tags: &[&str]) -> Tool {
        Tool {
            name: name.to_string(),
            description: description.to_string(),
            inputs: schema(),
            outputs: schema(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            average_response_size: None,
            provider: None,
        }
    }

    async fn setup_repo(tools: Vec<Tool>) -> Arc<InMemoryToolRepository> {
        let repo = Arc::new(InMemoryToolRepository::new());
        let provider = Arc::new(BaseProvider {
            name: "test".to_string(),
            provider_type: ProviderType::Http,
            auth: None,
            allowed_communication_protocols: None,
        });
        repo.save_provider_with_tools(provider, tools)
            .await
            .unwrap();
        repo
    }

    #[tokio::test]
    async fn returns_top_scoring_tools_with_limit() {
        let repo = setup_repo(vec![
            make_tool(
                "p1.weather_primary",
                "Weather forecast endpoint",
                &["weather"],
            ),
            make_tool("p1.weather_backup", "Weather data service", &["climate"]),
            make_tool("p1.finance", "Stock price lookup", &["stocks"]),
        ])
        .await;

        let strategy = TagSearchStrategy::new(repo, 0.5);
        let results = strategy.search_tools("weather forecast", 2).await.unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "p1.weather_primary");
        assert_eq!(results[1].name, "p1.weather_backup");
    }

    #[tokio::test]
    async fn falls_back_when_no_positive_scores() {
        let repo = setup_repo(vec![
            make_tool("p1.alpha", "No overlap here", &["alpha"]),
            make_tool("p1.beta", "Still nothing useful", &["beta"]),
            make_tool("p1.gamma", "More unrelated content", &["gamma"]),
        ])
        .await;

        let strategy = TagSearchStrategy::new(repo, 1.0);
        let results = strategy.search_tools("nonsense", 2).await.unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "p1.alpha");
        assert_eq!(results[1].name, "p1.beta");
    }

    #[tokio::test]
    async fn ties_are_sorted_by_name_within_limit() {
        let repo = setup_repo(vec![
            make_tool("p1.alpha", "Math helper", &["math"]),
            make_tool("p1.beta", "Math helper", &["math"]),
            make_tool("p1.gamma", "Math helper", &["math"]),
        ])
        .await;

        let strategy = TagSearchStrategy::new(repo, 1.0);
        let results = strategy.search_tools("math", 2).await.unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "p1.alpha");
        assert_eq!(results[1].name, "p1.beta");
    }
}
