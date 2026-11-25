//! `baml cloud members` and `baml cloud invitations`.

use anyhow::{Context as _, Result};
use bcs_api::cloud::{
    members::{Invitation, Member},
    organizations::Role,
};
use clap::{Args, Subcommand, ValueEnum};

use super::{Cloud, CloudCommand, CloudScope, Detail, Fields, Listing, Row, optional, time, word};
use crate::{
    ExitCode,
    output::{CommandContext, Outcome},
    reporter::Reporter,
};

#[derive(ValueEnum, Clone, Copy, Debug, Default)]
pub(crate) enum RoleArg {
    #[default]
    Member,
    Admin,
}

impl From<RoleArg> for Role {
    fn from(role: RoleArg) -> Self {
        match role {
            RoleArg::Member => Self::Member,
            RoleArg::Admin => Self::Admin,
        }
    }
}

#[derive(Subcommand, Debug)]
pub(crate) enum MembersCommands {
    /// List the organization's members.
    #[command(
        after_long_help = "Examples:\n  List the members of the selected organization:\n    baml cloud members list\n\n  As JSON:\n    baml cloud members list --output-format json"
    )]
    List(MembersListArgs),
    /// Change a member's role.
    #[command(
        after_long_help = "Examples:\n  Make a member an administrator:\n    baml cloud members update alice@acme.com --role admin"
    )]
    Update(MembersUpdateArgs),
    /// Remove a member from the organization.
    #[command(
        after_long_help = "Examples:\n  Remove a member:\n    baml cloud members delete alice@acme.com\n\n  Remove a member and revoke the API keys they created:\n    baml cloud members delete alice@acme.com --revoke-keys"
    )]
    Delete(MembersDeleteArgs),
}

impl MembersCommands {
    pub(crate) fn run(&self, cx: &CommandContext, scope: &CloudScope) -> Result<ExitCode> {
        match self {
            Self::List(args) => super::run(args, cx, scope),
            Self::Update(args) => super::run(args, cx, scope),
            Self::Delete(args) => super::run(args, cx, scope),
        }
    }
}

#[derive(Args, Debug)]
pub(crate) struct MembersListArgs {}

#[derive(Args, Debug)]
pub(crate) struct MembersUpdateArgs {
    /// The member, by email, user ID or membership ID.
    pub user: String,

    /// The member's new role.
    #[arg(long, value_enum)]
    pub role: RoleArg,
}

#[derive(Args, Debug)]
pub(crate) struct MembersDeleteArgs {
    /// The member, by email, user ID or membership ID.
    pub user: String,

    /// Also revoke the API keys the member created.
    #[arg(long)]
    pub revoke_keys: bool,
}

impl Row for Member {
    const COLUMNS: &'static [&'static str] = &["EMAIL", "NAME", "ROLE", "STATUS", "ID"];

    fn cells(&self) -> Vec<String> {
        vec![
            self.email.clone(),
            optional(self.name.as_deref()),
            word(&self.role),
            word(&self.status),
            self.id.clone(),
        ]
    }
}

impl Fields for Member {
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("Email", self.email.clone()),
            ("Name", optional(self.name.as_deref())),
            ("Role", word(&self.role)),
            ("Status", word(&self.status)),
            ("ID", self.id.clone()),
            ("User ID", self.user_id.clone()),
        ]
    }
}

/// The member `user` names in `organization`: an email, a user ID or a
/// membership ID.
fn find_member(cloud: &Cloud<'_>, organization: &str, user: &str) -> Result<Member> {
    cloud
        .client
        .list_members(organization)?
        .members
        .into_iter()
        .find(|member| {
            member.id == user || member.user_id == user || member.email.eq_ignore_ascii_case(user)
        })
        .with_context(|| format!("the organization has no member `{user}`"))
}

impl CloudCommand for MembersListArgs {
    type Output = Listing<Member>;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        let members = cloud.client.list_members(&cloud.organization()?.id)?;
        Ok(Outcome::ok(Listing(members.members)))
    }
}

