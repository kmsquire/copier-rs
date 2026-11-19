//! Rendering of the template tree into the project (`Worker._render_template`).

use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

use minijinja::value::Value as JinjaValue;

use super::Worker;
use crate::errors::{warn, CopierError, Result, WarningKind};
use crate::jinja::{from_jinja, Context};
use crate::output::{self, Status};
use crate::pathspec::PathSpec;
use crate::prompt::PromptError;
use crate::tools;
use crate::vcs::{self, Git};

/// Variables bound by `yield` tags along a rendered path.
type ExtraContext = Vec<(String, JinjaValue)>;

/// What a destination path is expected to hold after rendering.
enum Expected<'a> {
    Dir,
    File { contents: &'a [u8], mode: u32 },
    Symlink { target: &'a Path },
}

/// One entry of the template tree.
struct Entry {
    path: PathBuf,
    is_symlink: bool,
    is_dir: bool,
}

/// Walk `root` depth-first, parents before children, in name order. Symlinked
/// directories are entered only when `follow_symlinks` is set.
fn scan_tree(root: &Path, follow_symlinks: bool, out: &mut Vec<Entry>) -> Result<()> {
    let mut children: Vec<PathBuf> = std::fs::read_dir(root)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<_>>()?;
    children.sort();
    for path in children {
        let is_symlink = path.is_symlink();
        let is_dir = if is_symlink && !follow_symlinks {
            false
        } else {
            path.is_dir()
        };
        out.push(Entry {
            path: path.clone(),
            is_symlink,
            is_dir,
        });
        if is_dir {
            scan_tree(&path, follow_symlinks, out)?;
        }
    }
    Ok(())
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn posix_text(path: &Path) -> String {
    path_text(path).replace('\\', "/")
}

#[cfg(unix)]
fn mode_of(metadata: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;
    metadata.mode()
}

#[cfg(not(unix))]
fn mode_of(metadata: &std::fs::Metadata) -> u32 {
    if metadata.permissions().readonly() {
        0o100444
    } else {
        0o100644
    }
}

/// `stat.filemode`: the `ls -l` rendering of a regular file's mode.
fn filemode(mode: u32) -> String {
    let mut text = String::from("-");
    for shift in [6, 3, 0] {
        let bits = (mode >> shift) & 0o7;
        text.push(if bits & 4 != 0 { 'r' } else { '-' });
        text.push(if bits & 2 != 0 { 'w' } else { '-' });
        text.push(if bits & 1 != 0 { 'x' } else { '-' });
    }
    text
}

impl Worker {
    /// Directory of the template whose contents are rendered (`_subdirectory`).
    pub(super) fn template_copy_root(&self) -> Result<&PathBuf> {
        super::cached(&self.cache.template_copy_root, || {
            let template = self.template_ref()?;
            let root = template.local_abspath()?;
            let subdirectory = self.render_string(&template.subdirectory()?, &[])?;
            let path = tools::resolve(&root.join(&subdirectory));
            if !path.starts_with(&root) {
                return Err(CopierError::ForbiddenPath {
                    path: PathBuf::from(subdirectory),
                    hint: String::new(),
                });
            }
            Ok(path)
        })
    }

    /// The exclusion matcher for the current operation.
    pub(super) fn match_exclude(&self) -> Result<Rc<PathSpec>> {
        if let Some(spec) = self.cache.match_exclude.borrow().as_ref() {
            return Ok(Rc::clone(spec));
        }
        let extra = [Self::operation_variable()];
        let mut patterns = Vec::new();
        for exclusion in self.all_exclusions()? {
            let rendered = self.render_string(&exclusion, &extra)?;
            patterns.extend(
                rendered
                    .replace("\r\n", "\n")
                    .split(['\r', '\n'])
                    .map(str::to_owned),
            );
        }
        let spec = Rc::new(PathSpec::new(patterns)?);
        *self.cache.match_exclude.borrow_mut() = Some(Rc::clone(&spec));
        Ok(spec)
    }

    /// Forget the exclusion matcher: its patterns may depend on the operation.
    pub(super) fn reset_match_exclude(&self) {
        *self.cache.match_exclude.borrow_mut() = None;
    }

    pub(super) fn match_skip(&self) -> Result<&PathSpec> {
        super::cached(&self.cache.match_skip, || {
            let patterns: Vec<String> = self
                .all_skip_if_exists()?
                .iter()
                .map(|pattern| self.render_string(pattern, &[]))
                .collect::<Result<_>>()?;
            PathSpec::new(patterns)
        })
    }

    /// Render every file, folder and symlink of the template into the project.
    pub(super) fn render_template(&self) -> Result<()> {
        let template = self.template_ref()?;
        let template_root = template.local_abspath()?;
        let copy_root = self.template_copy_root()?.clone();
        let preserve_symlinks = template.preserve_symlinks()?;
        let dst_root = tools::resolve(&self.dst_path);
        let exclude = self.match_exclude()?;
        let mut entries = Vec::new();
        scan_tree(&copy_root, !preserve_symlinks, &mut entries)?;
        for entry in entries {
            let copy_relpath = entry
                .path
                .strip_prefix(&copy_root)
                .unwrap_or(&entry.path)
                .to_path_buf();
            if entry.is_symlink
                && !preserve_symlinks
                && !tools::resolve(&entry.path).starts_with(&template_root)
            {
                return Err(CopierError::ForbiddenPath {
                    path: copy_relpath,
                    hint: String::new(),
                });
            }
            let src_relpath = entry
                .path
                .strip_prefix(&template_root)
                .unwrap_or(&entry.path)
                .to_path_buf();
            for (dst_relpath, extra) in self.render_path(&copy_relpath)? {
                let dst_abspath = dst_root.join(&dst_relpath);
                // A symlink destination is judged by where the link lives, not its target.
                let dst_realpath = match (
                    dst_abspath.is_symlink(),
                    dst_abspath.parent(),
                    dst_abspath.file_name(),
                ) {
                    (true, Some(parent), Some(name)) => tools::resolve(parent).join(name),
                    _ => tools::resolve(&dst_abspath),
                };
                if !dst_realpath.starts_with(&dst_root) {
                    return Err(CopierError::ForbiddenPath {
                        path: dst_relpath,
                        hint: String::new(),
                    });
                }
                if exclude.matches(&path_text(&dst_relpath)) {
                    continue;
                }
                if entry.is_symlink && preserve_symlinks {
                    self.render_symlink(&src_relpath, &dst_relpath)?;
                } else if entry.is_dir {
                    self.render_folder(&dst_relpath)?;
                } else {
                    self.render_file(&src_relpath, &dst_relpath, &extra)?;
                }
            }
        }
        Ok(())
    }

    /// Destination paths (with their `yield` variables) of one template path.
    fn render_path(&self, relpath: &Path) -> Result<Vec<(PathBuf, ExtraContext)>> {
        let suffix = self.template_ref()?.templates_suffix()?;
        let file_name = relpath
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let is_template = file_name.ends_with(&suffix);
        let mut relpath = relpath.to_path_buf();
        if !suffix.is_empty() {
            // A plain file is shadowed by its templated sibling.
            let sibling = self
                .template_copy_root()?
                .join(format!("{}{suffix}", path_text(&relpath)));
            if sibling.exists() {
                return Ok(Vec::new());
            }
            if is_template {
                relpath.set_extension("");
            }
        }
        let parts: Vec<String> = relpath
            .components()
            .filter_map(|component| match component {
                Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        let mut rendered = Vec::new();
        self.render_parts(
            &parts,
            PathBuf::new(),
            Vec::new(),
            is_template,
            PathBuf::new(),
            &mut rendered,
        )?;
        Ok(rendered)
    }

    fn render_part(&self, part: &str, extra: &ExtraContext, source_path: &Path) -> Result<String> {
        self.render_string(part, extra)
            .map_err(|error| match error {
                CopierError::Template { message, .. } => CopierError::UserMessage(format!(
                    "Error rendering template path {}: {message}",
                    source_path.display()
                )),
                other => other,
            })
    }

    fn render_parts(
        &self,
        parts: &[String],
        rendered_parts: PathBuf,
        extra: ExtraContext,
        is_template: bool,
        source_parts: PathBuf,
        out: &mut Vec<(PathBuf, ExtraContext)>,
    ) -> Result<()> {
        let template = self.template_ref()?;
        let Some((part, rest)) = parts.split_first() else {
            let sibling = template.local_abspath()?.join(format!(
                "{}{}",
                path_text(&rendered_parts),
                template.templates_suffix()?
            ));
            if is_template || !sibling.exists() {
                out.push((rendered_parts, extra));
            }
            return Ok(());
        };
        let source_path = source_parts.join(part);
        let answers_relpath = self.answers_relpath()?;
        let emit = |rendered_part: String,
                    extra: ExtraContext,
                    out: &mut Vec<(PathBuf, ExtraContext)>|
         -> Result<()> {
            if path_text(&answers_relpath) == rendered_part {
                if !rendered_parts.as_os_str().is_empty() {
                    warn(
                        WarningKind::Deprecation,
                        "Answers file template locations other than the template root directory are deprecated. Please move the answers file template to the template root directory.",
                    );
                }
                out.push((answers_relpath.clone(), extra));
                return Ok(());
            }
            if rendered_part.is_empty() {
                return Ok(());
            }
            self.render_parts(
                rest,
                rendered_parts.join(rendered_part),
                extra,
                is_template,
                source_path.clone(),
                out,
            )
        };
        let env = self.env()?;
        let Some(tag) = env.yield_tag(part)? else {
            let rendered_part = self.render_part(part, &extra, &source_path)?;
            return emit(rendered_part, extra, out);
        };
        let wrap = |error: CopierError| match error {
            CopierError::Template { message, .. } => CopierError::UserMessage(format!(
                "Error rendering template path {}: {message}",
                source_path.display()
            )),
            other => other,
        };
        let mut context: Context = self.context()?;
        for (name, value) in &extra {
            context.insert(name.clone(), value.clone());
        }
        let iterable = env.eval(&tag.iterable, &context).map_err(wrap)?;
        let items: Vec<JinjaValue> = match iterable.try_iter() {
            Ok(items) => items.collect(),
            // Python iterates `iterable or ()`: nothing to yield.
            Err(_) if !iterable.is_true() => Vec::new(),
            Err(_) => {
                return Err(wrap(CopierError::Template {
                    kind: crate::errors::TemplateErrorKind::Other,
                    message: format!(
                        "'{}' object is not iterable",
                        from_jinja(&iterable).py_type_name()
                    ),
                }));
            }
        };
        for item in items {
            let mut item_extra = extra.clone();
            item_extra.retain(|(name, _)| name != &tag.name);
            item_extra.push((tag.name.clone(), item));
            let rendered_part = self.render_part(&tag.body, &item_extra, &source_path)?;
            emit(rendered_part, item_extra, out)?;
        }
        Ok(())
    }

    fn render_file(
        &self,
        src_relpath: &Path,
        dst_relpath: &Path,
        extra: &ExtraContext,
    ) -> Result<()> {
        let template = self.template_ref()?;
        let suffix = template.templates_suffix()?;
        let src_abspath = template.local_abspath()?.join(src_relpath);
        let raw = std::fs::read(&src_abspath)?;
        let is_template = src_relpath
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(&suffix));
        let new_content = match (is_template, String::from_utf8(raw)) {
            (false, Ok(text)) => text.into_bytes(),
            (false, Err(binary)) => binary.into_bytes(),
            // With an empty suffix every file is a template, binary ones included.
            (true, Err(binary)) if suffix.is_empty() => binary.into_bytes(),
            (true, Err(binary)) => {
                let error = binary.utf8_error();
                return Err(CopierError::Value(format!(
                    "'utf-8' codec can't decode byte 0x{:02x} in position {}: invalid start byte",
                    binary.as_bytes()[error.valid_up_to()],
                    error.valid_up_to()
                )));
            }
            (true, Ok(source)) => {
                let env = self.env()?;
                if env.has_yield_tag(&source) {
                    return Err(CopierError::YieldTagInFile(format!(
                        "File {} contains a yield tag, but it is not allowed.",
                        src_relpath.display()
                    )));
                }
                let mut context = self.context()?;
                for (name, value) in extra {
                    context.insert(name.clone(), value.clone());
                }
                env.render_file(&posix_text(src_relpath), &context)?
                    .into_bytes()
            }
        };
        let stat_mode = mode_of(&std::fs::metadata(&src_abspath)?);
        // Git's index is the authority on executable bits: the filesystem may not
        // be able to represent them.
        let src_mode = match template.git_index_modes().get(&posix_text(src_relpath)) {
            Some(git_mode) => (stat_mode & !0o111) | (git_mode & 0o111),
            None => stat_mode,
        };
        if !self.render_allowed(
            dst_relpath,
            &Expected::File {
                contents: &new_content,
                mode: src_mode,
            },
        )? {
            return Ok(());
        }
        if self.pretend {
            return Ok(());
        }
        let dst_abspath = self.subproject()?.local_abspath.join(dst_relpath);
        if let Some(parent) = dst_abspath.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if dst_abspath.is_symlink() {
            std::fs::remove_file(&dst_abspath)?;
        }
        std::fs::write(&dst_abspath, &new_content)?;
        let dst_mode = mode_of(&std::fs::metadata(&dst_abspath)?);
        if dst_mode != src_mode {
            if let Err(error) = set_mode(&dst_abspath, src_mode) {
                if error.kind() != std::io::ErrorKind::PermissionDenied {
                    return Err(error.into());
                }
                warn(
                    WarningKind::User,
                    format!(
                        "Path permissions for {} cannot be changed from {} to {}",
                        dst_abspath.display(),
                        filemode(dst_mode),
                        filemode(src_mode)
                    ),
                );
            }
        }
        if vcs::is_git_available() {
            self.sync_git_index_executable_bit(dst_relpath, src_mode)?;
        }
        Ok(())
    }

    /// Record the executable bit in git's index when the repository ignores file
    /// modes (`core.fileMode=false`), so the bit survives on such filesystems.
    fn sync_git_index_executable_bit(&self, dst_relpath: &Path, src_mode: u32) -> Result<()> {
        let git = Git::in_dir(&self.subproject()?.local_abspath);
        let ignores_file_mode = *self.cache.ignores_file_mode.get_or_init(|| {
            git.run(&["config", "--type=bool", "--get", "core.fileMode"])
                .is_ok_and(|value| value.trim() == "false")
        });
        if !ignores_file_mode {
            return Ok(());
        }
        let relpath = path_text(dst_relpath);
        let Ok(staged) = git.run(&["ls-files", "--stage", "--", &relpath]) else {
            return Ok(());
        };
        let mut meta = staged
            .trim()
            .split('\t')
            .next()
            .unwrap_or_default()
            .split_whitespace();
        let (Some(mode), Some(sha)) = (
            meta.next()
                .and_then(|mode| u32::from_str_radix(mode, 8).ok()),
            meta.next(),
        ) else {
            return Ok(());
        };
        let desired_executable = src_mode & 0o111 != 0;
        if desired_executable == (mode & 0o111 != 0) {
            return Ok(());
        }
        let new_mode = if desired_executable {
            "100755"
        } else {
            "100644"
        };
        // Best effort, like the Python implementation.
        let _ = git.run(&[
            "update-index",
            "--cacheinfo",
            &format!("{new_mode},{sha},{relpath}"),
        ]);
        Ok(())
    }

    fn render_symlink(&self, src_relpath: &Path, dst_relpath: &Path) -> Result<()> {
        let template = self.template_ref()?;
        let src_abspath = template.local_abspath()?.join(src_relpath);
        let src_target = std::fs::read_link(&src_abspath)?;
        let suffix = template.templates_suffix()?;
        let is_template = src_abspath
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(&suffix));
        let dst_target = if is_template {
            PathBuf::from(self.render_string(&path_text(&src_target), &[])?)
        } else {
            src_target
        };
        if !self.render_allowed(
            dst_relpath,
            &Expected::Symlink {
                target: &dst_target,
            },
        )? {
            return Ok(());
        }
        if self.pretend {
            return Ok(());
        }
        let dst_abspath = self.subproject()?.local_abspath.join(dst_relpath);
        if dst_abspath.is_symlink() || dst_abspath.exists() {
            std::fs::remove_file(&dst_abspath)?;
        }
        if let Some(parent) = dst_abspath.parent() {
            std::fs::create_dir_all(parent)?;
        }
        make_symlink(&dst_target, &dst_abspath)?;
        Ok(())
    }

    fn render_folder(&self, dst_relpath: &Path) -> Result<()> {
        if !self.pretend && self.render_allowed(dst_relpath, &Expected::Dir)? {
            std::fs::create_dir_all(self.subproject()?.local_abspath.join(dst_relpath))?;
        }
        Ok(())
    }

    fn status(&self, action: &str, dst_relpath: &Path, status: Status) {
        if !self.quiet {
            output::printf(action, &path_text(dst_relpath), status, 10);
        }
    }

    /// Decide whether `dst_relpath` may be written, reporting what happens to it.
    fn render_allowed(&self, dst_relpath: &Path, expected: &Expected<'_>) -> Result<bool> {
        let dst_abspath = self.subproject()?.local_abspath.join(dst_relpath);
        let previous_is_symlink = dst_abspath.is_symlink();
        let exists = previous_is_symlink || dst_abspath.exists();
        if !exists {
            self.status("create", dst_relpath, Status::Ok);
            return Ok(true);
        }
        let identical = match expected {
            Expected::Dir => true,
            Expected::Symlink { target } => {
                previous_is_symlink
                    && std::fs::read_link(&dst_abspath).is_ok_and(|previous| previous == *target)
            }
            Expected::File { contents, mode } => {
                let same_contents = !previous_is_symlink
                    && std::fs::read(&dst_abspath).is_ok_and(|previous| previous == *contents);
                let same_mode = previous_is_symlink
                    || std::fs::metadata(&dst_abspath)
                        .is_ok_and(|metadata| mode_of(&metadata) & 0o111 == mode & 0o111);
                same_contents && same_mode
            }
        };
        if identical {
            self.status("identical", dst_relpath, Status::Ignore);
            return Ok(matches!(expected, Expected::Dir));
        }
        self.solve_render_conflict(dst_relpath)
    }

    fn solve_render_conflict(&self, dst_relpath: &Path) -> Result<bool> {
        self.status("conflict", dst_relpath, Status::Danger);
        if self.match_skip()?.matches(&path_text(dst_relpath)) {
            self.status("skip", dst_relpath, Status::Ok);
            return Ok(false);
        }
        if self.overwrite || dst_relpath == self.answers_relpath()? {
            self.status("overwrite", dst_relpath, Status::Warning);
            return Ok(true);
        }
        let answer = self
            .prompter
            .borrow_mut()
            .confirm(&format!(" Overwrite {}?", dst_relpath.display()), true);
        match answer {
            Ok(answer) => Ok(answer),
            Err(PromptError::Eof) => Err(CopierError::InteractiveSession(
                "Consider using `--overwrite`".to_owned(),
            )),
            Err(PromptError::Interrupted) => Err(CopierError::UserMessage(
                "Execution stopped by user".to_owned(),
            )),
        }
    }
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & 0o7777))
}

#[cfg(not(unix))]
fn set_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_readonly(mode & 0o200 == 0);
    std::fs::set_permissions(path, permissions)
}

#[cfg(unix)]
fn make_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn make_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    if link
        .parent()
        .is_some_and(|parent| parent.join(target).is_dir())
    {
        std::os::windows::fs::symlink_dir(target, link)
    } else {
        std::os::windows::fs::symlink_file(target, link)
    }
}
