"""Compose the weekly release and changelog reminder."""

from datetime import date

from slack_sdk import WebClient

from .current import current_oncall
from .slack import email_for, lookup_user_id


def compose_release_reminder(
    today: date | None = None, wc: WebClient | None = None
) -> str:
    names = current_oncall(today, rotation="oncall-releases")
    mentions = [
        f"<@{lookup_user_id(wc, email_for(name))}>" if wc else f"@{name}"
        for name in names
    ]
    return (
        f"cc current oncall {' '.join(mentions)}: please prepare the changelog "
        "and put out this week's BAML release.\n\n"
        "See <https://github.com/BoundaryML/baml/blob/canary/"
        "baml_language/RELEASING.md|RELEASING.md> for the release steps."
    )
