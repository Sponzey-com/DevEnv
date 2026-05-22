use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use devenv_core::{
    Clock, CoreError, CoreResult, InstallPlan, InstallTransaction, InstallTransactionManager,
    Platform, ToolName, Version,
};

use crate::store::{DevEnvHome, FileInstallStore};

#[derive(Debug, Clone)]
pub struct FileInstallTransactionManager {
    installs_dir: PathBuf,
}

impl FileInstallTransactionManager {
    pub fn new(installs_dir: impl Into<PathBuf>) -> Self {
        Self {
            installs_dir: installs_dir.into(),
        }
    }

    pub fn at_home(home: &DevEnvHome) -> Self {
        Self::new(home.installs_dir())
    }

    fn temp_root(&self, plan: &InstallPlan) -> PathBuf {
        self.installs_dir.join(".tmp").join(format!(
            "{}-{}-{}",
            plan.tool().as_str(),
            plan.version().raw(),
            plan.platform().id()
        ))
    }
}

impl InstallTransactionManager for FileInstallTransactionManager {
    fn install_root(&self, tool: &ToolName, version: &Version, platform: Platform) -> PathBuf {
        FileInstallStore::new(&self.installs_dir).install_root(tool, version, platform)
    }

    fn begin(&mut self, plan: &InstallPlan) -> CoreResult<InstallTransaction> {
        let temp_root = self.temp_root(plan);
        if temp_root.exists() {
            std::fs::remove_dir_all(&temp_root).map_err(|error| {
                CoreError::message(format!(
                    "failed to reset install temp `{}`: {error}",
                    temp_root.display()
                ))
            })?;
        }

        let download_dir = temp_root.join("download");
        let extract_root = temp_root.join("extract");
        create_dir(&download_dir, "download temp")?;
        create_dir(&extract_root, "extract temp")?;

        Ok(InstallTransaction::new(
            plan.install_root(),
            &temp_root,
            download_dir.join(plan.artifact().filename()),
            &extract_root,
        ))
    }

    fn commit(&mut self, transaction: &InstallTransaction) -> CoreResult<()> {
        if transaction.install_root().exists() {
            return Err(CoreError::message(format!(
                "install root `{}` already exists",
                transaction.install_root().display()
            )));
        }

        if let Some(parent) = transaction.install_root().parent() {
            create_dir(parent, "install parent")?;
        }
        std::fs::rename(transaction.extract_root(), transaction.install_root()).map_err(|error| {
            CoreError::message(format!(
                "failed to commit install `{}` to `{}`: {error}",
                transaction.extract_root().display(),
                transaction.install_root().display()
            ))
        })
    }

    fn cleanup(&mut self, transaction: &InstallTransaction) -> CoreResult<()> {
        if transaction.temp_root().exists() {
            std::fs::remove_dir_all(transaction.temp_root()).map_err(|error| {
                CoreError::message(format!(
                    "failed to clean install temp `{}`: {error}",
                    transaction.temp_root().display()
                ))
            })?;
        }

        Ok(())
    }
}

fn create_dir(path: &Path, label: &str) -> CoreResult<()> {
    std::fs::create_dir_all(path).map_err(|error| {
        CoreError::message(format!(
            "failed to create {label} `{}`: {error}",
            path.display()
        ))
    })
}

#[derive(Debug, Clone, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_utc(&self) -> CoreResult<String> {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| CoreError::message(format!("system clock is before epoch: {error}")))?
            .as_secs();

        Ok(format!("unix:{seconds}"))
    }
}
