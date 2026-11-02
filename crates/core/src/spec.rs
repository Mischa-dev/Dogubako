//! Declarative tool specifications.
//!
//! A [`ToolSpec`] fully describes a tool: identity, parameters, outputs,
//! platform support, dependencies, permissions and resource limits. Every
//! interface (desktop UI, web UI, CLI, HTTP API, MCP) is derived from it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Tool categories. Stable string ids are used everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Image,
    Video,
    Audio,
    Speech,
    Pdf,
    Files,
    Text,
    Dev,
    Data,
    Network,
    Design,
    System,
}

impl Category {
    pub const ALL: [Category; 12] = [
        Category::Image,
        Category::Video,
        Category::Audio,
        Category::Speech,
        Category::Pdf,
        Category::Files,
        Category::Text,
        Category::Dev,
        Category::Data,
        Category::Network,
        Category::Design,
        Category::System,
    ];

    pub fn id(&self) -> &'static str {
        match self {
            Category::Image => "image",
            Category::Video => "video",
            Category::Audio => "audio",
            Category::Speech => "speech",
            Category::Pdf => "pdf",
            Category::Files => "files",
            Category::Text => "text",
            Category::Dev => "dev",
            Category::Data => "data",
            Category::Network => "network",
            Category::Design => "design",
            Category::System => "system",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Category::Image => "Images",
            Category::Video => "Video",
            Category::Audio => "Audio",
            Category::Speech => "Speech & Voice",
            Category::Pdf => "PDF & Documents",
            Category::Files => "Files",
            Category::Text => "Text",
            Category::Dev => "Developer",
            Category::Data => "Data",
            Category::Network => "Network & Web",
            Category::Design => "Design",
            Category::System => "System",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Category::Image => "Convert, resize, compress, inspect and edit images",
            Category::Video => "Convert, compress, trim and inspect video",
            Category::Audio => "Convert, trim, normalize and analyze audio",
            Category::Speech => "Offline transcription, voice activity and subtitles",
            Category::Pdf => "Merge, split, OCR, protect and convert documents",
            Category::Files => "Checksums, archives, duplicates and file inspection",
            Category::Text => "Transform, compare and clean up text",
            Category::Dev => "Formatters, encoders, hashes and generators",
            Category::Data => "CSV, spreadsheets and structured data",
            Category::Network => "DNS, HTTP, TLS, IP and URL utilities",
            Category::Design => "Colors, palettes, gradients and icons",
            Category::System => "Hardware, battery, storage and OS information",
        }
    }

    pub fn parse(s: &str) -> Option<Category> {
        Category::ALL.iter().copied().find(|c| c.id() == s)
    }
}

/// Operating systems a tool can run on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Linux,
    Macos,
    Windows,
}

impl Platform {
    pub fn current() -> Platform {
        if cfg!(target_os = "macos") {
            Platform::Macos
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            Platform::Linux
        }
    }
    pub const ALL: [Platform; 3] = [Platform::Linux, Platform::Macos, Platform::Windows];
}

/// Capabilities beyond "process the inputs I was given". Each must be
/// explicitly granted to the caller; the engine enforces this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// Read arbitrary host paths (within configured roots).
    HostFsRead,
    /// Modify host paths (within configured roots).
    HostFsWrite,
    /// Make outbound network requests.
    Network,
    /// Read hardware / OS information about the host.
    SystemInfo,
    /// Read process listings of the host.
    ProcessInfo,
}

