use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::{CoreError, CoreResult, ToolDistribution, ToolName, VersionRequirement};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    DevenvToml,
    ToolVersions,
    JavaVersion,
    GoVersion,
    NodeVersion,
    Nvmrc,
    PythonVersion,
    RubyVersion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigScope {
    CliOverride,
    Shell,
    Project,
    Global,
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigSource {
    path: PathBuf,
    scope: ConfigScope,
    format: ConfigFormat,
}

impl ConfigSource {
    pub fn new(path: PathBuf, scope: ConfigScope, format: ConfigFormat) -> Self {
        Self {
            path,
            scope,
            format,
        }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn scope(&self) -> ConfigScope {
        self.scope
    }

    pub fn format(&self) -> ConfigFormat {
        self.format
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectConfig {
    source: Option<ConfigSource>,
    tools: BTreeMap<ToolName, ToolConfig>,
}

impl ProjectConfig {
    pub fn empty() -> Self {
        Self {
            source: None,
            tools: BTreeMap::new(),
        }
    }

    pub fn with_source(mut self, source: ConfigSource) -> Self {
        self.source = Some(source);
        self
    }

    pub fn source(&self) -> Option<&ConfigSource> {
        self.source.as_ref()
    }

    pub fn insert_tool(&mut self, tool: ToolName, config: ToolConfig) {
        self.tools.insert(tool, config);
    }

    pub fn set_tool_requirement(&mut self, tool: ToolName, requirement: VersionRequirement) {
        self.insert_tool(tool, ToolConfig::new(requirement));
    }

    pub fn tool(&self, tool: &ToolName) -> Option<&ToolConfig> {
        self.tools.get(tool)
    }

    pub fn tools(&self) -> &BTreeMap<ToolName, ToolConfig> {
        &self.tools
    }

    pub fn to_devenv_toml(&self) -> String {
        let mut inline_tools = Vec::new();
        let mut table_tools = Vec::new();

        for (tool, config) in &self.tools {
            if config.distribution().is_some() {
                table_tools.push((tool, config));
            } else {
                inline_tools.push((tool, config));
            }
        }

        let mut output = String::new();

        if !inline_tools.is_empty() {
            output.push_str("[tools]\n");
            for (tool, config) in inline_tools {
                output.push_str(tool.as_str());
                output.push_str(" = \"");
                output.push_str(&escape_toml_string(config.requirement().raw()));
                output.push_str("\"\n");
            }
        }

        for (tool, config) in table_tools {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str("[tools.");
            output.push_str(tool.as_str());
            output.push_str("]\n");
            output.push_str("version = \"");
            output.push_str(&escape_toml_string(config.requirement().raw()));
            output.push_str("\"\n");
            if let Some(distribution) = config.distribution() {
                output.push_str("distribution = \"");
                output.push_str(&escape_toml_string(distribution.as_str()));
                output.push_str("\"\n");
            }
        }

        output
    }
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolConfig {
    requirement: VersionRequirement,
    distribution: Option<ToolDistribution>,
}

impl ToolConfig {
    pub fn new(requirement: VersionRequirement) -> Self {
        Self {
            requirement,
            distribution: None,
        }
    }

    pub fn with_distribution(mut self, distribution: ToolDistribution) -> Self {
        self.distribution = Some(distribution);
        self
    }

    pub fn requirement(&self) -> &VersionRequirement {
        &self.requirement
    }

    pub fn distribution(&self) -> Option<&ToolDistribution> {
        self.distribution.as_ref()
    }
}

pub fn parse_devenv_toml(input: &str) -> CoreResult<ProjectConfig> {
    let document = input
        .parse::<toml::Value>()
        .map_err(|error| CoreError::message(format!("invalid devenv.toml: {error}")))?;
    let mut config = ProjectConfig::empty();
    let Some(tools) = document.get("tools") else {
        return Ok(config);
    };
    let Some(tools_table) = tools.as_table() else {
        return Err(CoreError::message(
            "invalid devenv.toml: expected [tools] to be a table",
        ));
    };

    for (tool_name, value) in tools_table {
        let tool = ToolName::new(tool_name)?;
        let tool_config = parse_toml_tool_value(tool_name, value)?;
        config.insert_tool(tool, tool_config);
    }

    Ok(config)
}

pub fn parse_tool_versions(input: &str) -> CoreResult<ProjectConfig> {
    let mut config = ProjectConfig::empty();

    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let content = strip_comment(line).trim();

        if content.is_empty() {
            continue;
        }

        let parts = content.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 2 {
            return Err(CoreError::message(format!(
                "invalid .tool-versions line {line_number}: expected <tool> <version>"
            )));
        }

        let tool = ToolName::new(parts[0])?;
        let requirement = VersionRequirement::exact(parts[1])?;
        config.insert_tool(tool, ToolConfig::new(requirement));
    }

    Ok(config)
}

pub fn parse_java_version(input: &str) -> CoreResult<ProjectConfig> {
    parse_single_tool_version("java", ".java-version", input)
}

pub fn parse_go_version(input: &str) -> CoreResult<ProjectConfig> {
    parse_single_tool_version("go", ".go-version", input)
}

pub fn parse_node_version(input: &str) -> CoreResult<ProjectConfig> {
    parse_single_tool_version("node", ".node-version", input)
}

pub fn parse_nvmrc(input: &str) -> CoreResult<ProjectConfig> {
    parse_single_tool_version("node", ".nvmrc", input)
}

pub fn parse_python_version(input: &str) -> CoreResult<ProjectConfig> {
    parse_single_tool_version("python", ".python-version", input)
}

pub fn parse_ruby_version(input: &str) -> CoreResult<ProjectConfig> {
    parse_single_tool_version("ruby", ".ruby-version", input)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SelectionSource {
    CliOverride,
    Shell,
    Project,
    Global,
    Default,
}

impl SelectionSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::CliOverride => "cli",
            Self::Shell => "shell",
            Self::Project => "project",
            Self::Global => "global",
            Self::Default => "default",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionCandidate {
    source: SelectionSource,
    source_path: Option<PathBuf>,
    requirement: VersionRequirement,
}

impl SelectionCandidate {
    pub fn new(source: SelectionSource, requirement: VersionRequirement) -> Self {
        Self {
            source,
            source_path: None,
            requirement,
        }
    }

    pub fn with_source_path(mut self, source_path: impl Into<PathBuf>) -> Self {
        self.source_path = Some(source_path.into());
        self
    }

    pub fn source(&self) -> SelectionSource {
        self.source
    }

    pub fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }

    pub fn requirement(&self) -> &VersionRequirement {
        &self.requirement
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSelection {
    tool: ToolName,
    source: SelectionSource,
    source_path: Option<PathBuf>,
    requirement: VersionRequirement,
}

impl ResolvedSelection {
    pub fn tool(&self) -> &ToolName {
        &self.tool
    }

    pub fn source(&self) -> SelectionSource {
        self.source
    }

    pub fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }

    pub fn requirement(&self) -> &VersionRequirement {
        &self.requirement
    }
}

pub fn resolve_tool_selection(
    tool: ToolName,
    candidates: impl IntoIterator<Item = SelectionCandidate>,
) -> Option<ResolvedSelection> {
    candidates
        .into_iter()
        .min_by_key(selection_rank)
        .map(|candidate| ResolvedSelection {
            tool,
            source: candidate.source,
            source_path: candidate.source_path,
            requirement: candidate.requirement,
        })
}

fn parse_toml_tool_value(tool_name: &str, value: &toml::Value) -> CoreResult<ToolConfig> {
    if let Some(version) = value.as_str() {
        return Ok(ToolConfig::new(VersionRequirement::exact(version)?));
    }

    let Some(table) = value.as_table() else {
        return Err(CoreError::message(format!(
            "invalid devenv.toml: expected tools.{tool_name} to be a string or table"
        )));
    };
    let version = table
        .get("version")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| {
            CoreError::message(format!(
                "invalid devenv.toml: expected tools.{tool_name}.version to be a string"
            ))
        })?;
    let mut config = ToolConfig::new(VersionRequirement::exact(version)?);

    if let Some(distribution) = table.get("distribution") {
        let distribution = distribution.as_str().ok_or_else(|| {
            CoreError::message(format!(
                "invalid devenv.toml: expected tools.{tool_name}.distribution to be a string"
            ))
        })?;
        config = config.with_distribution(ToolDistribution::named(distribution));
    }

    Ok(config)
}

fn parse_single_tool_version(
    tool_name: &str,
    filename: &str,
    input: &str,
) -> CoreResult<ProjectConfig> {
    let version = input
        .lines()
        .map(strip_comment)
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| CoreError::message(format!("invalid {filename}: expected a version")))?;

    let mut config = ProjectConfig::empty();
    config.insert_tool(
        ToolName::new(tool_name)?,
        ToolConfig::new(VersionRequirement::exact(version)?),
    );

    Ok(config)
}

fn strip_comment(line: &str) -> &str {
    line.split_once('#')
        .map_or(line, |(before_comment, _)| before_comment)
}

fn escape_toml_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn selection_rank(candidate: &SelectionCandidate) -> u8 {
    match candidate.source {
        SelectionSource::CliOverride => 0,
        SelectionSource::Shell => 1,
        SelectionSource::Project => 2,
        SelectionSource::Global => 3,
        SelectionSource::Default => 4,
    }
}
