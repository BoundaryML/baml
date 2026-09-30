"""Pydantic mirrors of `baml_src/*.prompt.baml` for the OpenAI and Anthropic structured-output paths.

Keep these field-for-field identical to the BAML classes so all three backends are asked for the same shape.
"""

from typing import Literal, Optional

from pydantic import BaseModel


class User(BaseModel):
    id: str
    is_bot: bool
    login: str
    name: str


class Actor(BaseModel):
    login: str


class Label(BaseModel):
    name: str


class ChangedFile(BaseModel):
    path: str
    additions: int
    deletions: int
    changeType: Literal["ADDED", "MODIFIED", "DELETED", "RENAMED", "COPIED", "CHANGED"]


class CommitAuthor(BaseModel):
    email: str
    id: str
    login: str
    name: str


class Commit(BaseModel):
    authoredDate: str
    authors: list[CommitAuthor]
    committedDate: str
    messageBody: str
    messageHeadline: str
    oid: str


class CommitRef(BaseModel):
    oid: str


class Review(BaseModel):
    id: str
    author: Actor
    authorAssociation: str
    body: str
    submittedAt: str
    state: Literal["APPROVED", "CHANGES_REQUESTED", "COMMENTED", "DISMISSED", "PENDING"]
    commit: CommitRef


class Comment(BaseModel):
    id: str
    author: Actor
    authorAssociation: str
    body: str
    createdAt: str
    url: str


class PullRequest(BaseModel):
    number: int
    title: str
    url: str
    state: Literal["OPEN", "CLOSED", "MERGED"]
    isDraft: bool
    author: User
    createdAt: str
    mergedAt: Optional[str]
    closedAt: Optional[str]
    baseRefName: str
    headRefName: str
    labels: list[Label]
    additions: int
    deletions: int
    changedFiles: int
    body: str
    files: list[ChangedFile]
    commits: list[Commit]
    reviews: list[Review]
    comments: list[Comment]
