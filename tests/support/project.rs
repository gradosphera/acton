use crate::common::{acton_exe, assert_ui};
use crate::support::assertions::TestOutput;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;

pub struct ProjectBuilder {
    name: String,
    temp_dir: TempDir,
    contracts: Vec<(String, String)>,
    tests: Vec<(String, String)>,
}

impl ProjectBuilder {
    pub fn new(name: &str) -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        Self {
            name: name.to_string(),
            temp_dir,
            contracts: Vec::new(),
            tests: Vec::new(),
        }
    }

    pub fn contract(mut self, name: &str, code: &str) -> Self {
        self.contracts.push((name.to_string(), code.to_string()));
        self
    }

    pub fn test_file(mut self, name: &str, code: &str) -> Self {
        self.tests.push((name.to_string(), code.to_string()));
        self
    }

    pub fn build(self) -> Project {
        let project_path = self.temp_dir.path().join(&self.name);
        fs::create_dir_all(&project_path).expect("Failed to create project dir");

        Self::copy_lib_to(&self.temp_dir.path());

        let contracts_dir = project_path.join("contracts");
        fs::create_dir_all(&contracts_dir).expect("Failed to create contracts dir");

        let tests_dir = project_path.join("tests");
        fs::create_dir_all(&tests_dir).expect("Failed to create tests dir");

        for (name, code) in &self.contracts {
            let file_path = contracts_dir.join(format!("{}.tolk", name));
            fs::write(file_path, code).expect("Failed to write contract file");
        }

        for (name, code) in &self.tests {
            let adjusted_code = Self::adjust_imports(code);
            let file_path = tests_dir.join(format!("{}_test.tolk", name));
            fs::write(file_path, adjusted_code).expect("Failed to write test file");
        }

        Self::create_acton_toml(&project_path, &self.name, &self.contracts);

        Project {
            path: project_path,
            _temp_dir: self.temp_dir,
        }
    }

    fn copy_lib_to(temp_path: &Path) {
        use include_dir::{Dir, include_dir};
        static LIB_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/lib");

        let lib_path = temp_path.join("lib");
        fs::create_dir_all(&lib_path).expect("Failed to create lib dir");
        LIB_DIR
            .extract(&lib_path)
            .expect("Failed to extract lib dir");
    }

    fn adjust_imports(code: &str) -> String {
        code.replace("import \"../../../../lib/", "import \"../../lib/")
    }

    fn create_acton_toml(project_path: &Path, name: &str, contracts: &[(String, String)]) {
        let mut toml_content = format!(
            r#"[package]
name = "{}"
description = "A test project"
version = "0.1.0"
license = "MIT"

"#,
            name
        );

        for (contract_name, _) in contracts {
            toml_content.push_str(&format!(
                r#"[contracts.{}]
name = "{}"
src = "contracts/{}.tolk"
depends = []

"#,
                contract_name.to_lowercase(),
                contract_name,
                contract_name
            ));
        }

        let config_path = project_path.join("Acton.toml");
        fs::write(config_path, toml_content).expect("Failed to write Acton.toml");
    }
}

pub struct Project {
    path: PathBuf,
    _temp_dir: TempDir,
}

impl Project {
    pub fn acton(&self) -> ActonCommand {
        let cmd = snapbox::cmd::Command::new(acton_exe()).with_assert(assert_ui());
        ActonCommand {
            cmd,
            project: Arc::new(ProjectRef {
                path: self.path.clone(),
            }),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub struct ProjectRef {
    pub path: PathBuf,
}

pub struct ActonCommand {
    pub(crate) cmd: snapbox::cmd::Command,
    pub(crate) project: Arc<ProjectRef>,
}

impl ActonCommand {
    pub fn test(mut self) -> Self {
        self.cmd = self
            .cmd
            .arg("test")
            .current_dir(&self.project.path)
            .arg(".");
        self
    }

    pub fn with_backtrace(mut self, level: &str) -> Self {
        self.cmd = self.cmd.arg("--backtrace").arg(level);
        self
    }

    pub fn with_coverage(mut self) -> Self {
        self.cmd = self.cmd.arg("--coverage");
        self
    }

    pub fn run(mut self) -> TestOutput {
        self.cmd = self.cmd.env("NO_COLOR", "1");
        let output = self.cmd.assert();
        TestOutput {
            output,
            project_path: self.project.path.clone(),
        }
    }
}