impl Permission {
    pub const ALL: [Permission; 5] = [
        Permission::HostFsRead,
        Permission::HostFsWrite,
        Permission::Network,
        Permission::SystemInfo,
        Permission::ProcessInfo,
    ];
    pub fn id(&self) -> &'static str {
        match self {
            Permission::HostFsRead => "host_fs_read",
            Permission::HostFsWrite => "host_fs_write",
            Permission::Network => "network",
            Permission::SystemInfo => "system_info",
            Permission::ProcessInfo => "process_info",
        }
    }
    pub fn parse(s: &str) -> Option<Permission> {
        Permission::ALL.iter().copied().find(|p| p.id() == s)
    }
    pub fn describe(&self) -> &'static str {
        match self {
            Permission::HostFsRead => "Read files and folders on the host within allowed roots",
            Permission::HostFsWrite => "Modify files and folders on the host within allowed roots",
            Permission::Network => "Make outbound network requests",
            Permission::SystemInfo => "Read hardware and operating system information",
            Permission::ProcessInfo => "List running processes",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnumOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PathKind {
    File,
    Dir,
    Any,
}

/// Parameter types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParamKind {
    Text {
        #[serde(default)]
        multiline: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_len: Option<usize>,
        /// Editor syntax hint: json, yaml, xml, sql, markdown, html, csv...
        #[serde(default, skip_serializing_if = "Option::is_none")]
        syntax: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
    },
    Secret,
    Integer {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<i64>,
    },
    Number {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<f64>,
    },
    Boolean,
    Enum {
        options: Vec<EnumOption>,
        #[serde(default)]
        multi: bool,
    },
    Color,
    /// Uploaded / provided input file(s). Resolved by the caller to paths.
    File {
        #[serde(default)]
        accept: Vec<String>,
        #[serde(default)]
        multiple: bool,
    },
    /// A path on the host filesystem (requires host_fs permission).
    Path { kind: PathKind },
    /// Arbitrary JSON value (e.g. list of regions), optionally with a schema.
    Json {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schema: Option<Value>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParamSpec {
    pub name: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(flatten)]
    pub kind: ParamKind,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    /// Hidden behind "advanced options" in UIs.
    #[serde(default)]
    pub advanced: bool,
    /// Used as positional argument in the CLI.
    #[serde(default)]
    pub primary: bool,
    /// Only show in UI when another param equals one of the values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_if: Option<ShowIf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShowIf {
    pub param: String,
    pub values: Vec<Value>,
}

fn humanize(name: &str) -> String {
    let s = name.replace(['_', '-'], " ");
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Builder-style constructors for parameters.
impl ParamSpec {
    fn new(name: &str, kind: ParamKind) -> Self {
        ParamSpec {
            name: name.to_string(),
            label: humanize(name),
            description: String::new(),
            kind,
            required: false,
            default: None,
            advanced: false,
            primary: false,
            show_if: None,
        }
    }
    pub fn text(name: &str) -> Self {
        Self::new(name, ParamKind::Text { multiline: false, max_len: None, syntax: None, placeholder: None })
    }
    /// Multi-line text input (the common "paste your text here" field).
    pub fn textarea(name: &str) -> Self {
        Self::new(name, ParamKind::Text { multiline: true, max_len: None, syntax: None, placeholder: None })
    }
    pub fn secret(name: &str) -> Self {
        Self::new(name, ParamKind::Secret)
    }
    pub fn integer(name: &str) -> Self {
        Self::new(name, ParamKind::Integer { min: None, max: None })
    }
    pub fn number(name: &str) -> Self {
        Self::new(name, ParamKind::Number { min: None, max: None, step: None })
    }
    pub fn boolean(name: &str) -> Self {
        Self::new(name, ParamKind::Boolean).default_value(Value::Bool(false))
    }
    /// Enum from `(value, label)` pairs.
    pub fn choice(name: &str, options: &[(&str, &str)]) -> Self {
        Self::new(
            name,
            ParamKind::Enum {
                options: options
                    .iter()
                    .map(|(v, l)| EnumOption { value: v.to_string(), label: l.to_string() })
                    .collect(),
                multi: false,
            },
        )
    }
    /// Enum where labels equal values.
    pub fn choice_of(name: &str, values: &[&str]) -> Self {
        let pairs: Vec<(&str, &str)> = values.iter().map(|v| (*v, *v)).collect();
        Self::choice(name, &pairs)
    }
    pub fn multi_choice(name: &str, options: &[(&str, &str)]) -> Self {
        let mut p = Self::choice(name, options);
        if let ParamKind::Enum { multi, .. } = &mut p.kind {
            *multi = true;
        }
        p
    }
    pub fn color(name: &str) -> Self {
        Self::new(name, ParamKind::Color)
    }
    pub fn file(name: &str) -> Self {
        Self::new(name, ParamKind::File { accept: vec![], multiple: false })
    }
    pub fn files(name: &str) -> Self {
        Self::new(name, ParamKind::File { accept: vec![], multiple: true })
    }
    pub fn path(name: &str, kind: PathKind) -> Self {
        Self::new(name, ParamKind::Path { kind })
    }
    pub fn json(name: &str) -> Self {
        Self::new(name, ParamKind::Json { schema: None })
    }

    pub fn label(mut self, l: &str) -> Self {
        self.label = l.to_string();
        self
    }
    pub fn describe(mut self, d: &str) -> Self {
        self.description = d.to_string();
        self
    }
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }
    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }
    pub fn primary(mut self) -> Self {
        self.primary = true;
        self
    }
    pub fn advanced(mut self) -> Self {
        self.advanced = true;
        self
    }
    pub fn default_value(mut self, v: Value) -> Self {
        self.default = Some(v);
        self
    }
    pub fn default_str(self, v: &str) -> Self {
        self.default_value(Value::String(v.to_string()))
    }
    pub fn default_int(self, v: i64) -> Self {
        self.default_value(Value::from(v))
    }
    pub fn default_num(self, v: f64) -> Self {
        self.default_value(Value::from(v))
    }
    pub fn default_bool(self, v: bool) -> Self {
        self.default_value(Value::Bool(v))
    }
    pub fn min(mut self, m: f64) -> Self {
        match &mut self.kind {
            ParamKind::Integer { min, .. } => *min = Some(m as i64),
            ParamKind::Number { min, .. } => *min = Some(m),
            _ => {}
        }
        self
    }
    pub fn max(mut self, m: f64) -> Self {
        match &mut self.kind {
            ParamKind::Integer { max, .. } => *max = Some(m as i64),
            ParamKind::Number { max, .. } => *max = Some(m),
            _ => {}
        }
        self
    }
    pub fn range(self, lo: f64, hi: f64) -> Self {
        self.min(lo).max(hi)
    }
    pub fn step(mut self, s: f64) -> Self {
        if let ParamKind::Number { step, .. } = &mut self.kind {
            *step = Some(s);
        }
        self
    }
    pub fn max_len(mut self, n: usize) -> Self {
        if let ParamKind::Text { max_len, .. } = &mut self.kind {
            *max_len = Some(n);
        }
        self
    }
    pub fn syntax(mut self, s: &str) -> Self {
        if let ParamKind::Text { syntax, .. } = &mut self.kind {
            *syntax = Some(s.to_string());
        }
        self
    }
    pub fn placeholder(mut self, s: &str) -> Self {
        if let ParamKind::Text { placeholder, .. } = &mut self.kind {
            *placeholder = Some(s.to_string());
        }
        self
    }
    /// Accepted MIME types / extensions for file params, e.g. `image/*`, `.pdf`.
    pub fn accept(mut self, a: &[&str]) -> Self {
        if let ParamKind::File { accept, .. } = &mut self.kind {
            *accept = a.iter().map(|s| s.to_string()).collect();
        }
        self
    }
    pub fn schema(mut self, s: Value) -> Self {
        if let ParamKind::Json { schema } = &mut self.kind {
            *schema = Some(s);
        }
        self
    }
    pub fn show_if(mut self, param: &str, values: &[&str]) -> Self {
        self.show_if = Some(ShowIf {
            param: param.to_string(),
            values: values.iter().map(|v| Value::String(v.to_string())).collect(),
        });
        self
    }
    pub fn show_if_bool(mut self, param: &str, value: bool) -> Self {
        self.show_if = Some(ShowIf { param: param.to_string(), values: vec![Value::Bool(value)] });
        self
    }

    pub fn is_file(&self) -> bool {
        matches!(self.kind, ParamKind::File { .. })
    }
    pub fn is_secret(&self) -> bool {
        matches!(self.kind, ParamKind::Secret)
    }
}

