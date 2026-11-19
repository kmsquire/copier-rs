//! Update (`Worker.run_update`) and update checks.
//!
//! An update regenerates the project from the old template version, diffs that
//! against the current project to learn what the user changed, applies the new
//! template version over the project and then replays the user's changes with git.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::{ConflictMode, Operation, Phase, UpdateData, VcsRef, Worker};
use crate::errors::{CopierError, Result};
use crate::output;
use crate::questions::AnswersMap;
use crate::template::{Stage, Template};
use crate::tools::{escape_git_path, split_nul};
use crate::value::{Mapping, Value};
use crate::vcs::Git;

/// Names Python's `filecmp.dircmp` never compares.
const DIRCMP_IGNORE: &[&str] = &[
    "RCS",
    "CVS",
    "tags",
    ".git",
    ".hg",
    ".bzr",
    "_darcs",
    "__pycache__",
];

fn user_message(message: impl Into<String>) -> CopierError {
    CopierError::UserMessage(message.into())
}

/// `git init`, stage everything and commit, the way Copier prepares its scratch
/// repositories.
fn git_initialize_repo(git: &Git) -> Result<()> {
    // Re-initializing an existing repository is harmless.
    let _ = git.output(&["init"])?;
    git.run(&["add", "."])?;
    git_commit(git, "dumb commit")
}

/// Commit twice: once letting hooks run (and possibly fail), once without them, so
/// the result is always committed.
fn git_commit(git: &Git, message: &str) -> Result<()> {
    let _ = git.output(&[
        "commit",
        "--allow-empty",
        "-am",
        &format!("{message} 1"),
        "--no-gpg-sign",
    ])?;
    git.run(&[
        "commit",
        "--allow-empty",
        "-am",
        &format!("{message} 2"),
        "--no-gpg-sign",
        "--no-verify",
    ])?;
    Ok(())
}

/// Let the repository at `path` borrow git objects from `repos`.
fn set_git_alternates(repos: &[&Path], path: &Path) -> Result<()> {
    let objects_dir = |repo: &Path| -> Result<PathBuf> {
        let relative = Git::in_dir(repo).run(&["rev-parse", "--git-path", "objects"])?;
        Ok(std::path::absolute(repo.join(relative.trim()))?)
    };
    let alternates_file = objects_dir(path)?.join("info").join("alternates");
    if let Some(parent) = alternates_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let lines: Vec<String> = repos
        .iter()
        .map(|repo| objects_dir(repo).map(|dir| dir.to_string_lossy().into_owned()))
        .collect::<Result<_>>()?;
    std::fs::write(alternates_file, lines.join("\n"))?;
    Ok(())
}

/// The part of `filecmp.dircmp` the update needs.
struct DirCmp {
    left: PathBuf,
    left_only: Vec<String>,
    common_files: Vec<String>,
    common_dirs: Vec<String>,
}

impl DirCmp {
    /// Compare two directories; a missing or unreadable side compares as empty.
    fn new(left: &Path, right: &Path) -> Self {
        let names = |dir: &Path| -> BTreeSet<String> {
            std::fs::read_dir(dir)
                .map(|entries| {
                    entries
                        .flatten()
                        .map(|entry| entry.file_name().to_string_lossy().into_owned())
                        .filter(|name| !DIRCMP_IGNORE.contains(&name.as_str()))
                        .collect()
                })
                .unwrap_or_default()
        };
        let (left_names, right_names) = (names(left), names(right));
        let mut cmp = Self {
            left: left.to_path_buf(),
            left_only: Vec::new(),
            common_files: Vec::new(),
            common_dirs: Vec::new(),
        };
        for name in left_names {
            if !right_names.contains(&name) {
                cmp.left_only.push(name);
            } else if left.join(&name).is_dir() && right.join(&name).is_dir() {
                cmp.common_dirs.push(name);
            } else if left.join(&name).is_file() && right.join(&name).is_file() {
                cmp.common_files.push(name);
            }
        }
        cmp
    }
}

