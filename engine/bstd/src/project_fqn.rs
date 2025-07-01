use std::fmt::Display;

use anyhow::{Context, Result};
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFqn {
    /// Example: "boundaryml" for "boundaryml/my-project"
    org_slug: String,
    /// Example: "my-project" for "boundaryml/my-project"
    project_shortname: String,
}

impl ProjectFqn {
    pub fn is_valid_project_shortname(project_shortname: &str) -> Result<(), String> {
        let project_shortname_regex = Regex::new(r"^[a-z0-9_-]+$").map_err(|e| e.to_string())?;

        if !project_shortname_regex.is_match(project_shortname) {
            return Err(format!(
                "invalid project name ('{project_shortname}') - allowed characters: a-z, 0-9, -, and _"
            ));
        }
        if !project_shortname.starts_with(|c: char| c.is_ascii_lowercase()) {
            return Err(format!(
                "invalid project name ('{project_shortname}') - must start with a lowercase letter"
            ));
        }
        if project_shortname.contains("--") {
            return Err(format!(
                "invalid project name ('{project_shortname}') - cannot contain '--'"
            ));
        }
        Ok(())
    }

    pub fn new(org_slug: String, project_name: String) -> Self {
        Self {
            org_slug,
            project_shortname: project_name,
        }
    }

    pub fn parse(fqn: impl AsRef<str>) -> Result<Self> {
        let fqn = fqn.as_ref();
        let (org_slug, project_shortname) = fqn.split_once('/').context(format!(
            "'{fqn}' is not a valid fully-qualified project name - must specify both an org and project name"
        ))?;
        let org_slug = org_slug.to_string();
        let project_shortname = project_shortname.to_string();

        let org_slug_regex = Regex::new(r"^[a-z0-9_][a-z0-9_-]*$")
            .context("Failed to construct org name validator")?;

        if !org_slug_regex.is_match(&org_slug) {
            anyhow::bail!("'{}' contains an invalid org name ('{}')", fqn, org_slug);
        }
        Self::is_valid_project_shortname(&project_shortname)
            .map_err(|e| anyhow::anyhow!("'{}' contains an {e}", fqn))?;
        Ok(Self {
            org_slug,
            project_shortname,
        })
    }
}

impl Display for ProjectFqn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.org_slug, self.project_shortname)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_parse_outcomes() {
        let allowed_project_fqids = vec![
            ("org/proj", Ok(())),
            ("_org/proj", Ok(())),
            (
                "-org/proj",
                Err("'-org/proj' contains an invalid org name ('-org')"),
            ),
            (
                "@org/proj",
                Err("'@org/proj' contains an invalid org name ('@org')"),
            ),
            (
                "%org/proj",
                Err("'%org/proj' contains an invalid org name ('%org')"),
            ),
            ("123/proj", Ok(())),
            ("org1/proj", Ok(())),
            ("org-1/proj", Ok(())),
            ("org_1/proj", Ok(())),
            ("org/proj-1", Ok(())),
            ("org/proj_1", Ok(())),
            ("org/proj-", Ok(())),
            ("org/proj_", Ok(())),
            (
                "org/1proj",
                Err("'org/1proj' contains an invalid project name ('1proj') - must start with a lowercase letter"),
            ),
            (
                "org/-proj",
                Err("'org/-proj' contains an invalid project name ('-proj') - must start with a lowercase letter"),
            ),
            (
                "org/_proj",
                Err("'org/_proj' contains an invalid project name ('_proj') - must start with a lowercase letter"),
            ),
        ];

        let parse_failures = allowed_project_fqids
            .iter()
            .map(|(fqid_str, expected_outcome)| {
                let expected_outcome = expected_outcome.map_err(|e| e.to_string());
                let fqid = ProjectFqn::parse(fqid_str);
                match fqid {
                    Ok(_) => (fqid_str, expected_outcome, Ok(())),
                    Err(e) => (fqid_str, expected_outcome, Err(format!("{e:?}"))),
                }
            })
            .filter_map(|(fqid_str, expected_outcome, actual_outcome)| {
                if expected_outcome == actual_outcome {
                    None
                } else {
                    Some((fqid_str, expected_outcome, actual_outcome))
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(parse_failures, vec![]);
    }
}
