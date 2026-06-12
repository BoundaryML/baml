"""Changelog generation worker (the absorbed baml-changelog2 service).

Claims queued changelogEntries rows and runs the two-agent draft/critique
loop with real code validation, writing the finished entry back onto the row.
"""
