import argparse
import asyncio
import os
import sys
from datetime import datetime
from typing import Optional

import discord
import requests
from dotenv import load_dotenv
from notion_client import AsyncClient

load_dotenv()

from baml_client import b
from baml_client.types import Issue, Message, MessageType, ThreadMessage


intents = discord.Intents.default()
intents.message_content = True

discord_client = discord.Client(intents=intents)
notion_client = AsyncClient(auth=os.environ["NOTION_API_KEY"])

# TODO: Environment variables.

# Discord channel ID where we read messages/threads from.
GENERAL_CHANNEL_ID = 1119375594984050779

# Notion page ID where we create databases. The Notion integration needs to
# have access to this page.
NOTION_PAGE_ID = "135bb2d2621680a99929f9a31e96c4bc"

# Repository URL. If the repo is not public you need to add Bearer token to the
# request headers in the fetch_github_issues() function.
GITHUB_REPO_URL = "https://api.github.com/repos/boundaryml/baml"

# 3 requests per second.
NOTION_RATE_LIMIT = 1/3

# Number of messages to fetch and send to LLM in a single batch. Ideally this
# should be based on how "big" the overall prompt ends up being because it
# depends on the length of each messsages.
MESSAGES_BATCH_SIZE = 10


# Just for type hinting.
class Args(argparse.Namespace):
    after: datetime
    before: Optional[datetime]


# Global CLI args.
args = Args()


class ClassifiedMessage:
    def __init__(self, message: discord.Message, type: MessageType):
        self.message = message
        self.message_type = type


class SummerizedThread:
    def __init__(self, messages: list[discord.Message], summary: str):
        self.messages = messages
        self.thread_summary = summary

class RelatedIssue:
    def __init__(self, message: discord.Message, issue: Issue):
        self.message = message
        self.issue = issue


NotionRequest = ClassifiedMessage | SummerizedThread | RelatedIssue


def notion_message_category(message: MessageType) -> str:
    match message:
        case MessageType.FeatureRequest:
            return "Feature Request"
        case MessageType.BugReport:
            return "Bug Report"
        case MessageType.Question:
            return "Question"
        case MessageType.Uncategorized:
            return "Uncategorized"


def format_time_period() -> str:
    after = args.after.strftime("%Y-%m-%d")
    before = (args.before or datetime.now()).strftime("%Y-%m-%d")

    return f"{after} to {before}"


# TODO. RAG + Embeddings. Store all issues in a database and keep in sync with
# Github webhooks.
def fetch_github_issues() -> list[Issue]:
    url = f"{GITHUB_REPO_URL}/issues"

    # Results per page. Max is 100. For the purposes of this example we'll
    # just fetch one page of issues. Ideally we would store all of them in a
    # vector database and use RAG.
    per_page = 100 

    # State of the issue. Can be either open, closed, or all.
    state = "all"

    # TODO: Bearer token for private repos.
    response = requests.get(f"{url}?state={state}&per_page={per_page}")
    response.raise_for_status()

    issues = []
    for issue in response.json():
        issues.append(Issue(
            number=issue["number"],
            title=issue["title"],
            body=issue["body"],
            type="pull_request" if "pull_request" in issue else "issue",
        ))

    return issues


