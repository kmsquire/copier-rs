//! Template version detection from git history, following the `dunamai` library
//! Python Copier relies on.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::vcs::Git;
use crate::version::{self, Version};

static TAG_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    // dunamai's `Pattern.DefaultUnprefixed`; a literal, so it always compiles.
    #[allow(clippy::expect_used)]
    Regex::new(
        r"^v?((?P<epoch>\d+)!)?(?P<base>\d+(\.\d+)*)([-._]?((?P<stage>[a-zA-Z]+)[-._]?(?P<revision>\d+)?))?(\+(?P<tagged_metadata>.+))?$",
    )
    .expect("valid tag pattern")
});
static DESCRIBE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    Regex::new(r"^(.+)-(\d+)-(g\w+)$").expect("valid describe pattern")
});

/// PEP 440 version of the checkout at `repo`, whose `git describe` is `commit`.
///
/// Tags in the form dunamai understands yield `1.2.3` on the tag itself and
/// `1.2.3.post<N>.dev0+<hash>` after it. Otherwise the describe output is used.
pub(super) fn template_version(repo: &Path, commit: &str) -> Option<Version> {
    if let Some(serialized) = from_git(repo) {
        return version::parse(&serialized);
    }
    if let Some(captures) = DESCRIBE_SUFFIX.captures(commit) {
        return version::parse(&format!("{}.post{}+{}", &captures[1], &captures[2], &captures[3]));
    }
    version::parse(commit)
}

/// `dunamai.Version.from_git(pattern=DefaultUnprefixed).serialize(style=Pep440)`;
/// `None` where dunamai raises `ValueError`.
fn from_git(repo: &Path) -> Option<String> {
    let git = Git::in_dir(repo);
    let commit = git.run(&["log", "-n", "1", "--format=format:%h"]).ok()?.trim().to_owned();
    if commit.is_empty() {
        return Some("0.0.0".to_owned());
    }
    let decorated = git
        .run(&["log", "--simplify-by-decoration", "--topo-order", "--decorate=full", "HEAD", "--format=%H%d"])
        .ok()?;
    let tags: Vec<&str> = decorated
        .lines()
        .filter_map(|line| line.split_once(" (").map(|(_, refs)| refs.trim_end_matches(')')))
        .flat_map(|refs| refs.split(", "))
        .filter_map(|name| name.strip_prefix("tag: refs/tags/"))
        .collect();
    if tags.is_empty() {
        let distance: u64 = git.run(&["rev-list", "--count", "HEAD"]).ok()?.trim().parse().ok()?;
        return serialize("0.0.0", None, None, distance, &commit);
    }
    let (tag, captures) = tags.iter().find_map(|tag| TAG_PATTERN.captures(tag).map(|captures| (tag, captures)))?;
    let distance: u64 =
        git.run(&["rev-list", "--count", &format!("refs/tags/{tag}..HEAD")]).ok()?.trim().parse().ok()?;
    let base = match captures.name("epoch") {
        Some(epoch) => format!("{}!{}", epoch.as_str(), &captures["base"]),
        None => captures["base"].to_owned(),
    };
    let revision = captures.name("revision").and_then(|m| m.as_str().parse::<u64>().ok());
    serialize(&base, captures.name("stage").map(|m| m.as_str()), revision, distance, &commit)
}

fn serialize(base: &str, stage: Option<&str>, revision: Option<u64>, distance: u64, commit: &str) -> Option<String> {
    let (mut stage, mut post, mut dev) = (stage, None, None);
    match stage {
        Some("post") => {
            stage = None;
            post = revision;
        }
        Some("dev") => {
            stage = None;
            dev = revision;
        }
        _ => {}
    }
    if distance > 0 {
        match (post, dev) {
            (None, None) => {
                post = Some(distance);
                dev = Some(0);
            }
            (_, None) => dev = Some(distance),
            (_, Some(current)) => dev = Some(current + distance),
        }
    }
    let mut out = base.to_owned();
    if let Some(stage) = stage {
        let lower = stage.to_lowercase();
        out.push_str(match lower.as_str() {
            "alpha" => "a",
            "beta" => "b",
            "c" | "pre" | "preview" => "rc",
            other => other,
        });
        // dunamai validates its own output; unknown stages make it give up.
        if !matches!(lower.as_str(), "a" | "b" | "rc" | "alpha" | "beta" | "c" | "pre" | "preview") {
            return None;
        }
        out.push_str(&revision.unwrap_or(0).to_string());
    }
    if let Some(post) = post {
        out.push_str(&format!(".post{post}"));
    }
    if let Some(dev) = dev {
        out.push_str(&format!(".dev{dev}"));
    }
    if distance > 0 {
        out.push_str(&format!("+{commit}"));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialization() {
        assert_eq!(serialize("1.2.3", None, None, 0, "abc").as_deref(), Some("1.2.3"));
        assert_eq!(serialize("1.2.3", None, None, 3, "abc").as_deref(), Some("1.2.3.post3.dev0+abc"));
        assert_eq!(serialize("3.0.0", Some("alpha"), None, 0, "abc").as_deref(), Some("3.0.0a0"));
        assert_eq!(serialize("1.0", Some("rc"), Some(2), 1, "abc").as_deref(), Some("1.0rc2.post1.dev0+abc"));
        assert_eq!(serialize("1.0", Some("post"), Some(2), 1, "abc").as_deref(), Some("1.0.post2.dev1+abc"));
        assert_eq!(serialize("1.0", Some("foo"), None, 0, "abc"), None);
    }
}
