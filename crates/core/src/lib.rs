//! # dogubako-core
//!
//! The shared foundation of Dogubako: declarative tool specifications, the
//! registry, parameter validation, JSON-schema generation, dependency and
//! model management, security policies and the safe execution engine.
//!
//! Every interface — desktop app, web UI, CLI, HTTP API and MCP — runs tools
//! through [`engine::Engine::run`], so validation and authorization are
//! enforced in exactly one place.

pub mod deps;
pub mod engine;
pub mod engines;
pub mod error;
pub mod llm;
pub mod manifest;
pub mod models;
pub mod net;
pub mod params;
pub mod paths;
pub mod policy;
pub mod process;
pub mod registry;
pub mod schema;
pub mod spec;
pub mod testing;
pub mod tool;
pub mod util;

/// Everything a tool implementation usually needs.
pub mod prelude {
    pub use crate::error::{Context, ToolError, ToolResult};
    pub use crate::params::{InputFile, Params};
    pub use crate::spec::{
        Category, Execution, OutputKind, ParamSpec, PathKind, Permission, Platform, ToolSpec,
    };
    pub use crate::tool::{
        async_tool, sync_tool, Artifact, Environment, Tool, ToolContext, ToolOutput,
    };
    pub use crate::util::{human_size, suffixed, with_extension};
    pub use serde_json::{json, Value};
    pub use std::sync::Arc;
}
