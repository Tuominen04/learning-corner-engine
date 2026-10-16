# Security

## Reporting a problem

Please **do not** open a public issue for a security problem. Report it privately instead:
[open a private advisory](https://github.com/Tuominen04/learning-corner-engine/security/advisories/new).
Only the maintainer can see it.

Say what you found, how to reproduce it, and what someone could do with it. You will get a reply
within a week. Once it is fixed and released, the advisory is published with credit to you,
unless you would rather stay anonymous.

## What counts

The engine reads and writes files in a folder a person chose, and parses files they bring in —
PDFs, images, imported text. The most serious problems are those where a crafted file or library:

- reads or writes outside the library folder,
- destroys or silently changes content the person did not ask to change,
- crashes the engine in a way that leaves a library damaged.

The engine makes no network requests, so there is no server or account to attack.

## Supported versions

Only the latest release receives fixes.