async def send_notion_requests(
    database_id: str,
    request_queue: asyncio.Queue[NotionRequest]
):
    # Maps a message ID to the corresponding Notion page ID. That way we can
    # grab the page ID when we get the thread summary and append a block to the
    # notion page without querying the Notion database. Same for linking github
    # issues or PR, we get the page ID from here.
    # TODO: Pop items out of the dict when both the PR and summary are added.
    # It's tricky because there might be no PR to link.
    page_ids: dict[int, str] = {}

    while True:
        request = await request_queue.get()

        if isinstance(request, ClassifiedMessage):
            message = request.message
            category = notion_message_category(request.message_type)

            response = await notion_client.pages.create(
                parent={"database_id": database_id},
                properties={
                    "Message": {
                        "rich_text": [{"text": {"content": message.content}}]
                    },
                    "Category": {
                        "select": {"name": category}
                    },
                    "Message Type": {
                        "select": {"name": "Thread" if message.thread else "Default"}
                    },
                    "ID": {
                        "title": [{"text": {"content": str(message.id)}}]
                    },
                    "Link": {
                        "url": message.jump_url
                    },
                    "Created Date": {
                        "date": {"start": message.created_at.isoformat()}
                    }
                }
            )

            if message.thread is not None:
                page_ids[message.id] = response["id"]
        elif isinstance(request, SummerizedThread):
            try:
                page_id = page_ids[request.messages[0].id]
                await notion_client.blocks.children.append(page_id, children=[
                    {
                        "object": "block",
                        "type": "heading_2",
                        "heading_2": {
                            "rich_text": [{ "type": "text", "text": { "content": "Summary" } }]
                        }
                    },
                    {
                        "object": "block",
                        "type": "paragraph",
                        "paragraph": {
                            "rich_text": [
                                {"type": "text", "text": {"content": request.thread_summary}}
                            ]
                        },
                    },
                ])
            except KeyError:
                print(f"Failed to find page ID for message {request.messages[0].id}", file=sys.stderr)
        elif isinstance(request, RelatedIssue):
            try:
                page_id = page_ids[request.message.id]

                route = "pull" if request.issue.type == "pull_request" else "issues"
                url = f"https://github.com/BoundaryML/baml/{route}/{request.issue.number}"

                await notion_client.pages.update(page_id, properties={"Issue/PR": {"url": url}})
            except KeyError:
                print(f"Failed to find page ID for message {request.message.id}", file=sys.stderr)
        else:
            print(f"Ignoring unknown task type: {type(request)}", file=sys.stderr)

        # TODO: Add retry and exponential backoff. Right now everything seems
        # to run slow enough so we don't hit the rate limit.
        # asyncio.sleep(NOTION_RATE_LIMIT)

        request_queue.task_done()


async def pull_discord_threads(
    thread_reading_queue: asyncio.Queue[discord.Message],
    thread_summary_queue: asyncio.Queue[list[discord.Message]]
):
    while True:
        message = await thread_reading_queue.get()

        if message.thread is None:
            continue

        thread = [message]
        async for thread_message in message.thread.history():
            thread.append(thread_message)
        
        thread_summary_queue.put_nowait(thread)

        thread_reading_queue.task_done()


async def summerize_threads(
    thread_summary_queue: asyncio.Queue[list[discord.Message]],
    notion_request_queue: asyncio.Queue[NotionRequest]
):
    while True:
        thread = await thread_summary_queue.get()
        summary = await b.SummerizeThread(
            [ThreadMessage(user_id=m.author.id, content=m.content) for m in thread]
        )

        notion_request_queue.put_nowait(SummerizedThread(thread, summary))

        thread_summary_queue.task_done()


async def find_related_issues(
    issues: list[Issue],
    related_issues_queue: asyncio.Queue[discord.Message],
    notion_request_queue: asyncio.Queue[NotionRequest]
):
    mapping = {issue.number: issue for issue in issues}

    while True:
        message = await related_issues_queue.get()
        if issue_number := await b.FindRelatedIssue(message.content, issues):
            if issue_number in mapping:
                notion_request_queue.put_nowait(RelatedIssue(message, mapping[issue_number]))
            else:
                print(f"Issue {issue_number} not found", file=sys.stderr)

        related_issues_queue.task_done()


async def classify_discord_messages(
    message_queue: asyncio.Queue[list[discord.Message]],
    notion_request_queue: asyncio.Queue[NotionRequest],
    thread_reading_queue: asyncio.Queue[discord.Message],
    related_issues_queue: asyncio.Queue[discord.Message],
):
    while True:
        messages = await message_queue.get()

        mapping = {message.id: message for message in messages}
        batch = [Message(id=m.id, content=m.content) for m in messages]

        for classification in await b.ClassifyMessages(batch):
            # This should not happen but the LLM might return the wrong ID.
            if classification.message_id not in mapping:
                print(f"Ignoring message with unknown ID: {classification.message_id}", file=sys.stderr)
                continue

            # Skip uncategorized messages
            if classification.message_type == MessageType.Uncategorized:
                continue

            # Get the message object back.
            message = mapping[classification.message_id]

            # Put message in the queue for Notion.
            notion_request_queue.put_nowait(ClassifiedMessage(message, classification.message_type))

            # Now that we've put the message request in the notion queue we can
            # safely pull the thread and summerize it, since notion requests are
            # processed in order which means by the time we get the thread
            # summary the Notion page for that thread will already exist.
            if message.thread is not None:
                thread_reading_queue.put_nowait(message)

            # Same with finding issues.
            if classification.message_type in [MessageType.FeatureRequest, MessageType.BugReport]:
                related_issues_queue.put_nowait(message)

        message_queue.task_done()


