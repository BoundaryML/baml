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

    macro_rules! test_parse_outcomes {
        ($($name:ident: $fqid:expr => $expected:expr),* $(,)?) => {
            $(
                #[test]
                fn $name() {
                    let fqid_str = $fqid;
                    let expected_outcome = $expected.map_err(|e| e.to_string());
                    let actual_outcome = ProjectFqn::parse(fqid_str);
                    let actual_outcome = match actual_outcome {
                        Ok(_) => Ok(()),
                        Err(e) => Err(format!("{e}")),
                    };
                    assert_eq!(expected_outcome, actual_outcome, "Failed for fqid: {}", fqid_str);
                }
            )*
        };
    }

    test_parse_outcomes! {
        test_parse_org_proj: "org/proj" => Ok::<(), anyhow::Error>(()),
        test_parse_underscore_org: "_org/proj" => Ok::<(), anyhow::Error>(()),
        test_parse_dash_org: "-org/proj" => Err("'-org/proj' contains an invalid org name ('-org')"),
        test_parse_at_org: "@org/proj" => Err("'@org/proj' contains an invalid org name ('@org')"),
        test_parse_percent_org: "%org/proj" => Err("'%org/proj' contains an invalid org name ('%org')"),
        test_parse_numeric_org: "123/proj" => Ok::<(), anyhow::Error>(()),
        test_parse_org1: "org1/proj" => Ok::<(), anyhow::Error>(()),
        test_parse_org_dash_1: "org-1/proj" => Ok::<(), anyhow::Error>(()),
        test_parse_org_underscore_1: "org_1/proj" => Ok::<(), anyhow::Error>(()),
        test_parse_proj_dash_1: "org/proj-1" => Ok::<(), anyhow::Error>(()),
        test_parse_proj_underscore_1: "org/proj_1" => Ok::<(), anyhow::Error>(()),
        test_parse_proj_dash_end: "org/proj-" => Ok::<(), anyhow::Error>(()),
        test_parse_proj_underscore_end: "org/proj_" => Ok::<(), anyhow::Error>(()),
        test_parse_proj_numeric_start: "org/1proj" => Err("'org/1proj' contains an invalid project name ('1proj') - must start with a lowercase letter"),
        test_parse_proj_dash_start: "org/-proj" => Err("'org/-proj' contains an invalid project name ('-proj') - must start with a lowercase letter"),
        test_parse_proj_underscore_start: "org/_proj" => Err("'org/_proj' contains an invalid project name ('_proj') - must start with a lowercase letter"),
    }
}
