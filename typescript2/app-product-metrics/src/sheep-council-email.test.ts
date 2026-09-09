import assert from 'node:assert/strict';
import test from 'node:test';

import type {
  LoopsCampaign,
  LoopsClient,
  UpdateLoopsEmailMessageInput,
} from './clients/loops.js';
import {
  assertSheepCouncilDraftReady,
  parseSheepCouncilEmailLmx,
  prepareSheepCouncilEmail,
} from './sheep-council-email.js';

const lmx = `---
name: "Sheep Council Mon Sep 7"
campaignGroupId: "group-id"
mailingListId: "list-id"
subject: "Sheep Council Mon Sep 7, 9am PT - Agents & tools"
previewText: "Join us for agents and tools."
fromName: "Vaibhav from Boundary"
fromEmail: "vbv"
replyToEmail: "vbv@boundaryml.com"
emailFormat: "styled"
contactPropertiesFallbacks: {"firstName":"there"}
---
<Style bodyFontFamily="Default" bodyFontCategory="sans-serif" textBaseFontSize="14" textBaseLineHeight="150" />
<Paragraph>Hello Sheep Councillor {contact.firstName}!</Paragraph>
<Paragraph></Paragraph>
<Paragraph>We’ll see you on <Strong textColor="#6d28d9">Mon Sep 7, 9am PT.</Strong></Paragraph>
<Paragraph><Link href="https://example.com/?one=1&amp;two=2"><Underline>Report back</Underline></Link></Paragraph>`;

test('parseSheepCouncilEmailLmx passes raw LMX through with API metadata', () => {
  const email = parseSheepCouncilEmailLmx(lmx);
  assert.equal(email.campaign.name, 'Sheep Council Mon Sep 7');
  assert.equal(
    email.message.subject,
    'Sheep Council Mon Sep 7, 9am PT - Agents & tools',
  );
  assert.equal(email.message.fromName, 'Vaibhav from Boundary');
  assert.equal(email.message.fromEmail, 'vbv');
  assert.equal(email.message.lmx, lmx.slice(lmx.indexOf('\n---\n') + 5));
  assert.deepEqual(email.message.contactPropertiesFallbacks, {
    firstName: 'there',
  });
});

test('parseSheepCouncilEmailLmx requires Loops sender fields', () => {
  assert.throws(
    () => parseSheepCouncilEmailLmx(lmx.replace(/^fromName:.*\n/m, '')),
    /frontmatter.fromName/,
  );
});

test('assertSheepCouncilDraftReady rejects unfinished LMX', () => {
  assert.throws(
    () => assertSheepCouncilDraftReady(`${lmx}\nTODO: finish this`),
    /still contains TODO/,
  );
  assert.doesNotThrow(() => assertSheepCouncilDraftReady(lmx));
});

function fakeClient(existing?: LoopsCampaign) {
  const calls: {
    create: unknown[];
    getMessage: string[];
    updateCampaign: unknown[];
    updateMessage: Array<{
      id: string;
      input: UpdateLoopsEmailMessageInput;
    }>;
  } = { create: [], getMessage: [], updateCampaign: [], updateMessage: [] };
  const campaign: LoopsCampaign =
    existing ??
    ({
      campaignGroupId: 'group-id',
      emailMessageId: 'message-id',
      id: 'new-campaign-id',
      mailingListId: 'list-id',
      name: 'Sheep Council Mon Sep 7',
      status: 'Draft',
      url: 'https://app.loops.so/campaigns/new-campaign-id',
    } satisfies LoopsCampaign);
  const client: LoopsClient = {
    async createCampaign(input) {
      calls.create.push(input);
      return { ...campaign, emailMessageContentRevisionId: 'new-revision' };
    },
    async getEmailMessage(id) {
      calls.getMessage.push(id);
      return {
        contentRevisionId: 'existing-revision',
        id,
        lmx: '<Paragraph>Old</Paragraph>',
        subject: 'Old',
      };
    },
    async listCampaignGroups() {
      return [{ id: 'group-id', name: 'sheep council' }];
    },
    async listCampaigns() {
      return existing ? [existing] : [];
    },
    async listMailingLists() {
      return [{ id: 'list-id', name: 'Sheep Council' }];
    },
    async updateCampaign(id, input) {
      calls.updateCampaign.push({ id, input });
      return campaign;
    },
    async updateEmailMessage(id, input) {
      calls.updateMessage.push({ id, input });
      return {
        contentRevisionId: 'updated-revision',
        id,
        lmx: input.lmx,
        subject: input.subject,
      };
    },
  };
  return { calls, client };
}

test('prepareSheepCouncilEmail dry-run does not write', async () => {
  const { calls, client } = fakeClient();
  const spec = parseSheepCouncilEmailLmx(lmx);
  const result = await prepareSheepCouncilEmail(client, spec, false);
  assert.equal(result.action, 'create');
  assert.equal(result.applied, false);
  assert.deepEqual(calls.create, []);
  assert.deepEqual(calls.updateMessage, []);
});

test('prepareSheepCouncilEmail creates and fills a new draft', async () => {
  const { calls, client } = fakeClient();
  const spec = parseSheepCouncilEmailLmx(lmx);
  const result = await prepareSheepCouncilEmail(client, spec, true);
  assert.equal(result.action, 'create');
  assert.equal(result.applied, true);
  assert.equal(calls.create.length, 1);
  assert.equal(
    calls.updateMessage[0]?.input.expectedRevisionId,
    'new-revision',
  );
});

test('prepareSheepCouncilEmail updates the matching draft revision', async () => {
  const existing: LoopsCampaign = {
    campaignGroupId: 'group-id',
    emailMessageId: 'existing-message-id',
    id: 'existing-campaign-id',
    mailingListId: 'list-id',
    name: 'Sheep Council Mon Sep 7',
    status: 'Draft',
    url: 'https://app.loops.so/campaigns/existing-campaign-id',
  };
  const { calls, client } = fakeClient(existing);
  const spec = parseSheepCouncilEmailLmx(lmx);
  const result = await prepareSheepCouncilEmail(client, spec, true);
  assert.equal(result.action, 'update');
  assert.equal(result.applied, true);
  assert.deepEqual(calls.getMessage, ['existing-message-id']);
  assert.equal(
    calls.updateMessage[0]?.input.expectedRevisionId,
    'existing-revision',
  );
});