# TODO: According to the documentation this function could run multiple times
# so handle that.
# https://discordpy.readthedocs.io/en/stable/api.html#discord.on_ready
@discord_client.event
async def on_ready():
    print(f'We have logged in as {discord_client.user}')

    print("Fetching issues from GitHub...")

    issues = await asyncio.to_thread(fetch_github_issues)

    print("Creating Notion database...")

    database = await notion_client.databases.create(
        parent={"type": "page_id", "page_id": NOTION_PAGE_ID},
        title= [{
            "type": "text",
            "text": {"content": format_time_period()}
        }],
        properties={
            "ID": {"title": {}},
            "Category": {
                "select": {
                    "options": [
                        {"name": "Feature Request", "color": "blue"},
                        {"name": "Bug Report", "color": "brown"},
                        {"name": "Uncategorized", "color": "pink"},
                        {"name": "Question", "color": "yellow"},
                    ]
                },
            },
            "Message": {"rich_text": {}},
            "Message Type": {
                "select": {
                    "options": [
                        {"name": "Thread", "color": "green"},
                        {"name": "Default", "color": "blue"},
                    ]
                },
            },
            "Link": {"url": {}},
            "Issue/PR": {"url": {}},
            "Created Date": {"date": {}},
        }
    )

    print(f"Notion database created: {database['url']}")

    message_queue = asyncio.Queue()
    thread_reading_queue = asyncio.Queue()
    thread_summary_queue = asyncio.Queue()
    related_issues_queue = asyncio.Queue()
    notion_request_queue = asyncio.Queue()

    async with asyncio.TaskGroup() as tg:
        # Spawn workers.
        workers = [tg.create_task(t) for t in (
            send_notion_requests(database["id"], notion_request_queue),
            classify_discord_messages(message_queue, notion_request_queue, thread_reading_queue, related_issues_queue),
            pull_discord_threads(thread_reading_queue, thread_summary_queue),
            summerize_threads(thread_summary_queue, notion_request_queue),
            find_related_issues(issues, related_issues_queue, notion_request_queue),
        )]

        batch = []

        channel = discord_client.get_channel(GENERAL_CHANNEL_ID)

        # Read messages in batches.
        async for message in channel.history(after=args.after, before=args.before):
            # Skip messages like "user created thread"
            if message.type != discord.MessageType.default:
                continue

            # Skip empty messages
            if message.content.strip() == "":
                continue

            batch.append(message)

            # Put batch in the queue and continue reading messages.
            if len(batch) >= MESSAGES_BATCH_SIZE:
                message_queue.put_nowait(batch)
                batch = []

        # Process remaining messages.
        if len(batch) > 0:
            message_queue.put_nowait(batch)

        # Wait for all requests to finish.
        for queue in [
            message_queue,
            thread_reading_queue,
            thread_summary_queue,
            related_issues_queue,
            notion_request_queue,
        ]:
            await queue.join()

        # Cancel workers.
        for worker in workers:
            worker.cancel()

    # Close connection to Discord.
    await discord_client.close()

    print(f"\n\nBatch {format_time_period()} completed")


def main():
    parser = argparse.ArgumentParser()

    parser.add_argument(
        "--after",
        type=datetime.fromisoformat,
        help="Start date for fetching messages",
        required=True,
    )

    parser.add_argument(
        "--before",
        type=datetime.fromisoformat, 
        help="End date for fetching messages",
    )

    global args
    args = Args(**vars(parser.parse_args()))

    if args.before is not None and args.before < args.after:
        parser.error("End date must be after start date")

    discord_client.run(os.environ['DISCORD_BOT_TOKEN'])


if __name__ == "__main__":
    main()