/// Delete from the project at `prefix` what the old template generated and the
/// new one no longer does, keeping anything the user added.
fn remove_old_files(prefix: &Path, old: &Path, new: &Path, remove_common: bool) {
    let cmp = DirCmp::new(old, new);
    let mut to_remove = cmp.left_only.clone();
    if remove_common {
        to_remove.extend(cmp.common_files.iter().cloned());
        to_remove.extend(cmp.common_dirs.iter().cloned());
    }
    for name in &to_remove {
        let target = prefix.join(name);
        if target.is_symlink() {
            continue;
        }
        if target.is_file() {
            let _ = std::fs::remove_file(&target);
        } else {
            // Inside a directory the template dropped, everything it generated goes.
            remove_old_files(&target, &cmp.left.join(name), &target, true);
            // Fails, as intended, when the user left files of their own inside.
            let _ = std::fs::remove_dir(&target);
        }
    }
    for name in &cmp.common_dirs {
        let subdir = prefix.join(name);
        if subdir.is_symlink() {
            continue;
        }
        remove_old_files(&subdir, &old.join(name), &new.join(name), false);
        let _ = std::fs::remove_dir(&subdir);
    }
}

fn nul_paths(git: &Git, args: &[&str]) -> Result<Vec<String>> {
    Ok(split_nul(&git.run(args)?))
}

#[cfg(unix)]
fn touch_with_mode_of(path: &Path, like: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mode = std::fs::metadata(like)?.permissions().mode() & 0o7777;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(mode)
        .open(path)
        .map(|_| ())
}

#[cfg(not(unix))]
fn touch_with_mode_of(path: &Path, _like: &Path) -> std::io::Result<()> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map(|_| ())
}

impl Worker {
    fn old_template(&self) -> Result<&Template> {
        self.subproject()?
            .template()
            .ok_or_else(Self::missing_template)
    }

    pub(super) fn update(&mut self) -> Result<()> {
        let _operation = Operation::enter(Operation::Update);
        self.check_unsafe(Operation::Update)?;
        let subproject = self.subproject()?;
        if !subproject.is_git() {
            return Err(user_message(
                "Updating is only supported in git-tracked subprojects.",
            ));
        }
        if subproject.is_dirty()? {
            return Err(user_message(
                "Destination repository is dirty; cannot continue. Please commit or stash your local changes and retry.",
            ));
        }
        let old_template = match subproject.template() {
            Some(template) if template.git_ref.is_some() => template,
            _ => {
                return Err(user_message(format!(
                    "Cannot update because cannot obtain old template references from `{}`.",
                    subproject.answers_relpath.display()
                )));
            }
        };
        let template = self.template_ref()?;
        if template.commit()?.is_none() {
            return Err(user_message(
                "Updating is only supported in git-tracked templates.",
            ));
        }
        let Some(old_version) = old_template.version()? else {
            return Err(user_message(
                "Cannot update: version from last update not detected.",
            ));
        };
        let Some(new_version) = template.version()? else {
            return Err(user_message(
                "Cannot update: version from template not detected.",
            ));
        };
        if old_version > new_version {
            return Err(user_message(format!(
                "You are downgrading from {old_version} to {new_version}. Downgrades are not supported."
            )));
        }
        if !self.overwrite {
            return Err(user_message("Enable overwrite to update a subproject."));
        }
        self.print_message(&template.message("before_update")?)?;
        if !self.quiet {
            let verb = if old_version == new_version {
                "Keeping"
            } else {
                "Updating to"
            };
            output::eprintln(&format!("{verb} template version {new_version}"));
        }
        let after_update = template.message("after_update")?;
        self.reset_match_exclude();
        self.apply_update()?;
        self.print_message(&after_update)
    }

    fn run_migrations(&self, stage: Stage) -> Result<()> {
        let _phase = Phase::enter(Phase::Migrate);
        let tasks = self
            .template_ref()?
            .migration_tasks(stage, self.old_template()?)?;
        self.execute_tasks(&tasks)
    }

