# Sheep Council email

Sheep Council meets every Monday at 9:00 AM Pacific Time. We announce the upcoming meeting with a Loops email every Friday at 1:00 PM Pacific Time.

## Prepare the weekly email

Prepare and upload the Loops email before the human chooses the week's topic. Do not wait for the topic to be finalized; leave clear topic and description placeholders for the human to edit in Loops.

1. Copy [`email-data/template.lmx`](email-data/template.lmx) to a new dated `.lmx` file in `email-data/`.
2. Use Browser to open Google Calendar and find the `BAML Team Review` event on the upcoming Monday. Use that specific recurring event instance, not an event from another week.
3. Update the new LMX file with the event's Monday date, 9:00 AM PT time, and Google Calendar event link. Replace the template's static date and calendar link with the values from the upcoming event.
4. Leave the topic, preview text, and topic details as obvious placeholders if the human has not chosen them yet.
5. Upload the prepared email to Loops. Creating a campaign requires the explicit LMX file:

```sh
infisical run --projectId=bdd280e2-259c-4750-9b16-a8597a67214c --env=dev-humans -- uv run tools/sheep-council/prepare-sheep-council-email.py upload tools/sheep-council/email-data/YYYY-MM-DD-sheep-council.lmx
```

To update a campaign that was already created, choose it explicitly with `--campaign-id`. Pass `--email-message-id` as well when the campaign's specific email message must be selected:

```sh
infisical run --projectId=bdd280e2-259c-4750-9b16-a8597a67214c --env=dev-humans -- uv run tools/sheep-council/prepare-sheep-council-email.py upload tools/sheep-council/email-data/YYYY-MM-DD-sheep-council.lmx --campaign-id CAMPAIGN_ID --email-message-id EMAIL_MESSAGE_ID
```

The uploader prints a `campaignUrl`. Give that exact link to the human so they can open the draft in Loops, choose or revise the topic, update the subject and body, review the rendered email, and schedule or send it for Friday at 1:00 PM PT. The uploader does not schedule or send the campaign.

## Download the campaign archive

Download every campaign and email message in the Sheep Council campaign group as uploadable LMX source and Markdown previews:

```sh
infisical run --projectId=bdd280e2-259c-4750-9b16-a8597a67214c --env=dev-humans -- uv run tools/sheep-council/prepare-sheep-council-email.py download
```

Files are written to the ignored `email-data/downloads/` directory by default. Pass `--output-dir PATH` to choose a different destination.
