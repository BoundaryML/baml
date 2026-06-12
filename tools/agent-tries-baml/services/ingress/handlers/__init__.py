"""@bammy intent handlers: bench, changelog_edit, promo, feedback.

Each handler is a plain async function taking the ServiceClient explicitly
(no module-global service) so the ingress app and the bammy router can share
them and tests can inject fakes.
"""
