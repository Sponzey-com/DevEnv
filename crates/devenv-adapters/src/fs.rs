use std::path::{Path, PathBuf};

use devenv_core::{
    ConfigFormat, ConfigRepository, ConfigScope, ConfigSource, CoreError, CoreResult,
    ProjectConfig, ToolName, VersionRequirement, parse_devenv_toml, parse_go_version,
    parse_java_version, parse_node_version, parse_nvmrc, parse_python_version, parse_ruby_version,
    parse_tool_versions,
};

const CONFIG_CANDIDATES: [(&str, ConfigFormat); 8] = [
    ("devenv.toml", ConfigFormat::DevenvToml),
    (".tool-versions", ConfigFormat::ToolVersions),
    (".java-version", ConfigFormat::JavaVersion),
    (".go-version", ConfigFormat::GoVersion),
    (".node-version", ConfigFormat::NodeVersion),
    (".nvmrc", ConfigFormat::Nvmrc),
    (".python-version", ConfigFormat::PythonVersion),
    (".ruby-version", ConfigFormat::RubyVersion),
];

pub fn discover_project_config(
    root: impl AsRef<Path>,
    current_dir: impl AsRef<Path>,
) -> CoreResult<Option<ProjectConfig>> {
    let root = canonicalize_dir(root.as_ref(), "root")?;
    let mut cursor = canonicalize_dir(current_dir.as_ref(), "current directory")?;

    if !cursor.starts_with(&root) {
        return Err(CoreError::message(format!(
            "cannot discover project config: current directory `{}` is outside root `{}`",
            cursor.display(),
            root.display()
        )));
    }

    loop {
        if let Some(config) = discover_in_directory(&cursor)? {
            return Ok(Some(config));
        }

        if cursor == root {
            return Ok(None);
        }

        let Some(parent) = cursor.parent() else {
            return Ok(None);
        };
        cursor = parent.to_path_buf();
    }
}

pub fn discover_project_config_from(
    current_dir: impl AsRef<Path>,
) -> CoreResult<Option<ProjectConfig>> {
    let mut cursor = canonicalize_dir(current_dir.as_ref(), "current directory")?;

    loop {
        if let Some(config) = discover_in_directory(&cursor)? {
            return Ok(Some(config));
        }

        let Some(parent) = cursor.parent() else {
            return Ok(None);
        };

        if parent == cursor {
            return Ok(None);
        }

        cursor = parent.to_path_buf();
    }
}

#[derive(Debug, Clone)]
pub struct NativeConfigRepository {
    path: PathBuf,
    scope: ConfigScope,
}

impl NativeConfigRepository {
    pub fn new(path: impl Into<PathBuf>, scope: ConfigScope) -> Self {
        Self {
            path: path.into(),
            scope,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn read_config(&self) -> CoreResult<Option<ProjectConfig>> {
        read_devenv_toml_config(&self.path, self.scope)
    }
}

impl ConfigRepository for NativeConfigRepository {
    fn get_requirement(&self, tool: &ToolName) -> CoreResult<Option<VersionRequirement>> {
        Ok(self
            .read_config()?
            .and_then(|config| config.tool(tool).map(|tool| tool.requirement().clone())))
    }

    fn set_requirement(
        &mut self,
        tool: ToolName,
        requirement: VersionRequirement,
    ) -> CoreResult<()> {
        write_devenv_toml_tool(&self.path, self.scope, tool, requirement)
    }
}

pub fn read_devenv_toml_config(
    path: impl AsRef<Path>,
    scope: ConfigScope,
) -> CoreResult<Option<ProjectConfig>> {
    let path = path.as_ref();

    if !path.exists() {
        return Ok(None);
    }

    if !path.is_file() {
        return Err(CoreError::message(format!(
            "failed to read config `{}`: expected a file",
            path.display()
        )));
    }

    let contents = std::fs::read_to_string(path).map_err(|error| {
        CoreError::message(format!(
            "failed to read config `{}`: {error}",
            path.display()
        ))
    })?;
    let config = parse_devenv_toml(&contents).map_err(|error| {
        CoreError::message(format!(
            "failed to parse config `{}`: {error}",
            path.display()
        ))
    })?;
    let source_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    Ok(Some(config.with_source(ConfigSource::new(
        source_path,
        scope,
        ConfigFormat::DevenvToml,
    ))))
}

pub fn write_devenv_toml_tool(
    path: impl AsRef<Path>,
    scope: ConfigScope,
    tool: ToolName,
    requirement: VersionRequirement,
) -> CoreResult<()> {
    let path = path.as_ref();
    let mut config = read_devenv_toml_config(path, scope)?.unwrap_or_else(ProjectConfig::empty);
    config.set_tool_requirement(tool, requirement);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            CoreError::message(format!(
                "failed to create config directory `{}`: {error}",
                parent.display()
            ))
        })?;
    }

    std::fs::write(path, config.to_devenv_toml()).map_err(|error| {
        CoreError::message(format!(
            "failed to write config `{}`: {error}",
            path.display()
        ))
    })
}

fn discover_in_directory(directory: &Path) -> CoreResult<Option<ProjectConfig>> {
    for (filename, format) in CONFIG_CANDIDATES {
        let path = directory.join(filename);

        if !path.is_file() {
            continue;
        }

        let contents = std::fs::read_to_string(&path).map_err(|error| {
            CoreError::message(format!(
                "failed to read config `{}`: {error}",
                path.display()
            ))
        })?;
        let config = parse_by_format(format, &contents).map_err(|error| {
            CoreError::message(format!(
                "failed to parse config `{}`: {error}",
                path.display()
            ))
        })?;

        return Ok(Some(config.with_source(ConfigSource::new(
            path,
            ConfigScope::Project,
            format,
        ))));
    }

    Ok(None)
}

fn parse_by_format(format: ConfigFormat, contents: &str) -> CoreResult<ProjectConfig> {
    match format {
        ConfigFormat::DevenvToml => parse_devenv_toml(contents),
        ConfigFormat::ToolVersions => parse_tool_versions(contents),
        ConfigFormat::JavaVersion => parse_java_version(contents),
        ConfigFormat::GoVersion => parse_go_version(contents),
        ConfigFormat::NodeVersion => parse_node_version(contents),
        ConfigFormat::Nvmrc => parse_nvmrc(contents),
        ConfigFormat::PythonVersion => parse_python_version(contents),
        ConfigFormat::RubyVersion => parse_ruby_version(contents),
    }
}

fn canonicalize_dir(path: &Path, label: &str) -> CoreResult<PathBuf> {
    path.canonicalize().map_err(|error| {
        CoreError::message(format!(
            "cannot use {label} `{}` for config discovery: {error}",
            path.display()
        ))
    })
}
