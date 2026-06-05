"""BAML release channel classification, shared by the api, client, and worker.

Two channels are tracked in the build registry:

  * nightly — pre-release builds tagged ``baml-language-…-nightly.<date>.<letter>``.
  * canary  — the plain stable releases ``baml-language-<X.Y.Z>`` (e.g.
              ``baml-language-0.11.2``), with no nightly/alpha suffix.

Legacy ``alpha`` and any other tags are untracked (``None``) and get pruned.
Keeping this logic in one place means the GitHub-release resolver, the retention
prune, and the worker's per-run selection all agree on what channel a build is.
"""

from __future__ import annotations

import re
from typing import Optional

#: Channels the registry retains (newest-N each) and that a run may select.
TRACKED_CHANNELS: tuple[str, ...] = ("nightly", "canary")
#: Channel used when a run does not specify one.
DEFAULT_CHANNEL = "nightly"

# A canary build is a plain stable release: baml-language-<major>.<minor>.<patch>
# with no pre-release suffix (no -nightly / -alpha / etc.).
_STABLE_RE = re.compile(r"^baml-language-\d+\.\d+\.\d+$")


def channel_of_tag(tag: Optional[str]) -> Optional[str]:
    """Classify a GitHub release tag into a tracked channel.

    Args:
        tag: The release ``tag_name`` (e.g. ``baml-language-0.11.2``), or None.

    Returns:
        ``"nightly"`` for nightly pre-releases, ``"canary"`` for plain stable
        ``baml-language-X.Y.Z`` releases, or None for untracked tags (alpha,
        non-baml-language, or anything else).
    """
    if not tag or not tag.startswith("baml-language-"):
        return None
    if "nightly" in tag:
        return "nightly"
    if _STABLE_RE.match(tag):
        return "canary"
    return None
