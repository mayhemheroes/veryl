use crate::build::{Build, Target};
use crate::format::Format;
use crate::git::Git;
use crate::lint::Lint;
use crate::lockfile::Lockfile;
use crate::project::Project;
use crate::pubfile::{Pubfile, Release};
use crate::publish::Publish;
use crate::utils;
use crate::MetadataError;
use directories::ProjectDirs;
use log::{debug, info};
use regex::Regex;
use semver::VersionReq;
use serde::{Deserialize, Serialize};
use spdx::Expression;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use url::Url;

#[derive(Clone, Debug)]
pub struct PathPair {
    pub prj: String,
    pub src: PathBuf,
    pub dst: PathBuf,
}

#[derive(Clone, Copy, Debug)]
pub enum BumpKind {
    Major,
    Minor,
    Patch,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub project: Project,
    #[serde(default)]
    pub build: Build,
    #[serde(default)]
    pub format: Format,
    #[serde(default)]
    pub lint: Lint,
    #[serde(default)]
    pub publish: Publish,
    #[serde(default)]
    pub dependencies: HashMap<Url, Dependency>,
    #[serde(skip)]
    pub metadata_path: PathBuf,
    #[serde(skip)]
    pub pubfile_path: PathBuf,
    #[serde(skip)]
    pub pubfile: Pubfile,
    #[serde(skip)]
    pub lockfile_path: PathBuf,
    #[serde(skip)]
    pub lockfile: Lockfile,
}

impl Metadata {
    pub fn search_from_current() -> Result<PathBuf, MetadataError> {
        Metadata::search_from(env::current_dir()?)
    }

    pub fn search_from<T: AsRef<Path>>(from: T) -> Result<PathBuf, MetadataError> {
        for path in from.as_ref().ancestors() {
            let path = path.join("Veryl.toml");
            if path.is_file() {
                return Ok(path);
            }
        }

        Err(MetadataError::FileNotFound)
    }

    pub fn load<T: AsRef<Path>>(path: T) -> Result<Self, MetadataError> {
        let path = path.as_ref().canonicalize()?;
        let text = fs::read_to_string(&path)?;
        let mut metadata: Metadata = Self::from_str(&text)?;
        metadata.metadata_path = path.clone();
        metadata.pubfile_path = path.with_file_name("Veryl.pub");
        metadata.lockfile_path = path.with_file_name("Veryl.lock");
        metadata.check()?;

        if metadata.pubfile_path.exists() {
            metadata.pubfile = Pubfile::load(&metadata.pubfile_path)?;
        }

        debug!(
            "Loaded metadata ({})",
            metadata.metadata_path.to_string_lossy()
        );
        Ok(metadata)
    }

    pub fn publish(&mut self) -> Result<(), MetadataError> {
        let prj_path = self.metadata_path.parent().unwrap();
        let git = Git::open(prj_path)?;
        if !git.is_clean()? {
            return Err(MetadataError::ModifiedProject(prj_path.to_path_buf()));
        }

        for release in &self.pubfile.releases {
            if release.version == self.project.version {
                return Err(MetadataError::PublishedVersion(
                    self.project.version.clone(),
                ));
            }
        }

        let version = self.project.version.clone();
        let revision = git.get_revision()?;

        info!("Publishing release ({} @ {})", version, revision);

        let release = Release { version, revision };

        self.pubfile.releases.push(release);

        self.pubfile.save(&self.pubfile_path)?;
        info!("Writing metadata ({})", self.pubfile_path.to_string_lossy());

        if self.publish.publish_commit {
            git.add(&self.pubfile_path)?;
            git.commit(&self.publish.publish_commit_message)?;
            info!(
                "Committing metadata ({})",
                self.pubfile_path.to_string_lossy()
            );
        }

        Ok(())
    }

    pub fn check(&self) -> Result<(), MetadataError> {
        let valid_project_name = Regex::new(r"^[a-zA-Z_][0-9a-zA-Z_]*$").unwrap();
        if !valid_project_name.is_match(&self.project.name) {
            return Err(MetadataError::InvalidProjectName(self.project.name.clone()));
        }

        if let Some(ref license) = self.project.license {
            let _ = Expression::parse(license)?;
        }

        Ok(())
    }