    fn apply_update(&mut self) -> Result<()> {
        let project = self.subproject()?.local_abspath.clone();
        let top_text = Git::in_dir(&project).run(&["rev-parse", "--show-toplevel"])?;
        let subproject_top = PathBuf::from(top_text.trim());
        let subproject_subdir = project
            .canonicalize()?
            .strip_prefix(subproject_top.canonicalize()?)
            .map(Path::to_path_buf)
            .map_err(|_| {
                CopierError::Value(format!(
                    "{} is not inside {}",
                    project.display(),
                    subproject_top.display()
                ))
            })?;
        let old_copy = crate::tools::tempdir("copier._main.old_copy.")?;
        let new_copy = crate::tools::tempdir("copier._main.new_copy.")?;
        let (old_path, new_path) = (
            old_copy.path().canonicalize()?,
            new_copy.path().canonicalize()?,
        );
        let old_template = self.old_template()?.clone();

        // What the old template version generates, with the recorded answers.
        let mut old_worker = self.derive()?;
        old_worker.dst_path = old_path.join(&subproject_subdir);
        old_worker.data = self.subproject()?.last_answers().clone();
        old_worker.defaults = true;
        old_worker.quiet = true;
        old_worker.src_path = Some(old_template.url.clone());
        old_worker.vcs_ref = old_template.commit()?.map(VcsRef::Ref);
        old_worker.exclude = self.all_exclusions()?;
        old_worker.ask = Vec::new();
        old_worker.copy()?;
        drop(old_worker);

        self.run_migrations(Stage::Before)?;

        let top_git = Git::in_dir(&subproject_top);
        let subproject_head = top_git.run(&["write-tree"])?.trim().to_owned();
        let old_git = Git::in_dir(&old_path);
        git_initialize_repo(&old_git)?;
        set_git_alternates(&[&subproject_top], &old_path)?;
        // Paths the user deleted stay deleted, unless they must always exist.
        let mut files_removed = Vec::new();
        for path in nul_paths(
            &old_git,
            &[
                "diff-tree",
                "-z",
                "-r",
                "--diff-filter=D",
                "--name-only",
                "HEAD",
                &subproject_head,
            ],
        )? {
            if !subproject_top.join(&path).exists() && !self.match_skip()?.matches(&path) {
                files_removed.push(path);
            }
        }

        if !self.skip_answered {
            // Migrations may have rewritten the answers file.
            self.update_answers(|answers| *answers = AnswersMap::default());
            // Only the answers are re-read: which template the project came from
            // must not change midway.
            if let Some(subproject) = self.cache.subproject.get_mut() {
                subproject.reload_answers();
            }
        }
        let mut current_worker = self.derive()?;
        current_worker.quiet = true;
        current_worker.answers = self.answers.clone();
        current_worker.copy()?;
        let answers = std::mem::take(&mut current_worker.answers);
        self.update_answers(|current| *current = answers);
        drop(current_worker);
        for filename in &files_removed {
            let _ = std::fs::remove_file(subproject_top.join(filename));
        }

        // What the new template version generates, without the user's changes.
        let mut new_worker = self.derive()?;
        new_worker.dst_path = new_path.join(&subproject_subdir);
        new_worker.data = self.replayable_answers();
        new_worker.defaults = true;
        new_worker.quiet = true;
        new_worker.src_path = Some(old_template.url.clone());
        new_worker.vcs_ref = self.resolved_vcs_ref()?.map(VcsRef::Ref);
        new_worker.ask = Vec::new();
        new_worker.copy()?;
        drop(new_worker);
        for filename in &files_removed {
            let _ = std::fs::remove_file(new_path.join(filename));
        }
        let new_git = Git::in_dir(&new_path);
        git_initialize_repo(&new_git)?;
        let new_copy_head = new_git.run(&["rev-parse", "HEAD"])?.trim().to_owned();

        set_git_alternates(&[&subproject_top, &new_path], &old_path)?;
        // A file added by both the user and the new template must look like a
        // modification, not an addition, or git could not merge the two.
        let added = |tree: &str| {
            nul_paths(
                &old_git,
                &[
                    "diff-tree",
                    "-z",
                    "-r",
                    "--diff-filter=A",
                    "--name-only",
                    "HEAD",
                    tree,
                ],
            )
        };
        let added_in_project: BTreeSet<String> = added(&subproject_head)?.into_iter().collect();
        for filename in added(&new_copy_head)? {
            if !added_in_project.contains(&filename) {
                continue;
            }
            let file = old_path.join(&filename);
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            touch_with_mode_of(&file, &subproject_top.join(&filename))?;
            old_git.run(&["add", "--force", "--", &filename])?;
        }
        git_commit(&old_git, "add new empty files")?;

        let unified = format!("--unified={}", self.context_lines);
        let diff_base = [
            "diff-tree",
            unified.as_str(),
            "HEAD",
            subproject_head.as_str(),
        ];
        // An empty pathspec is an error; the repository root is `.`.
        let subdir_text = match subproject_subdir.to_string_lossy() {
            text if text.is_empty() => ".".to_owned(),
            text => text.into_owned(),
        };
        let mut changed_args = diff_base.to_vec();
        changed_args.extend([
            "-z",
            "-r",
            "--no-commit-id",
            "--name-only",
            "--",
            subdir_text.as_str(),
        ]);
        let mut skip_if_exists_files = Vec::new();
        for file in nul_paths(&old_git, &changed_args)? {
            let relative = Path::new(&file)
                .strip_prefix(&subproject_subdir)
                .unwrap_or(Path::new(&file));
            if self.match_skip()?.matches(&relative.to_string_lossy()) {
                skip_if_exists_files.push(escape_git_path(&file));
            }
        }
        let mut diff_args = diff_base.to_vec();
        diff_args.push("--inter-hunk-context=-1");
        let diff = match old_git.output(&diff_args)? {
            output if output.status.success() => output.stdout,
            _ => {
                output::eprintln(&output::style::warn(
                    "Make sure Git >= 2.24 is installed to improve updates.",
                ));
                let mut fallback_args = diff_base.to_vec();
                fallback_args.push("--inter-hunk-context=0");
                let output = old_git.output(&fallback_args)?;
                if !output.status.success() {
                    return Err(CopierError::Process(
                        String::from_utf8_lossy(&output.stderr).into_owned(),
                    ));
                }
                output.stdout
            }
        };

        // Replay the user's changes on top of the freshly applied template.
        let mut apply_args: Vec<OsString> = vec!["apply".into(), "--reject".into()];
        apply_args.push(
            format!(
                "--exclude={}",
                subproject_subdir
                    .join(self.answers_relpath()?)
                    .to_string_lossy()
            )
            .into(),
        );
        apply_args.extend(
            skip_if_exists_files
                .iter()
                .map(|file| OsString::from(format!("--exclude={file}"))),
        );
        for entry in nul_paths(&top_git, &["status", "-z", "--ignored", "--porcelain"])? {
            if let Some(filepath) = entry.strip_prefix("!! ") {
                // An ignored file the new template generates must still get the diff.
                if !new_path.join(filepath).exists() {
                    apply_args.push(format!("--exclude={}", escape_git_path(filepath)).into());
                }
            }
        }
        // Rejected hunks are expected; they are resolved below.
        let _ = top_git.output_with_stdin(&apply_args, &diff)?;
        if self.conflict == ConflictMode::Inline {
            self.resolve_rejections_inline(&top_git, &subproject_top, &old_path, &new_path)?;
        }
        remove_old_files(&subproject_top, &old_path, &new_path, false);
        drop((old_copy, new_copy));

        self.run_migrations(Stage::After)
    }

