import '@fontsource/geist/latin-400.css';
import '@fontsource/geist/latin-600.css';
import 'hack-font/build/web/hack.css';
import * as monaco from 'monaco-editor/editor/editor.api';
import 'monaco-editor/languages/definitions/rust/register';
import 'monaco-editor/languages/definitions/markdown/register';
import EditorWorker from 'monaco-editor/editor/editor.worker?worker';
import '../node_modules/monaco-editor/esm/vs/base/browser/ui/codicons/codicon/codicon.css';
import { files, reviewers, ownership, initialState, fileStatus } from './mock.js';
import './style.css';

self.MonacoEnvironment = { getWorker: () => new EditorWorker() };
monaco.languages.register({ id: 'baml' });
monaco.languages.setMonarchTokensProvider('baml', { tokenizer: { root: [[/\b(class|enum|function|client|test|retry_policy)\b/, 'keyword'], [/\b(string|int|float|bool)\b/, 'type'], [/"[^"\\]*(?:\\.[^"\\]*)*"/, 'string'], [/\/\/.*$/, 'comment'], [/\b\d+\b/, 'number'], [/@[\w]+/, 'annotation']] } });
monaco.editor.defineTheme('codeturtle', {
  base: 'vs-dark', inherit: true,
  rules: [{ token: 'comment', foreground: '7D8594' }, { token: 'keyword', foreground: 'C8BAFF' }, { token: 'string', foreground: '91D8C0' }, { token: 'number', foreground: 'E4B77B' }, { token: 'type', foreground: '8DB8CF' }],
  colors: { 'editor.background': '#18171a', 'editorGutter.background': '#18171a', 'editorLineNumber.foreground': '#5f6777', 'editorLineNumber.activeForeground': '#d0d0d7', 'editor.lineHighlightBackground': '#232225', 'diffEditor.insertedTextBackground': '#73be8528', 'diffEditor.removedTextBackground': '#da777c25', 'diffEditor.insertedLineBackground': '#51835c12', 'diffEditor.removedLineBackground': '#b7585f12', 'editor.selectionBackground': '#ff570a30', 'editorWidget.background': '#232225', 'editorOverviewRuler.border': '#2b292d' },
});

const escape = value => String(value).replace(/[&<>"']/g, char => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[char]);
let state = initialState();
let diffEditor, models = [];
const app = document.querySelector('#app');
const selected = () => files.find(file => file.id === state.selected);
const mine = () => files.filter(file => file.owners.includes(state.user));
const myDrafts = () => state.drafts.filter(draft => draft.user === state.user);
const myPendingDecisions = () => state.pendingDecisions.filter(decision => decision.user === state.user);
const currentDecision = file => [...state.decisions].reverse().find(decision => decision.file === file.id && decision.user === state.user && decision.revision === state.revision[file.id]);
const stagedDecision = file => myPendingDecisions().find(decision => decision.file === file.id && decision.revision === state.revision[file.id]);
const myDecision = file => stagedDecision(file) || currentDecision(file);
const undecidedFiles = () => mine().filter(file => !myDecision(file));
function personalStatus(file) {
  const decision = myDecision(file);
  return { state: decision?.kind === 'approve' ? 'approved' : decision?.kind === 'changes' ? 'blocked' : 'pending', label: decision ? `${decision.kind === 'approve' ? 'Approved' : 'Rejected'}${stagedDecision(file) ? ' · draft' : ''}` : 'Not reviewed' };
}
const time = () => new Date().toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' });
const audit = text => state.activity.push({ text, at: time() });
const statusLabel = { pending: 'Awaiting sign-off', approved: 'Approved', blocked: 'Changes requested', unowned: 'No owner assigned' };

function avatar(user, small = false) {
  const info = reviewers[user];
  return `<span class="avatar ${small ? 'small' : ''}" style="--avatar-color:${info?.color || '#8191aa'}" aria-hidden="true">${info?.initials || 'S'}</span>`;
}

function summary() {
  const owned = files.filter(file => file.owners.length);
  const approved = owned.filter(file => fileStatus(file, state).state === 'approved').length;
  const blocked = owned.filter(file => fileStatus(file, state).state === 'blocked').length;
  return { owned, approved, blocked, ready: approved === owned.length && !blocked };
}

function reviewerProgress(user) {
  const owned = files.filter(file => file.owners.includes(user));
  const approved = owned.filter(file => fileStatus(file, state).approvals.has(user)).length;
  return `${approved}/${owned.length}`;
}

function header() {
  return `<header class="appbar">
    <div class="brand"><img class="brand-logo" src="/codeturtle.svg" alt="" /><span class="brand-wordmark">codeturtle<span class="brand-tagline">Review at human speed.</span></span> <span class="brand-divider">/</span><span class="repo-name">BoundaryML / baml</span><span class="prototype">Prototype</span></div>
    <nav aria-label="Workspace"><button data-tab="review" class="${state.tab === 'review' ? 'active' : ''}">Review</button><button data-tab="github" class="${state.tab === 'github' ? 'active' : ''}">PR message</button><button data-tab="slack" class="${state.tab === 'slack' ? 'active' : ''}">Slack</button><button data-tab="ownership" class="${state.tab === 'ownership' ? 'active' : ''}">Ownership</button></nav>
    <label class="identity">${avatar(state.user, true)}<select id="user" aria-label="Demo reviewer">${Object.keys(reviewers).map(user => `<option value="${user}" ${state.user === user ? 'selected' : ''}>@${user}</option>`).join('')}</select></label>
  </header>
  <section class="prbar"><div class="pr-title"><div class="pr-kicker"><span class="codicon codicon-git-pull-request" aria-hidden="true"></span><span>Pull request</span><span class="pr-number">#42</span><span class="open-label">Open</span></div><h1>Add a configurable request timeout</h1><div class="prmeta"><span>demo-author</span><span>·</span><code>feature/request-timeout</code><span>→</span><code>canary</code><span>·</span><span>4 files changed</span></div></div><details class="demo-tools"><summary><span class="codicon codicon-settings-gear" aria-hidden="true"></span> Demo controls</summary><div><span class="muted">Fictional PR · ${state.commit === 1 ? 'e2c1fa9' : 'f43ab' + state.commit + 'd'}</span><button class="button ghost" data-action="commit">Simulate new commit</button><button class="button ghost" data-action="reset">Reset demo</button></div></details></section>`;
}

function fileTree() {
  const visible = state.showAll ? files : mine();
  return `<aside class="filetree"><div class="section-label">${state.showAll ? 'Changed files' : 'Your files'} <span>${visible.length}</span></div><div class="tree-filter"><span class="codicon codicon-filter" aria-hidden="true"></span>${state.showAll ? 'All changed files' : `Owned by @${state.user}`}</div><div class="filelist">${visible.map(file => {
    const status = file.owners.includes(state.user) ? personalStatus(file) : { ...fileStatus(file, state), label: statusLabel[fileStatus(file, state).state] };
    const shortPath = file.directory.includes('baml_env') ? 'crates / baml_env / src' : file.directory.includes('baml_std') ? 'baml_std / baml / ns_http' : file.directory === 'baml_language' ? 'baml_language' : file.directory;
    return `<div class="file-group"><div class="file-directory" title="${escape(file.directory)}">${escape(shortPath)}</div><button class="file ${state.selected === file.id ? 'selected' : ''}" data-file="${file.id}" title="${escape(file.directory + '/' + file.name)}"><span class="codicon codicon-file-code" aria-hidden="true"></span><strong>${file.name}</strong><span class="file-dot ${status.state}" role="img" aria-label="${status.label}"></span></button></div>`;
  }).join('')}</div><button class="reveal-files" data-action="show-all"><span class="codicon codicon-${state.showAll ? 'eye-closed' : 'eye'}" aria-hidden="true"></span>${state.showAll ? 'Hide other files' : `Show ${files.length - mine().length} other ${files.length - mine().length === 1 ? 'file' : 'files'}`}</button><div class="tree-foot"><button data-tab="ownership"><span class="codicon codicon-symbol-key" aria-hidden="true"></span> .github/codeturtle/OWNERS</button><span>Rules from the target branch</span></div></aside>`;
}

function commentCard(comment, draft = false) {
  const outdated = comment.revision !== state.revision[comment.file];
  return `<article class="comment-card ${draft ? 'draft' : ''}"><div class="comment-meta">${avatar(comment.user, true)}<strong>@${comment.user}</strong><span>${comment.side === 'LEFT' ? 'Original' : 'New'} line ${comment.line}</span>${draft ? '<span class="draft-label">Draft</span>' : ''}${outdated ? '<span class="outdated">Outdated</span>' : ''}</div><p>${escape(comment.body)}</p>${draft ? `<button class="text-button" data-remove-draft="${comment.id}">Discard draft</button>` : `<button class="text-button" data-reply="${comment.id}">Reply</button>`}</article>`;
}

function reviewPane() {
  const file = selected();
  const isMine = file.owners.includes(state.user);
  const status = fileStatus(file, state);
  const comments = state.comments.filter(comment => comment.file === file.id);
  const drafts = myDrafts().filter(comment => comment.file === file.id);
  const decision = myDecision(file);
  const staged = stagedDecision(file);
  return `<section class="review-pane"><div class="editor-toolbar"><div class="editor-file"><span class="codicon codicon-file-code" aria-hidden="true"></span><strong>${file.name}</strong><span class="changes"><span>+${file.additions}</span><span>−${file.deletions}</span></span></div><div class="editor-actions"><button class="button small ghost" data-action="layout"><span class="codicon codicon-split-horizontal" aria-hidden="true"></span>${state.split ? 'Unified' : 'Split'}</button><button class="button small ghost" data-action="comment" ${!isMine ? 'disabled' : ''}><span class="codicon codicon-comment" aria-hidden="true"></span>Add comment</button></div></div>
    <div class="file-breadcrumb" title="${escape(file.directory + '/' + file.name)}">${escape(file.directory)}<span>/</span><strong>${file.name}</strong></div><div id="editor" aria-label="Diff for ${escape(file.name)}"></div>
    <div class="file-decision-bar"><div class="decision-summary"><span class="decision-label">Your decision</span><span class="${decision?.kind === 'approve' ? 'approved' : decision?.kind === 'changes' ? 'blocked' : ''}">${isMine ? decision ? `${decision.kind === 'approve' ? 'Approved' : 'Rejected'}${staged ? ' · unpublished' : ' · submitted'}` : 'Not reviewed' : 'Read-only · not your file'}</span></div>${isMine ? `<div class="decision-actions"><button class="button small approve-file ${decision?.kind === 'approve' ? 'chosen' : ''}" data-action="approve-file" aria-pressed="${decision?.kind === 'approve'}"><span class="codicon codicon-check" aria-hidden="true"></span>Approve file</button><button class="button small reject-file ${decision?.kind === 'changes' ? 'chosen' : ''}" data-action="reject-file" aria-pressed="${decision?.kind === 'changes'}"><span class="codicon codicon-close" aria-hidden="true"></span>Reject file</button>${staged ? '<button class="text-button undo-decision" data-action="undo-decision">Undo</button>' : ''}</div>` : ''}</div>
    ${decision?.kind === 'changes' ? `<div class="decision-reason">${escape(decision.body)}</div>` : ''}
    <div class="file-review-state"><span class="status-tag ${status.state}"><span class="file-dot ${status.state}"></span>${statusLabel[status.state]}</span><span>Required owner approvals</span></div>
    <section class="conversation ${comments.length + drafts.length ? 'has-comments' : ''}"><div class="conversation-heading"><h2><span class="codicon codicon-comment-discussion" aria-hidden="true"></span>Comments <span>${comments.length + drafts.length}</span></h2><span>${drafts.length ? `${drafts.length} unpublished ${drafts.length === 1 ? 'draft' : 'drafts'}` : 'Publish with your review'}</span></div>${[...comments.map(c => commentCard(c)), ...drafts.map(c => commentCard(c, true))].join('')}</section></section>`;
}

function progressPanel() {
  const data = summary();
  const file = selected();
  const status = fileStatus(file, state);
  const remaining = undecidedFiles().length;
  const decisionCount = mine().length - remaining;
  const publishable = myPendingDecisions().length || myDrafts().length;
  return `<aside class="progress-panel"><div class="review-context"><section class="progress-overview"><div class="section-label">Pull request status</div><div class="gate-state ${data.ready ? 'approved' : data.blocked ? 'blocked' : ''}"><span class="codicon codicon-${data.ready ? 'pass' : data.blocked ? 'error' : 'circle-large-outline'}" aria-hidden="true"></span>${data.ready ? 'Reviews complete' : data.blocked ? 'Changes requested' : 'Review required'}</div><div class="approval-summary"><span>Files approved</span><strong>${data.approved} / ${data.owned.length}</strong></div><div class="progress-track"><span style="width:${data.approved / data.owned.length * 100}%"></span></div></section>
    <section class="file-ownership"><div class="section-label">Owners of ${file.name}</div><p class="ownership-mode">${file.all ? 'Both owners must approve' : file.owners.length ? 'One owner must approve' : 'No owner assigned'}</p>${file.owners.map(user => `<div class="reviewer-row">${avatar(user, true)}<strong>@${user}</strong><span class="owner-decision ${status.activeBlocks.some(b => b.user === user) ? 'blocked' : status.approvals.has(user) ? 'approved' : ''}" title="${status.activeBlocks.some(b => b.user === user) ? 'Changes requested' : status.approvals.has(user) ? 'Approved' : 'Awaiting approval'}"><span class="codicon codicon-${status.activeBlocks.some(b => b.user === user) ? 'error' : status.approvals.has(user) ? 'check' : 'circle-outline'}" aria-hidden="true"></span></span></div>`).join('')}
    <details class="ownership-details"><summary>Why these owners?</summary><div><code>${escape(file.rule || 'No matching rule')}</code><button class="text-button" data-tab="ownership">.github/codeturtle/OWNERS${file.rule ? `:${file.ruleLine}` : ''} ↗</button><p>${file.rule ? 'Matching ownership rule on the target branch.' : 'This file has no matching ownership rule.'}</p></div></details></section>
    ${status.activeBlocks.map(block => `<div class="block-card"><strong>@${block.user} requested changes</strong><p>${escape(block.body || 'This file needs another look.')}</p>${reviewers[state.user].role === 'Super admin' ? `<button class="button small warning" data-override="${block.user}">Override block</button>` : '<span>The reviewer or a super admin can clear this block.</span>'}</div>`).join('')}
    <section class="all-reviewers"><div class="section-label">All reviewers <span>Files approved</span></div>${Object.keys(reviewers).map(user => `<div class="reviewer-row">${avatar(user, true)}<strong>@${user}</strong><span class="review-count">${reviewerProgress(user)}</span></div>`).join('')}</section>
    <details class="activity"><summary>Activity <span>${state.activity.length}</span></summary>${state.activity.slice(-4).reverse().map(event => `<div><p>${escape(event.text)}</p><span>${event.at}</span></div>`).join('')}</details>
    </div><div class="submit-section"><div class="your-review-progress"><span>Your review</span><strong>${decisionCount} / ${mine().length} files</strong></div><p>${remaining ? `Decide on ${remaining === 1 ? 'the remaining file' : `each of the ${remaining} remaining files`}` : 'Every file has an explicit decision'}</p><button class="button primary" data-action="submit" ${remaining || !publishable ? 'disabled' : ''}>Submit review<span class="codicon codicon-arrow-right" aria-hidden="true"></span></button><span>${publishable ? `${myPendingDecisions().length} ${myPendingDecisions().length === 1 ? 'decision' : 'decisions'} · ${myDrafts().length} ${myDrafts().length === 1 ? 'comment' : 'comments'} unpublished` : remaining ? 'Approve or reject each file in the diff' : 'No unpublished decisions or comments'}</span></div></aside>`;
}

function githubPreview() {
  const data = summary();
  return `<main class="preview-page"><div class="preview-heading"><span class="eyebrow">GITHUB · PR CONVERSATION</span><h2>GitHub comment</h2><p>CodeTurtle updates its comment as reviews progress.</p></div><div class="github-shell"><div class="github-comment"><div class="github-comment-header"><span class="brandmark tiny"><img src="/codeturtle.svg" alt="CodeTurtle" /></span><strong>codeturtle[bot]</strong><span class="bot-label">bot</span><span>commented just now</span></div><div class="github-comment-body"><h3>CodeTurtle · Owner review</h3><p class="github-summary"><strong>${data.approved} of ${data.owned.length} owned files approved</strong><span class="status-tag ${data.ready ? 'approved' : data.blocked ? 'blocked' : 'pending'}">${data.ready ? 'Ready to merge' : data.blocked ? 'Changes requested' : 'Awaiting sign-off'}</span></p><table><thead><tr><th>Code owner</th><th>Sign-offs</th><th>Files</th></tr></thead><tbody>${Object.keys(reviewers).map(user => `<tr><td>@${user}</td><td>${reviewerProgress(user)}</td><td>${files.filter(file => file.owners.includes(user)).map(file => `<code>${file.name}</code>`).join(' ')}</td></tr>`).join('')}</tbody></table><p>Environment changes require both @hellovai and @aaronvg.</p>${state.overrides.map(o => `<div class="override-record"><strong>Super admin override · @${o.by}</strong><p>${escape(files.find(f => f.id === o.file).name)} · @${o.user}'s changes request${o.revision !== state.revision[o.file] ? ' · previous revision' : ''}</p><p>${escape(o.reason)}</p></div>`).join('')}<button class="button primary" data-tab="review">Review your files</button><p class="github-footnote">Each file must be explicitly approved or rejected by its reviewer. Changed files need a fresh decision.<br>Ownership comes from <code>.github/codeturtle/OWNERS</code> on the target branch.</p></div></div><div class="github-check"><span class="check-symbol ${data.ready ? 'approved' : ''}">${data.ready ? '✓' : '◷'}</span><div><strong>CodeTurtle / owner review</strong><p>${data.ready ? 'All required owners have signed off.' : 'Waiting for required file sign-offs.'}</p></div><button class="text-button" data-tab="review">Details</button></div></div><p class="preview-caption">Message preview · actions update this prototype only.</p></main>`;
}

function slackPreview() {
  const data = summary();
  return `<main class="preview-page"><div class="preview-heading"><span class="eyebrow">SLACK · REPO CHANNEL</span><h2>Slack notification</h2><p>One thread per PR. Code owners are tagged when a PR becomes ready for review.</p></div><div class="slack-shell"><div class="slack-channel"># <strong>baml-reviews</strong><span>Repo channel</span></div><div class="slack-message"><span class="brandmark"><img src="/codeturtle.svg" alt="CodeTurtle" /></span><div><div class="slack-author"><strong>CodeTurtle</strong><span class="bot-label">APP</span><time>11:03 AM</time></div><p><strong>Review requested</strong> · BoundaryML/baml #42</p><p class="slack-pr-title">Add a configurable request timeout</p><p><span class="mention">@hellovai</span> <span class="mention">@aaronvg</span> <span class="mention">@2kai2kai2</span> — review and decide on each of your files.</p><div class="slack-attachment"><strong>${data.approved}/${data.owned.length} owned files approved</strong><p>Environment changes need both @hellovai and @aaronvg.</p><button class="button small primary" data-tab="review">Review your files</button></div><p class="slack-thread-label">${Math.max(0, state.activity.length - 1)} thread ${state.activity.length - 1 === 1 ? 'reply' : 'replies'}</p></div></div><div class="slack-thread"><div class="section-label">THREAD</div>${state.activity.slice(1).map(event => `<div class="slack-message"><span class="brandmark tiny"><img src="/codeturtle.svg" alt="CodeTurtle" /></span><div><div class="slack-author"><strong>CodeTurtle</strong><time>${event.at}</time></div><p>${escape(event.text)}</p></div></div>`).join('') || '<p class="muted">Review progress, new commits, and overrides by super admins appear here.</p>'}</div></div><p class="preview-caption">Message preview · no Slack messages are sent.</p></main>`;
}

function ownershipPreview() {
  const extension = `function adds_environment_read(change: codeturtle.FileChange) -> bool throws never {
  change.added_code.includes("std::env::var(")
    || change.added_code.includes("env::var(")
}`;
  return `<main class="ownership-page"><div class="preview-heading"><span class="eyebrow">OWNERSHIP · TARGET BRANCH</span><h2>Ownership rules</h2><p>Familiar ownership rules. BAML functions for the changes that need a closer look.</p></div><div class="ownership-columns"><section><div class="code-title"><strong>.github/codeturtle/OWNERS</strong><span class="source-badge">RULES</span></div><pre>${escape(ownership)}</pre></section><section><div class="code-title"><strong>ns_checks/ns_environment/environment.baml</strong><span class="source-badge">CONDITION</span></div><pre>${escape(extension)}</pre><p class="extension-note">Lives in .github/codeturtle/. Nested folders use BAML namespaces; <code>when=checks.environment.adds_environment_read</code> resolves this function.</p></section></div><div class="rule-explanations"><article><h3>Review what you own</h3><p>Any listed owner can sign off by default. Matching rules accumulate; conditional rules add reviewers.</p></article><article><h3>Every file. Every owner.</h3><p><code># codeturtle: all</code> requires every listed owner. Each file gets its own explicit decision.</p></article><article><h3>An accountable escape hatch</h3><p>Super admins can override a block with a reason. The decision appears in the audit log, PR comment, and Slack thread.</p></article></div><p class="preview-caption">Illustrative configuration · this prototype does not modify repository files or enforce GitHub checks.</p></main>`;
}

function render() {
  diffEditor?.dispose(); models.forEach(model => model.dispose()); models = [];
  app.innerHTML = header() + (state.tab === 'review' ? `<main class="workspace">${fileTree()}${reviewPane()}${progressPanel()}</main>` : state.tab === 'github' ? githubPreview() : state.tab === 'slack' ? slackPreview() : ownershipPreview()) + '<div id="toast" role="status" aria-live="polite"></div>';
  if (state.tab !== 'review') return;
  const file = selected();
  const modified = file.modified + (file.id === 'env' && state.revision.env > 1 ? `\n// Revision ${state.revision.env}: timeout validation is being refined.\n` : '');
  models = [monaco.editor.createModel(file.original, file.language), monaco.editor.createModel(modified, file.language)];
  diffEditor = monaco.editor.createDiffEditor(document.querySelector('#editor'), { theme: 'codeturtle', readOnly: true, originalEditable: false, renderSideBySide: state.split, useInlineViewWhenSpaceIsLimited: true, automaticLayout: true, minimap: { enabled: false }, fontSize: 13, lineHeight: 22, fontFamily: 'Hack, monospace', scrollBeyondLastLine: false, glyphMargin: true, lineNumbersMinChars: 3, renderOverviewRuler: false, diffCodeLens: false, wordWrap: 'on', stickyScroll: { enabled: false }, accessibilitySupport: 'on', padding: { top: 16, bottom: 16 }, hideUnchangedRegions: { enabled: false } });
  diffEditor.setModel({ original: models[0], modified: models[1] });
  for (const [editor, side] of [[diffEditor.getOriginalEditor(), 'LEFT'], [diffEditor.getModifiedEditor(), 'RIGHT']]) {
    editor.addAction({ id: `codeturtle-comment-${side}`, label: 'Draft a CodeTurtle comment', contextMenuGroupId: 'navigation', contextMenuOrder: 1, run: () => openComment(editor.getPosition()?.lineNumber || 1, side) });
    editor.onMouseDown(event => {
      if (event.target.type === monaco.editor.MouseTargetType.GUTTER_GLYPH_MARGIN && event.target.position) openComment(event.target.position.lineNumber, side);
    });
    const comments = [...state.comments, ...myDrafts()].filter(c => c.file === file.id && c.side === side && c.revision === state.revision[file.id]);
    editor.createDecorationsCollection(comments.map(c => ({ range: new monaco.Range(c.line, 1, c.line, 1), options: { isWholeLine: true, className: 'codeturtle-comment-line', glyphMarginClassName: 'codeturtle-comment-glyph', glyphMarginHoverMessage: { value: `CodeTurtle comment by @${c.user}` } } })));
  }
}

function toast(message) {
  const node = document.querySelector('#toast'); node.textContent = message; node.classList.add('visible');
  setTimeout(() => node.classList.remove('visible'), 4500);
}

function modal(content, onSubmit) {
  const dialog = document.createElement('dialog');
  dialog.className = 'codeturtle-dialog'; dialog.innerHTML = `<form>${content}</form>`;
  document.body.append(dialog);
  dialog.addEventListener('close', () => dialog.remove());
  dialog.querySelector('[data-cancel]')?.addEventListener('click', () => dialog.close());
  dialog.querySelector('form').addEventListener('submit', event => { event.preventDefault(); onSubmit(new FormData(event.target), dialog); });
  dialog.showModal(); return dialog;
}

function openComment(line, side = 'RIGHT', replyTo = null) {
  const file = selected();
  if (!file.owners.includes(state.user)) { toast('You can comment on files you own.'); return; }
  modal(`<div class="modal-eyebrow">DRAFT COMMENT</div><h2>${replyTo ? 'Reply to this thread' : 'Leave a note'}</h2><p>${file.name} · ${side === 'LEFT' ? 'original' : 'new'} line ${line}</p><label class="field-label" for="comment-body">Comment</label><textarea id="comment-body" name="body" required rows="5" placeholder="What should the author know?" autofocus></textarea><div class="modal-note">This stays private until you submit your review.</div><div class="modal-actions"><button class="button ghost" type="button" data-cancel>Cancel</button><button class="button primary" type="submit">Save draft</button></div>`, (data, dialog) => {
    const body = data.get('body').trim(); if (!body) return;
    state.drafts.push({ id: crypto.randomUUID(), file: file.id, revision: state.revision[file.id], user: state.user, line, side, body, replyTo });
    dialog.close(); render(); toast('Draft saved. It will publish with your review.');
  });
}

function stageDecision(kind, body = '') {
  const file = selected();
  if (!file.owners.includes(state.user)) return;
  state.pendingDecisions = state.pendingDecisions.filter(decision => !(decision.file === file.id && decision.user === state.user));
  state.pendingDecisions.push({ id: crypto.randomUUID(), file: file.id, user: state.user, revision: state.revision[file.id], kind, body });
  render();
}

function rejectFile() {
  const file = selected();
  if (!file.owners.includes(state.user)) return;
  modal(`<div class="modal-eyebrow">FILE DECISION</div><h2>Reject ${file.name}</h2><p>Request changes on this file. Other files keep their own decisions.</p><label class="field-label" for="rejection-reason">What needs to change?</label><textarea id="rejection-reason" name="reason" rows="4" required autofocus placeholder="Explain the issue in this file">${escape(myDecision(file)?.kind === 'changes' ? myDecision(file).body : '')}</textarea><div class="modal-note">This decision stays private until you submit your review.</div><div class="form-error" role="alert"></div><div class="modal-actions"><button class="button ghost" type="button" data-cancel>Cancel</button><button class="button reject-file" type="submit">Save rejection</button></div>`, (data, dialog) => {
    const reason = data.get('reason').trim();
    if (!reason) { dialog.querySelector('.form-error').textContent = 'Explain what needs to change in this file.'; return; }
    dialog.close(); stageDecision('changes', reason);
  });
}

function openReview() {
  if (undecidedFiles().length) { toast('Approve or reject each of your files before submitting.'); return; }
  const pending = myPendingDecisions();
  const drafts = myDrafts();
  if (!pending.length && !drafts.length) return;
  modal(`<div class="modal-eyebrow">PUBLISH REVIEW</div><h2>Submit your review</h2><p>Publish the decisions you made individually. No other files will be approved.</p><div class="review-decisions">${pending.map(decision => `<div><span>${files.find(file => file.id === decision.file).name}</span><strong class="${decision.kind === 'approve' ? 'approved' : 'blocked'}">${decision.kind === 'approve' ? 'Approve' : 'Reject'}</strong>${decision.kind === 'changes' ? `<p>${escape(decision.body)}</p>` : ''}</div>`).join('')}</div><div class="modal-note">${drafts.length} draft ${drafts.length === 1 ? 'comment' : 'comments'} will publish with this review. Return to a file to change its decision.</div><div class="form-error" role="alert"></div><div class="modal-actions"><button class="button ghost" type="button" data-cancel>Back to files</button><button class="button primary" type="submit">Publish review</button></div>`, (data, dialog) => {
    const fail = message => { dialog.querySelector('.form-error').textContent = message; };
    if (undecidedFiles().length) { fail('A file still needs an explicit decision. Return to your files.'); return; }
    if (pending.some(decision => decision.revision !== state.revision[decision.file]) || drafts.some(draft => draft.revision !== state.revision[draft.file])) { fail('A decision or comment refers to an older revision. Review that file again and discard any outdated comments.'); return; }
    for (const decision of pending) {
      state.decisions = state.decisions.filter(previous => !(previous.file === decision.file && previous.user === decision.user && previous.revision === decision.revision));
      state.decisions.push(decision);
      audit(decision.kind === 'approve' ? `@${state.user} approved ${files.find(file => file.id === decision.file).name}.` : `@${state.user} requested changes on ${files.find(file => file.id === decision.file).name}: ${decision.body}`);
    }
    state.pendingDecisions = state.pendingDecisions.filter(decision => decision.user !== state.user);
    state.comments.push(...drafts); state.drafts = state.drafts.filter(draft => draft.user !== state.user);
    if (drafts.length) audit(`@${state.user} published ${drafts.length} ${drafts.length === 1 ? 'comment' : 'comments'}.`);
    dialog.close(); render(); toast('Individual file decisions published. PR and Slack previews updated.');
  });
}

function openOverride(user) {
  if (reviewers[state.user].role !== 'Super admin') return;
  const file = selected();
  const block = fileStatus(file, state).activeBlocks.find(decision => decision.user === user);
  if (!block) return;
  modal(`<div class="modal-eyebrow">SUPER ADMIN OVERRIDE</div><h2>Override this block?</h2><p>Clear @${user}'s changes request on <strong>${file.name}</strong>.</p><label class="field-label" for="override-reason">Reason for override</label><textarea id="override-reason" name="reason" required minlength="10" rows="4" placeholder="Explain why this can proceed"></textarea><div class="modal-note">Your name and reason will appear in CodeTurtle, the PR comment, and the Slack thread. Required file sign-offs still apply.</div><div class="modal-actions"><button class="button ghost" type="button" data-cancel>Cancel</button><button class="button warning" type="submit">Confirm override</button></div>`, (data, dialog) => {
    const reason = data.get('reason').trim(); if (reason.length < 10) return;
    state.overrides.push({ file: file.id, user, by: state.user, revision: block.revision, block: block.id, reason });
    audit(`Super admin @${state.user} overrode @${user}'s block on ${file.name}: ${reason}`);
    dialog.close(); render(); toast('Override recorded in the audit log and both message previews.');
  });
}

app.addEventListener('change', event => {
  if (event.target.id !== 'user') return;
  state.user = event.target.value;
  if (!selected().owners.includes(state.user) && !state.showAll) state.selected = mine()[0]?.id || 'env';
  render();
});

app.addEventListener('click', event => {
  const button = event.target.closest('button'); if (!button || button.disabled) return;
  if (button.dataset.tab) { state.tab = button.dataset.tab; render(); return; }
  if (button.dataset.file) { state.selected = button.dataset.file; render(); return; }
  if (button.dataset.override) { openOverride(button.dataset.override); return; }
  if (button.dataset.removeDraft) { state.drafts = state.drafts.filter(d => d.id !== button.dataset.removeDraft); render(); return; }
  if (button.dataset.reply) { const comment = state.comments.find(c => c.id === button.dataset.reply); openComment(comment.line, comment.side, comment.id); return; }
  switch (button.dataset.action) {
    case 'show-all': state.showAll = !state.showAll; if (!state.showAll && !selected().owners.includes(state.user)) state.selected = mine()[0]?.id || 'env'; render(); break;
    case 'layout': state.split = !state.split; render(); break;
    case 'comment': openComment(diffEditor?.getModifiedEditor().getPosition()?.lineNumber || 1); break;
    case 'approve-file': stageDecision('approve'); break;
    case 'reject-file': rejectFile(); break;
    case 'undo-decision': state.pendingDecisions = state.pendingDecisions.filter(decision => !(decision.file === state.selected && decision.user === state.user)); render(); break;
    case 'submit': openReview(); break;
    case 'commit': state.commit++; state.revision.env++; state.pendingDecisions = state.pendingDecisions.filter(decision => decision.file !== 'env'); audit('New commit changed lib.rs. @hellovai and @aaronvg: fresh sign-offs needed. Unchanged files keep their sign-offs.'); render(); toast('lib.rs changed. Its previous sign-offs are now outdated.'); break;
    case 'reset': state = initialState(); render(); toast('Demo reset.'); break;
  }
});

render();
