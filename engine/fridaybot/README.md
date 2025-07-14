# Friday Bot

First install [`poetry`](https://python-poetry.org/docs/#installation). Then
install the dependencies:

```bash
poetry install
```

After that copy the `.env.example` file and set your API keys:

```bash
cp .env.example .env
```

Note that some of the variables are still hard coded like the Notion page ID or
Discord channel ID where messages are read from. You can change those in the
code at the top of [`fridaybot/app.py`](./fridaybot/app.py).

Run the script like this:

```bash
python -m fridaybot.app --after=2024-11-01
```

The `--after` parameter is the start date and that's required, optionally you
can give it a `--before` date (end date) which defaults to `datetime.now()`.