/// What kind of result the tool produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputKind {
    /// Primarily a text result (formatted JSON, transformed text, ...)
    Text,
    /// Structured data (tables, reports)
    Data,
    /// One or more output files
    Files,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OutputSpec {
    pub kind: OutputKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// MIME type of text output when kind == text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_mime: Option<String>,
    /// JSON schema of the structured `data` field, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_schema: Option<Value>,
}

impl Default for OutputSpec {
    fn default() -> Self {
        OutputSpec { kind: OutputKind::Data, description: String::new(), text_mime: None, data_schema: None }
    }
}

/// Where a tool's work happens, so UIs can tell users where their data goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Execution {
    /// Pure in-process computation.
    Native,
    /// Invokes an external program.
    External,
    /// Runs a local machine-learning model.
    Model,
    /// Talks to the network.
    Network,
}

/// Complete tool description.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    /// Stable id: `<category>.<name>` e.g. `image.convert`.
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: Category,
    #[serde(default)]
    pub keywords: Vec<String>,
    pub params: Vec<ParamSpec>,
    pub output: OutputSpec,
    pub platforms: Vec<Platform>,
    /// Dependency ids (see [`crate::deps`]) that must all be present.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Model group: at least one model in this group must be installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_model: Option<String>,
    #[serde(default)]
    pub permissions: Vec<Permission>,
    pub timeout_secs: u64,
    /// Whether the tool can be cancelled mid-flight (all can be aborted; this
    /// marks long-running tools that report progress).
    #[serde(default)]
    pub reports_progress: bool,
    pub execution: Execution,
    pub version: String,
    /// Optional name of a specialized frontend component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<String>,
    /// Whether results may modify state outside the job (e.g. batch rename).
    #[serde(default)]
    pub destructive: bool,
    /// Source: builtin or extension id.
    #[serde(default = "builtin")]
    pub source: String,
}

