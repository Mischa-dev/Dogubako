//! Tool registry and search.

use crate::spec::{Category, ToolSpec};
use crate::tool::Tool;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default)]
pub struct Registry {
    tools: Vec<Arc<dyn Tool>>,
    by_id: HashMap<String, usize>,
    /// Lower-cased searchable text per tool.
    haystack: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("duplicate tool id '{0}'")]
    Duplicate(String),
    #[error("invalid tool id '{0}' (expected lowercase dotted id like 'image.convert')")]
    InvalidId(String),
    #[error("tool '{0}': {1}")]
    InvalidSpec(String, String),
}

pub fn valid_tool_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('.').collect();
    parts.len() >= 2
        && id.len() <= 100
        && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_'))
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) -> Result<(), RegistryError> {
        let spec = tool.spec();
        let id = spec.id.clone();
        if !valid_tool_id(&id) {
            return Err(RegistryError::InvalidId(id));
        }
        if self.by_id.contains_key(&id) {
            return Err(RegistryError::Duplicate(id));
        }
        let mut names = std::collections::HashSet::new();
        for p in &spec.params {
            if !names.insert(p.name.as_str()) {
                return Err(RegistryError::InvalidSpec(id, format!("duplicate parameter '{}'", p.name)));
            }
            if !p.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
                return Err(RegistryError::InvalidSpec(id, format!("parameter '{}' must be snake_case", p.name)));
            }
            if let (Some(d), crate::spec::ParamKind::Enum { options, multi: false }) = (&p.default, &p.kind) {
                if !options.iter().any(|o| Some(o.value.as_str()) == d.as_str()) {
                    return Err(RegistryError::InvalidSpec(id, format!("default of '{}' is not an option", p.name)));
                }
            }
        }
        let hay = format!(
            "{} {} {} {} {} {}",
            spec.id,
            spec.name,
            spec.description,
            spec.keywords.join(" "),
            spec.category.id(),
            spec.category.label()
        )
        .to_lowercase();
        self.by_id.insert(id, self.tools.len());
        self.tools.push(tool);
        self.haystack.push(hay);
        Ok(())
    }

    /// Register many tools, panicking on programmer errors (builtins).
    pub fn register_all(&mut self, tools: Vec<Arc<dyn Tool>>) {
        for t in tools {
            if let Err(e) = self.register(t) {
                panic!("{e}");
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<&Arc<dyn Tool>> {
        self.by_id.get(id).map(|i| &self.tools[*i])
    }

    /// Look up by id or MCP-style name (`image_convert`).
    pub fn find(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.get(name).or_else(|| self.tools.iter().find(|t| t.spec().mcp_name() == name))
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn Tool>> {
        self.tools.iter()
    }

    pub fn specs(&self) -> impl Iterator<Item = &ToolSpec> {
        self.tools.iter().map(|t| t.spec())
    }

    pub fn by_category(&self, c: Category) -> impl Iterator<Item = &ToolSpec> {
        self.specs().filter(move |s| s.category == c)
    }

    /// Ranked search over id, name, keywords, description and category.
    pub fn search(&self, query: &str) -> Vec<&ToolSpec> {
        let terms: Vec<String> = query.to_lowercase().split_whitespace().map(|s| s.to_string()).collect();
        if terms.is_empty() {
            return self.specs().collect();
        }
        let mut scored: Vec<(i64, usize)> = vec![];
        for (i, t) in self.tools.iter().enumerate() {
            let s = t.spec();
            let hay = &self.haystack[i];
            let name = s.name.to_lowercase();
            let mut score = 0i64;
            let mut all = true;
            for term in &terms {
                if !hay.contains(term.as_str()) {
                    all = false;
                    break;
                }
                if s.id == *term || name == *term {
                    score += 100;
                }
                if name.split(|c: char| !c.is_alphanumeric()).any(|w| w == term) {
                    score += 40;
                } else if name.contains(term.as_str()) {
                    score += 25;
                }
                if s.id.contains(term.as_str()) {
                    score += 20;
                }
                if s.keywords.iter().any(|k| k.to_lowercase() == *term) {
                    score += 15;
                }
                if s.category.id() == term {
                    score += 10;
                }
                score += 1;
            }
            if all {
                scored.push((score, i));
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(self.tools[a.1].spec().id.cmp(&self.tools[b.1].spec().id)));
        scored.into_iter().map(|(_, i)| self.tools[i].spec()).collect()
    }
}