    /// Answers to feed back when regenerating: public, applicable and plain data.
    fn replayable_answers(&self) -> Mapping {
        self.answers
            .combined()
            .into_iter()
            .filter(|(key, value)| {
                key.as_str().is_some_and(|name| {
                    !name.starts_with('_')
                        && !self.answers.hidden.iter().any(|hidden| hidden == name)
                }) && !matches!(value, Value::Timestamp(_))
            })
            .collect()
    }

    /// Turn every `*.rej` file into a three-way merge with conflict markers, and
    /// mark files that still conflict as unmerged in git's index.
    fn resolve_rejections_inline(
        &self,
        git: &Git,
        top: &Path,
        old_path: &Path,
        new_path: &Path,
    ) -> Result<()> {
        let mut conflicted: Vec<String> = Vec::new();
        for line in nul_paths(git, &["status", "-z", "--porcelain", "--ignored"])? {
            let Some(rejected) = line
                .strip_prefix("?? ")
                .or_else(|| line.strip_prefix("!! "))
            else {
                continue;
            };
            let Some(fname) = rejected.strip_suffix(".rej") else {
                continue;
            };
            let hooks_off = format!(
                "core.hooksPath={}",
                if cfg!(windows) { "NUL" } else { "/dev/null" }
            );
            git.run(&["-c", &hooks_off, "checkout", "--", fname])?;
            let (old_file, new_file) = (old_path.join(fname), new_path.join(fname));
            let merge_args: Vec<OsString> = [
                "merge-file",
                "-L",
                "before updating",
                "-L",
                "last update",
                "-L",
                "after updating",
                "--",
                fname,
            ]
            .into_iter()
            .map(OsString::from)
            .chain([old_file.into_os_string(), new_file.into_os_string()])
            .collect();
            // A non-zero status only says that conflicts remain.
            let _ = git.output(&merge_args)?;
            std::fs::remove_file(top.join(format!("{fname}.rej")))?;
            let merged = std::fs::read(top.join(fname))?;
            let has_markers = merged.split(|byte| *byte == b'\n').any(|line| {
                let line = line.trim_ascii_end();
                line == b"<<<<<<< before updating" || line == b">>>>>>> after updating"
            });
            if has_markers {
                conflicted.push(fname.to_owned());
            }
        }
        if conflicted.is_empty() {
            return Ok(());
        }
        let mut list_args = vec!["ls-files", "-z", "--stage", "--"];
        list_args.extend(conflicted.iter().map(String::as_str));
        let mut input_lines = Vec::new();
        for line in nul_paths(git, &list_args)? {
            let Some((meta, path)) = line.split_once('\t') else {
                continue;
            };
            let mut meta = meta.split_whitespace();
            let (Some(perms), Some(sha)) = (meta.next(), meta.next()) else {
                continue;
            };
            input_lines.push(format!("0 {}\t{path}", "0".repeat(40)));
            input_lines.push(format!("{perms} {sha} 2\t{path}"));
            for (stage, side) in [(1, old_path), (3, new_path)] {
                let file = side.join(path);
                let hash_args: Vec<OsString> = ["hash-object", "-w", "--"]
                    .into_iter()
                    .map(OsString::from)
                    .chain([file.into_os_string()])
                    .collect();
                if let Ok(side_sha) = git.run(&hash_args) {
                    input_lines.push(format!("{perms} {} {stage}\t{path}", side_sha.trim()));
                }
            }
        }
        git.run_with_stdin(
            &["update-index", "-z", "--index-info"],
            format!("{}\0", input_lines.join("\0")).as_bytes(),
        )?;
        Ok(())
    }
}

