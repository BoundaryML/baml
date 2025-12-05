#!/usr/bin/env -S uv run --script
# /// script
# dependencies = []
# ///
"""
Summarize BEP changes between origin/canary and HEAD for Slack notifications.
"""
import subprocess
import re
import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]


def run_git(args: list[str]) -> str:
    """Run git command and return stdout."""
    try:
        result = subprocess.run(
            ["git"] + args,
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
        return result.stdout.strip()
    except subprocess.CalledProcessError:
        return ""


def get_canary_ref() -> str:
    """Get the canary branch reference."""
    # Try origin/canary first (CI), then local canary
    result = subprocess.run(
        ["git", "rev-parse", "--verify", "origin/canary"],
        cwd=REPO_ROOT,
        capture_output=True,
        check=False
    )
    if result.returncode == 0:
        return "origin/canary"
    return "canary"


def get_changed_files() -> dict:
    """Get files changed between canary and HEAD in beps/docs/proposals/."""
    canary_ref = get_canary_ref()
    
    # Get diff with status (A=added, M=modified, D=deleted)
    diff_output = run_git([
        "diff", "--name-status", canary_ref, "HEAD", "--", "beps/docs/proposals/"
    ])
    
    changes = {"added": [], "modified": [], "deleted": []}
    
    for line in diff_output.splitlines():
        if not line.strip():
            continue
        parts = line.split("\t", 1)
        if len(parts) != 2:
            continue
        status, filepath = parts
        
        if status.startswith("A"):
            changes["added"].append(filepath)
        elif status.startswith("M"):
            changes["modified"].append(filepath)
        elif status.startswith("D"):
            changes["deleted"].append(filepath)
    
    return changes


def extract_bep_info(filepath: str) -> dict | None:
    """Extract BEP ID, title, status, and feedback link from a file path."""
    # Match BEP-XXX pattern in path (may have suffix like BEP-001-exceptions)
    match = re.search(r"(BEP-\d+)(-[^/]+)?", filepath)
    if not match:
        return None
    
    bep_id = match.group(1)
    bep_dir_name = match.group(0)  # Full match including suffix
    
    title = None
    status = "Draft"  # Default to Draft
    feedback = None
    
    # Try to get title, status, and feedback from README.md frontmatter
    readme_path = REPO_ROOT / f"beps/docs/proposals/{bep_dir_name}/README.md"
    if readme_path.exists():
        try:
            content = readme_path.read_text()
            if content.startswith("---"):
                frontmatter_end = content.find("---", 3)
                if frontmatter_end != -1:
                    frontmatter = content[3:frontmatter_end]
                    for line in frontmatter.splitlines():
                        if line.startswith("title:"):
                            title = line.split(":", 1)[1].strip().strip('"')
                        elif line.startswith("status:"):
                            status = line.split(":", 1)[1].strip().strip('"')
                        elif line.startswith("feedback:"):
                            feedback = line.split(":", 1)[1].strip().strip('"')
        except Exception:
            pass
    
    return {"id": bep_id, "title": title, "dir": bep_dir_name, "status": status, "feedback": feedback}


def generate_summary(base_url: str = "") -> dict:
    """Generate a summary of changes for Slack.
    
    Args:
        base_url: Base URL for BEP links (e.g., https://beps.example.com/canary)
    """
    changes = get_changed_files()
    
    # Group by BEP
    beps_touched = {}
    
    for filepath in changes["added"] + changes["modified"]:
        info = extract_bep_info(filepath)
        if info:
            bep_id = info["id"]
            if bep_id not in beps_touched:
                beps_touched[bep_id] = {
                    "title": info["title"],
                    "dir": info.get("dir", bep_id),
                    "status": info.get("status", "Draft"),
                    "feedback": info.get("feedback"),
                    "added": 0,
                    "modified": 0,
                }
            
            if filepath in changes["added"]:
                beps_touched[bep_id]["added"] += 1
            else:
                beps_touched[bep_id]["modified"] += 1
    
    # Filter out Draft BEPs for notifications
    non_draft_beps = {k: v for k, v in beps_touched.items() if v["status"] != "Draft"}
    
    # Build summary text
    total_added = len(changes["added"])
    total_modified = len(changes["modified"])
    total_deleted = len(changes["deleted"])
    
    summary_parts = []
    
    if total_added:
        summary_parts.append(f"{total_added} file{'s' if total_added != 1 else ''} added")
    if total_modified:
        summary_parts.append(f"{total_modified} file{'s' if total_modified != 1 else ''} modified")
    if total_deleted:
        summary_parts.append(f"{total_deleted} file{'s' if total_deleted != 1 else ''} deleted")
    
    file_summary = ", ".join(summary_parts) if summary_parts else "No file changes"
    
    # Build BEP list with links (only non-Draft)
    bep_lines = []
    bep_details = []
    for bep_id, info in sorted(non_draft_beps.items()):
        title = info["title"] or "Untitled"
        bep_dir = info["dir"]
        status = info["status"]
        parts = []
        if info["added"]:
            parts.append(f"+{info['added']}")
        if info["modified"]:
            parts.append(f"~{info['modified']}")
        change_str = " ".join(parts)
        
        # Build link if base_url provided
        if base_url:
            bep_url = f"{base_url.rstrip('/')}/proposals/{bep_dir}/"
            bep_lines.append(f"• <{bep_url}|*{bep_id}*: {title}> [{status}] ({change_str})")
        else:
            bep_lines.append(f"• *{bep_id}*: {title} [{status}] ({change_str})")
        
        bep_details.append({
            "id": bep_id,
            "title": title,
            "dir": bep_dir,
            "status": status,
            "feedback": info.get("feedback"),
            "added": info["added"],
            "modified": info["modified"],
        })
    
    bep_summary = "\n".join(bep_lines) if bep_lines else "No BEP changes detected"
    
    return {
        "file_summary": file_summary,
        "bep_summary": bep_summary,
        "beps_touched": list(non_draft_beps.keys()),
        "bep_details": bep_details,
        "total_files": total_added + total_modified + total_deleted,
    }


def generate_slack_blocks(summary: dict, env: str, base_url: str, workflow_url: str = "") -> dict:
    """Generate Slack Block Kit payload for rich notifications."""
    
    # If no non-draft BEPs, return empty payload
    if not summary.get("bep_details"):
        return {"blocks": []}
    
    env_label = "Canary" if env == "canary" else f"Preview ({env})"
    
    blocks = [
        {
            "type": "header",
            "text": {
                "type": "plain_text",
                "text": f"BEPs Deployed — {env_label}",
                "emoji": False
            }
        },
        {
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": f"*Changes vs canary:* {summary['file_summary']}"
            }
        },
        {"type": "divider"},
    ]
    
    # Add each BEP as a section
    for bep in summary.get("bep_details", []):
        change_parts = []
        if bep["added"]:
            change_parts.append(f"+{bep['added']} added")
        if bep["modified"]:
            change_parts.append(f"~{bep['modified']} modified")
        change_str = ", ".join(change_parts) if change_parts else "no changes"
        
        bep_url = f"{base_url.rstrip('/')}/proposals/{bep['dir']}/"
        status = bep.get("status", "Proposed")
        feedback_url = bep.get("feedback")
        
        blocks.append({
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": f"*<{bep_url}|{bep['id']}: {bep['title']}>*\n{status} · {change_str}"
            }
        })
        
        # Add buttons row
        buttons = [
            {
                "type": "button",
                "text": {
                    "type": "plain_text",
                    "text": "View",
                    "emoji": False
                },
                "url": bep_url,
                "action_id": f"view-{bep['id'].lower()}"
            }
        ]
        
        if feedback_url:
            buttons.append({
                "type": "button",
                "text": {
                    "type": "plain_text",
                    "text": "Leave Feedback",
                    "emoji": False
                },
                "url": feedback_url,
                "action_id": f"feedback-{bep['id'].lower()}"
            })
        
        blocks.append({
            "type": "actions",
            "elements": buttons
        })
    
    # Add context footer
    context_elements = []
    if workflow_url:
        context_elements.append({
            "type": "mrkdwn",
            "text": f"<{workflow_url}|View workflow run>"
        })
    context_elements.append({
        "type": "mrkdwn", 
        "text": f"<{base_url}|View full site>"
    })
    
    blocks.append({"type": "divider"})
    blocks.append({
        "type": "context",
        "elements": context_elements
    })
    
    return {"blocks": blocks}


def main():
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-url", default="", help="Base URL for BEP links")
    parser.add_argument("--env", default="canary", help="Environment name (canary or branch name)")
    parser.add_argument("--workflow-url", default="", help="URL to GitHub workflow run")
    parser.add_argument("--format", choices=["json", "slack"], default="json", help="Output format")
    args = parser.parse_args()
    
    summary = generate_summary(base_url=args.base_url)
    
    if args.format == "slack":
        output = generate_slack_blocks(
            summary, 
            env=args.env, 
            base_url=args.base_url,
            workflow_url=args.workflow_url
        )
    else:
        output = summary
    
    print(json.dumps(output))


if __name__ == "__main__":
    main()

