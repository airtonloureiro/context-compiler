use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum StackProfile {
    NodeTypeScript,
    NodeJavaScript,
    Python,
    Rust,
    Docker,
    Prisma,
    Unknown,
}

pub struct StackDetector;

impl StackDetector {
    /// Identifica a stack do projeto baseada em arquivos chaves.
    pub fn detect(repo_root: &Path) -> Vec<StackProfile> {
        let mut profiles = Vec::new();

        if repo_root.join("package.json").exists() {
            if repo_root.join("tsconfig.json").exists() || repo_root.join("pnpm-workspace.yaml").exists() {
                profiles.push(StackProfile::NodeTypeScript);
            } else {
                profiles.push(StackProfile::NodeJavaScript);
            }
        }

        if repo_root.join("Cargo.toml").exists() {
            profiles.push(StackProfile::Rust);
        }

        if repo_root.join("requirements.txt").exists() || repo_root.join("pyproject.toml").exists() {
            profiles.push(StackProfile::Python);
        }

        if repo_root.join("Dockerfile").exists() || repo_root.join("docker-compose.yml").exists() {
            profiles.push(StackProfile::Docker);
        }

        if repo_root.join("prisma").join("schema.prisma").exists() {
            profiles.push(StackProfile::Prisma);
        }

        if profiles.is_empty() {
            profiles.push(StackProfile::Unknown);
        }

        profiles
    }
}