    pub fn bump_version(&mut self, kind: BumpKind) -> Result<(), MetadataError> {
        let prj_path = self.metadata_path.parent().unwrap();
        let git = Git::open(prj_path)?;

        let mut bumped_version = self.project.version.clone();
        match kind {
            BumpKind::Major => {
                bumped_version.major += 1;
                bumped_version.minor = 0;
                bumped_version.patch = 0;
            }
            BumpKind::Minor => {
                bumped_version.minor += 1;
                bumped_version.patch = 0;
            }
            BumpKind::Patch => bumped_version.patch += 1,
        }
        info!(
            "Bumping version ({} -> {})",
            self.project.version, bumped_version
        );

        self.project.version = bumped_version.clone();

        let toml = fs::read_to_string(&self.metadata_path)?;
        let re = Regex::new(r##"version\s+=\s+"([^"]*)""##).unwrap();
        let caps = re
            .captures(&toml)
            .expect("safely unwrap because metadata is valid");
        let bumped_field = caps[0].replace(&caps[1], &bumped_version.to_string());
        let bumped_toml = re.replace(&toml, bumped_field);
        fs::write(&self.metadata_path, bumped_toml.as_bytes())?;
        info!(
            "Updating version field ({})",
            self.metadata_path.to_string_lossy()
        );

        if self.publish.bump_commit {
            git.add(&self.metadata_path)?;
            git.commit(&self.publish.bump_commit_message)?;
            info!(
                "Committing metadata ({})",
                self.metadata_path.to_string_lossy()
            );
        }

        Ok(())
    }

    pub fn update_lockfile(&mut self) -> Result<(), MetadataError> {
        let modified = if self.lockfile_path.exists() {
            let mut lockfile = Lockfile::load(&self.lockfile_path)?;
            let modified = lockfile.update(self, false)?;
            self.lockfile = lockfile;
            modified
        } else {
            self.lockfile = Lockfile::new(self)?;
            true
        };
        if modified {
            self.lockfile.save(&self.lockfile_path)?;
        }
        Ok(())
    }

    pub fn paths<T: AsRef<Path>>(&mut self, files: &[T]) -> Result<Vec<PathPair>, MetadataError> {
        let base = self.metadata_path.parent().unwrap();

        let src_files = if files.is_empty() {
            utils::gather_files_with_extension(base, "vl")?
        } else {
            files.iter().map(|x| x.as_ref().to_path_buf()).collect()
        };

        let mut ret = Vec::new();
        for src in src_files {
            let dst = match self.build.target {
                Target::Source => src.with_extension("sv"),
                Target::Directory { ref path } => {
                    base.join(path.join(src.with_extension("sv").file_name().unwrap()))
                }
            };
            ret.push(PathPair {
                prj: self.project.name.clone(),
                src: src.to_path_buf(),
                dst,
            });
        }

        let base_dst = self.metadata_path.parent().unwrap().join("dependencies");
        if !base_dst.exists() {
            fs::create_dir(&base_dst)?;
        }

        self.update_lockfile()?;

        let mut deps = self.lockfile.paths(&base_dst)?;
        ret.append(&mut deps);

        Ok(ret)
    }

    pub fn create_default_toml(name: &str) -> String {
        format!(
            r###"[project]
name = "{name}"
version = "0.1.0""###
        )
    }

    pub fn cache_dir() -> PathBuf {
        let project_dir = ProjectDirs::from("", "dalance", "veryl").unwrap();
        project_dir.cache_dir().to_path_buf()
    }
}

impl FromStr for Metadata {
    type Err = MetadataError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let metadata: Metadata = toml::from_str(s)?;
        Ok(metadata)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
#[serde(deny_unknown_fields)]
pub enum Dependency {
    Version(VersionReq),
    Single(DependencyEntry),
    Multi(Vec<DependencyEntry>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyEntry {
    pub name: String,
    pub version: VersionReq,
}