/// Report whether the project at `dst_path` can be updated to a newer template version.
///
/// # Errors
///
/// Fails when the project lacks the recorded template data needed to compare versions.
pub fn get_update_data(
    dst_path: &Path,
    answers_file: Option<&Path>,
    use_prereleases: bool,
) -> Result<UpdateData> {
    let mut worker = Worker::new()
        .dst_path(dst_path)
        .use_prereleases(use_prereleases);
    if let Some(answers_file) = answers_file {
        worker = worker.answers_file(answers_file);
    }
    let subproject = worker.subproject()?;
    let old_template = match subproject.template() {
        Some(template) if template.git_ref.is_some() => template,
        _ => {
            return Err(user_message(format!(
                "Cannot check because cannot obtain old template references from `{}`.",
                subproject.answers_relpath.display()
            )));
        }
    };
    let template = worker.template_ref()?;
    if template.commit()?.is_none() {
        return Err(user_message(
            "Checking is only supported in git-tracked templates.",
        ));
    }
    let Some(current) = old_template.version()? else {
        return Err(user_message(
            "Cannot check: version from last update not detected.",
        ));
    };
    let Some(latest) = template.version()? else {
        return Err(user_message(
            "Cannot check: version from template not detected.",
        ));
    };
    Ok(UpdateData {
        update_available: latest > current,
        current_version: current.to_string(),
        latest_version: latest.to_string(),
    })
}