impl CloudCommand for MembersUpdateArgs {
    type Output = Detail<Member>;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        let organization = &cloud.organization()?.id;
        let member = find_member(cloud, organization, &self.user)?;
        let member = cloud
            .client
            .change_member_role(organization, &member.id, self.role.into())?;
        Ok(Outcome::ok(Detail(member)))
    }
}

impl CloudCommand for MembersDeleteArgs {
    type Output = ();

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<()>> {
        let organization = cloud.organization()?;
        let member = find_member(cloud, &organization.id, &self.user)?;
        cloud
            .client
            .remove_member(&organization.id, &member.id, self.revoke_keys)?;
        Reporter::new().status(
            "Removed",
            format!("{} from {}", member.email, organization.name),
        );
        Ok(Outcome::ok(()))
    }
}

#[derive(Subcommand, Debug)]
pub(crate) enum InvitationsCommands {
    /// List the organization's pending invitations.
    #[command(
        after_long_help = "Examples:\n  List pending invitations:\n    baml cloud invitations list"
    )]
    List(InvitationsListArgs),
    /// Invite someone to the organization by email.
    #[command(
        after_long_help = "Examples:\n  Invite a member:\n    baml cloud invitations create alice@acme.com\n\n  Invite an administrator:\n    baml cloud invitations create alice@acme.com --role admin"
    )]
    Create(InvitationsCreateArgs),
    /// Revoke a pending invitation.
    #[command(
        after_long_help = "Examples:\n  Revoke an invitation by the ID `baml cloud invitations list` shows:\n    baml cloud invitations delete inv_123"
    )]
    Delete(InvitationsDeleteArgs),
}

impl InvitationsCommands {
    pub(crate) fn run(&self, cx: &CommandContext, scope: &CloudScope) -> Result<ExitCode> {
        match self {
            Self::List(args) => super::run(args, cx, scope),
            Self::Create(args) => super::run(args, cx, scope),
            Self::Delete(args) => super::run(args, cx, scope),
        }
    }
}

#[derive(Args, Debug)]
pub(crate) struct InvitationsListArgs {}

#[derive(Args, Debug)]
pub(crate) struct InvitationsCreateArgs {
    /// Who to invite.
    pub email: String,

    /// The role the invitation grants.
    #[arg(long, value_enum, default_value_t)]
    pub role: RoleArg,
}

#[derive(Args, Debug)]
pub(crate) struct InvitationsDeleteArgs {
    /// The invitation's ID.
    pub id: String,
}

impl Row for Invitation {
    const COLUMNS: &'static [&'static str] = &["EMAIL", "ROLE", "EXPIRES", "ID"];

    fn cells(&self) -> Vec<String> {
        vec![
            self.email.clone(),
            word(&self.role),
            time(self.expires_at),
            self.id.clone(),
        ]
    }
}

impl Fields for Invitation {
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("Email", self.email.clone()),
            ("Role", word(&self.role)),
            ("Expires", time(self.expires_at)),
            ("ID", self.id.clone()),
        ]
    }
}

impl CloudCommand for InvitationsListArgs {
    type Output = Listing<Invitation>;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        let members = cloud.client.list_members(&cloud.organization()?.id)?;
        Ok(Outcome::ok(Listing(members.invitations)))
    }
}

impl CloudCommand for InvitationsCreateArgs {
    type Output = Detail<Invitation>;

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<Self::Output>> {
        let invitation =
            cloud
                .client
                .invite_member(&cloud.organization()?.id, &self.email, self.role.into())?;
        Ok(Outcome::ok(Detail(invitation)))
    }
}

impl CloudCommand for InvitationsDeleteArgs {
    type Output = ();

    fn run(&self, cloud: &Cloud<'_>) -> Result<Outcome<()>> {
        let invitation = cloud
            .client
            .revoke_invitation(&cloud.organization()?.id, &self.id)?;
        Reporter::new().status("Revoked", format!("the invitation to {}", invitation.email));
        Ok(Outcome::ok(()))
    }
}