fn builtin() -> String {
    "builtin".to_string()
}

impl ToolSpec {
    pub fn new(id: &str, name: &str, category: Category) -> Self {
        ToolSpec {
            id: id.to_string(),
            name: name.to_string(),
            description: String::new(),
            category,
            keywords: vec![],
            params: vec![],
            output: OutputSpec::default(),
            platforms: Platform::ALL.to_vec(),
            requires: vec![],
            requires_model: None,
            permissions: vec![],
            timeout_secs: 120,
            reports_progress: false,
            execution: Execution::Native,
            version: "1.0.0".to_string(),
            ui: None,
            destructive: false,
            source: builtin(),
        }
    }
    pub fn describe(mut self, d: &str) -> Self {
        self.description = d.to_string();
        self
    }
    pub fn keywords(mut self, k: &[&str]) -> Self {
        self.keywords.extend(k.iter().map(|s| s.to_string()));
        self
    }
    pub fn param(mut self, p: ParamSpec) -> Self {
        self.params.push(p);
        self
    }
    pub fn params(mut self, ps: Vec<ParamSpec>) -> Self {
        self.params.extend(ps);
        self
    }
    pub fn output(mut self, kind: OutputKind) -> Self {
        self.output.kind = kind;
        self
    }
    pub fn text_output(mut self, mime: &str) -> Self {
        self.output.kind = OutputKind::Text;
        self.output.text_mime = Some(mime.to_string());
        self
    }
    pub fn output_description(mut self, d: &str) -> Self {
        self.output.description = d.to_string();
        self
    }
    pub fn data_schema(mut self, s: Value) -> Self {
        self.output.data_schema = Some(s);
        self
    }
    pub fn platforms(mut self, p: &[Platform]) -> Self {
        self.platforms = p.to_vec();
        self
    }
    pub fn requires(mut self, deps: &[&str]) -> Self {
        self.requires.extend(deps.iter().map(|s| s.to_string()));
        if self.execution == Execution::Native {
            self.execution = Execution::External;
        }
        self
    }
    pub fn requires_model(mut self, group: &str) -> Self {
        self.requires_model = Some(group.to_string());
        self.execution = Execution::Model;
        self
    }
    pub fn permission(mut self, p: Permission) -> Self {
        if !self.permissions.contains(&p) {
            self.permissions.push(p);
        }
        if p == Permission::Network {
            self.execution = Execution::Network;
        }
        self
    }
    pub fn timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }
    pub fn progress(mut self) -> Self {
        self.reports_progress = true;
        self
    }
    pub fn execution(mut self, e: Execution) -> Self {
        self.execution = e;
        self
    }
    pub fn version(mut self, v: &str) -> Self {
        self.version = v.to_string();
        self
    }
    pub fn ui(mut self, component: &str) -> Self {
        self.ui = Some(component.to_string());
        self
    }
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    pub fn param_spec(&self, name: &str) -> Option<&ParamSpec> {
        self.params.iter().find(|p| p.name == name)
    }

    /// MCP / CLI friendly name: dots replaced by underscores.
    pub fn mcp_name(&self) -> String {
        self.id.replace(['.', '-'], "_")
    }

    pub fn supports_current_platform(&self) -> bool {
        self.platforms.contains(&Platform::current())
    }
}
