# n8n IT Ops Onboarding Pipeline
function N8nItOpsOnboard(user_id: string) -> string {
    ## Retrieve User From Microsoft Entra
    let entra_user = GetUserFromMicrosoftEntra(user_id);

    ## Create Jira Account
    let jira_user = CreateJiraUser(entra_user);

    ### Manager Routing
    let slack_action = if (IsManager(entra_user)) {
        #### Add To Slack Channel
        let channel_result = AddToSlackChannel(entra_user, "managers-oncall");
        channel_result
    } else {
        #### Update Slack Profile
        let profile_result = UpdateSlackProfile(entra_user);
        profile_result
    };

    ## Completion
    slack_action
}

function GetUserFromMicrosoftEntra(user_id: string) -> struct {
    id: string,
    email: string,
    is_manager: bool,
} {
    {
        id: user_id,
        email: "user@example.com",
        is_manager: user_id.starts_with("mgr"),
    }
}

function CreateJiraUser(entra_user: struct { id: string, email: string, is_manager: bool }) -> string {
    "jira-user-id"
}

function IsManager(entra_user: struct { id: string, email: string, is_manager: bool }) -> bool {
    entra_user.is_manager
}

function AddToSlackChannel(entra_user: struct { id: string, email: string, is_manager: bool }, channel: string) -> string {
    "added-to-channel"
}

function UpdateSlackProfile(entra_user: struct { id: string, email: string, is_manager: bool }) -> string {
    "profile-updated"
}
