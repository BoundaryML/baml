# CodeTurtle UI prototype

Hosted mock: https://codeturtle-ui-prototype.vercel.app/ (the previous Shep URL also works).

CodeTurtle is a proposed GitHub App for owner-focused review. This directory contains a UI-only prototype with fictional PR contents and browser-memory state. It does not authenticate users, connect to GitHub or Slack, post messages, generate CODEOWNERS, or enforce merge requirements. Reloading or resetting clears the demo.

Run `npm ci` and `npm run dev` in this directory. The directory remains `tools/shep` to preserve the existing Vercel project connection. Open `http://127.0.0.1:4317`. Run `npm run build` for the static production bundle. A Vercel project can use `tools/shep` as its root directory.

The workspace opens on owned files with a Monaco diff editor. Each owned file requires a manual Approve file or Reject file action in the diff. Rejections need a file-specific reason. Decisions and comments stay private until Submit review publishes them; submission is enabled only after every owned file has an explicit current-revision decision. There is no bulk approval, default decision, or automatic move to another file. Switch the demo reviewer to exercise both-owner environment requirements and independent reviewer decisions. The mock super admin is `@hellovai`; this is illustrative, not a real permission grant. Overrides require a reason, clear the selected changes-request block, and appear in the audit log and message previews. They do not replace required sign-offs. Simulating a commit changes `lib.rs`, making only its approvals and associated draft comments outdated.

The ownership preview shows `.github/codeturtle/OWNERS` and a BAML extension, matching the local CodeTurtle runner's policy layout. The GitHub preview represents one updated bot comment and a required check. The Slack preview represents one thread in a repo channel. These are product previews; real authorization, GitHub App registration, Slack installation, persistence, and policy enforcement remain future implementation work.

Brand reference: [CodeRabbit's brand guidelines](https://www.coderabbit.ai/brand). The mock uses orange `#FF570A`, dark mauve neutrals, Geist for the interface, and Hack for code. The turtle SVG is original CodeTurtle artwork. Fonts are bundled locally through npm, with their upstream licenses in their packages.
